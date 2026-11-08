//! Groups on the canvas: labelled areas that carry the cards inside them.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn create(app: &TestApp, cookie: &str, title: &str, x: f64, y: f64) -> Value {
    let res = app
        .call(
            "POST",
            "/api/v1/projects",
            Some(cookie),
            Some(json!({"title": title, "position": {"x": x, "y": y}})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body
}

async fn canvas_id(app: &TestApp, cookie: &str) -> String {
    let view = app.call("GET", "/api/v1/canvases/home", Some(cookie), None).await;
    view.body["canvas"]["id"].as_str().unwrap().to_string()
}

fn groups(view: &Value) -> Vec<Value> {
    view["nodes"].as_array().unwrap().iter().filter(|n| n["kind"] == "group").cloned().collect()
}

#[tokio::test]
async fn draw_rename_move_and_remove_a_group() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = create(&app, &me, "Car restoration", 40.0, 60.0).await;
    let canvas = canvas_id(&app, &me).await;

    let res = app
        .call(
            "POST",
            &format!("/api/v1/canvases/{canvas}/groups"),
            Some(&me),
            Some(json!({"label": "  Cars ", "color": "slot-2", "x": 0, "y": 0, "width": 700, "height": 400})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let gid = res.body["id"].as_str().unwrap().to_string();
    assert_eq!(res.body["kind"], "group");
    assert_eq!(res.body["label"], "Cars");
    assert_eq!(res.body["width"], 700.0);

    // Bad input.
    for body in [
        json!({"label": "", "x": 0, "y": 0, "width": 700, "height": 400}),
        json!({"label": "Tiny", "x": 0, "y": 0, "width": 50, "height": 400}),
        json!({"label": "Pink", "color": "hotpink", "x": 0, "y": 0, "width": 700, "height": 400}),
    ] {
        let res = app.call("POST", &format!("/api/v1/canvases/{canvas}/groups"), Some(&me), Some(body)).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.body);
    }

    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{canvas}/groups/{gid}"),
            Some(&me),
            Some(json!({"label": "Car projects", "color": "#a3a36a"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["label"], "Car projects");
    assert_eq!(res.body["x"], 0.0);

    // A drag of the group sends the group and the cards inside together.
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{canvas}/nodes"),
            Some(&me),
            Some(json!({
                "nodes": [{"project_id": car["id"], "x": 540, "y": 60}],
                "groups": [{"id": gid, "x": 500, "y": 0, "width": 800}]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    let view = app.call("GET", "/api/v1/canvases/home", Some(&me), None).await.body;
    let g = &groups(&view)[0];
    assert_eq!(
        (g["x"].as_f64(), g["width"].as_f64(), g["height"].as_f64()),
        (Some(500.0), Some(800.0), Some(400.0))
    );
    let card = view["nodes"].as_array().unwrap().iter().find(|n| n["project_id"] == car["id"]).unwrap();
    assert_eq!(card["x"], 540.0);

    let res = app.call("DELETE", &format!("/api/v1/canvases/{canvas}/groups/{gid}"), Some(&me), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let view = app.call("GET", "/api/v1/canvases/home", Some(&me), None).await.body;
    assert!(groups(&view).is_empty());
    // The card stays.
    assert_eq!(view["projects"].as_array().unwrap().len(), 1);
    let res = app.call("DELETE", &format!("/api/v1/canvases/{canvas}/groups/{gid}"), Some(&me), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn groups_are_your_own() {
    let app = TestApp::new().await;
    let owner = app.setup_owner().await;
    let invite = app.call("POST", "/api/v1/invites", Some(&owner), Some(json!({}))).await;
    let sam = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": invite.body["token"], "name": "Sam", "email": "sam@example.com", "password": "sam's password"})),
        )
        .await
        .cookie
        .unwrap();
    let canvas = canvas_id(&app, &owner).await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/canvases/{canvas}/groups"),
            Some(&owner),
            Some(json!({"label": "Electronics", "x": 0, "y": 0, "width": 500, "height": 300})),
        )
        .await;
    let gid = res.body["id"].as_str().unwrap().to_string();

    // Sam can't draw on, change or see Logan's canvas groups.
    let res = app
        .call(
            "POST",
            &format!("/api/v1/canvases/{canvas}/groups"),
            Some(&sam),
            Some(json!({"label": "Mine", "x": 0, "y": 0, "width": 500, "height": 300})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    for (method, body) in [("PATCH", Some(json!({"label": "Mine"}))), ("DELETE", None)] {
        let res =
            app.call(method, &format!("/api/v1/canvases/{canvas}/groups/{gid}"), Some(&sam), body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND);
    }
    let sams_canvas = canvas_id(&app, &sam).await;
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{sams_canvas}/nodes"),
            Some(&sam),
            Some(json!({"groups": [{"id": gid, "x": 1, "y": 1}]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let view = app.call("GET", "/api/v1/canvases/home", Some(&sam), None).await.body;
    assert!(groups(&view).is_empty());
}
