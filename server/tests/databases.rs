//! Project databases: columns, rows, views, relations to tasks, sharing,
//! and CSV import and export.

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

fn names(rows: &Value, title: &str) -> Vec<String> {
    rows.as_array().unwrap().iter().map(|r| r["values"][title].as_str().unwrap_or("").to_string()).collect()
}

#[tokio::test]
async fn columns_rows_views_and_task_links() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Beetle"}))).await);
    let task = id(&ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Fit brake lines"})),
    )
    .await);

    let db =
        ok(&app, "POST", &format!("/projects/{car}/databases"), &me, Some(json!({"name": "Parts list"})))
            .await;
    let db_id = id(&db["database"]);
    let title = db["columns"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(db["columns"][0]["is_title"], true);
    assert_eq!(db["views"][0]["name"], "All");

    // Same name again in the same project is refused.
    let dup = app
        .call(
            "POST",
            &format!("/api/v1/projects/{car}/databases"),
            Some(&me),
            Some(json!({"name": "parts LIST"})),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let cost = id(&ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/columns"),
        &me,
        Some(json!({"name": "Cost", "kind": "currency", "options": {"currency": "gbp"}})),
    )
    .await);
    let status = ok(&app, "POST", &format!("/databases/{db_id}/columns"), &me,
        Some(json!({"name": "Status", "kind": "select", "options": {"choices": [{"name": "To order"}, {"name": "Ordered"}]}}))).await;
    let status_id = id(&status);
    let to_order = status["options"]["choices"][0]["id"].as_str().unwrap().to_string();
    let ordered = status["options"]["choices"][1]["id"].as_str().unwrap().to_string();
    assert_eq!(status["options"]["choices"][0]["color"], "slot-1");
    let for_task = id(&ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/columns"),
        &me,
        Some(json!({"name": "For task", "kind": "relation", "options": {"target": "task"}})),
    )
    .await);

    let bad = app
        .call(
            "POST",
            &format!("/api/v1/databases/{db_id}/rows"),
            Some(&me),
            Some(json!({"values": {status_id.clone(): "nope"}})),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let kit = ok(&app, "POST", &format!("/databases/{db_id}/rows"), &me, Some(json!({
        "values": {title.clone(): "Copper brake line kit", cost.clone(): "42", status_id.clone(): to_order},
        "relations": {for_task.clone(): [{"kind": "task", "id": task}]}
    }))).await;
    assert_eq!(kit["values"][&cost], 42.0);
    assert_eq!(kit["relations"][&for_task][0]["title"], "T-1 Fit brake lines");
    ok(&app, "POST", &format!("/databases/{db_id}/rows"), &me, Some(json!({
        "values": {title.clone(): "Front calipers", cost.clone(): 118.5, status_id.clone(): ordered.clone()},
        "relations": {for_task.clone(): [{"kind": "task", "id": task}]}
    }))).await;
    let fluid = ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/rows"),
        &me,
        Some(json!({
            "values": {title.clone(): "DOT 4 fluid", cost.clone(): 9.99, status_id.clone(): to_order}
        })),
    )
    .await;

    // A view: still to order, cheapest first.
    let view = ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/views"),
        &me,
        Some(json!({
            "name": "To order", "kind": "table",
            "config": {"filters": [{"column_id": status_id, "op": "is", "value": to_order}],
                       "sorts": [{"column_id": cost, "desc": false}]}
        })),
    )
    .await;
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows?view={}", id(&view)), &me, None).await;
    assert_eq!(names(&rows, &title), ["DOT 4 fluid", "Copper brake line kit"]);
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows?q=calip"), &me, None).await;
    assert_eq!(names(&rows, &title), ["Front calipers"]);

    // A board picks the select column on its own.
    let board = ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/views"),
        &me,
        Some(json!({"name": "Board", "kind": "board"})),
    )
    .await;
    assert_eq!(board["config"]["group_by"], status_id);

    // The task sees the rows that link to it.
    let linked = ok(&app, "GET", &format!("/tasks/{task}/rows"), &me, None).await;
    assert_eq!(linked[0]["database"]["name"], "Parts list");
    assert_eq!(linked[0]["column_id"], for_task);
    assert_eq!(names(&linked[0]["rows"], &title), ["Copper brake line kit", "Front calipers"]);

    // Edit a cell, link another row, then move it first.
    let fluid_id = id(&fluid);
    let moved = ok(
        &app,
        "PATCH",
        &format!("/db-rows/{fluid_id}"),
        &me,
        Some(json!({
            "values": {status_id.clone(): ordered},
            "relations": {for_task.clone(): [{"kind": "task", "id": task}]},
            "after_id": null
        })),
    )
    .await;
    assert_eq!(moved["values"][&status_id], ordered);
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    assert_eq!(names(&rows, &title)[0], "DOT 4 fluid");

    // Turning the select into text keeps the names.
    ok(&app, "PATCH", &format!("/db-columns/{status_id}"), &me, Some(json!({"kind": "text"}))).await;
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    assert_eq!(rows[0]["values"][&status_id], "Ordered");
    // And back: the names become choices again.
    let back =
        ok(&app, "PATCH", &format!("/db-columns/{status_id}"), &me, Some(json!({"kind": "select"}))).await;
    assert_eq!(back["options"]["choices"].as_array().unwrap().len(), 2);

    // The name column stays.
    let res = app.call("DELETE", &format!("/api/v1/db-columns/{title}"), Some(&me), None).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Deleting the task drops the links to it.
    ok(&app, "DELETE", &format!("/tasks/{task}"), &me, None).await;
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    assert!(rows.as_array().unwrap().iter().all(|r| r["relations"].get(&for_task).is_none()));

    // Removing a column cleans it out of the rows and the views.
    ok(&app, "DELETE", &format!("/db-columns/{cost}"), &me, None).await;
    let detail = ok(&app, "GET", &format!("/databases/{db_id}"), &me, None).await;
    let to_order_view = detail["views"].as_array().unwrap().iter().find(|v| v["name"] == "To order").unwrap();
    assert_eq!(to_order_view["config"]["sorts"], json!([]));
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    assert!(rows[0]["values"].get(&cost).is_none());
}

#[tokio::test]
async fn databases_follow_project_sharing() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let invite = ok(&app, "POST", "/invites", &logan, Some(json!({}))).await;
    let sam = app
        .call("POST", "/api/v1/auth/accept-invite", None, Some(json!({
            "token": invite["token"], "name": "Sam", "email": "sam@example.com", "password": "a long password"
        })))
        .await
        .cookie
        .unwrap();
    let sam_id =
        ok(&app, "GET", "/auth/status", &sam, None).await["user"]["id"].as_str().unwrap().to_string();
    let car = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Beetle"}))).await);
    let db =
        ok(&app, "POST", &format!("/projects/{car}/databases"), &logan, Some(json!({"name": "Parts"}))).await;
    let db_id = id(&db["database"]);
    let title = db["columns"][0]["id"].as_str().unwrap().to_string();
    let who = id(&ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/columns"),
        &logan,
        Some(json!({"name": "Who", "kind": "person"})),
    )
    .await);

    // Not shared: nothing to see.
    let res = app.call("GET", &format!("/api/v1/databases/{db_id}"), Some(&sam), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(ok(&app, "GET", "/databases", &sam, None).await, json!([]));
    // And Sam can't be picked as a person yet.
    let res = app
        .call(
            "POST",
            &format!("/api/v1/databases/{db_id}/rows"),
            Some(&logan),
            Some(json!({"values": {who.clone(): sam_id}})),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // View only: read, but not write.
    let share = ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": sam_id, "project_id": car, "role": "viewer"})),
    )
    .await;
    let detail = ok(&app, "GET", &format!("/databases/{db_id}"), &sam, None).await;
    assert_eq!(detail["can_edit"], false);
    assert_eq!(ok(&app, "GET", "/databases", &sam, None).await[0]["name"], "Parts");
    let res = app
        .call(
            "POST",
            &format!("/api/v1/databases/{db_id}/rows"),
            Some(&sam),
            Some(json!({"values": {title.clone(): "Spark plugs"}})),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // Editor: can add rows, and can be assigned.
    ok(&app, "PATCH", &format!("/shares/{}", id(&share)), &logan, Some(json!({"role": "editor"}))).await;
    let row = ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/rows"),
        &sam,
        Some(json!({
            "values": {title.clone(): "Spark plugs", who.clone(): sam_id}
        })),
    )
    .await;
    assert_eq!(row["values"][&who], sam_id);

    // A relation to one of Logan's private projects is refused for Sam.
    let private = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Secret"}))).await);
    let rel = id(&ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/columns"),
        &sam,
        Some(json!({"name": "Project", "kind": "relation", "options": {"target": "project"}})),
    )
    .await);
    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/db-rows/{}", id(&row)),
            Some(&sam),
            Some(json!({"relations": {rel.clone(): [{"kind": "project", "id": private}]}})),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    // Logan links it; Sam's view of the row leaves it out.
    ok(&app, "PATCH", &format!("/db-rows/{}", id(&row)), &logan,
        Some(json!({"relations": {rel.clone(): [{"kind": "project", "id": private}, {"kind": "project", "id": car}]}}))).await;
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &sam, None).await;
    assert_eq!(rows[0]["relations"][&rel].as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["relations"][&rel][0]["title"], "Beetle");
}

#[tokio::test]
async fn csv_import_and_export() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Beetle"}))).await);
    let csv = "\u{feff}Part,Cost,Status,Tags,Ordered on,Done,Link\r\n\
        \"Kit, copper\",£42.00,To order,\"brakes, lines\",\"November 6, 2026\",No,https://example.com/kit\r\n\
        Calipers,118.50,Ordered,brakes,2026-11-07,Yes,\r\n\
        Fluid,9.99,To order,fluids,,No,\r\n\
        Pads,30,Ordered,brakes,,No,\r\n";
    let (status, body) = app
        .call_raw(
            "POST",
            &format!("/api/v1/projects/{car}/databases/import?name=Parts"),
            &me,
            "text/csv",
            csv.into(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&body));
    let db: Value = serde_json::from_slice(&body).unwrap();
    let kinds: Vec<&str> =
        db["columns"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["text", "number", "select", "multi_select", "date", "checkbox", "url"]);
    assert_eq!(db["database"]["row_count"], 4);
    let db_id = id(&db["database"]);
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    let date_col = db["columns"][4]["id"].as_str().unwrap();
    assert_eq!(rows[0]["values"][date_col], "2026-11-06");

    let (status, zip) =
        app.call_raw("GET", &format!("/api/v1/databases/{db_id}/export"), &me, "text/plain", vec![]).await;
    assert_eq!(status, StatusCode::OK);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut names: Vec<String> =
        (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        ["Parts.csv", "Parts/Calipers.md", "Parts/Fluid.md", "Parts/Kit, copper.md", "Parts/Pads.md"]
    );
    let mut text = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("Parts.csv").unwrap(), &mut text).unwrap();
    assert!(
        text.contains("\"Kit, copper\",42,To order,\"brakes, lines\",2026-11-06,No,https://example.com/kit"),
        "{text}"
    );
    let mut md = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("Parts/Calipers.md").unwrap(), &mut md).unwrap();
    assert!(
        md.contains("\"Cost\": 118.5\n")
            && md.contains("\"Tags\": [\"brakes\"]\n")
            && md.contains("\"Done\": true\n"),
        "{md}"
    );
}
