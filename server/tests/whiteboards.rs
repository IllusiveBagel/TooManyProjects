//! Whiteboards (T-30): boards in a project, saving by element with versions,
//! changes since a save, cursors and sharing.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn ok(app: &TestApp, method: &str, path: &str, cookie: &str, body: Option<Value>) -> Value {
    let res = app.call(method, &format!("/api/v1{path}"), Some(cookie), body).await;
    assert!(res.status.is_success(), "{method} {path}: {} {:?}", res.status, res.body);
    res.body
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

async fn join(app: &TestApp, owner: &str, name: &str) -> String {
    let created = app.call("POST", "/api/v1/invites", Some(owner), Some(json!({}))).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let email = format!("{}@example.com", name.to_lowercase());
    let res = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": created.body["token"], "name": name, "email": email, "password": "a long password"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    res.cookie.unwrap()
}

async fn user_id(app: &TestApp, cookie: &str) -> String {
    let res = app.call("GET", "/api/v1/auth/status", Some(cookie), None).await;
    res.body["user"]["id"].as_str().unwrap().to_string()
}

fn ids(v: &Value) -> Vec<&str> {
    v["elements"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn boards_save_by_element_and_report_changes() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Beetle"}))).await);

    let wiring =
        ok(&app, "POST", &format!("/projects/{car}/whiteboards"), &me, Some(json!({"title": "Wiring"})))
            .await;
    assert_eq!(wiring["can_edit"], true);
    assert_eq!(wiring["element_count"], 0);
    let board = id(&wiring);
    let outline = id(&ok(
        &app,
        "POST",
        &format!("/projects/{car}/whiteboards"),
        &me,
        Some(json!({"title": "Outline"})),
    )
    .await);

    let bad = app
        .call("POST", &format!("/api/v1/projects/{car}/whiteboards"), Some(&me), Some(json!({"title": "  "})))
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Move Outline first, rename Wiring.
    ok(&app, "PATCH", &format!("/whiteboards/{outline}"), &me, Some(json!({"after": null}))).await;
    ok(&app, "PATCH", &format!("/whiteboards/{board}"), &me, Some(json!({"title": "Wiring plan"}))).await;
    let list = ok(&app, "GET", &format!("/projects/{car}/whiteboards"), &me, None).await;
    let titles: Vec<&str> = list.as_array().unwrap().iter().map(|b| b["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Outline", "Wiring plan"]);

    // First save: two shapes and an arrow between them.
    let path = format!("/whiteboards/{board}/elements");
    let saved = ok(
        &app,
        "PATCH",
        &path,
        &me,
        Some(json!({"elements": [
            {"id": "battery", "kind": "box", "version": 1, "data": {"x": 0, "y": 0, "w": 160, "h": 80, "text": "Battery"}},
            {"id": "relay", "kind": "diamond", "version": 1, "data": {"x": 300, "y": 0, "w": 120, "h": 120}},
            {"id": "a1", "kind": "arrow", "version": 1, "data": {"from": "battery", "to": "relay", "label": "12V"}},
        ]})),
    )
    .await;
    assert_eq!(saved["seq"], 1);
    assert_eq!(saved["rejected"], json!([]));

    let all = ok(&app, "GET", &path, &me, None).await;
    assert_eq!(all["seq"], 1);
    assert_eq!(ids(&all), ["battery", "relay", "a1"]);
    assert_eq!(all["elements"][0]["data"]["text"], "Battery");

    // Nothing new since 1.
    let none = ok(&app, "GET", &format!("{path}?since=1"), &me, None).await;
    assert_eq!(none["elements"], json!([]));

    // Change one, delete another: only those come back since 1.
    ok(
        &app,
        "PATCH",
        &path,
        &me,
        Some(json!({"elements": [
            {"id": "battery", "kind": "box", "version": 2, "data": {"x": 20, "y": 0, "w": 160, "h": 80, "text": "Battery"}},
            {"id": "a1", "kind": "arrow", "version": 2, "deleted": true},
        ]})),
    )
    .await;
    let changed = ok(&app, "GET", &format!("{path}?since=1"), &me, None).await;
    assert_eq!(changed["seq"], 2);
    assert_eq!(ids(&changed), ["battery", "a1"]);
    assert_eq!(changed["elements"][1]["deleted"], true);
    let now = ok(&app, "GET", &path, &me, None).await;
    assert_eq!(ids(&now), ["battery", "relay"]);

    // An older version loses and comes back as the server has it.
    let stale = ok(
        &app,
        "PATCH",
        &path,
        &me,
        Some(json!({"elements": [{"id": "battery", "kind": "box", "version": 1, "data": {"x": 999}}]})),
    )
    .await;
    assert_eq!(stale["rejected"][0]["id"], "battery");
    assert_eq!(stale["rejected"][0]["data"]["x"], 20);

    // Junk is refused whole.
    for junk in [
        json!({"id": "bad id!", "kind": "box", "version": 1, "data": {}}),
        json!({"id": "x", "kind": "rocket", "version": 1, "data": {}}),
        json!({"id": "x", "kind": "box", "version": 0, "data": {}}),
        json!({"id": "x", "kind": "box", "version": 1, "data": [1, 2]}),
    ] {
        let res =
            app.call("PATCH", &format!("/api/v1{path}"), Some(&me), Some(json!({"elements": [junk]}))).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.body);
    }

    let one = ok(&app, "GET", &format!("/whiteboards/{board}"), &me, None).await;
    assert_eq!(one["element_count"], 2);

    // Deleting the board takes its elements.
    let del = app.call("DELETE", &format!("/api/v1/whiteboards/{board}"), Some(&me), None).await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let gone = app.call("GET", &format!("/api/v1{path}"), Some(&me), None).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn viewers_look_editors_draw_and_cursors_are_shared() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let ali = join(&app, &logan, "Ali").await;
    let car = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Beetle"}))).await);
    let board = id(&ok(
        &app,
        "POST",
        &format!("/projects/{car}/whiteboards"),
        &logan,
        Some(json!({"title": "Wiring"})),
    )
    .await);
    let path = format!("/api/v1/whiteboards/{board}/elements");

    // Not shared yet: nothing to see.
    let hidden = app.call("GET", &path, Some(&sam), None).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);

    let sam_id = user_id(&app, &sam).await;
    let ali_id = user_id(&app, &ali).await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": sam_id, "project_id": car, "role": "viewer"})),
    )
    .await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": ali_id, "project_id": car, "role": "editor"})),
    )
    .await;

    // The viewer reads but can't draw or rename.
    let seen = ok(&app, "GET", &format!("/projects/{car}/whiteboards"), &sam, None).await;
    assert_eq!(seen[0]["can_edit"], false);
    let draw = json!({"elements": [{"id": "s1", "kind": "sticky", "version": 1, "data": {"text": "hi"}}]});
    let res = app.call("PATCH", &path, Some(&sam), Some(draw.clone())).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app
        .call("PATCH", &format!("/api/v1/whiteboards/{board}"), Some(&sam), Some(json!({"title": "Mine"})))
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // The editor draws; Logan sees who changed it.
    let res = app.call("PATCH", &path, Some(&ali), Some(draw)).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let all = ok(&app, "GET", &format!("/whiteboards/{board}/elements"), &logan, None).await;
    assert_eq!(all["elements"][0]["updated_by_name"], "Ali");

    // Cursors: everyone sees the others, not themselves.
    let put = app
        .call(
            "PUT",
            &format!("/api/v1/whiteboards/{board}/cursor"),
            Some(&sam),
            Some(json!({"x": 10.5, "y": -4})),
        )
        .await;
    assert_eq!(put.status, StatusCode::NO_CONTENT);
    let for_logan = ok(&app, "GET", &format!("/whiteboards/{board}/elements?since=99"), &logan, None).await;
    assert_eq!(for_logan["cursors"][0]["name"], "Sam");
    assert_eq!(for_logan["cursors"][0]["x"], 10.5);
    let for_sam = ok(&app, "GET", &format!("/whiteboards/{board}/elements?since=99"), &sam, None).await;
    assert_eq!(for_sam["cursors"], json!([]));

    // Only editors delete.
    let res = app.call("DELETE", &format!("/api/v1/whiteboards/{board}"), Some(&sam), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}
