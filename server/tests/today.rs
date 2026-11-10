//! Phase 4c: the Today screen, search over everything, quick add and the Inbox.

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

/// The board's columns as (name, id).
async fn columns(app: &TestApp, cookie: &str, project: &str) -> Vec<(String, String)> {
    let list = ok(app, "GET", &format!("/projects/{project}/statuses"), cookie, None).await;
    list.as_array().unwrap().iter().map(|s| (s["name"].as_str().unwrap().to_string(), id(s))).collect()
}

fn column(cols: &[(String, String)], name: &str) -> String {
    cols.iter().find(|(n, _)| n == name).unwrap().1.clone()
}

fn titles(list: &Value) -> Vec<String> {
    list.as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap().to_string()).collect()
}

fn date_in(days: i64) -> String {
    (chrono::Utc::now().date_naive() + chrono::Duration::days(days)).format("%Y-%m-%d").to_string()
}

#[tokio::test]
async fn today_gathers_what_to_touch_next() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let sam = join(&app, &me, "Sam").await;
    let sam_id = user_id(&app, &sam).await;

    let car = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Supra"}))).await);
    let cols = columns(&app, &me, &car).await;
    let doing = column(&cols, "Doing");
    let done = column(&cols, "Done");
    let blocked = id(&ok(
        &app,
        "POST",
        &format!("/projects/{car}/statuses"),
        &me,
        Some(json!({"name": "Waiting on parts", "category": "blocked"})),
    )
    .await);
    let new_task = |title: &str, extra: Value| {
        let mut body = json!({"title": title});
        body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        body
    };
    let tasks_path = format!("/projects/{car}/tasks");
    ok(&app, "POST", &tasks_path, &me, Some(new_task("Strip interior", json!({"status_id": doing})))).await;
    ok(&app, "POST", &tasks_path, &me, Some(new_task("Fuel pump", json!({"status_id": blocked})))).await;
    ok(&app, "POST", &tasks_path, &me, Some(new_task("Book MOT", json!({"due_date": date_in(3)})))).await;
    ok(&app, "POST", &tasks_path, &me, Some(new_task("Tax it", json!({"due_date": date_in(-2)})))).await;
    ok(&app, "POST", &tasks_path, &me, Some(new_task("Someday", json!({"due_date": date_in(30)})))).await;
    ok(
        &app,
        "POST",
        &tasks_path,
        &me,
        Some(new_task("Old job", json!({"status_id": done, "due_date": date_in(1)}))),
    )
    .await;

    // Sam's own project, shared with me: only what's assigned to me shows.
    let garden = id(&ok(&app, "POST", "/projects", &sam, Some(json!({"title": "Garden"}))).await);
    let me_id = user_id(&app, &me).await;
    ok(
        &app,
        "POST",
        "/shares",
        &sam,
        Some(json!({"grantee_id": me_id, "project_id": garden, "role": "editor"})),
    )
    .await;
    let sam_doing = column(&columns(&app, &sam, &garden).await, "Doing");
    ok(
        &app,
        "POST",
        &format!("/projects/{garden}/tasks"),
        &sam,
        Some(json!({"title": "Sam digs", "status_id": sam_doing})),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("/projects/{garden}/tasks"),
        &sam,
        Some(json!({"title": "Logan waters", "status_id": sam_doing, "assignee_id": me_id})),
    )
    .await;
    let _ = sam_id;

    // A session today and a quiet project.
    let today = date_in(0);
    ok(
        &app,
        "POST",
        "/calendar/events",
        &me,
        Some(json!({"kind": "session", "project_id": car, "title": "Seats out",
                    "start_local": format!("{today}T19:00"), "minutes": 120, "tz": "UTC"})),
    )
    .await;
    ok(
        &app,
        "POST",
        "/calendar/events",
        &me,
        Some(json!({"kind": "busy", "title": "Work", "start_local": format!("{today}T09:00"), "minutes": 480, "tz": "UTC"})),
    )
    .await;
    let homelab = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Homelab"}))).await);
    ok(&app, "POST", "/projects", &me, Some(json!({"title": "Paused thing", "status": "paused"}))).await;
    sqlx::query("UPDATE projects SET created_at = '2026-01-01T00:00:00Z', updated_at = '2026-01-01T00:00:00Z' WHERE title IN ('Homelab', 'Paused thing')")
        .execute(&app.state.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE activity SET created_at = '2026-01-02T00:00:00Z'
         WHERE project_id IN (SELECT id FROM projects WHERE title IN ('Homelab', 'Paused thing'))",
    )
    .execute(&app.state.db)
    .await
    .unwrap();

    // One capture in the Inbox, one filed straight under a project.
    ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": "Ask about a power button header", "kind": "inbox"})),
    )
    .await;
    ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": "Seat fabric ideas", "kind": "note", "project_id": car})),
    )
    .await;

    let t = ok(&app, "GET", "/today?tz=UTC", &me, None).await;
    assert_eq!(t["date"], today);
    let mut now = titles(&t["now"]);
    now.sort();
    assert_eq!(now, ["Logan waters", "Strip interior"]);
    assert_eq!(titles(&t["blocked"]), ["Fuel pump"]);
    assert_eq!(titles(&t["due"]), ["Tax it", "Book MOT"]);
    assert_eq!(t["due"][0]["overdue"], true);
    assert_eq!(t["sessions"].as_array().unwrap().len(), 1, "busy time isn't a session: {}", t["sessions"]);
    assert_eq!(t["sessions"][0]["event"]["title"], "Seats out");
    assert_eq!(t["quiet"].as_array().unwrap().len(), 1, "{}", t["quiet"]);
    assert_eq!(t["quiet"][0]["id"], homelab);
    assert_eq!(t["quiet"][0]["total_tasks"], 0);
    assert!(t["quiet"][0]["quiet_days"].as_i64().unwrap() > 14);
    assert_eq!(titles(&t["inbox"]), ["Ask about a power button header"]);

    // Sam sees none of my day.
    let theirs = ok(&app, "GET", "/today?tz=UTC", &sam, None).await;
    assert_eq!(titles(&theirs["now"]), ["Sam digs"]);
    assert!(theirs["inbox"].as_array().unwrap().is_empty());

    assert_eq!(status(&app, "GET", "/today?tz=Mars/Base", &me, None).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn quick_add_files_tasks_notes_and_the_inbox() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let prints = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Things to Print"}))).await);
    // A board whose first column isn't a to-do one.
    let cols = columns(&app, &me, &prints).await;
    ok(
        &app,
        "POST",
        &format!("/projects/{prints}/statuses"),
        &me,
        Some(json!({"name": "Ideas", "category": "in_progress", "after_id": null})),
    )
    .await;

    let added = ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": "Articulated dragon (Printables)\nsecond line", "kind": "task", "project_id": prints,
                    "url": "https://www.printables.com/model/123-dragon",
                    "body": "Check out this model https://www.printables.com/model/123-dragon"})),
    )
    .await;
    assert_eq!(added["kind"], "task");
    assert_eq!(added["column"], "To do");
    let task = &added["task"];
    assert_eq!(task["title"], "Articulated dragon (Printables)");
    assert_eq!(task["status_id"], column(&cols, "To do"));
    assert_eq!(task["description"], "Check out this model\n\nhttps://www.printables.com/model/123-dragon");

    let note = ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": "Filament to try", "kind": "note", "project_id": prints})),
    )
    .await;
    let detail = ok(&app, "GET", &format!("/notes/{}", note["note_id"].as_str().unwrap()), &me, None).await;
    assert_eq!(detail["note"]["projects"][0]["id"], prints);

    let link_only = ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": " ", "kind": "inbox", "url": "https://example.com/a"})),
    )
    .await;
    assert_eq!(link_only["title"], "Link from example.com");
    let inbox = ok(&app, "GET", "/notes?folder=Inbox", &me, None).await;
    assert_eq!(inbox.as_array().unwrap().len(), 1);

    for (body, code) in [
        (json!({"text": "x", "kind": "task"}), StatusCode::BAD_REQUEST),
        (json!({"text": "", "kind": "inbox"}), StatusCode::BAD_REQUEST),
        (json!({"text": "x", "kind": "idea"}), StatusCode::BAD_REQUEST),
        (json!({"text": "x", "kind": "inbox", "url": "javascript:alert(1)"}), StatusCode::BAD_REQUEST),
        (json!({"text": "x", "kind": "task", "project_id": "nope"}), StatusCode::NOT_FOUND),
    ] {
        assert_eq!(status(&app, "POST", "/quick-add", &me, Some(body.clone())).await, code, "{body}");
    }

    // A viewer can't add to someone else's board.
    let sam = join(&app, &me, "Sam").await;
    let sam_id = user_id(&app, &sam).await;
    ok(
        &app,
        "POST",
        "/shares",
        &me,
        Some(json!({"grantee_id": sam_id, "project_id": prints, "role": "viewer"})),
    )
    .await;
    assert_eq!(
        status(
            &app,
            "POST",
            "/quick-add",
            &sam,
            Some(json!({"text": "x", "kind": "task", "project_id": prints}))
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

const BOUNDARY: &str = "----tmp-test-boundary";

async fn upload_png(app: &TestApp, cookie: &str, query: &str) -> Value {
    let mut png = vec![];
    image::RgbImage::from_pixel(8, 8, image::Rgb([200, 100, 50]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"dragon.png\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&png);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let (code, bytes) = app
        .call_raw(
            "POST",
            &format!("/api/v1/attachments?{query}"),
            cookie,
            &format!("multipart/form-data; boundary={BOUNDARY}"),
            body,
        )
        .await;
    assert_eq!(code, StatusCode::CREATED);
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn filing_the_inbox_moves_captures_into_projects() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let prints = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Things to Print"}))).await);

    // A shared photo lands in the Inbox with its link…
    let captured = ok(
        &app,
        "POST",
        "/quick-add",
        &me,
        Some(json!({"text": "Articulated dragon", "kind": "inbox", "url": "https://www.printables.com/model/1"})),
    )
    .await;
    let note = captured["note_id"].as_str().unwrap().to_string();
    let photo = upload_png(&app, &me, &format!("note_id={note}")).await;
    let old_path: String = sqlx::query_scalar("SELECT path FROM attachments WHERE id = ?")
        .bind(photo[0]["id"].as_str().unwrap())
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert!(old_path.starts_with("vaults/"));
    let detail = ok(&app, "GET", &format!("/notes/{note}"), &me, None).await;
    ok(
        &app,
        "PATCH",
        &format!("/notes/{note}"),
        &me,
        Some(json!({"body": format!("{}\n\n![[dragon.png]]", detail["body"].as_str().unwrap())})),
    )
    .await;

    // …and filing it as a task moves the photo onto the task and the note goes.
    let filed = ok(
        &app,
        "POST",
        &format!("/inbox/{note}/file"),
        &me,
        Some(json!({"kind": "task", "project_id": prints, "title": "Print the dragon"})),
    )
    .await;
    let task = &filed["task"];
    assert_eq!(task["title"], "Print the dragon");
    assert_eq!(task["description"], "https://www.printables.com/model/1");
    assert_eq!(task["attachment_count"], 1);
    assert_eq!(status(&app, "GET", &format!("/notes/{note}"), &me, None).await, StatusCode::NOT_FOUND);
    let new_path: String = sqlx::query_scalar("SELECT path FROM attachments WHERE task_id = ?")
        .bind(task["id"].as_str().unwrap())
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert!(new_path.starts_with("files/"), "{new_path}");
    assert!(app.data_dir.join(&new_path).exists());
    assert!(!app.data_dir.join(&old_path).exists());
    let (code, _) = app
        .call_raw(
            "GET",
            &format!("/api/v1/attachments/{}/file", photo[0]["id"].as_str().unwrap()),
            &me,
            "",
            vec![],
        )
        .await;
    assert_eq!(code, StatusCode::OK);

    // Filing as a note keeps it, under the project.
    let idea =
        ok(&app, "POST", "/quick-add", &me, Some(json!({"text": "Lithophane lamp", "kind": "inbox"}))).await;
    let idea = idea["note_id"].as_str().unwrap().to_string();
    ok(
        &app,
        "POST",
        &format!("/inbox/{idea}/file"),
        &me,
        Some(json!({"kind": "note", "project_id": prints})),
    )
    .await;
    let n = ok(&app, "GET", &format!("/notes/{idea}"), &me, None).await;
    assert_eq!(n["note"]["projects"][0]["id"], prints);
    assert_eq!(n["note"]["path"], "Projects/Things to Print/Lithophane lamp.md");
    let t = ok(&app, "GET", "/today?tz=UTC", &me, None).await;
    assert!(t["inbox"].as_array().unwrap().is_empty());

    // Only your own Inbox.
    let sam = join(&app, &me, "Sam").await;
    let other = ok(&app, "POST", "/quick-add", &me, Some(json!({"text": "Mine", "kind": "inbox"}))).await;
    let sam_project = id(&ok(&app, "POST", "/projects", &sam, Some(json!({"title": "Sam's"}))).await);
    assert_eq!(
        status(
            &app,
            "POST",
            &format!("/inbox/{}/file", other["note_id"].as_str().unwrap()),
            &sam,
            Some(json!({"kind": "note", "project_id": sam_project}))
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn search_finds_everything_you_can_see() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = id(&ok(
        &app,
        "POST",
        "/projects",
        &me,
        Some(json!({"title": "MkIII Supra", "summary": "Fuel-injected toy"})),
    )
    .await);
    let task =
        ok(&app, "POST", &format!("/projects/{car}/tasks"), &me, Some(json!({"title": "Remove fuel tank"})))
            .await;
    let other =
        ok(&app, "POST", &format!("/projects/{car}/tasks"), &me, Some(json!({"title": "Strip interior"})))
            .await;
    ok(
        &app,
        "POST",
        &format!("/tasks/{}/comments", id(&other)),
        &me,
        Some(json!({"body": "Drain the fuel before dropping the tank."})),
    )
    .await;
    ok(
        &app,
        "POST",
        "/notes",
        &me,
        Some(json!({"title": "Supra fuel system", "body": "Walbro pump", "project_ids": [car]})),
    )
    .await;
    let db =
        ok(&app, "POST", &format!("/projects/{car}/databases"), &me, Some(json!({"name": "Parts list"})))
            .await;
    let title_col = db["columns"][0]["id"].as_str().unwrap().to_string();
    ok(
        &app,
        "POST",
        &format!("/databases/{}/rows", id(&db["database"])),
        &me,
        Some(json!({"values": {title_col: "Fuel pump (Walbro 255)"}})),
    )
    .await;

    let hits = ok(&app, "GET", "/search?q=fuel", &me, None).await;
    let kinds: Vec<&str> = hits.as_array().unwrap().iter().map(|h| h["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["task", "comment", "note", "row", "project"], "{hits}");
    assert_eq!(hits[0]["title"], "Remove fuel tank");
    assert_eq!(hits[0]["task_id"], id(&task));
    assert_eq!(hits[0]["context"], "To do");
    assert_eq!(hits[1]["task_id"], id(&other));
    assert!(hits[1]["snippet"].as_str().unwrap().contains("[fuel]"), "{}", hits[1]);
    assert_eq!(hits[2]["title"], "Supra fuel system");
    assert_eq!(hits[3]["title"], "Fuel pump (Walbro 255)");
    assert_eq!(hits[3]["context"], "Parts list");
    assert_eq!(hits[4]["title"], "MkIII Supra");

    // Prefixes, renamed tasks and wildcards.
    let pre = ok(&app, "GET", "/search?q=interi", &me, None).await;
    assert_eq!(pre[0]["title"], "Strip interior");
    ok(&app, "PATCH", &format!("/tasks/{}", id(&task)), &me, Some(json!({"title": "Drop the petrol tank"})))
        .await;
    let renamed = ok(&app, "GET", "/search?q=petrol", &me, None).await;
    assert_eq!(renamed[0]["id"], id(&task));
    assert!(ok(&app, "GET", "/search?q=%25", &me, None).await.as_array().unwrap().is_empty());
    assert!(ok(&app, "GET", "/search?q=%20", &me, None).await.as_array().unwrap().is_empty());

    // Someone else finds none of it.
    let sam = join(&app, &me, "Sam").await;
    assert!(ok(&app, "GET", "/search?q=fuel", &sam, None).await.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn link_previews_refuse_local_addresses() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    // Not fetched: the answer is just the address back.
    let p = ok(&app, "GET", "/link-preview?url=http://127.0.0.1:8080/api/health", &me, None).await;
    assert_eq!(p["url"], "http://127.0.0.1:8080/api/health");
    assert!(p["title"].is_null());
    assert_eq!(
        status(&app, "GET", "/link-preview?url=file:///etc/passwd", &me, None).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.call("GET", "/api/v1/link-preview?url=https://example.com", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
}
