//! End-to-end checks of setup, login, invites and sessions against a real
//! SQLite file in a temp dir.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::TestApp;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn setup_runs_once_and_signs_in() {
    let app = TestApp::new().await;
    let res = app.call("GET", "/api/v1/auth/status", None, None).await;
    assert_eq!(res.body["needs_setup"], true);

    let cookie = app.setup_owner().await;
    let me = app.call("GET", "/api/v1/me", Some(&cookie), None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["email"], "logan@example.com");
    assert_eq!(me.body["role"], "owner");
    assert_eq!(me.body["theme_id"], "workshop");

    let again = app
        .call(
            "POST",
            "/api/v1/auth/setup",
            None,
            Some(json!({"name": "Eve", "email": "eve@example.com", "password": "another one!"})),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let status = app.call("GET", "/api/v1/auth/status", Some(&cookie), None).await;
    assert_eq!(status.body["needs_setup"], false);
    assert_eq!(status.body["user"]["name"], "Logan");
}

#[tokio::test]
async fn login_and_logout() {
    let app = TestApp::new().await;
    app.setup_owner().await;

    let wrong = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "logan@example.com", "password": "nope nope nope"})),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong.body["code"], "invalid_credentials");
    let unknown = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "who@example.com", "password": "correct horse"})),
        )
        .await;
    assert_eq!(unknown.body["code"], "invalid_credentials");

    let ok = app
        .call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": " LOGAN@example.com ", "password": "correct horse"})),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie = ok.cookie.unwrap();
    assert_eq!(app.call("GET", "/api/v1/me", Some(&cookie), None).await.status, StatusCode::OK);

    let out = app.call("POST", "/api/v1/auth/logout", Some(&cookie), None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(app.call("GET", "/api/v1/me", Some(&cookie), None).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invites_are_the_only_way_in() {
    let app = TestApp::new().await;
    let owner = app.setup_owner().await;

    // Nobody can join without a token.
    let bogus = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": "made-up", "name": "Sam", "email": "sam@example.com", "password": "sam's password"})),
        )
        .await;
    assert_eq!(bogus.status, StatusCode::NOT_FOUND);

    let created = app
        .call("POST", "/api/v1/invites", Some(&owner), Some(json!({"note": "Sam", "expires_in_days": 3})))
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let token = created.body["token"].as_str().unwrap().to_string();
    assert_eq!(created.body["url"].as_str().unwrap(), format!("http://localhost:8080/invite/{token}"));

    let lookup = app.call("GET", &format!("/api/v1/invites/lookup/{token}"), None, None).await;
    assert_eq!(lookup.status, StatusCode::OK);
    assert_eq!(lookup.body["invited_by"], "Logan");

    let accepted = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": token, "name": "Sam", "email": "sam@example.com", "password": "sam's password"})),
        )
        .await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.body);
    assert_eq!(accepted.body["role"], "member");
    let sam = accepted.cookie.unwrap();

    // Used once only.
    let reuse = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": token, "name": "Kim", "email": "kim@example.com", "password": "kim's password"})),
        )
        .await;
    assert_eq!(reuse.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.call("GET", &format!("/api/v1/invites/lookup/{token}"), None, None).await.status,
        StatusCode::NOT_FOUND
    );

    // Members cannot make invites.
    let member_invite = app.call("POST", "/api/v1/invites", Some(&sam), Some(json!({}))).await;
    assert_eq!(member_invite.status, StatusCode::FORBIDDEN);

    let list = app.call("GET", "/api/v1/invites", Some(&owner), None).await;
    assert_eq!(list.body[0]["accepted_by_name"], "Sam");
}

#[tokio::test]
async fn revoked_invite_cannot_be_used() {
    let app = TestApp::new().await;
    let owner = app.setup_owner().await;
    let created = app.call("POST", "/api/v1/invites", Some(&owner), Some(json!({}))).await;
    let id = created.body["invite"]["id"].as_str().unwrap();
    let token = created.body["token"].as_str().unwrap();
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/invites/{id}"), Some(&owner), None).await.status,
        StatusCode::NO_CONTENT
    );
    let res = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": token, "name": "Sam", "email": "sam@example.com", "password": "sam's password"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn validation_and_preferences() {
    let app = TestApp::new().await;
    let weak = app
        .call(
            "POST",
            "/api/v1/auth/setup",
            None,
            Some(json!({"name": "L", "email": "l@example.com", "password": "short"})),
        )
        .await;
    assert_eq!(weak.body["code"], "weak_password");
    let bad_email = app
        .call(
            "POST",
            "/api/v1/auth/setup",
            None,
            Some(json!({"name": "L", "email": "nope", "password": "long enough pw"})),
        )
        .await;
    assert_eq!(bad_email.body["code"], "invalid_email");

    let cookie = app.setup_owner().await;
    let res = app
        .call("PATCH", "/api/v1/me/preferences", Some(&cookie), Some(json!({"theme_mode": "light"})))
        .await;
    assert_eq!(res.body["theme_mode"], "light");
    let res =
        app.call("PATCH", "/api/v1/me/preferences", Some(&cookie), Some(json!({"theme_id": "nope"}))).await;
    assert_eq!(res.body["code"], "unknown_theme");
}

#[tokio::test]
async fn cross_site_writes_are_refused() {
    let app = TestApp::new().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/setup")
        .header(header::HOST, "localhost:8080")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"name": "L", "email": "l@example.com", "password": "long enough pw"}).to_string(),
        ))
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn themes_and_health() {
    let app = TestApp::new().await;
    let list = app.call("GET", "/api/v1/themes", None, None).await;
    assert_eq!(list.body[0]["id"], "workshop");
    let theme = app.call("GET", "/api/v1/themes/workshop", None, None).await;
    assert_eq!(theme.body["modes"]["dark"]["semantic"]["accent"], "{sage-300}");
    assert_eq!(app.call("GET", "/api/v1/themes/missing", None, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.call("GET", "/api/health", None, None).await.body["status"], "ok");
    assert_eq!(app.call("GET", "/api/v1/nope", None, None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_passwords_slow_down_and_a_bad_invite_says_nothing() {
    let app = TestApp::new().await;
    app.setup_owner().await;
    let login = |password: &'static str| {
        app.call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "logan@example.com", "password": password})),
        )
    };
    for _ in 0..5 {
        assert_eq!(login("wrong password").await.body["code"], "invalid_credentials");
    }
    // Now even the right password has to wait.
    let res = login("correct horse").await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS, "{:?}", res.body);
    assert_eq!(res.body["code"], "too_many_attempts");
    // Other emails are unaffected.
    let other = app
        .call("POST", "/api/v1/auth/login", None, Some(json!({"email": "sam@example.com", "password": "x"})))
        .await;
    assert_eq!(other.body["code"], "invalid_credentials");

    // Without a live invite, accepting one doesn't reveal that the email is taken.
    let res = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": "made-up", "name": "X", "email": "logan@example.com", "password": "a long password"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn big_bodies_are_refused_outside_imports() {
    let app = TestApp::new().await;
    let big = "x".repeat(toomanyprojects::app::MAX_BODY_BYTES + 1);
    let res =
        app.call("POST", "/api/v1/auth/login", None, Some(json!({"email": "a@b.co", "password": big}))).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
}
