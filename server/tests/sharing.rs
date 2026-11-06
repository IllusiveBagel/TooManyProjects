//! Sharing a whole account or one project, as viewer or editor, and every
//! place that has to respect it.

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

async fn join(app: &TestApp, owner: &str, name: &str, invite: Value) -> String {
    let created = app.call("POST", "/api/v1/invites", Some(owner), Some(invite)).await;
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

#[tokio::test]
async fn a_viewer_can_look_but_not_change() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam", json!({})).await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let garden = project(&app, &logan, json!({"title": "Garden"})).await;
    let task = app
        .call(
            "POST",
            &format!("/api/v1/projects/{car}/tasks"),
            Some(&logan),
            Some(json!({"title": "Bleed brakes"})),
        )
        .await;
    let task_id = task.body["id"].as_str().unwrap().to_string();

    // Nothing shared yet.
    assert_eq!(app.call("GET", "/api/v1/projects", Some(&sam), None).await.body, json!([]));
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{car}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );

    let s = share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "role": "viewer"})).await;
    assert_eq!(s["kind"], "project");
    assert_eq!(s["project_title"], "Beetle restoration");

    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Beetle restoration"]);
    assert_eq!(list[0]["role"], "viewer");
    assert_eq!(list[0]["owner_name"], "Logan");
    let mine = app.call("GET", "/api/v1/projects", Some(&logan), None).await.body;
    assert!(mine.as_array().unwrap().iter().all(|p| p["role"] == "owner"));

    // Reads work.
    for path in [
        format!("/api/v1/projects/{car}"),
        format!("/api/v1/projects/{car}/tasks"),
        format!("/api/v1/projects/{car}/statuses"),
        format!("/api/v1/projects/{car}/dashboard"),
        format!("/api/v1/projects/{car}/notes"),
        format!("/api/v1/projects/{car}/people"),
        format!("/api/v1/tasks/{task_id}"),
    ] {
        let res = app.call("GET", &path, Some(&sam), None).await;
        assert_eq!(res.status, StatusCode::OK, "{path}: {:?}", res.body);
    }
    let board = app.call("GET", "/api/v1/tasks", Some(&sam), None).await.body;
    assert_eq!(board.as_array().unwrap().len(), 1);
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{garden}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );

    // Writes are refused with a reason.
    let tries = [
        ("POST", format!("/api/v1/projects/{car}/tasks"), Some(json!({"title": "Nope"}))),
        ("PATCH", format!("/api/v1/projects/{car}"), Some(json!({"title": "Mine now"}))),
        ("PATCH", format!("/api/v1/tasks/{task_id}"), Some(json!({"title": "Nope"}))),
        ("DELETE", format!("/api/v1/tasks/{task_id}"), None),
        (
            "POST",
            format!("/api/v1/projects/{car}/statuses"),
            Some(json!({"name": "Mine", "category": "todo"})),
        ),
        ("POST", format!("/api/v1/tasks/{task_id}/checklist"), Some(json!({"text": "x"}))),
        ("DELETE", format!("/api/v1/projects/{car}"), None),
    ];
    for (method, path, body) in tries {
        let res = app.call(method, &path, Some(&sam), body).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{method} {path}: {:?}", res.body);
    }
    let res = app
        .call("POST", "/api/v1/projects", Some(&sam), Some(json!({"title": "Kid", "spawned_from": car})))
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // The canvas shows it, with its own layout for Sam.
    let canvas = app.call("GET", "/api/v1/canvases/home", Some(&sam), None).await.body;
    assert_eq!(titles(&canvas["projects"]), vec!["Beetle restoration"]);
    assert_eq!(canvas["nodes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn an_editor_can_change_but_not_delete_or_reshare() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let logan_id = user_id(&app, &logan).await;
    let sam = join(&app, &logan, "Sam", json!({})).await;
    let sam_id = user_id(&app, &sam).await;
    let alex = join(&app, &logan, "Alex", json!({})).await;
    let alex_id = user_id(&app, &alex).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "role": "editor"})).await;

    let task = app
        .call(
            "POST",
            &format!("/api/v1/projects/{car}/tasks"),
            Some(&sam),
            Some(json!({"title": "Order pads", "assignee_id": sam_id})),
        )
        .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.body);
    assert_eq!(task.body["assignee_name"], "Sam");
    let task_id = task.body["id"].as_str().unwrap();
    let res = app
        .call("PATCH", &format!("/api/v1/tasks/{task_id}"), Some(&sam), Some(json!({"assignee_id": alex_id})))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "Alex can't see the project");
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/tasks/{task_id}"),
            Some(&sam),
            Some(json!({"assignee_id": logan_id})),
        )
        .await;
    assert_eq!(res.body["assignee_name"], "Logan");
    let mine = app.call("GET", "/api/v1/tasks?mine=true", Some(&logan), None).await.body;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    assert_eq!(app.call("GET", "/api/v1/tasks?mine=true", Some(&sam), None).await.body, json!([]));

    let res = app
        .call("PATCH", &format!("/api/v1/projects/{car}"), Some(&sam), Some(json!({"status": "paused"})))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let activity =
        app.call("GET", &format!("/api/v1/projects/{car}/dashboard"), Some(&logan), None).await.body;
    assert!(activity["activity"].as_array().unwrap().iter().any(|a| a["user_name"] == "Sam"));

    assert_eq!(
        app.call("DELETE", &format!("/api/v1/projects/{car}"), Some(&sam), None).await.status,
        StatusCode::FORBIDDEN
    );
    let res = app
        .call("POST", "/api/v1/shares", Some(&sam), Some(json!({"grantee_id": alex_id, "project_id": car})))
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // A project Sam spawns joins Logan's family, and Sam keeps editing it.
    let kid = project(&app, &sam, json!({"title": "Engine rebuild", "spawned_from": car})).await;
    let kid_view = app.call("GET", &format!("/api/v1/projects/{kid}"), Some(&logan), None).await.body;
    assert_eq!(kid_view["project"]["role"], "owner");
    let sams = app.call("GET", &format!("/api/v1/projects/{kid}"), Some(&sam), None).await.body;
    assert_eq!(sams["project"]["role"], "editor");
    assert_eq!(sams["project"]["owner_name"], "Logan");

    let people = app.call("GET", &format!("/api/v1/projects/{car}/people"), Some(&sam), None).await.body;
    let names: Vec<(&str, &str)> = people
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["name"].as_str().unwrap(), m["via"].as_str().unwrap()))
        .collect();
    assert_eq!(names, vec![("Logan", "owner"), ("Sam", "project")]);
}

#[tokio::test]
async fn children_come_along_only_when_asked_and_links_stay_honest() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam", json!({})).await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let vids = project(&app, &logan, json!({"title": "YouTube videos", "spawned_from": car})).await;
    let anim = project(&app, &logan, json!({"title": "Blender animations", "spawned_from": vids})).await;

    let s = share(&app, &logan, json!({"grantee_id": sam_id, "project_id": vids})).await;
    let canvas = app.call("GET", "/api/v1/canvases/home", Some(&sam), None).await.body;
    assert_eq!(titles(&canvas["projects"]), vec!["YouTube videos"]);
    // The car is linked but not shared: a locked card, no title.
    let locked = canvas["locked"].as_array().unwrap();
    assert_eq!(locked.len(), 2, "{locked:?}");
    assert!(locked.iter().any(|l| l["id"] == car.as_str() && l["owner_name"] == "Logan"));
    assert!(!canvas.to_string().contains("Beetle restoration"));
    assert_eq!(canvas["links"].as_array().unwrap().len(), 2);
    assert_eq!(canvas["nodes"].as_array().unwrap().len(), 3);
    let detail = app.call("GET", &format!("/api/v1/projects/{vids}"), Some(&sam), None).await.body;
    assert_eq!(detail["parents"], json!([]));

    let id = s["id"].as_str().unwrap();
    let res = app
        .call("PATCH", &format!("/api/v1/shares/{id}"), Some(&logan), Some(json!({"include_children": true})))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Blender animations", "YouTube videos"]);
    let people = app.call("GET", &format!("/api/v1/projects/{anim}/people"), Some(&logan), None).await.body;
    assert_eq!(people[1]["via"], "family");

    // Sam can leave.
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/shares/{id}"), Some(&sam), None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(app.call("GET", "/api/v1/projects", Some(&sam), None).await.body, json!([]));
}

#[tokio::test]
async fn notes_follow_their_projects_and_account_shares() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam", json!({})).await;
    let sam_id = user_id(&app, &sam).await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let garden = project(&app, &logan, json!({"title": "Garden"})).await;
    let note = |title: &str, projects: Vec<&str>| json!({"title": title, "body": format!("about {title}"), "project_ids": projects});
    let bleed =
        app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Bleeding brakes", vec![&car]))).await;
    let bleed_id = bleed.body["note"]["id"].as_str().unwrap().to_string();
    app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Knowledge only", vec![]))).await;
    app.call("POST", "/api/v1/notes", Some(&logan), Some(note("Tomatoes", vec![&garden]))).await;

    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "role": "viewer"})).await;
    let all = app.call("GET", "/api/v1/notes", Some(&sam), None).await.body;
    assert_eq!(titles(&all), vec!["Bleeding brakes"]);
    assert_eq!(all[0]["owner_name"], "Logan");
    assert_eq!(all[0]["can_edit"], false);
    assert_eq!(app.call("GET", "/api/v1/notes?shared=false", Some(&sam), None).await.body, json!([]));
    let read = app.call("GET", &format!("/api/v1/notes/{bleed_id}"), Some(&sam), None).await;
    assert_eq!(read.body["body"].as_str().unwrap().trim(), "about Bleeding brakes");
    let res = app
        .call("PATCH", &format!("/api/v1/notes/{bleed_id}"), Some(&sam), Some(json!({"body": "changed"})))
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    // Viewers can't file their own notes under the project either.
    let res = app.call("POST", "/api/v1/notes", Some(&sam), Some(note("Mine", vec![&car]))).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // As an editor, Sam changes Logan's file in Logan's vault, and can file notes.
    share(&app, &logan, json!({"grantee_id": sam_id, "project_id": car, "role": "editor"})).await;
    let res = app
        .call("PATCH", &format!("/api/v1/notes/{bleed_id}"), Some(&sam), Some(json!({"body": "use DOT 4"})))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let logan_id = user_id(&app, &logan).await;
    let file = std::fs::read_to_string(
        app.data_dir.join("vaults").join(&logan_id).join("Projects/Beetle restoration/Bleeding brakes.md"),
    )
    .unwrap();
    assert!(file.contains("use DOT 4"));
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/notes/{bleed_id}"), Some(&sam), None).await.status,
        StatusCode::FORBIDDEN
    );
    let sams = app.call("POST", "/api/v1/notes", Some(&sam), Some(note("Pad sizes", vec![&car]))).await;
    assert_eq!(sams.status, StatusCode::CREATED);
    let on_car = app.call("GET", &format!("/api/v1/projects/{car}/notes"), Some(&logan), None).await.body;
    assert_eq!(titles(&on_car["notes"]), vec!["Bleeding brakes", "Pad sizes"]);

    // A whole-account share takes in every project; knowledge notes only when ticked.
    let s = share(&app, &logan, json!({"grantee_id": sam_id, "role": "viewer"})).await;
    assert_eq!(s["kind"], "account");
    let all = app.call("GET", "/api/v1/notes?shared=true", Some(&sam), None).await.body;
    assert_eq!(titles(&all), vec!["Bleeding brakes", "Tomatoes"]);
    let id = s["id"].as_str().unwrap();
    app.call("PATCH", &format!("/api/v1/shares/{id}"), Some(&logan), Some(json!({"include_notes": true})))
        .await;
    let all = app.call("GET", "/api/v1/notes?shared=true", Some(&sam), None).await.body;
    assert_eq!(titles(&all), vec!["Bleeding brakes", "Knowledge only", "Tomatoes"]);
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    let roles: Vec<(&str, &str)> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["title"].as_str().unwrap(), p["role"].as_str().unwrap()))
        .collect();
    assert_eq!(
        roles,
        vec![("Beetle restoration", "editor"), ("Garden", "viewer")],
        "the stronger share wins"
    );

    let mine = app.call("GET", "/api/v1/shares", Some(&sam), None).await.body;
    assert_eq!(mine["received"].as_array().unwrap().len(), 2);
    assert_eq!(mine["given"], json!([]));
}

#[tokio::test]
async fn invites_can_carry_a_share() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let car = project(&app, &logan, json!({"title": "Beetle restoration"})).await;
    let created = app
        .call(
            "POST",
            "/api/v1/invites",
            Some(&logan),
            Some(json!({"scope": "project", "project_id": car, "role": "editor"})),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["invite"]["project_title"], "Beetle restoration");
    let token = created.body["token"].as_str().unwrap();
    let lookup = app.call("GET", &format!("/api/v1/invites/lookup/{token}"), None, None).await.body;
    assert_eq!(lookup["scope"], "project");
    assert_eq!(lookup["project_title"], "Beetle restoration");
    assert_eq!(lookup["role"], "editor");
    let res = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": token, "name": "Sam", "email": "sam@example.com", "password": "a long password"})),
        )
        .await;
    let sam = res.cookie.unwrap();
    let list = app.call("GET", "/api/v1/projects", Some(&sam), None).await.body;
    assert_eq!(titles(&list), vec!["Beetle restoration"]);
    assert_eq!(list[0]["role"], "editor");

    let res = app
        .call(
            "POST",
            "/api/v1/invites",
            Some(&logan),
            Some(json!({"scope": "project", "project_id": "nope"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app.call("POST", "/api/v1/invites", Some(&logan), Some(json!({"scope": "galaxy"}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    let alex = join(&app, &logan, "Alex", json!({"scope": "account"})).await;
    let list = app.call("GET", "/api/v1/projects", Some(&alex), None).await.body;
    assert_eq!(list[0]["role"], "viewer");
    let people = app.call("GET", "/api/v1/people", Some(&logan), None).await.body;
    let names: Vec<&str> = people.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Alex", "Sam"]);
}
