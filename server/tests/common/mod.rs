//! Shared test harness: the real router on a SQLite file in a temp dir.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use toomanyprojects::{AppState, Config, build_router, db, themes::ThemeStore, updates::UpdateChecker};
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub data_dir: std::path::PathBuf,
    pub state: AppState,
    _dir: tempfile::TempDir,
}

pub struct Res {
    pub status: StatusCode,
    pub cookie: Option<String>,
    pub body: Value,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// A test app with some settings changed from [`Config::for_tests`].
    pub async fn with_config(change: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::for_tests(dir.path().to_path_buf());
        change(&mut config);
        let tls = toomanyprojects::tls::Tls::start(&config).await.unwrap();
        let db = db::connect(&config.data_dir).await.unwrap();
        let updates = UpdateChecker::from_config(&config);
        let state = AppState {
            db,
            config: Arc::new(config),
            themes: Arc::new(ThemeStore::bundled()),
            updates,
            logins: Default::default(),
            tls,
        };
        Self { router: build_router(state.clone()), data_dir: dir.path().to_path_buf(), state, _dir: dir }
    }

    pub async fn call(&self, method: &str, path: &str, cookie: Option<&str>, body: Option<Value>) -> Res {
        let mut req = Request::builder().method(method).uri(path).header(header::HOST, "localhost:8080");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let req = match body {
            Some(b) => {
                req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap()
            }
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Res { status, cookie, body }
    }

    /// A request with a raw body; returns the status and the raw response.
    pub async fn call_raw(
        &self,
        method: &str,
        path: &str,
        cookie: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:8080")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body))
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (status, res.into_body().collect().await.unwrap().to_bytes().to_vec())
    }

    pub async fn setup_owner(&self) -> String {
        let res = self
            .call(
                "POST",
                "/api/v1/auth/setup",
                None,
                Some(json!({"name": "Logan", "email": "Logan@Example.com", "password": "correct horse"})),
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.cookie.unwrap()
    }
}
