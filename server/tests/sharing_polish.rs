//! Tickets from the first friend: removing someone from one project (T-25),
//! copying the owner's canvas layout (T-23) and new tasks assigned to their
//! creator (T-26).

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn user_id(app: &TestApp, cookie: &str) -> String {
    let res = app.call("GET", "/api/v1/auth/status", Some(cookie), None).await;
    res.body["user"]["id"].as_str().unwrap().to_string()
}

async fn project(app: &TestApp, cookie: &str, body: Value) -> String {
    let res = app.call("POST", "/api/v1/projects", Some(cookie), Some(body)).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body["id"].as_str().unwrap().to_string()
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

async fn share(app: &TestApp, cookie: &str, body: Value) -> Value {
    let res = app.call("POST", "/api/v1/shares", Some(cookie), Some(body)).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body
}

fn titles(list: &Value) -> Vec<&str> {
    let mut t: Vec<&str> = list.as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap()).collect();
    t.sort();
    t
}

async fn canvas(app: &TestApp, cookie: &str) -> Value {
    let res = app.call("GET", "/api/v1/canvases/home", Some(cookie), None).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    res.body
}

fn card(view: &Value, project_id: &str) -> (f64, f64) {
    let n = view["nodes"].as_array().unwrap().iter().find(|n| n["project_id"] == project_id).unwrap();
    (n["x"].as_f64().unwrap(), n["y"].as_f64().unwrap())
}

fn groups(view: &Value) -> Vec<(String, f64, f64)> {
    let mut g: Vec<(String, f64, f64)> = view["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == "group")
        .map(|n| {
            (n["label"].as_str().unwrap().to_string(), n["x"].as_f64().unwrap(), n["y"].as_f64().unwrap())
        })
        .collect();
    g.sort_by(|a, b| a.0.cmp(&b.0));
    g
}

async fn group(app: &TestApp, cookie: &str, label: &str, x: f64, y: f64, w: f64, h: f64) {
    let id = canvas(app, cookie).await["canvas"]["id"].as_str().unwrap().to_string();
    let res = app
        .call(
            "POST",
            &format!("/api/v1/canvases/{id}/groups"),
            Some(cookie),
            Some(json!({"label": label, "x": x, "y": y, "width": w, "height": h})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
}

#[tokio::test]
async fn remove_someone_from_one_project_of_a_whole_account_share() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let vids = project(&app, &logan, json!({"title": "YouTube videos", "spawned_from": car})).await;
    let garden = project(&app, &logan, json!({"title": "Garden"})).await;
    let note =
        |title: &str, projects: Vec<&str>| json!({"title": title, "body": "x", "project_ids": projects});
    app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Bleeding brakes", vec![&car]))).await;
    app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Tomatoes", vec![&garden]))).await;
    app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Knowledge only", vec![]))).await;

    let s = share(&app, &logan, json!({"grantee_id": sam_id, "role": "editor", "include_notes": true})).await;
    let task = app
        .call(
            "POST",
            &format!("/api/v1/projects/{car}/tasks"),
            Some(&logan),
            Some(json!({"title": "Bleed brakes", "assignee_id": sam_id})),
        )
        .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.body);
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Beetle restoration", "Garden", "YouTube videos"]);

    // Only the owner can remove people, and not themselves.
    let path = format!("/api/v1/projects/{car}/exclusions/{sam_id}");
    assert_eq!(app.call("PUT", &path, Some(&sam), None).await.status, StatusCode::FORBIDDEN);
    let logan_id = user_id(&app, &logan).await;
    let res =
        app.call("PUT", &format!("/api/v1/projects/{car}/exclusions/{logan_id}"), Some(&logan), None).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    assert_eq!(app.call("PUT", &path, Some(&logan), None).await.status, StatusCode::NO_CONTENT);
    // Twice is fine.
    assert_eq!(app.call("PUT", &path, Some(&logan), None).await.status, StatusCode::NO_CONTENT);

    // The car and what was spawned from it are gone for Sam; the garden stays.
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Garden"]);
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{car}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    let notes = app.call("GET", "/api/v1/notes?shared=true", Some(&sam), None).await.body;
    assert_eq!(titles(&notes), vec!["Knowledge only", "Tomatoes"]);
    let mine = app.call("GET", "/api/v1/tasks?mine=true", Some(&sam), None).await.body;
    assert!(!mine.to_string().contains("Bleed brakes"), "{mine}");
    let view = canvas(&app, &sam).await;
    assert!(!view["projects"].to_string().contains("Beetle"));

    // Logan sees who was removed, in the Share dialog and in Settings.
    let people = app.call("GET", &format!("/api/v1/projects/{car}/people"), Some(&logan), None).await.body;
    let sam_row = people.as_array().unwrap().iter().find(|m| m["name"] == "Sam").unwrap();
    assert_eq!(sam_row["via"], "excluded");
    assert_eq!(sam_row["role"], "none");
    let shares = app.call("GET", "/api/v1/shares", Some(&logan), None).await.body;
    assert_eq!(shares["removed"][0]["project_title"], "Beetle restoration");
    assert_eq!(shares["removed"][0]["grantee_name"], "Sam");

    // Let back in: everything returns, the assignment too.
    assert_eq!(app.call("DELETE", &path, Some(&logan), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.call("DELETE", &path, Some(&logan), None).await.status, StatusCode::NOT_FOUND);
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Beetle restoration", "Garden", "YouTube videos"]);
    let mine = app.call("GET", "/api/v1/tasks?mine=true", Some(&sam), None).await.body;
    assert!(mine.to_string().contains("Bleed brakes"), "{mine}");

    // Someone given the project on its own loses it by removing that share.
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": garden})).await;
    let res =
        app.call("PUT", &format!("/api/v1/projects/{garden}/exclusions/{sam_id}"), Some(&logan), None).await;
    assert_eq!(res.status, StatusCode::CONFLICT);

    // A family share is trimmed the same way.
    let _ = vids;
    // Ending every share from Logan forgets the removals.
    app.call("PUT", &path, Some(&logan), None).await;
    let shares = app.call("GET", "/api/v1/shares", Some(&logan), None).await.body;
    for given in shares["given"].as_array().unwrap() {
        let id = given["id"].as_str().unwrap();
        assert_eq!(
            app.call("DELETE", &format!("/api/v1/shares/{id}"), Some(&logan), None).await.status,
            StatusCode::NO_CONTENT
        );
    }
    let shares = app.call("GET", "/api/v1/shares", Some(&logan), None).await.body;
    assert_eq!(shares["removed"], json!([]));
    let _ = s;
}

#[tokio::test]
async fn a_family_share_skips_the_removed_branch() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let vids = project(&app, &logan, json!({"title": "YouTube videos", "spawned_from": car})).await;
    let anim = project(&app, &logan, json!({"title": "Blender animations", "spawned_from": vids})).await;
    project(&app, &logan, json!({"title": "Rear brakes", "spawned_from": car})).await;
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "include_children": true})).await;
    let people = app.call("GET", &format!("/api/v1/projects/{vids}/people"), Some(&logan), None).await.body;
    assert_eq!(people[1]["via"], "family");

    let res =
        app.call("PUT", &format!("/api/v1/projects/{vids}/exclusions/{sam_id}"), Some(&logan), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Beetle restoration", "Rear brakes"]);
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{anim}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn shared_projects_land_where_their_owner_keeps_them() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    let logan_id = user_id(&app, &logan).await;

    // Logan's canvas: two cars in a group, a server in another, a private
    // project in a third group that Sam must never see.
    let car = project(&app, &logan, json!({"title": "Beetle", "position": {"x": 80, "y": 120}})).await;
    let brakes =
        project(&app, &logan, json!({"title": "Rear brakes", "position": {"x": 80, "y": 320}})).await;
    let server = project(&app, &logan, json!({"title": "BC250", "position": {"x": 880, "y": 120}})).await;
    let secret = project(&app, &logan, json!({"title": "Surprise", "position": {"x": 880, "y": 600}})).await;
    group(&app, &logan, "Cars", 40.0, 60.0, 720.0, 460.0).await;
    group(&app, &logan, "Electronics", 840.0, 60.0, 720.0, 300.0).await;
    group(&app, &logan, "Secret stuff", 840.0, 560.0, 400.0, 260.0).await;

    // Sam already has a card of their own.
    let bike = project(&app, &sam, json!({"title": "Bike", "position": {"x": 0, "y": 100}})).await;
    canvas(&app, &sam).await;

    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car})).await;
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": brakes})).await;
    let view = canvas(&app, &sam).await;
    // Logan's block goes right of Sam's bike (0..256), tops lined up: dx = 256 + 160 - 40, dy = 100 - 60.
    let (dx, dy) = (376.0, 40.0);
    assert_eq!(card(&view, &car), (80.0 + dx, 120.0 + dy));
    assert_eq!(card(&view, &brakes), (80.0 + dx, 320.0 + dy));
    assert_eq!(card(&view, &bike), (0.0, 100.0));
    assert_eq!(groups(&view), vec![("Cars".into(), 40.0 + dx, 60.0 + dy)]);

    // Shared later: lands beside its old neighbours, its group copied once.
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": server})).await;
    let view = canvas(&app, &sam).await;
    assert_eq!(card(&view, &server), (880.0 + dx, 120.0 + dy));
    assert_eq!(
        groups(&view),
        vec![("Cars".into(), 40.0 + dx, 60.0 + dy), ("Electronics".into(), 840.0 + dx, 60.0 + dy)]
    );
    assert!(!view.to_string().contains("Secret stuff"));
    let _ = secret;

    // Sam rearranges; copying Logan's layout again replaces the copies, no duplicates.
    let sam_canvas = view["canvas"]["id"].as_str().unwrap().to_string();
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/canvases/{sam_canvas}/nodes"),
            Some(&sam),
            Some(json!({"nodes": [{"project_id": car, "x": 3000, "y": 3000}]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    let res = app
        .call("POST", "/api/v1/canvases/home/copy-layout", Some(&sam), Some(json!({"owner_id": logan_id})))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["owner_name"], "Logan");
    assert_eq!(res.body["moved"], 3);
    assert_eq!(res.body["groups"], 2);
    let undo = res.body["undo"].clone();
    let view = canvas(&app, &sam).await;
    assert_eq!(card(&view, &car), (80.0 + dx, 120.0 + dy));
    assert_eq!(groups(&view).len(), 2);

    // Undo puts the card back and the old copies too.
    let res = app.call("POST", "/api/v1/canvases/home/restore-layout", Some(&sam), Some(undo)).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{:?}", res.body);
    let view = canvas(&app, &sam).await;
    assert_eq!(card(&view, &car), (3000.0, 3000.0));
    assert_eq!(
        groups(&view),
        vec![("Cars".into(), 40.0 + dx, 60.0 + dy), ("Electronics".into(), 840.0 + dx, 60.0 + dy)]
    );

    // Nothing changed on Logan's canvas.
    let logans = canvas(&app, &logan).await;
    assert_eq!(card(&logans, &car), (80.0, 120.0));
    assert_eq!(groups(&logans).len(), 3);

    // Only someone who shares with you, and never yourself.
    let res = app
        .call("POST", "/api/v1/canvases/home/copy-layout", Some(&logan), Some(json!({"owner_id": sam_id})))
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app
        .call("POST", "/api/v1/canvases/home/copy-layout", Some(&sam), Some(json!({"owner_id": sam_id})))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn new_tasks_go_to_whoever_makes_them() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle"})).await;
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "role": "editor"})).await;
    let new = |cookie: &str, body: Value| {
        let cookie = cookie.to_string();
        let app = &app;
        let car = car.clone();
        async move {
            let res =
                app.call("POST", &format!("/api/v1/projects/{car}/tasks"), Some(&cookie), Some(body)).await;
            assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
            res.body
        }
    };
    assert_eq!(new(&logan, json!({"title": "Mine"})).await["assignee_name"], "Logan");
    assert_eq!(new(&sam, json!({"title": "Sam's"})).await["assignee_name"], "Sam");
    assert_eq!(
        new(&logan, json!({"title": "Nobody's", "assignee_id": null})).await["assignee_id"],
        Value::Null
    );
    assert_eq!(new(&logan, json!({"title": "For Sam", "assignee_id": sam_id})).await["assignee_name"], "Sam");

    // Quick add does the same.
    let res = app
        .call(
            "POST",
            "/api/v1/quick-add",
            Some(&sam),
            Some(json!({"text": "Order pads", "project_id": car, "kind": "task"})),
        )
        .await;
    assert!(res.status.is_success(), "{:?}", res.body);
    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&logan), None).await.body;
    let pads = tasks.as_array().unwrap().iter().find(|t| t["title"] == "Order pads").unwrap();
    assert_eq!(pads["assignee_name"], "Sam");
}
