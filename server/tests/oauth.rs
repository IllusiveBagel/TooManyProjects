//! Connecting an app by signing in: discovery, registration, Allow, tokens,
//! refresh, and disconnecting.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use common::TestApp;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const REDIRECT: &str = "https://claude.ai/api/mcp/auth_callback";

fn challenge() -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()))
}

struct Raw {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
}

async fn send(app: &TestApp, req: Request<Body>) -> Raw {
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Raw { status, headers, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
}

async fn get(app: &TestApp, path: &str, extra: &[(&str, &str)]) -> Raw {
    let mut req = Request::builder().uri(path).header(header::HOST, "projects.example.com");
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

async fn form(app: &TestApp, path: &str, pairs: &[(&str, &str)]) -> Raw {
    let body = pairs.iter().map(|(k, v)| format!("{k}={}", urlencode(v))).collect::<Vec<_>>().join("&");
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, "projects.example.com")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    send(app, req).await
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn mcp(app: &TestApp, token: &str, method: &str, params: Value) -> Raw {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "projects.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

async fn register(app: &TestApp) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/register")
        .header(header::HOST, "projects.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"client_name": "Claude", "redirect_uris": [REDIRECT]}).to_string()))
        .unwrap();
    let res = send(app, req).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    assert_eq!(res.body["token_endpoint_auth_method"], "none");
    res.body["client_id"].as_str().unwrap().to_string()
}

fn approve_body(client: &str, scope: &str, allow: bool) -> Value {
    json!({
        "client_id": client, "redirect_uri": REDIRECT, "response_type": "code",
        "code_challenge": challenge(), "code_challenge_method": "S256",
        "state": "xyz", "scope": scope, "allow": allow,
    })
}

/// Allow, then swap the code for tokens. Returns (access, refresh).
async fn connect(app: &TestApp, cookie: &str, client: &str, scope: &str) -> (String, String) {
    let res = app
        .call("POST", "/api/v1/oauth/approve", Some(cookie), Some(approve_body(client, scope, true)))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let to = reqwest::Url::parse(res.body["redirect_to"].as_str().unwrap()).unwrap();
    assert!(to.as_str().starts_with(REDIRECT));
    let q: std::collections::HashMap<_, _> = to.query_pairs().into_owned().collect();
    assert_eq!(q["state"], "xyz");
    assert_eq!(q["iss"], "http://localhost:8080");
    let tok = form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &q["code"]),
            ("redirect_uri", REDIRECT),
            ("client_id", client),
            ("code_verifier", VERIFIER),
        ],
    )
    .await;
    assert_eq!(tok.status, StatusCode::OK, "{}", tok.body);
    assert_eq!(tok.body["token_type"], "Bearer");
    assert_eq!(tok.body["scope"], scope);
    // The same code never works twice.
    let again = form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &q["code"]),
            ("redirect_uri", REDIRECT),
            ("client_id", client),
            ("code_verifier", VERIFIER),
        ],
    )
    .await;
    assert_eq!(again.body["error"], "invalid_grant");
    (
        tok.body["access_token"].as_str().unwrap().to_string(),
        tok.body["refresh_token"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn discovery_points_clients_at_sign_in() {
    let app = TestApp::new().await;
    // With no token, /mcp says where to sign in.
    let res = get(&app, "/mcp", &[]).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let www = res.headers[header::WWW_AUTHENTICATE].to_str().unwrap();
    assert!(
        www.contains(
            "resource_metadata=\"https://projects.example.com/.well-known/oauth-protected-resource/mcp\""
        ) || www.contains("resource_metadata=\"http://projects.example.com/"),
        "{www}"
    );

    let prm = get(&app, "/.well-known/oauth-protected-resource/mcp", &[("x-forwarded-proto", "https")]).await;
    assert_eq!(prm.status, StatusCode::OK);
    assert_eq!(prm.body["resource"], "https://projects.example.com/mcp");
    assert_eq!(prm.body["authorization_servers"][0], "https://projects.example.com");

    let asm = get(&app, "/.well-known/oauth-authorization-server", &[("x-forwarded-proto", "https")]).await;
    assert_eq!(asm.body["issuer"], "https://projects.example.com");
    assert_eq!(asm.body["registration_endpoint"], "https://projects.example.com/oauth/register");
    assert_eq!(asm.body["code_challenge_methods_supported"][0], "S256");

    // Other .well-known paths are still a plain 404, not the web app.
    assert_eq!(get(&app, "/.well-known/something-else", &[]).await.status, StatusCode::NOT_FOUND);

    // The authorize address hands over to the web app's Allow page.
    let res = get(&app, "/oauth/authorize?client_id=a&state=b", &[]).await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.headers[header::LOCATION], "/connect?client_id=a&state=b");
}

#[tokio::test]
async fn registration_refuses_unsafe_return_addresses() {
    let app = TestApp::new().await;
    for uri in ["http://claude.ai/cb", "javascript:alert(1)", "https://claude.ai/cb#frag"] {
        let req = Request::builder()
            .method("POST")
            .uri("/oauth/register")
            .header(header::HOST, "projects.example.com")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"client_name": "x", "redirect_uris": [uri]}).to_string()))
            .unwrap();
        let res = send(&app, req).await;
        assert_eq!(res.body["error"], "invalid_redirect_uri", "{uri}");
    }
}

#[tokio::test]
async fn sign_in_allow_and_use_the_mcp_server() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let client = register(&app).await;

    // The Allow page sees who is asking and where they'll go back to.
    let q = format!(
        "/api/v1/oauth/request?client_id={client}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256",
        urlencode(REDIRECT),
        challenge()
    );
    let res = app.call("GET", &q, Some(&me), None).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["client_name"], "Claude");
    assert_eq!(res.body["redirect_host"], "claude.ai");
    assert!(res.body["warning"].is_null());
    // Signed out, it needs a sign-in first.
    assert_eq!(app.call("GET", &q, None, None).await.status, StatusCode::UNAUTHORIZED);

    // A return address the app didn't register is refused, and nothing is sent there.
    let mut bad = approve_body(&client, "write", true);
    bad["redirect_uri"] = json!("https://evil.example/cb");
    let res = app.call("POST", "/api/v1/oauth/approve", Some(&me), Some(bad)).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Cancel sends the app access_denied.
    let res = app
        .call("POST", "/api/v1/oauth/approve", Some(&me), Some(approve_body(&client, "write", false)))
        .await;
    assert!(res.body["redirect_to"].as_str().unwrap().contains("error=access_denied"));

    let (access, refresh) = connect(&app, &me, &client, "write").await;
    assert!(access.starts_with("tmp_at_") && refresh.starts_with("tmp_rt_"));

    // The access token works on /mcp, as Logan, with write access.
    let init = mcp(
        &app,
        &access,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
    )
    .await;
    assert_eq!(init.status, StatusCode::OK, "{}", init.body);
    let made = mcp(
        &app,
        &access,
        "tools/call",
        json!({"name": "spawn_project", "arguments": {"parent": "nope", "title": "x"}}),
    )
    .await;
    assert_eq!(made.status, StatusCode::OK);
    assert!(!made.body["result"]["content"][0]["text"].as_str().unwrap_or("").contains("read-only"));

    // It shows under Connected apps.
    let list = app.call("GET", "/api/v1/oauth/connections", Some(&me), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);
    assert_eq!(list.body[0]["client_name"], "Claude");
    assert_eq!(list.body[0]["scope"], "write");
    assert!(list.body[0]["last_used_at"].is_string());

    // Refresh gives a new pair; the old refresh token is spent.
    let r = form(
        &app,
        "/oauth/token",
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &client)],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let access2 = r.body["access_token"].as_str().unwrap().to_string();
    let refresh2 = r.body["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(mcp(&app, &access2, "tools/list", json!({})).await.status, StatusCode::OK);

    // A wrong PKCE verifier is refused.
    let res =
        app.call("POST", "/api/v1/oauth/approve", Some(&me), Some(approve_body(&client, "read", true))).await;
    let to = reqwest::Url::parse(res.body["redirect_to"].as_str().unwrap()).unwrap();
    let code = to.query_pairs().find(|(k, _)| k == "code").unwrap().1.into_owned();
    let wrong = form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client),
            ("code_verifier", "x".repeat(50).as_str()),
        ],
    )
    .await;
    assert_eq!(wrong.body["error"], "invalid_grant");

    // Disconnect stops it at once, with invalid_token so the app knows.
    let id = list.body[0]["id"].as_str().unwrap();
    let res = app.call("DELETE", &format!("/api/v1/oauth/connections/{id}"), Some(&me), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let after = mcp(&app, &access2, "tools/list", json!({})).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert!(after.headers[header::WWW_AUTHENTICATE].to_str().unwrap().contains("invalid_token"));
    let r = form(
        &app,
        "/oauth/token",
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh2), ("client_id", &client)],
    )
    .await;
    assert_eq!(r.body["error"], "invalid_grant");
    let list = app.call("GET", "/api/v1/oauth/connections", Some(&me), None).await;
    assert!(list.body.as_array().unwrap().is_empty());
}

async fn refresh_grant(app: &TestApp, client: &str, token: String) -> Raw {
    form(
        app,
        "/oauth/token",
        &[("grant_type", "refresh_token"), ("refresh_token", &token), ("client_id", client)],
    )
    .await
}

#[tokio::test]
async fn reusing_a_refresh_token_ends_the_connection() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let client = register(&app).await;
    let (_, refresh) = connect(&app, &me, &client, "write").await;
    let refresh_with = |token: String| refresh_grant(&app, &client, token);

    let r = refresh_with(refresh.clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let access2 = r.body["access_token"].as_str().unwrap().to_string();
    let refresh2 = r.body["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(mcp(&app, &access2, "tools/list", json!({})).await.status, StatusCode::OK);

    // The first refresh token comes back (say, stolen): refused, and the
    // whole connection ends, including the tokens the real app holds.
    let reused = refresh_with(refresh.clone()).await;
    assert_eq!(reused.body["error"], "invalid_grant");
    assert_eq!(mcp(&app, &access2, "tools/list", json!({})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refresh_with(refresh2).await.body["error"], "invalid_grant");
    let list = app.call("GET", "/api/v1/oauth/connections", Some(&me), None).await;
    assert!(list.body.as_array().unwrap().is_empty());

    // Connecting again works, and the old spent token can't end the new one.
    let (access3, _) = connect(&app, &me, &client, "write").await;
    assert_eq!(refresh_with(refresh).await.body["error"], "invalid_grant");
    assert_eq!(mcp(&app, &access3, "tools/list", json!({})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn read_only_connections_cannot_change_things() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let client = register(&app).await;
    let (access, _) = connect(&app, &me, &client, "read").await;
    let res = mcp(
        &app,
        &access,
        "tools/call",
        json!({"name": "spawn_project", "arguments": {"parent": "anything", "title": "x"}}),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body["result"]["content"][0]["text"].as_str().unwrap().contains("read-only"), "{}", res.body);
}

#[tokio::test]
async fn a_lookalike_app_gets_a_warning() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/register")
        .header(header::HOST, "projects.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"client_name": "Claude", "redirect_uris": ["https://evil.example/cb"]}).to_string(),
        ))
        .unwrap();
    let client = send(&app, req).await.body["client_id"].as_str().unwrap().to_string();
    let q = format!(
        "/api/v1/oauth/request?client_id={client}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256",
        urlencode("https://evil.example/cb"),
        challenge()
    );
    let res = app.call("GET", &q, Some(&me), None).await;
    assert_eq!(res.body["redirect_host"], "evil.example");
    assert!(res.body["warning"].as_str().unwrap().contains("evil.example"));
}
