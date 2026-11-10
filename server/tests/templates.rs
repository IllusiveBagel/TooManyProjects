//! Project templates: the built-ins, starting a project from one, saving a
//! project as one, sharing, and switching an empty board.

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

fn names(v: &Value, key: &str) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x[key].as_str().unwrap().to_string()).collect()
}

async fn user_id(app: &TestApp, cookie: &str) -> String {
    let res = app.call("GET", "/api/v1/auth/status", Some(cookie), None).await;
    res.body["user"]["id"].as_str().unwrap().to_string()
}

async fn join(app: &TestApp, owner: &str, name: &str) -> String {
    let created = ok(app, "POST", "/invites", owner, Some(json!({}))).await;
    let email = format!("{}@example.com", name.to_lowercase());
    let res = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": created["token"], "name": name, "email": email, "password": "a long password"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    res.cookie.unwrap()
}

#[tokio::test]
async fn built_in_templates_set_up_a_new_project() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;

    let list = ok(&app, "GET", "/templates", &me, None).await;
    assert_eq!(
        names(&list, "name"),
        ["Blank", "Electronics / PCB", "3D print / prop", "Car / restoration", "Video", "Software"]
    );
    assert_eq!(list[1]["database_names"], json!(["Parts"]));

    // No template: today's To do, Doing, Done.
    let plain = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Plain"}))).await);
    let cols = ok(&app, "GET", &format!("/projects/{plain}/statuses"), &me, None).await;
    assert_eq!(names(&cols, "name"), ["To do", "Doing", "Done"]);

    let pcb = id(&ok(
        &app,
        "POST",
        "/projects",
        &me,
        Some(json!({"title": "Macro Keypad", "template_id": "builtin-pcb"})),
    )
    .await);
    let cols = ok(&app, "GET", &format!("/projects/{pcb}/statuses"), &me, None).await;
    assert_eq!(names(&cols, "name"), ["Design", "Ready to order", "Ordered", "Assembly", "Testing", "Done"]);
    assert_eq!(
        names(&cols, "category"),
        ["todo", "in_progress", "in_progress", "in_progress", "in_progress", "done"]
    );
    let labels = ok(&app, "GET", &format!("/projects/{pcb}/labels"), &me, None).await;
    assert_eq!(names(&labels, "name"), ["Firmware", "Layout", "Schematic"]);
    let dbs = ok(&app, "GET", &format!("/projects/{pcb}/databases"), &me, None).await;
    assert_eq!(dbs[0]["database"]["name"], "Parts");
    assert_eq!(dbs[0]["database"]["template"], "parts");

    // The prop template brings a starter task with its checklist.
    let prop = id(&ok(
        &app,
        "POST",
        "/projects",
        &me,
        Some(json!({"title": "Helmet", "template_id": "builtin-prop"})),
    )
    .await);
    let tasks = ok(&app, "GET", &format!("/projects/{prop}/tasks"), &me, None).await;
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    let task = ok(&app, "GET", &format!("/tasks/{}", id(&tasks[0])), &me, None).await;
    assert_eq!(task["checklist"].as_array().unwrap().len(), 10);

    let missing = app
        .call(
            "POST",
            "/api/v1/projects",
            Some(&me),
            Some(json!({"title": "X", "template_id": "builtin-nope"})),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn save_a_project_as_a_template_and_start_from_it() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let z80 = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Z80 Computer"}))).await);
    let cols = ok(&app, "GET", &format!("/projects/{z80}/statuses"), &me, None).await;
    let doing = id(&cols[1]);
    let done = id(&cols[2]);
    ok(
        &app,
        "POST",
        &format!("/projects/{z80}/statuses"),
        &me,
        Some(json!({"name": "Shipped", "category": "in_progress"})),
    )
    .await;
    let label =
        id(&ok(&app, "POST", &format!("/projects/{z80}/labels"), &me, Some(json!({"name": "Board"}))).await);
    let open = id(&ok(
        &app,
        "POST",
        &format!("/projects/{z80}/tasks"),
        &me,
        Some(json!({"title": "Solder the CPU", "status_id": doing, "label_ids": [label]})),
    )
    .await);
    ok(&app, "POST", &format!("/tasks/{open}/checklist"), &me, Some(json!({"text": "Check pin 1"}))).await;
    ok(
        &app,
        "POST",
        &format!("/projects/{z80}/tasks"),
        &me,
        Some(json!({"title": "Old done", "status_id": done})),
    )
    .await;
    let db =
        ok(&app, "POST", &format!("/projects/{z80}/databases"), &me, Some(json!({"name": "Chips"}))).await;
    let db_id = id(&db["database"]);
    let kind = id(&ok(&app, "POST", &format!("/databases/{db_id}/columns"), &me,
        Some(json!({"name": "Kind", "kind": "select", "options": {"choices": [{"name": "Logic"}, {"name": "Memory"}]}}))).await);
    ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/views"),
        &me,
        Some(json!({"name": "By kind", "kind": "board", "config": {"group_by": kind}})),
    )
    .await;
    ok(&app, "POST", &format!("/databases/{db_id}/rows"), &me, Some(json!({"values": {}}))).await;
    ok(&app, "POST", "/notes", &me, Some(json!({"title": "Build log", "body": "# Build log\nsecret words\n## Week 1\nmore", "project_ids": [z80]}))).await;

    let saved = ok(
        &app,
        "POST",
        "/templates",
        &me,
        Some(json!({"name": "Z80-style board", "project_id": z80, "notes": true})),
    )
    .await;
    assert_eq!(saved["source_title"], "Z80 Computer");
    assert_eq!(saved["task_count"], 1, "done tasks are left out");
    assert_eq!(saved["mine"], true);
    assert_eq!(saved["note_titles"], json!(["Build log"]));

    let keypad =
        id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Keypad", "template_id": id(&saved)})))
            .await);
    let cols = ok(&app, "GET", &format!("/projects/{keypad}/statuses"), &me, None).await;
    assert_eq!(names(&cols, "name"), ["To do", "Doing", "Done", "Shipped"]);
    let tasks = ok(&app, "GET", &format!("/projects/{keypad}/tasks"), &me, None).await;
    assert_eq!(names(&tasks, "title"), ["Solder the CPU"]);
    assert_eq!(tasks[0]["status_id"], cols[1]["id"]);
    assert_eq!(tasks[0]["labels"][0]["name"], "Board");
    let task = ok(&app, "GET", &format!("/tasks/{}", id(&tasks[0])), &me, None).await;
    assert_eq!(task["checklist"][0]["text"], "Check pin 1");
    let dbs = ok(&app, "GET", &format!("/projects/{keypad}/databases"), &me, None).await;
    assert_eq!(dbs[0]["database"]["name"], "Chips");
    assert_eq!(dbs[0]["database"]["row_count"], 0);
    let new_kind = dbs[0]["columns"].as_array().unwrap().iter().find(|c| c["name"] == "Kind").unwrap();
    assert_eq!(dbs[0]["views"][1]["config"]["group_by"], new_kind["id"]);
    let notes = ok(&app, "GET", &format!("/projects/{keypad}/notes"), &me, None).await;
    let note = ok(&app, "GET", &format!("/notes/{}", id(&notes["notes"][0])), &me, None).await;
    assert_eq!(note["body"].as_str().unwrap().trim(), "# Build log\n\n## Week 1");

    // Built-ins can't be changed, but can be copied.
    let edit =
        app.call("PATCH", "/api/v1/templates/builtin-video", Some(&me), Some(json!({"name": "x"}))).await;
    assert_eq!(edit.status, StatusCode::NOT_FOUND);
    let copy = ok(
        &app,
        "POST",
        "/templates",
        &me,
        Some(json!({"name": "My video", "from_template_id": "builtin-video"})),
    )
    .await;
    assert_eq!(copy["columns"][0]["name"], "Idea");
    let renamed =
        ok(&app, "PATCH", &format!("/templates/{}", id(&copy)), &me, Some(json!({"name": "Shorts"}))).await;
    assert_eq!(renamed["name"], "Shorts");
    ok(&app, "DELETE", &format!("/templates/{}", id(&copy)), &me, None).await;
}

#[tokio::test]
async fn shared_templates_and_switching_an_empty_board() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let sam = join(&app, &me, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    let car = id(&ok(
        &app,
        "POST",
        "/projects",
        &me,
        Some(json!({"title": "Beetle", "template_id": "builtin-car"})),
    )
    .await);
    let t = ok(&app, "POST", "/templates", &me, Some(json!({"name": "Car work", "project_id": car}))).await;

    // Private until shared, and then only to people Logan shares everything with.
    let theirs = ok(&app, "GET", "/templates", &sam, None).await;
    assert!(!names(&theirs, "name").contains(&"Car work".to_string()));
    ok(&app, "PATCH", &format!("/templates/{}", id(&t)), &me, Some(json!({"shared": true}))).await;
    let theirs = ok(&app, "GET", "/templates", &sam, None).await;
    assert!(!names(&theirs, "name").contains(&"Car work".to_string()));
    ok(&app, "POST", "/shares", &me, Some(json!({"grantee_id": sam_id}))).await;
    let theirs = ok(&app, "GET", "/templates", &sam, None).await;
    let shared = theirs.as_array().unwrap().iter().find(|x| x["name"] == "Car work").unwrap();
    assert_eq!(shared["mine"], false);
    assert!(shared["owner_name"].is_string());
    let del = app.call("DELETE", &format!("/api/v1/templates/{}", id(&t)), Some(&sam), None).await;
    assert_eq!(del.status, StatusCode::FORBIDDEN);

    // An empty board can switch; one with tasks can't.
    let empty = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Channel"}))).await);
    ok(
        &app,
        "POST",
        &format!("/projects/{empty}/apply-template"),
        &me,
        Some(json!({"template_id": "builtin-video"})),
    )
    .await;
    let cols = ok(&app, "GET", &format!("/projects/{empty}/statuses"), &me, None).await;
    assert_eq!(names(&cols, "name"), ["Idea", "Script", "Shoot", "Edit", "Published"]);
    ok(&app, "POST", &format!("/projects/{empty}/tasks"), &me, Some(json!({"title": "Pilot"}))).await;
    let busy = app
        .call(
            "POST",
            &format!("/api/v1/projects/{empty}/apply-template"),
            Some(&me),
            Some(json!({"template_id": "builtin-software"})),
        )
        .await;
    assert_eq!(busy.status, StatusCode::CONFLICT);
}
