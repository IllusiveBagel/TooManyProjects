mod common;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Json, Router, http::StatusCode, routing::get};
use common::TestApp;
use serde_json::{Value, json};
use toomanyprojects::updates::UpdateChecker;

/// A stand-in for GitHub's "latest release" endpoint that answers with `body`
/// (or a 404 when it is `None`), counting calls.
async fn fake_github(body: Option<Value>) -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let app = Router::new().route(
        "/latest",
        get(move || {
            let body = body.clone();
            counter.fetch_add(1, Ordering::SeqCst);
            async move {
                match body {
                    Some(b) => (StatusCode::OK, Json(b)),
                    None => (StatusCode::NOT_FOUND, Json(json!({"message": "Not Found"}))),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/latest"), calls)
}

fn release(tag: &str) -> Value {
    json!({
        "tag_name": tag,
        "name": format!("{tag} — sharper boards"),
        "html_url": format!("https://github.com/IllusiveBagel/TooManyProjects/releases/tag/{tag}"),
        "published_at": "2026-11-08T12:00:00Z",
        "draft": false,
        "prerelease": false,
    })
}

#[tokio::test]
async fn finds_a_newer_release() {
    let (url, calls) = fake_github(Some(release("v1.3.0"))).await;
    let checker = UpdateChecker::new(true, url, Some("1.2.0"));
    assert!(checker.info().await.latest.is_none());

    checker.check().await;
    let info = checker.info().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(info.update_available);
    assert_eq!(info.current, "1.2.0");
    let latest = info.latest.unwrap();
    assert_eq!(latest.version, "1.3.0");
    assert!(latest.url.ends_with("/releases/tag/v1.3.0"));
    assert!(info.checked_at.is_some());
    assert!(info.error.is_none());
}

#[tokio::test]
async fn up_to_date_and_development_builds_show_nothing() {
    let (url, _) = fake_github(Some(release("v1.2.0"))).await;
    let same = UpdateChecker::new(true, url.clone(), Some("1.2.0"));
    same.check().await;
    assert!(!same.info().await.update_available);

    let dev = UpdateChecker::new(true, url, None);
    dev.check().await;
    let info = dev.info().await;
    assert!(info.development_build);
    assert!(!info.update_available);
    assert_eq!(info.latest.unwrap().version, "1.2.0");
}

#[tokio::test]
async fn no_release_failures_and_off_switch() {
    let (url, _) = fake_github(None).await;
    let none = UpdateChecker::new(true, url, Some("1.2.0"));
    none.check().await;
    let info = none.info().await;
    assert!(info.latest.is_none() && info.error.is_none() && !info.update_available);

    // Nothing listens on port 9 here, so the request fails and the error is kept.
    let down = UpdateChecker::new(true, "http://127.0.0.1:9/latest".into(), Some("1.2.0"));
    down.check().await;
    assert!(down.info().await.error.unwrap().starts_with("Couldn't reach GitHub"));

    let (url, calls) = fake_github(Some(release("v9.0.0"))).await;
    let off = UpdateChecker::new(false, url, Some("1.2.0"));
    off.check().await;
    let info = off.info().await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!info.check_enabled && !info.update_available);
}

#[tokio::test]
async fn only_the_owner_sees_it() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let res = app.call("GET", "/api/v1/system/version", Some(&logan), None).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    // Tests run with the check off.
    assert_eq!(res.body["check_enabled"], false);
    assert_eq!(res.body["update_available"], false);

    let invite = app.call("POST", "/api/v1/invites", Some(&logan), Some(json!({}))).await;
    let sam = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": invite.body["token"], "name": "Sam", "email": "sam@example.com", "password": "a long password"})),
        )
        .await
        .cookie
        .unwrap();
    let res = app.call("GET", "/api/v1/system/version", Some(&sam), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.call("POST", "/api/v1/system/version/check", Some(&sam), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.call("GET", "/api/v1/system/version", None, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}
