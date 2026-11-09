use std::sync::Arc;

use anyhow::Context;
use axum::http::{HeaderName, HeaderValue};
use toomanyprojects::{
    AppState, Config, app, build_router, db, notes, themes::ThemeStore, updates::UpdateChecker,
};
use tower_http::set_header::SetRequestHeaderLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        // Used to regenerate the web app's API client.
        Some("openapi") => {
            println!("{}", app::openapi_json());
            return Ok(());
        }
        // Used by the Docker health check; the runtime image has no curl.
        Some("healthcheck") => return healthcheck(),
        Some("serve") | None => {}
        Some(other) => anyhow::bail!("unknown command {other}; use `serve`, `openapi` or `healthcheck`"),
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "toomanyprojects=info,tower_http=info".into()),
        )
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let config = Config::from_env()?;
    let db = db::connect(&config.data_dir).await?;
    let themes = ThemeStore::load(&config.data_dir.join("themes"))?;
    match &config.static_dir {
        Some(dir) => tracing::info!(dir = %dir.display(), "serving web app"),
        None => tracing::warn!("no built web app found; serving the API only"),
    }

    let bind = config.bind;
    let updates = UpdateChecker::from_config(&config);
    updates.spawn();
    let tls = toomanyprojects::tls::Tls::start(&config).await.context("setting up HTTPS")?;
    let state = AppState {
        db,
        config: Arc::new(config),
        themes: Arc::new(themes),
        updates,
        logins: Default::default(),
        tls,
    };
    // Pick up notes edited outside the app (another editor, a synced folder).
    // Polling, because file watching is unreliable on Docker volumes.
    let scan_state = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tick.tick().await;
            if let Err(e) = notes::vault::scan_all(&scan_state.db, &scan_state.config.data_dir).await {
                tracing::warn!(error = %e, "scanning note vaults");
            }
        }
    });
    let https = axum_server::Handle::new();
    if let (Some(tls), Some(addr)) = (state.tls.clone(), state.config.https_bind) {
        // Tell the app the request came in over HTTPS, as a proxy would, so
        // sign-in addresses and the like start with https://. Overriding
        // drops whatever a client sent.
        let app = build_router(state.clone()).layer(SetRequestHeaderLayer::overriding(
            HeaderName::from_static("x-forwarded-proto"),
            HeaderValue::from_static("https"),
        ));
        let handle = https.clone();
        let server = axum_server::bind_rustls(addr, tls.rustls.clone()).handle(handle);
        tracing::info!(%addr, "listening for HTTPS");
        tokio::spawn(async move {
            if let Err(e) = server.serve(app.into_make_service()).await {
                tracing::error!(error = %e, %addr, "HTTPS stopped");
            }
        });
    }
    let listener = tokio::net::TcpListener::bind(bind).await.with_context(|| format!("binding {bind}"))?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, build_router(state)).with_graceful_shutdown(shutdown()).await?;
    https.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

fn healthcheck() -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let port = Config::from_env()?.bind.port();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    stream.write_all(b"GET /api/health HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf)?;
    let head = String::from_utf8_lossy(&buf[..n]);
    anyhow::ensure!(head.split_whitespace().nth(1) == Some("200"), "unhealthy: {head}");
    Ok(())
}
