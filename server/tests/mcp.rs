mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::TestApp;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// One JSON-RPC call to /mcp. Returns the HTTP status and the parsed body
/// (JSON, or the first `data:` line of an event stream).
async fn rpc(app: &TestApp, token: Option<&str>, method: &str, params: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "projects.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let res = app.router.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    let json = serde_json::from_str(&text).unwrap_or_else(|_| {
        text.lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
            .find(|v| v.get("id").is_some())
            .unwrap_or(Value::Null)
    });
    (status, json)
}

async fn initialize(app: &TestApp, token: &str) -> Value {
    let (status, body) = rpc(
        app,
        Some(token),
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Calls a tool and returns (is_error, text).
async fn tool(app: &TestApp, token: &str, name: &str, args: Value) -> (bool, String) {
    let (status, body) = rpc(app, Some(token), "tools/call", json!({"name": name, "arguments": args})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert!(result.is_object(), "{body}");
    let text = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (result["isError"].as_bool().unwrap_or(false), text)
}

async fn make_token(app: &TestApp, cookie: &str, scope: &str) -> String {
    let res = app
        .call("POST", "/api/v1/tokens", Some(cookie), Some(json!({"name": "Claude", "scope": scope})))
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let secret = res.body["secret"].as_str().unwrap().to_string();
    assert!(secret.starts_with("tmp_"));
    secret
}

async fn seed(app: &TestApp, me: &str) -> (String, String) {
    let car = app
        .call(
            "POST",
            "/api/v1/projects",
            Some(me),
            Some(json!({"title": "Car restoration", "summary": "1972 Beetle"})),
        )
        .await;
    let car = car.body["id"].as_str().unwrap().to_string();
    let vids = app
        .call(
            "POST",
            "/api/v1/projects",
            Some(me),
            Some(json!({"title": "Restoration videos", "spawned_from": car})),
        )
        .await;
    let vids = vids.body["id"].as_str().unwrap().to_string();
    let res = app
        .call(
            "POST",
            &format!("/api/v1/projects/{car}/tasks"),
            Some(me),
            Some(json!({"title": "Rebuild carburettor", "priority": "high", "due_date": "2020-01-01"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    app.call(
        "POST",
        &format!("/api/v1/projects/{vids}/tasks"),
        Some(me),
        Some(json!({"title": "Film the teardown"})),
    )
    .await;
    let res = app
        .call("POST", "/api/v1/notes", Some(me), Some(json!({"title": "Brake fluid", "body": "Use DOT 4 for the [[@Car restoration]].", "project_ids": [car], "tags": ["howto"]})))
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    (car, vids)
}

#[tokio::test]
async fn tokens_are_made_listed_and_revoked() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let secret = make_token(&app, &me, "write").await;
    let list = app.call("GET", "/api/v1/tokens", Some(&me), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);
    assert_eq!(list.body[0]["scope"], "write");
    assert!(
        list.body[0].get("secret").is_none() && !list.body.to_string().contains(&secret[4..]),
        "secrets are never listed"
    );

    let bad =
        app.call("POST", "/api/v1/tokens", Some(&me), Some(json!({"name": "x", "scope": "admin"}))).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    initialize(&app, &secret).await;
    let used = app.call("GET", "/api/v1/tokens", Some(&me), None).await;
    assert!(used.body[0]["last_used_at"].is_string());

    let id = list.body[0]["id"].as_str().unwrap();
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/tokens/{id}"), Some(&me), None).await.status,
        StatusCode::NO_CONTENT
    );
    let (status, _) = rpc(&app, Some(&secret), "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a revoked token stops working");
}

#[tokio::test]
async fn mcp_needs_a_live_token() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (status, body) = rpc(&app, None, "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let (status, _) = rpc(&app, Some("tmp_not-a-real-token"), "tools/list", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // A session cookie is not enough.
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::COOKIE, &me)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(app.router.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    // Clients are pointed at sign-in (see tests/oauth.rs); other .well-known paths stay a 404.
    let wk = app.call("GET", "/.well-known/oauth-protected-resource", None, None).await;
    assert_eq!(wk.status, StatusCode::OK);
    let wk = app.call("GET", "/.well-known/security.txt", None, None).await;
    assert_eq!(wk.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reads_projects_notes_and_briefings() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _vids) = seed(&app, &me).await;
    let token = make_token(&app, &me, "read").await;

    let init = initialize(&app, &token).await;
    assert!(init["result"]["capabilities"]["tools"].is_object(), "{init}");
    assert_eq!(init["result"]["serverInfo"]["name"], "toomanyprojects");
    assert!(init["result"]["instructions"].as_str().unwrap().contains("TooManyProjects"));

    let (_, tools) = rpc(&app, Some(&token), "tools/list", json!({})).await;
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "list_projects",
        "get_project_context",
        "search",
        "list_notes",
        "get_note",
        "create_task",
        "update_task",
        "add_note",
        "append_to_note",
        "update_note",
        "spawn_project",
        "create_project",
        "update_project",
        "add_column",
        "update_column",
        "get_task",
        "add_task_comment",
        "link_note_to_task",
        "get_calendar",
        "plan_time",
        "get_today",
        "quick_add",
        "get_shopping_list",
        "mark_parts",
    ] {
        assert!(names.contains(&n), "missing {n}: {names:?}");
    }

    let (err, text) = tool(&app, &token, "list_projects", json!({})).await;
    assert!(!err);
    assert!(text.contains("**Car restoration**") && text.contains("1 overdue"), "{text}");
    let (_, fam) = tool(&app, &token, "list_projects", json!({"family": "car restoration"})).await;
    assert!(fam.starts_with("2 projects"), "{fam}");

    let (err, brief) = tool(
        &app,
        &token,
        "get_project_context",
        json!({"project": "Car restoration", "include_family": true}),
    )
    .await;
    assert!(!err, "{brief}");
    for want in [
        "# Car restoration",
        "1972 Beetle",
        "Spawned **Restoration videos**",
        "T-1 Rebuild carburettor",
        "overdue",
        "Brake fluid",
        "## Spawned projects",
        "Film the teardown",
        "Recent activity",
    ] {
        assert!(brief.contains(want), "briefing lacks {want}:\n{brief}");
    }
    let (err, nope) = tool(&app, &token, "get_project_context", json!({"project": "Carr"})).await;
    assert!(err && nope.contains("Did you mean: Car restoration"), "{nope}");

    let (_, found) = tool(&app, &token, "search", json!({"query": "carb"})).await;
    assert!(found.contains("Rebuild carburettor"), "{found}");
    let (_, found) = tool(&app, &token, "search", json!({"query": "DOT"})).await;
    assert!(found.contains("**Brake fluid**"), "{found}");

    let (_, list) = tool(&app, &token, "list_notes", json!({"project": car})).await;
    assert!(list.contains("Brake fluid"), "{list}");
    let (_, note) = tool(&app, &token, "get_note", json!({"note": "brake fluid"})).await;
    assert!(note.contains("Use DOT 4") && note.contains("#howto"), "{note}");

    let (_, res) = rpc(&app, Some(&token), "resources/list", json!({})).await;
    let uri = res["result"]["resources"][0]["uri"].as_str().unwrap().to_string();
    assert!(uri.starts_with("tmp://projects/"), "{res}");
    let (_, read) =
        rpc(&app, Some(&token), "resources/read", json!({"uri": format!("tmp://projects/{car}")})).await;
    assert!(read["result"]["contents"][0]["text"].as_str().unwrap().contains("# Car restoration"), "{read}");

    let (_, prompts) = rpc(&app, Some(&token), "prompts/list", json!({})).await;
    assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(), 2);
    let (_, p) =
        rpc(&app, Some(&token), "prompts/get", json!({"name": "weekly_review", "arguments": {}})).await;
    assert!(p["result"]["messages"][0]["content"]["text"].as_str().unwrap().contains("Overdue (1)"), "{p}");

    // Read-only tokens can't write.
    let (err, text) = tool(&app, &token, "create_task", json!({"project": car, "title": "Nope"})).await;
    assert!(err && text.contains("read-only"), "{text}");
}

#[tokio::test]
async fn writes_are_marked_via_ai() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let token = make_token(&app, &me, "write").await;

    let (err, text) = tool(&app, &token, "create_task", json!({"project": "Car restoration", "title": "Order brake lines", "column": "doing", "due_date": "2030-01-01"})).await;
    assert!(!err, "{text}");
    assert!(text.contains("T-2 Order brake lines"), "{text}");
    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&me), None).await;
    let made =
        tasks.body.as_array().unwrap().iter().find(|t| t["title"] == "Order brake lines").unwrap().clone();
    assert_eq!(made["category"], "in_progress");

    let (err, text) =
        tool(&app, &token, "update_task", json!({"task": "T-2", "project": car, "done": true})).await;
    assert!(!err, "{text}");
    let (err, text) =
        tool(&app, &token, "update_task", json!({"task": made["id"], "column": "Nowhere"})).await;
    assert!(err && text.contains("The columns are"), "{text}");
    let (err, _) = tool(&app, &token, "update_task", json!({"task": "T-2"})).await;
    assert!(err);

    let detail =
        app.call("GET", &format!("/api/v1/tasks/{}", made["id"].as_str().unwrap()), Some(&me), None).await;
    assert_eq!(detail.body["task"]["category"], "done");
    let history = detail.body["activity"].as_array().unwrap();
    assert!(history.iter().all(|a| a["via"] == "ai"), "{history:?}");
    // The web app's own changes stay `web`.
    app.call(
        "PATCH",
        &format!("/api/v1/tasks/{}", made["id"].as_str().unwrap()),
        Some(&me),
        Some(json!({"title": "Order copper lines"})),
    )
    .await;
    let detail =
        app.call("GET", &format!("/api/v1/tasks/{}", made["id"].as_str().unwrap()), Some(&me), None).await;
    assert!(detail.body["activity"].as_array().unwrap().iter().any(|a| a["via"] == "web"));

    let (err, text) = tool(&app, &token, "add_note", json!({"title": "Carb rebuild log", "body": "Jets cleaned.", "project": "Car restoration", "tags": ["log"]})).await;
    assert!(!err && text.contains("Projects/Car restoration/Carb rebuild log.md"), "{text}");
    let (err, text) = tool(&app, &token, "add_note", json!({"body": "Idea: time-lapse the paint job"})).await;
    assert!(!err && text.contains("Inbox/"), "{text}");
    let (err, _) = tool(
        &app,
        &token,
        "append_to_note",
        json!({"note": "Carb rebuild log", "text": "Float height set to 13 mm."}),
    )
    .await;
    assert!(!err);
    let (_, note) = tool(&app, &token, "get_note", json!({"note": "Carb rebuild log"})).await;
    assert!(note.contains("Jets cleaned.\n\nFloat height set to 13 mm."), "{note}");

    let (err, text) = tool(
        &app,
        &token,
        "spawn_project",
        json!({"parent": "Car restoration", "title": "Engine rebuild", "summary": "1600 dual port"}),
    )
    .await;
    assert!(!err, "{text}");
    let detail = app.call("GET", &format!("/api/v1/projects/{car}"), Some(&me), None).await;
    assert!(detail.body["children"].as_array().unwrap().iter().any(|c| c["title"] == "Engine rebuild"));
}

#[tokio::test]
async fn a_token_sees_only_its_owner() {
    let app = TestApp::new().await;
    let owner = app.setup_owner().await;
    let (car, _) = seed(&app, &owner).await;
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
    let token = make_token(&app, &sam, "write").await;

    let (_, list) = tool(&app, &token, "list_projects", json!({})).await;
    assert_eq!(list, "No projects match.");
    let (err, _) = tool(&app, &token, "get_project_context", json!({"project": car})).await;
    assert!(err);
    let (err, _) = tool(&app, &token, "create_task", json!({"project": car, "title": "Sneaky"})).await;
    assert!(err);
    let (_, found) = tool(&app, &token, "search", json!({"query": "carburettor"})).await;
    assert!(!found.contains("Rebuild"), "{found}");
    let (err, _) = tool(&app, &token, "get_note", json!({"note": "Brake fluid"})).await;
    assert!(err);
    let (_, read) =
        rpc(&app, Some(&token), "resources/read", json!({"uri": format!("tmp://projects/{car}")})).await;
    assert!(read.get("error").is_some(), "{read}");
    // Tokens are per person.
    let theirs = app.call("GET", "/api/v1/tokens", Some(&owner), None).await;
    assert_eq!(theirs.body, json!([]));
}

#[tokio::test]
async fn reads_and_adds_database_rows() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car =
        app.call("POST", "/api/v1/projects", Some(&me), Some(json!({"title": "Beetle"}))).await.body["id"]
            .as_str()
            .unwrap()
            .to_string();
    app.call(
        "POST",
        &format!("/api/v1/projects/{car}/tasks"),
        Some(&me),
        Some(json!({"title": "Fit brake lines"})),
    )
    .await;
    let csv = "Part,Cost,Status\r\nKit,42,To order\r\nCalipers,118.5,Ordered\r\nFluid,9.99,To order\r\nPads,30,Ordered\r\n";
    let (status, body) = app
        .call_raw(
            "POST",
            &format!("/api/v1/projects/{car}/databases/import?name=Parts"),
            &me,
            "text/csv",
            csv.into(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let db: Value = serde_json::from_slice(&body).unwrap();
    let db_id = db["database"]["id"].as_str().unwrap().to_string();
    app.call(
        "POST",
        &format!("/api/v1/databases/{db_id}/columns"),
        Some(&me),
        Some(json!({"name": "For task", "kind": "relation", "options": {"target": "task"}})),
    )
    .await;

    let read = make_token(&app, &me, "read").await;
    initialize(&app, &read).await;
    let (err, out) = tool(&app, &read, "query_database", json!({"database": "parts"})).await;
    assert!(!err, "{out}");
    assert!(out.contains("| Part | Cost | Status | For task |"), "{out}");
    assert!(out.contains("| Calipers | 118.5 | Ordered |  |"), "{out}");
    assert!(out.contains("Totals across all 4 rows: Cost 200.49"), "{out}");
    let (err, out) =
        tool(&app, &read, "add_row", json!({"database": "Parts", "values": {"Part": "Hoses"}})).await;
    assert!(err && out.contains("read-only"), "{out}");
    let (_, brief) = tool(&app, &read, "get_project_context", json!({"project": "Beetle"})).await;
    assert!(brief.contains("- Parts: 4 rows"), "{brief}");

    let write = make_token(&app, &me, "write").await;
    let (err, out) = tool(
        &app,
        &write,
        "add_row",
        json!({"database": "Parts", "values": {"Part": "Hoses", "cost": "12.5", "Status": "nope"}}),
    )
    .await;
    assert!(err && out.contains("Status is one of: To order, Ordered"), "{out}");
    let (err, out) = tool(
        &app,
        &write,
        "add_row",
        json!({"database": "Parts", "values": {"Part": "Hoses", "cost": "12.5", "Status": "to order", "For task": "T-1"}}),
    )
    .await;
    assert!(!err, "{out}");
    assert_eq!(out, "Added “Hoses” to Parts in Beetle.");
    let (_, out) = tool(&app, &read, "query_database", json!({"database": "Parts", "search": "hoses"})).await;
    assert!(out.contains("| Hoses | 12.5 | To order | T-1 Fit brake lines |"), "{out}");
}

#[tokio::test]
async fn task_logs_and_linked_notes_over_mcp() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let token = make_token(&app, &me, "write").await;
    let reader = make_token(&app, &me, "read").await;

    let (err, text) = tool(
        &app,
        &token,
        "add_task_comment",
        json!({"task": "T-1", "project": "Car restoration", "text": "Float needle was stuck. Cleaned with carb spray."}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("T-1 Rebuild carburettor (1 comment)"), "{text}");
    let (err, text) =
        tool(&app, &reader, "add_task_comment", json!({"task": "T-1", "project": car, "text": "nope"})).await;
    assert!(err && text.contains("read-only"), "{text}");

    let (err, text) = tool(
        &app,
        &token,
        "link_note_to_task",
        json!({"task": "T-1", "project": car, "note": "brake fluid"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("Linked “Brake fluid” to T-1"), "{text}");
    let (err, text) = tool(
        &app,
        &token,
        "add_note",
        json!({"title": "Carb jets", "body": "Main jet 130.", "task": "T-1", "project": "Car restoration"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("linked to T-1 Rebuild carburettor"), "{text}");

    let (err, text) = tool(&app, &reader, "get_task", json!({"task": "T-1", "project": car})).await;
    assert!(!err, "{text}");
    assert!(text.contains("# T-1 Rebuild carburettor"), "{text}");
    assert!(text.contains("## Log (1)"), "{text}");
    assert!(text.contains("Float needle was stuck") && text.contains("(via AI)"), "{text}");
    assert!(
        text.contains("## Linked notes")
            && text.contains("**Brake fluid**")
            && text.contains("**Carb jets**"),
        "{text}"
    );

    let (_, note) = tool(&app, &reader, "get_note", json!({"note": "Carb jets"})).await;
    assert!(note.contains("Linked tasks:") && note.contains("T-1 Rebuild carburettor"), "{note}");
    let (_, brief) = tool(&app, &reader, "get_project_context", json!({"project": car})).await;
    assert!(brief.contains("1 comment; 2 linked notes"), "{brief}");
    let (_, found) = tool(&app, &reader, "search", json!({"query": "float needle"})).await;
    assert!(found.contains("Rebuild carburettor"), "{found}");

    let (err, text) = tool(
        &app,
        &token,
        "link_note_to_task",
        json!({"task": "T-1", "project": car, "note": "Brake fluid", "unlink": true}),
    )
    .await;
    assert!(!err, "{text}");
    let detail = app.call("GET", "/api/v1/tasks", Some(&me), None).await;
    let t =
        detail.body.as_array().unwrap().iter().find(|t| t["title"] == "Rebuild carburettor").unwrap().clone();
    assert_eq!(t["note_count"], 1);
    let full =
        app.call("GET", &format!("/api/v1/tasks/{}", t["id"].as_str().unwrap()), Some(&me), None).await;
    assert_eq!(full.body["comments"][0]["via"], "ai");
}

#[tokio::test]
async fn list_projects_shows_canvas_groups() {
    let app = TestApp::new().await;
    let cookie = app.setup_owner().await;
    for (title, x) in [("Beetle", 0.0), ("Amp kit", 2000.0)] {
        app.call(
            "POST",
            "/api/v1/projects",
            Some(&cookie),
            Some(json!({"title": title, "position": {"x": x, "y": 0}})),
        )
        .await;
    }
    let view = app.call("GET", "/api/v1/canvases/home", Some(&cookie), None).await;
    let canvas = view.body["canvas"]["id"].as_str().unwrap();
    for (label, x) in [("Cars", -50.0), ("Electronics", 1950.0)] {
        let res = app
            .call(
                "POST",
                &format!("/api/v1/canvases/{canvas}/groups"),
                Some(&cookie),
                Some(json!({"label": label, "x": x, "y": -50, "width": 400, "height": 300})),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED);
    }
    let token = make_token(&app, &cookie, "read").await;
    initialize(&app, &token).await;

    let (_, all) = tool(&app, &token, "list_projects", json!({})).await;
    assert!(
        all.contains("Canvas groups")
            && all.contains("**Cars**: Beetle")
            && all.contains("**Electronics**: Amp kit"),
        "{all}"
    );
    let (_, cars) = tool(&app, &token, "list_projects", json!({"group": "cars"})).await;
    assert!(cars.starts_with("1 projects") && cars.contains("Beetle") && !cars.contains("Amp kit"), "{cars}");
    let (_, none) = tool(&app, &token, "list_projects", json!({"group": "Garden"})).await;
    assert!(none.contains("No group is called") && none.contains("Electronics"), "{none}");
}

#[tokio::test]
async fn the_ai_reads_and_plans_the_calendar() {
    let app = TestApp::new().await;
    let cookie = app.setup_owner().await;
    app.call("POST", "/api/v1/projects", Some(&cookie), Some(json!({"title": "Beetle restoration"}))).await;
    let read = make_token(&app, &cookie, "read").await;
    let write = make_token(&app, &cookie, "write").await;
    initialize(&app, &write).await;

    let (err, text) = tool(
        &app,
        &read,
        "plan_time",
        json!({"project": "Beetle restoration", "start": "2026-11-14T13:00", "end": "17:00"}),
    )
    .await;
    assert!(err && text.contains("read-only"), "{text}");

    // Saturdays from now on, plus work on weekdays.
    let (err, text) = tool(
        &app,
        &write,
        "plan_time",
        json!({"project": "beetle restoration", "title": "Rear brakes", "start": "2026-11-14T13:00",
               "end": "17:00", "repeat": "Sat", "tz": "Europe/London"}),
    )
    .await;
    assert!(!err && text.contains("4h") && text.contains("repeating Sat"), "{text}");
    let (err, text) = tool(
        &app,
        &write,
        "plan_time",
        json!({"kind": "busy", "title": "Work", "start": "2026-11-09T09:00", "minutes": 480, "repeat": "weekdays"}),
    )
    .await;
    assert!(!err && text.contains("Europe/London"), "the zone carries over: {text}");
    let (err, text) =
        tool(&app, &write, "plan_time", json!({"start": "2026-11-14T13:00", "minutes": 60})).await;
    assert!(err && text.contains("project"), "{text}");

    let (err, cal) = tool(&app, &read, "get_calendar", json!({"from": "2026-11-09"})).await;
    assert!(!err, "{cal}");
    assert!(
        cal.contains("Sat 14 Nov 13:00–17:00 (4h, repeats Sat) · Beetle restoration: Rear brakes"),
        "{cal}"
    );
    assert!(
        cal.contains("Busy: Work")
            && cal.contains("Beetle restoration: 4h")
            && cal.contains("Busy (only the user sees this): 40h"),
        "{cal}"
    );

    let (_, brief) = tool(&app, &read, "get_project_context", json!({"project": "Beetle restoration"})).await;
    assert!(brief.contains("## Coming up") && brief.contains("Rear brakes"), "{brief}");
}

#[tokio::test]
async fn columns_projects_notes_and_done_tasks_can_be_changed() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let token = make_token(&app, &me, "write").await;
    initialize(&app, &token).await;

    // A board like Z80's: Done renamed into the In progress category, Done column gone.
    let columns = app.call("GET", &format!("/api/v1/projects/{car}/statuses"), Some(&me), None).await.body;
    let done = columns[2]["id"].as_str().unwrap().to_string();
    app.call(
        "PATCH",
        &format!("/api/v1/statuses/{done}"),
        Some(&me),
        Some(json!({"category": "in_progress"})),
    )
    .await;

    let (err, text) = tool(&app, &token, "get_project_context", json!({"project": "Car restoration"})).await;
    assert!(!err);
    assert!(text.contains("nothing on this board counts as finished"), "{text}");

    let (err, text) = tool(
        &app,
        &token,
        "create_task",
        json!({"project": "Car restoration", "title": "Old job", "done": true}),
    )
    .await;
    assert!(err && text.contains("no column that counts as done"), "{text}");

    let (err, text) = tool(
        &app,
        &token,
        "update_column",
        json!({"project": "Car restoration", "column": "done", "counts_as": "done"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("Done (Done)") && !text.contains("Note:"), "{text}");

    // Insert between, defaulting to the category on its left.
    let (err, text) = tool(
        &app,
        &token,
        "add_column",
        json!({"project": "Car restoration", "name": "Waiting on parts", "after": "Doing"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(
        text.contains("To do (To do), Doing (In progress), Waiting on parts (In progress), Done (Done)"),
        "{text}"
    );
    let (err, text) = tool(
        &app,
        &token,
        "update_column",
        json!({"project": "Car restoration", "column": "Waiting on parts", "after": "start", "counts_as": "blocked"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("in order: Waiting on parts (Blocked), To do"), "{text}");
    let (err, text) = tool(
        &app,
        &token,
        "update_column",
        json!({"project": "Car restoration", "column": "Nope", "name": "x"}),
    )
    .await;
    assert!(err && text.contains("The columns are"), "{text}");

    // A finished task logged after the fact keeps its date.
    let (err, text) = tool(
        &app,
        &token,
        "create_task",
        json!({"project": "Car restoration", "title": "Bled the brakes", "completed_on": "2026-11-01"}),
    )
    .await;
    assert!(!err, "{text}");
    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&me), None).await.body;
    let bled = tasks.as_array().unwrap().iter().find(|t| t["title"] == "Bled the brakes").unwrap();
    assert_eq!(bled["completed_at"], "2026-11-01T12:00:00Z");
    assert_eq!(bled["status_id"], done.as_str());

    // Project summary and status.
    let (err, text) = tool(
        &app,
        &token,
        "update_project",
        json!({"project": "Car restoration", "summary": "Back on the road by spring", "status": "Paused"}),
    )
    .await;
    assert!(!err, "{text}");
    let p = app.call("GET", &format!("/api/v1/projects/{car}"), Some(&me), None).await.body;
    assert_eq!(p["project"]["summary"], "Back on the road by spring");
    assert_eq!(p["project"]["status"], "paused");

    let (err, text) = tool(&app, &token, "create_project", json!({"title": "Garden shed"})).await;
    assert!(!err && text.contains("Started “Garden shed”"), "{text}");

    // Notes: replace one section, then the whole body.
    let res = app
        .call(
            "POST",
            "/api/v1/notes",
            Some(&me),
            Some(json!({"title": "Shed plan", "body": "# Shed plan\n\n## Materials\n\n- wood\n\n### Fixings\n\n- nails\n\n## Steps\n\n1. Dig\n"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let (err, text) = tool(
        &app,
        &token,
        "update_note",
        json!({"note": "Shed plan", "section": "## Materials", "section_body": "- treated timber\n- felt"}),
    )
    .await;
    assert!(!err, "{text}");
    let (_, note) = tool(&app, &token, "get_note", json!({"note": "Shed plan"})).await;
    assert!(note.contains("- treated timber\n- felt\n\n## Steps"), "{note}");
    assert!(!note.contains("nails"), "the subsection goes with its section: {note}");
    let (err, text) = tool(
        &app,
        &token,
        "update_note",
        json!({"note": "Shed plan", "section": "Nope", "section_body": "x"}),
    )
    .await;
    assert!(err && text.contains("no heading"), "{text}");
    let (err, _) = tool(&app, &token, "update_note", json!({"note": "Shed plan", "body": "Scrapped."})).await;
    assert!(!err);
    let (_, note) = tool(&app, &token, "get_note", json!({"note": "Shed plan"})).await;
    assert!(note.contains("Scrapped.") && !note.contains("Steps"), "{note}");

    // The history says what happened to the columns, via AI.
    let dash = app.call("GET", &format!("/api/v1/projects/{car}/dashboard"), Some(&me), None).await.body;
    let kinds: Vec<&str> =
        dash["activity"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap()).collect();
    for k in ["status.created", "status.moved", "status.recategorised"] {
        assert!(kinds.contains(&k), "{kinds:?}");
    }
}

#[tokio::test]
async fn today_and_quick_add_tools() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let read = make_token(&app, &me, "read").await;
    let write = make_token(&app, &me, "write").await;

    let (err, text) = tool(&app, &read, "get_today", json!({"tz": "UTC"})).await;
    assert!(!err, "{text}");
    assert!(
        text.starts_with("# Today, ")
            && text.contains("## Late or due")
            && text.contains("T-1 Rebuild carburettor"),
        "{text}"
    );

    let (err, text) = tool(&app, &read, "quick_add", json!({"text": "Nope"})).await;
    assert!(err && text.contains("read-only"), "{text}");

    let (err, text) = tool(
        &app,
        &write,
        "quick_add",
        json!({"text": "Order gaskets", "project": "Car restoration", "link": "https://example.com/gaskets"}),
    )
    .await;
    assert!(
        !err && text.contains("Added to Car restoration (To do)") && text.contains("Order gaskets"),
        "{text}"
    );
    let (err, text) = tool(&app, &write, "quick_add", json!({"text": "Try resin printing"})).await;
    assert!(!err && text.contains("to the Inbox"), "{text}");
    let (err, text) =
        tool(&app, &write, "quick_add", json!({"text": "x", "project": car, "as": "inbox"})).await;
    assert!(err, "{text}");
    let (_, today) = tool(&app, &read, "get_today", json!({})).await;
    assert!(today.contains("## Inbox, not filed yet (1)") && today.contains("Try resin printing"), "{today}");

    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&me), None).await.body;
    let added = tasks.as_array().unwrap().iter().find(|t| t["title"] == "Order gaskets").unwrap();
    assert_eq!(added["description"], "https://example.com/gaskets");
    let dash = app.call("GET", &format!("/api/v1/projects/{car}/dashboard"), Some(&me), None).await.body;
    assert!(
        dash["activity"].as_array().unwrap().iter().any(|a| a["kind"] == "task.created" && a["via"] == "ai"),
        "{dash}"
    );
}

#[tokio::test]
async fn shopping_list_and_mark_parts() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let call = |method: &'static str, path: String, body: Option<Value>| {
        let app = &app;
        let me = me.clone();
        async move {
            let res = app.call(method, &format!("/api/v1{path}"), Some(&me), body).await;
            assert!(res.status.is_success(), "{path}: {:?}", res.body);
            res.body
        }
    };
    let z80 = call("POST", "/projects".into(), Some(json!({"title": "Z80 Computer"}))).await;
    let z80 = z80["id"].as_str().unwrap().to_string();
    let task =
        call("POST", format!("/projects/{z80}/tasks"), Some(json!({"title": "Solder the board"}))).await;
    let task = task["id"].as_str().unwrap().to_string();
    let db = call(
        "POST",
        format!("/projects/{z80}/databases"),
        Some(json!({"name": "Parts", "template": "parts"})),
    )
    .await;
    let col = |role: &str| {
        db["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["role"] == role || (role == "item" && c["is_title"] == true))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let row = call(
        "POST",
        format!("/databases/{}/rows", db["database"]["id"].as_str().unwrap()),
        Some(json!({
            "values": {(col("item")): "Z80 CPU", (col("qty")): 2, (col("unit_cost")): 6.4, (col("supplier")): "LCSC"},
            "relations": {(col("for_task")): [{"kind": "task", "id": task}]}
        })),
    )
    .await;
    let row = row["id"].as_str().unwrap().to_string();

    let read = make_token(&app, &me, "read").await;
    let write = make_token(&app, &me, "write").await;
    let (err, text) = tool(&app, &read, "get_shopping_list", json!({"tz": "UTC"})).await;
    assert!(!err, "{text}");
    for want in
        ["# Shopping list", "### LCSC · 1 part · 12.80 GBP", "Z80 CPU · ×2", "T-1 Solder the board", &row]
    {
        assert!(text.contains(want), "missing {want}: {text}");
    }
    let (_, today) = tool(&app, &read, "get_today", json!({"tz": "UTC"})).await;
    assert!(today.contains("1 part to buy, 12.80 GBP"), "{today}");

    let (err, _) = tool(&app, &read, "mark_parts", json!({"rows": [row], "stage": "ordered"})).await;
    assert!(err);
    let (err, text) =
        tool(&app, &write, "mark_parts", json!({"rows": [row], "stage": "Arrived", "date": "2026-11-10"}))
            .await;
    assert!(!err, "{text}");
    assert!(
        text.contains("Moved 1 part to arrived (2026-11-10)") && text.contains("All parts are in for T-1"),
        "{text}"
    );
    let detail = call("GET", format!("/tasks/{task}"), None).await;
    assert_eq!(detail["comments"][0]["via"], "ai");
}

#[tokio::test]
async fn attach_file_finish_dates_and_the_weekly_review() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let token = make_token(&app, &me, "write").await;
    let read = make_token(&app, &me, "read").await;
    initialize(&app, &token).await;

    // A file Claude made, on a task, as text.
    let (err, text) = tool(
        &app,
        &token,
        "attach_file",
        json!({"project": "Car restoration", "task": "T-1", "name": "jets.csv",
               "text": "jet,size\nmain,130\nidle,55\n", "caption": "Jet sizes"}),
    )
    .await;
    assert!(!err, "{text}");
    assert!(text.contains("Attached to the task") && text.contains("jets.csv"), "{text}");
    let (_, list) = tool(&app, &token, "list_attachments", json!({"project": "Car restoration"})).await;
    assert!(list.contains("jets.csv") && list.contains("Jet sizes"), "{list}");
    let id = list.split("`").nth(1).unwrap().to_string();
    let (_, shown) = tool(&app, &token, "get_attachment", json!({"id": id})).await;
    assert!(shown.contains("main,130"), "{shown}");

    // Binary as base64 (a 1x1 PNG), on the project and on a note.
    let png =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let (err, text) =
        tool(&app, &token, "attach_file", json!({"project": car, "name": "dot.png", "base64": png})).await;
    assert!(!err && text.contains("Attached to the project"), "{text}");
    let (err, text) = tool(
        &app,
        &token,
        "attach_file",
        json!({"note": "Brake fluid", "name": "bleed-order.md", "text": "1. Rear right\n"}),
    )
    .await;
    assert!(!err && text.contains("Attached to the note"), "{text}");

    // Mistakes are explained; read-only connections can't attach.
    let (err, text) = tool(
        &app,
        &token,
        "attach_file",
        json!({"project": car, "name": "x.txt", "text": "a", "base64": "YQ=="}),
    )
    .await;
    assert!(err && text.contains("one of the two"), "{text}");
    let (err, text) =
        tool(&app, &token, "attach_file", json!({"project": car, "name": "x.bin", "base64": "not base64!"}))
            .await;
    assert!(err && text.contains("isn't valid base64"), "{text}");
    let (err, text) = tool(&app, &token, "attach_file", json!({"name": "x.txt", "text": "a"})).await;
    assert!(err && text.contains("Attach to one thing"), "{text}");
    let (err, text) =
        tool(&app, &read, "attach_file", json!({"project": car, "name": "x.txt", "text": "a"})).await;
    assert!(err && text.contains("read-only"), "{text}");

    // The history shows the file as added via AI.
    let dash = app.call("GET", &format!("/api/v1/projects/{car}/dashboard"), Some(&me), None).await.body;
    let added =
        dash["activity"].as_array().unwrap().iter().find(|a| a["kind"] == "attachment.added").unwrap();
    assert_eq!(added["via"], "ai", "{added}");

    // Finish a task on the day it was really done, then correct the date.
    let (err, text) = tool(
        &app,
        &token,
        "update_task",
        json!({"project": "Car restoration", "task": "T-1", "completed_on": "2026-11-02"}),
    )
    .await;
    assert!(!err, "{text}");
    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&me), None).await.body;
    let carb = tasks.as_array().unwrap().iter().find(|t| t["number"] == 1).unwrap();
    assert_eq!(carb["category"], "done");
    assert_eq!(carb["completed_at"], "2026-11-02T12:00:00Z");
    let (err, text) = tool(
        &app,
        &token,
        "update_task",
        json!({"project": "Car restoration", "task": "T-1", "completed_on": "2026-11-01"}),
    )
    .await;
    assert!(!err, "{text}");
    let tasks = app.call("GET", &format!("/api/v1/projects/{car}/tasks"), Some(&me), None).await.body;
    let carb = tasks.as_array().unwrap().iter().find(|t| t["number"] == 1).unwrap();
    assert_eq!(carb["completed_at"], "2026-11-01T12:00:00Z");
    let (err, text) = tool(
        &app,
        &token,
        "update_task",
        json!({"project": "Car restoration", "task": "T-1", "completed_on": "2999-01-01"}),
    )
    .await;
    assert!(err && text.contains("future"), "{text}");
    let (err, text) = tool(
        &app,
        &token,
        "update_task",
        json!({"project": "Car restoration", "task": "T-1", "column": "Doing", "completed_on": "2026-11-01"}),
    )
    .await;
    assert!(err && text.contains("doesn't count as done"), "{text}");

    // Weekly review: a task in progress with nothing logged for a fortnight is stuck,
    // one due in a few days is listed as coming up.
    for (title, column) in [("Sand the wings", "Doing"), ("Order seals", "Doing")] {
        let (err, text) =
            tool(&app, &token, "create_task", json!({"project": car, "title": title, "column": column}))
                .await;
        assert!(!err, "{text}");
    }
    let soon = (chrono::Utc::now().date_naive() + chrono::Duration::days(3)).to_string();
    let (err, text) =
        tool(&app, &token, "update_task", json!({"project": car, "task": "T-3", "due_date": soon})).await;
    assert!(!err, "{text}");
    let old = (chrono::Utc::now() - chrono::Duration::days(14)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    sqlx::query("UPDATE tasks SET updated_at = ? WHERE title = 'Sand the wings'")
        .bind(&old)
        .execute(&app.state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE activity SET created_at = ? WHERE task_id = (SELECT id FROM tasks WHERE title = 'Sand the wings')")
        .bind(&old)
        .execute(&app.state.db)
        .await
        .unwrap();
    let (err, review) = tool(&app, &read, "weekly_review", json!({})).await;
    assert!(!err, "{review}");
    let stuck = review.split("## Stuck").nth(1).unwrap().split("\n## ").next().unwrap();
    assert!(stuck.contains("Sand the wings") && !stuck.contains("Order seals"), "{review}");
    let due = review.split("## Due in the next 7 days").nth(1).unwrap().split("\n## ").next().unwrap();
    assert!(due.contains("Order seals"), "{review}");
    assert!(review.contains("suggest updates for me to approve"), "{review}");
}
