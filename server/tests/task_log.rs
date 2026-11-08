//! Comments on tasks and notes linked to tasks, and who may see and change them.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn ok(app: &TestApp, method: &str, path: &str, cookie: &str, body: Option<Value>) -> Value {
    let res = app.call(method, &format!("/api/v1{path}"), Some(cookie), body).await;
    assert!(res.status.is_success(), "{method} {path}: {} {:?}", res.status, res.body);
    res.body
}

async fn status(app: &TestApp, method: &str, path: &str, cookie: &str, body: Option<Value>) -> StatusCode {
    app.call(method, &format!("/api/v1{path}"), Some(cookie), body).await.status
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
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

/// A project with one task, as (project id, task id).
async fn seed(app: &TestApp, me: &str) -> (String, String) {
    let car = id(&ok(app, "POST", "/projects", me, Some(json!({"title": "Beetle restoration"}))).await);
    let task =
        id(&ok(app, "POST", &format!("/projects/{car}/tasks"), me, Some(json!({"title": "Bleed brakes"})))
            .await);
    (car, task)
}

#[tokio::test]
async fn comments_make_a_log_on_the_task() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, task) = seed(&app, &me).await;

    let first = ok(
        &app,
        "POST",
        &format!("/tasks/{task}/comments"),
        &me,
        Some(json!({"body": "Front left done. **Seized nipple** on the rear."})),
    )
    .await;
    assert_eq!(first["can_edit"], true);
    assert_eq!(first["can_delete"], true);
    assert_eq!(first["via"], "web");
    ok(
        &app,
        "POST",
        &format!("/tasks/{task}/comments"),
        &me,
        Some(json!({"body": "Penetrating oil overnight worked."})),
    )
    .await;
    assert_eq!(
        status(&app, "POST", &format!("/tasks/{task}/comments"), &me, Some(json!({"body": "   "}))).await,
        StatusCode::BAD_REQUEST
    );

    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    let bodies: Vec<&str> =
        detail["comments"].as_array().unwrap().iter().map(|c| c["body"].as_str().unwrap()).collect();
    assert_eq!(
        bodies,
        ["Front left done. **Seized nipple** on the rear.", "Penetrating oil overnight worked."]
    );
    assert_eq!(detail["task"]["comment_count"], 2);
    assert_eq!(detail["activity"][0]["kind"], "task.commented");

    // The board's cards carry the count too.
    let tasks = ok(&app, "GET", &format!("/projects/{car}/tasks"), &me, None).await;
    assert_eq!(tasks[0]["comment_count"], 2);

    // Edit, then delete.
    let edited = ok(
        &app,
        "PATCH",
        &format!("/comments/{}", id(&first)),
        &me,
        Some(json!({"body": "Front left done."})),
    )
    .await;
    assert_eq!(edited["body"], "Front left done.");
    assert!(edited["edited_at"].is_string());
    assert_eq!(
        status(&app, "DELETE", &format!("/comments/{}", id(&first)), &me, None).await,
        StatusCode::NO_CONTENT
    );
    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["comments"].as_array().unwrap().len(), 1);

    // Deleting the task takes its comments with it.
    ok(&app, "DELETE", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(status(&app, "GET", &format!("/tasks/{task}"), &me, None).await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sharing_decides_who_reads_writes_and_deletes_comments() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let ana = join(&app, &logan, "Ana").await;
    let stranger = join(&app, &logan, "Lee").await;
    let (car, task) = seed(&app, &logan).await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "project_id": car, "role": "viewer"})),
    )
    .await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &ana).await, "project_id": car, "role": "editor"})),
    )
    .await;

    let mine = ok(
        &app,
        "POST",
        &format!("/tasks/{task}/comments"),
        &logan,
        Some(json!({"body": "Bought new seals."})),
    )
    .await;

    // A viewer reads the log but can't add to it, edit or delete.
    let seen = ok(&app, "GET", &format!("/tasks/{task}"), &sam, None).await;
    assert_eq!(seen["comments"][0]["body"], "Bought new seals.");
    assert_eq!(seen["comments"][0]["user_name"], "Logan");
    assert_eq!(seen["comments"][0]["can_edit"], false);
    assert_eq!(seen["comments"][0]["can_delete"], false);
    assert_eq!(
        status(&app, "POST", &format!("/tasks/{task}/comments"), &sam, Some(json!({"body": "hi"}))).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "PATCH", &format!("/comments/{}", id(&mine)), &sam, Some(json!({"body": "x"}))).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/comments/{}", id(&mine)), &sam, None).await,
        StatusCode::FORBIDDEN
    );

    // Someone it isn't shared with can't tell the comment exists.
    assert_eq!(status(&app, "GET", &format!("/tasks/{task}"), &stranger, None).await, StatusCode::NOT_FOUND);
    assert_eq!(
        status(&app, "DELETE", &format!("/comments/{}", id(&mine)), &stranger, None).await,
        StatusCode::NOT_FOUND
    );

    // An editor adds their own, and can edit only that one.
    let hers =
        ok(&app, "POST", &format!("/tasks/{task}/comments"), &ana, Some(json!({"body": "Fitted them."})))
            .await;
    assert_eq!(
        status(&app, "PATCH", &format!("/comments/{}", id(&mine)), &ana, Some(json!({"body": "x"}))).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/comments/{}", id(&mine)), &ana, None).await,
        StatusCode::FORBIDDEN
    );

    // The owner can't reword hers, but can remove it.
    let seen = ok(&app, "GET", &format!("/tasks/{task}"), &logan, None).await;
    assert_eq!(seen["comments"][1]["can_edit"], false);
    assert_eq!(seen["comments"][1]["can_delete"], true);
    assert_eq!(
        status(&app, "PATCH", &format!("/comments/{}", id(&hers)), &logan, Some(json!({"body": "x"}))).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/comments/{}", id(&hers)), &logan, None).await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn notes_link_to_tasks_both_ways() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, task) = seed(&app, &me).await;
    let note = ok(
        &app,
        "POST",
        "/notes",
        &me,
        Some(json!({"title": "Brake fluid", "body": "DOT 4 only.", "folder": "knowledge"})),
    )
    .await;
    let note_id = note["note"]["id"].as_str().unwrap().to_string();

    let linked = ok(&app, "PUT", &format!("/tasks/{task}/notes/{note_id}"), &me, None).await;
    assert_eq!(linked["title"], "Brake fluid");
    // Linking again is harmless.
    ok(&app, "PUT", &format!("/tasks/{task}/notes/{note_id}"), &me, None).await;

    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["notes"][0]["title"], "Brake fluid");
    assert_eq!(detail["hidden_notes"], 0);
    assert_eq!(detail["task"]["note_count"], 1);
    let kinds: Vec<&str> =
        detail["activity"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "task.note_linked").count(), 1, "logged once: {kinds:?}");

    let from_note = ok(&app, "GET", &format!("/notes/{note_id}"), &me, None).await;
    assert_eq!(from_note["tasks"][0]["title"], "Bleed brakes");
    assert_eq!(from_note["tasks"][0]["number"], 1);

    // A new note made from the task is filed under its project and linked.
    let made =
        ok(&app, "POST", "/notes", &me, Some(json!({"title": "Bleeding order", "task_id": task}))).await;
    assert_eq!(made["note"]["projects"][0]["id"], car);
    assert!(made["note"]["path"].as_str().unwrap().starts_with("Projects/Beetle restoration/"));
    assert_eq!(made["tasks"][0]["id"], task);
    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["notes"].as_array().unwrap().len(), 2);

    // Unlink keeps the note; deleting a note drops its links.
    assert_eq!(
        status(&app, "DELETE", &format!("/tasks/{task}/notes/{note_id}"), &me, None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/tasks/{task}/notes/{note_id}"), &me, None).await,
        StatusCode::NOT_FOUND
    );
    ok(&app, "GET", &format!("/notes/{note_id}"), &me, None).await;
    let made_id = made["note"]["id"].as_str().unwrap().to_string();
    ok(&app, "DELETE", &format!("/notes/{made_id}"), &me, None).await;
    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["notes"], json!([]));
    assert_eq!(detail["task"]["note_count"], 0);

    assert_eq!(
        status(&app, "PUT", &format!("/tasks/{task}/notes/nope"), &me, None).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn linked_notes_respect_note_sharing() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let (car, task) = seed(&app, &logan).await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "project_id": car, "role": "editor"})),
    )
    .await;

    // One note filed under the project (shared with Sam), one private.
    let shared =
        ok(&app, "POST", "/notes", &logan, Some(json!({"title": "Bleeding order", "task_id": task}))).await;
    let private =
        ok(&app, "POST", "/notes", &logan, Some(json!({"title": "Shopping", "folder": "knowledge"}))).await;
    let private_id = private["note"]["id"].as_str().unwrap().to_string();
    ok(&app, "PUT", &format!("/tasks/{task}/notes/{private_id}"), &logan, None).await;

    let seen = ok(&app, "GET", &format!("/tasks/{task}"), &sam, None).await;
    let titles: Vec<&str> =
        seen["notes"].as_array().unwrap().iter().map(|n| n["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Bleeding order"]);
    assert_eq!(seen["hidden_notes"], 1);

    // Sam can't link a note he can't read, or unlink from a task he can't change.
    assert_eq!(
        status(&app, "PUT", &format!("/tasks/{task}/notes/{private_id}"), &sam, None).await,
        StatusCode::NOT_FOUND
    );
    let sams =
        ok(&app, "POST", "/notes", &sam, Some(json!({"title": "Sam's tip", "folder": "knowledge"}))).await;
    let sams_id = sams["note"]["id"].as_str().unwrap().to_string();
    ok(&app, "PUT", &format!("/tasks/{task}/notes/{sams_id}"), &sam, None).await;
    // Logan sees that Sam linked a note he can't read.
    let seen = ok(&app, "GET", &format!("/tasks/{task}"), &logan, None).await;
    assert_eq!(seen["hidden_notes"], 1);
    assert_eq!(seen["notes"].as_array().unwrap().len(), 2);

    // The note side shows only tasks the reader can see.
    let shared_id = shared["note"]["id"].as_str().unwrap().to_string();
    let other = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Private"}))).await);
    let hidden_task =
        id(&ok(&app, "POST", &format!("/projects/{other}/tasks"), &logan, Some(json!({"title": "Secret"})))
            .await);
    ok(&app, "PUT", &format!("/tasks/{hidden_task}/notes/{shared_id}"), &logan, None).await;
    let n = ok(&app, "GET", &format!("/notes/{shared_id}"), &sam, None).await;
    let titles: Vec<&str> =
        n["tasks"].as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Bleed brakes"]);
    let n = ok(&app, "GET", &format!("/notes/{shared_id}"), &logan, None).await;
    assert_eq!(n["tasks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_viewer_cannot_link_notes() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let (car, task) = seed(&app, &logan).await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "project_id": car, "role": "viewer"})),
    )
    .await;
    let note =
        ok(&app, "POST", "/notes", &logan, Some(json!({"title": "Torque specs", "task_id": task}))).await;
    let note_id = note["note"]["id"].as_str().unwrap().to_string();

    assert_eq!(
        status(&app, "DELETE", &format!("/tasks/{task}/notes/{note_id}"), &sam, None).await,
        StatusCode::FORBIDDEN
    );
    let sams = ok(&app, "POST", "/notes", &sam, Some(json!({"title": "Mine", "folder": "knowledge"}))).await;
    let sams_id = sams["note"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        status(&app, "PUT", &format!("/tasks/{task}/notes/{sams_id}"), &sam, None).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "POST", "/notes", &sam, Some(json!({"title": "From task", "task_id": task}))).await,
        StatusCode::FORBIDDEN
    );
    // But reads it fine.
    let seen = ok(&app, "GET", &format!("/tasks/{task}"), &sam, None).await;
    assert_eq!(seen["notes"][0]["title"], "Torque specs");
}
