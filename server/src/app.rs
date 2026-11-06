use std::sync::Arc;

use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderValue, Method, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    auth,
    config::Config,
    error::AppError,
    invites,
    openapi::ApiDoc,
    themes::{self, ThemeStore},
};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub themes: Arc<ThemeStore>,
}

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
}

/// Liveness check, used by the Docker health check.
#[utoipa::path(get, path = "/health", tag = "system", responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health { status: "ok", version: env!("CARGO_PKG_VERSION") })
}

/// The `/api/v1` routes and their OpenAPI description.
pub fn api() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, mut doc) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health))
        .merge(auth::router())
        .merge(invites::router())
        .merge(themes::router())
        .split_for_parts();
    // No licence is chosen yet; utoipa would otherwise emit an empty one from Cargo.toml.
    doc.info.license = None;
    (router, doc)
}

pub fn openapi_json() -> String {
    api().1.to_pretty_json().expect("OpenAPI serialises")
}

/// Refuses state-changing API calls whose `Origin` is another site. Together
/// with `SameSite=Lax` cookies and JSON-only bodies this blocks CSRF.
async fn same_origin(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe {
        let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok());
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok());
        if let (Some(origin), Some(host)) = (origin, host) {
            let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
            if origin_host != host {
                return AppError::Forbidden("Cross-site request refused.".into()).into_response();
            }
        }
    }
    next.run(req).await
}

pub fn build_router(state: AppState) -> Router {
    let (api, _) = api();
    let api = api.fallback(|| async { AppError::NotFound }).layer(middleware::from_fn(same_origin)).layer(
        SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
    );

    let mut router = Router::new().nest("/api/v1", api).route("/api/health", get(health)).route(
        "/api/openapi.json",
        get(|| async { ([(header::CONTENT_TYPE, "application/json")], openapi_json()) }),
    );

    if let Some(dir) = &state.config.static_dir {
        // Unknown paths get index.html so the web app's own routes work on reload.
        let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
        router = router.fallback_service(spa);
    }

    router
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
