//! Projects, links and the canvas.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn create(app: &TestApp, cookie: &str, body: Value) -> Value {
    let res = app.call("POST", "/api/v1/projects", Some(cookie), Some(body)).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body
}

async fn link(
    app: &TestApp,
    cookie: &str,
    source: &Value,
    target: &Value,
    kind: &str,
) -> (StatusCode, Value) {
    let res = app
        .call(
            "POST",
            "/api/v1/links",
            Some(cookie),
            Some(json!({"source_id": source["id"], "target_id": target["id"], "kind": kind})),
        )
        .await;
    (res.status, res.body)
}

#[tokio::test]
async fn create_edit_archive_and_delete() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;

    let car = create(&app, &me, json!({"title": "  Car restoration ", "summary": "1972 Beetle"})).await;
    assert_eq!(car["title"], "Car restoration");
    assert_eq!(car["status"], "active");
    assert_eq!(car["color"], "slot-1");
    assert_eq!(car["stats"]["total_tasks"], 0);

    let second = create(&app, &me, json!({"title": "Videos"})).await;
    assert_eq!(second["color"], "slot-2", "colours rotate through the slots");

    let id = car["id"].as_str().unwrap();
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/projects/{id}"),
            Some(&me),
            Some(json!({"summary": null, "target_date": "2027-03-01", "color": "#a1b2c3"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["summary"], Value::Null);
    assert_eq!(res.body["target_date"], "2027-03-01");
    assert_eq!(res.body["title"], "Car restoration", "fields left out stay");

    for (body, code) in [
        (json!({"color": "red"}), "invalid_color"),
        (json!({"status": "busy"}), "invalid_status"),
        (json!({"target_date": "next week"}), "invalid_date"),
        (json!({"title": " "}), "invalid_title"),
    ] {
        let res = app.call("PATCH", &format!("/api/v1/projects/{id}"), Some(&me), Some(body)).await;
        assert_eq!(res.body["code"], code);
    }

    let archived = app
        .call("PATCH", &format!("/api/v1/projects/{id}"), Some(&me), Some(json!({"status": "archived"})))
        .await;
    assert!(archived.body["archived_at"].is_string());
    let list = app.call("GET", "/api/v1/projects", Some(&me), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1, "archived projects are hidden by default");
    let all = app.call("GET", "/api/v1/projects?include_archived=true", Some(&me), None).await;
    assert_eq!(all.body.as_array().unwrap().len(), 2);
    let restored = app
        .call("PATCH", &format!("/api/v1/projects/{id}"), Some(&me), Some(json!({"status": "paused"})))
        .await;
    assert_eq!(restored.body["archived_at"], Value::Null);

    assert_eq!(
        app.call("DELETE", &format!("/api/v1/projects/{id}"), Some(&me), None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{id}"), Some(&me), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn spawned_children_and_loops() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;

    let car = create(&app, &me, json!({"title": "Car restoration"})).await;
    let videos = create(
        &app,
        &me,
        json!({"title": "Restoration videos", "spawned_from": car["id"], "position": {"x": 500, "y": 300}}),
    )
    .await;
    let blender =
        create(&app, &me, json!({"title": "Blender animations", "spawned_from": videos["id"]})).await;

    let detail = app
        .call("GET", &format!("/api/v1/projects/{}", videos["id"].as_str().unwrap()), Some(&me), None)
        .await;
    assert_eq!(detail.body["parents"][0]["title"], "Car restoration");
    assert_eq!(detail.body["children"][0]["title"], "Blender animations");
    assert_eq!(detail.body["project"]["stats"]["child_count"], 1);

    // Blender spawning the car would close the loop car -> videos -> blender -> car.
    let (status, body) = link(&app, &me, &blender, &car, "spawned").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "link_loop");
    // A second parent is fine: projects are a graph.
    let homelab = create(&app, &me, json!({"title": "Home lab"})).await;
    let (status, _) = link(&app, &me, &homelab, &blender, "spawned").await;
    assert_eq!(status, StatusCode::CREATED);
    // Related links have no direction, so the reverse is a duplicate.
    let (status, _) = link(&app, &me, &car, &homelab, "related").await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = link(&app, &me, &homelab, &car, "related").await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::CONFLICT, Some("duplicate_link")));
    let (status, body) = link(&app, &me, &car, &car, "related").await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::BAD_REQUEST, Some("self_link")));

    // Change a link's kind and label.
    let link_id = detail.body["children"][0]["link_id"].as_str().unwrap();
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/links/{link_id}"),
            Some(&me),
            Some(json!({"kind": "blocks", "label": "needs footage"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["kind"], "blocks");
    assert_eq!(res.body["label"], "needs footage");

    assert_eq!(
        app.call("DELETE", &format!("/api/v1/links/{link_id}"), Some(&me), None).await.status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn canvas_places_cards_and_saves_positions() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = create(&app, &me, json!({"title": "Car restoration"})).await;
    let videos = create(
        &app,
        &me,
        json!({"title": "Videos", "spawned_from": car["id"], "position": {"x": 640.5, "y": 120}}),
    )
    .await;
    let old = create(&app, &me, json!({"title": "Old thing", "status": "archived"})).await;

    let view = app.call("GET", "/api/v1/canvases/home", Some(&me), None).await;
    assert_eq!(view.status, StatusCode::OK);
    let canvas_id = view.body["canvas"]["id"].as_str().unwrap().to_string();
    assert_eq!(view.body["projects"].as_array().unwrap().len(), 2);
    assert_eq!(view.body["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(view.body["links"].as_array().unwrap().len(), 1);
    let node = |body: &Value, id: &Value| {
        body["nodes"].as_array().unwrap().iter().find(|n| n["project_id"] == *id).cloned().unwrap()
    };
    assert_eq!(node(&view.body, &car["id"])["x"], 0.0);
    assert_eq!(node(&view.body, &videos["id"])["x"], 640.5);

    let with_archived = app.call("GET", "/api/v1/canvases/home?include_archived=true", Some(&me), None).await;
    assert_eq!(with_archived.body["projects"].as_array().unwrap().len(), 3);
    assert!(node(&with_archived.body, &old["id"])["x"].is_number());

    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{canvas_id}/nodes"),
            Some(&me),
            Some(json!({"nodes": [{"project_id": car["id"], "x": -100, "y": 42.5}]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    let view = app.call("GET", "/api/v1/canvases/home", Some(&me), None).await;
    assert_eq!(node(&view.body, &car["id"])["y"], 42.5);
}

#[tokio::test]
async fn people_only_see_their_own_projects() {
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

    let car = create(&app, &owner, json!({"title": "Car restoration"})).await;
    let id = car["id"].as_str().unwrap();
    let sams = create(&app, &sam, json!({"title": "Sam's thing"})).await;

    assert_eq!(app.call("GET", "/api/v1/projects", Some(&sam), None).await.body.as_array().unwrap().len(), 1);
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{id}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call("PATCH", &format!("/api/v1/projects/{id}"), Some(&sam), Some(json!({"title": "Mine now"})))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/projects/{id}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    let (status, _) = link(&app, &sam, &sams, &car, "spawned").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let res = app
        .call(
            "POST",
            "/api/v1/projects",
            Some(&sam),
            Some(json!({"title": "Child", "spawned_from": car["id"]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    let owner_canvas = app.call("GET", "/api/v1/canvases/home", Some(&owner), None).await;
    let canvas_id = owner_canvas.body["canvas"]["id"].as_str().unwrap();
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{canvas_id}/nodes"),
            Some(&sam),
            Some(json!({"nodes": [{"project_id": car["id"], "x": 1, "y": 1}]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
