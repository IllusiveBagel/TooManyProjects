//! Phase 4d: the Parts template, the Shopping list, spend and tasks that wait
//! on parts.

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

/// Role to column id.
fn roles(db: &Value) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for c in db["columns"].as_array().unwrap() {
        let key = c["role"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| c["name"].as_str().unwrap().to_lowercase());
        out.insert(key, id(c));
    }
    out
}

#[allow(clippy::too_many_arguments)]
async fn add_part(
    app: &TestApp,
    me: &str,
    db: &str,
    r: &std::collections::HashMap<String, String>,
    item: &str,
    qty: f64,
    cost: f64,
    supplier: &str,
    task: Option<&str>,
) -> String {
    let mut values = serde_json::Map::new();
    values.insert(r["item"].clone(), json!(item));
    values.insert(r["qty"].clone(), json!(qty));
    values.insert(r["unit_cost"].clone(), json!(cost));
    values.insert(r["supplier"].clone(), json!(supplier));
    let mut relations = serde_json::Map::new();
    if let Some(t) = task {
        relations.insert(r["for_task"].clone(), json!([{"kind": "task", "id": t}]));
    }
    id(&ok(
        app,
        "POST",
        &format!("/databases/{db}/rows"),
        me,
        Some(json!({"values": values, "relations": relations})),
    )
    .await)
}

#[tokio::test]
async fn parts_template_shopping_list_and_spend() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let supra = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "MkIII Supra Build"}))).await);
    let videos = id(&ok(
        &app,
        "POST",
        "/projects",
        &me,
        Some(json!({"title": "Supra YouTube videos", "spawned_from": supra})),
    )
    .await);
    let z80 = id(&ok(&app, "POST", "/projects", &me, Some(json!({"title": "Z80 Computer"}))).await);
    let fuel = id(&ok(
        &app,
        "POST",
        &format!("/projects/{supra}/tasks"),
        &me,
        Some(json!({"title": "Remove fuel tank"})),
    )
    .await);

    // Currency is set for the install before the template is used.
    let s = ok(&app, "GET", "/instance-settings", &me, None).await;
    assert_eq!(s["currency"], "GBP");

    let db = ok(
        &app,
        "POST",
        &format!("/projects/{supra}/databases"),
        &me,
        Some(json!({"name": "Parts List", "template": "parts"})),
    )
    .await;
    assert_eq!(db["database"]["template"], "parts");
    let names: Vec<&str> =
        db["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "Item",
            "Qty",
            "Unit cost",
            "Supplier",
            "Link",
            "Status",
            "For task",
            "Ordered on",
            "Arrived on",
            "Notes"
        ]
    );
    let views: Vec<&str> =
        db["views"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert_eq!(views, ["All", "By status", "To buy", "Gallery"]);
    let r = roles(&db);
    let db_id = id(&db["database"]);
    let status = db["columns"].as_array().unwrap().iter().find(|c| c["role"] == "status").unwrap().clone();
    let stages: Vec<&str> =
        status["options"]["choices"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(stages, ["need", "ordered", "shipped", "arrived", "fitted"]);

    // An unknown template is refused.
    let bad = app
        .call(
            "POST",
            &format!("/api/v1/projects/{supra}/databases"),
            Some(&me),
            Some(json!({"name": "X", "template": "recipes"})),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Template columns can be renamed but keep their type.
    ok(&app, "PATCH", &format!("/db-columns/{}", r["supplier"]), &me, Some(json!({"name": "Shop"}))).await;
    let locked = app
        .call("PATCH", &format!("/api/v1/db-columns/{}", r["qty"]), Some(&me), Some(json!({"kind": "text"})))
        .await;
    assert_eq!(locked.status, StatusCode::BAD_REQUEST);
    assert_eq!(locked.body["code"], "template_column");

    // New parts start in Need.
    let pump =
        add_part(&app, &me, &db_id, &r, "Walbro 255 fuel pump", 1.0, 89.0, "Amazon", Some(&fuel)).await;
    let seal =
        add_part(&app, &me, &db_id, &r, "Fuel tank sender seal", 1.0, 12.4, "Supra Store", Some(&fuel)).await;
    let clips = add_part(&app, &me, &db_id, &r, "Interior clips", 2.0, 4.995, "amazon ", None).await;

    let zdb = ok(
        &app,
        "POST",
        &format!("/projects/{z80}/databases"),
        &me,
        Some(json!({"name": "Parts", "template": "parts"})),
    )
    .await;
    let zr = roles(&zdb);
    let zdb_id = id(&zdb["database"]);
    let cpu = add_part(&app, &me, &zdb_id, &zr, "Z80 CPU", 1.0, 6.4, "LCSC", None).await;
    let sram = add_part(&app, &me, &zdb_id, &zr, "62256 SRAM", 2.0, 3.1, "LCSC", None).await;

    // A part on the spawned project counts in the family total.
    let vdb = ok(
        &app,
        "POST",
        &format!("/projects/{videos}/databases"),
        &me,
        Some(json!({"name": "Kit", "template": "parts"})),
    )
    .await;
    let vr = roles(&vdb);
    let mic = add_part(&app, &me, &id(&vdb["database"]), &vr, "Lav mic", 1.0, 12.0, "", None).await;
    ok(
        &app,
        "POST",
        "/shopping/mark",
        &me,
        Some(json!({"row_ids": [mic], "stage": "ordered", "date": "2026-11-01"})),
    )
    .await;

    let list = ok(&app, "GET", "/shopping?tz=UTC", &me, None).await;
    let groups: Vec<(&str, usize)> = list["need"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["supplier"].as_str().unwrap(), g["parts"].as_array().unwrap().len()))
        .collect();
    assert_eq!(groups, [("Amazon", 2), ("LCSC", 2), ("Supra Store", 1)]);
    assert_eq!(list["need"][0]["total"], json!(98.99));
    assert_eq!(list["to_buy"], json!(123.99));
    assert_eq!(list["project_count"], 2);
    assert_eq!(list["on_the_way_total"], json!(12.0));
    assert_eq!(list["need"][0]["parts"][0]["tasks"][0]["title"], "Remove fuel tank");

    // The task waits on two parts.
    let task = ok(&app, "GET", &format!("/tasks/{fuel}"), &me, None).await;
    assert_eq!(task["task"]["parts_total"], 2);
    assert_eq!(task["task"]["parts_waiting"], 2);

    // Mark ordered moves several parts at once and dates them.
    let res = ok(
        &app,
        "POST",
        "/shopping/mark",
        &me,
        Some(json!({"row_ids": [cpu, sram, seal], "stage": "ordered", "date": "2026-11-10"})),
    )
    .await;
    assert_eq!(res["moved"], 3);
    let rows = ok(&app, "GET", &format!("/databases/{zdb_id}/rows"), &me, None).await;
    assert_eq!(rows[0]["values"][&zr["status"]], "ordered");
    assert_eq!(rows[0]["values"][&zr["ordered_on"]], "2026-11-10");

    let spend = ok(&app, "GET", &format!("/projects/{supra}/spend"), &me, None).await;
    assert_eq!(spend["currency"], "GBP");
    assert_eq!(spend["part_count"], 3);
    assert_eq!(spend["spent"], json!(12.4));
    assert_eq!(spend["to_buy"], json!(98.99));
    assert_eq!(spend["need_count"], 2);
    assert_eq!(spend["family"]["spent"], json!(24.4));
    assert_eq!(spend["family"]["projects"], json!(["Supra YouTube videos"]));
    assert_eq!(spend["waiting"][0]["title"], "Remove fuel tank");
    assert_eq!(spend["waiting"][0]["need"], 1);
    assert_eq!(spend["waiting"][0]["ordered"], 1);

    // One part arriving isn't enough; the last one adds a comment to the task.
    ok(
        &app,
        "PATCH",
        &format!("/db-rows/{seal}"),
        &me,
        Some(json!({"values": {(r["status"].clone()): "arrived"}})),
    )
    .await;
    let task = ok(&app, "GET", &format!("/tasks/{fuel}"), &me, None).await;
    assert_eq!(task["task"]["parts_waiting"], 1);
    assert_eq!(task["comments"].as_array().map(|c| c.len()).unwrap_or(0), 0, "{task}");
    let res = ok(
        &app,
        "POST",
        "/shopping/mark",
        &me,
        Some(json!({"row_ids": [pump], "stage": "arrived", "tz": "UTC"})),
    )
    .await;
    assert_eq!(res["tasks_ready"][0]["title"], "Remove fuel tank");
    let task = ok(&app, "GET", &format!("/tasks/{fuel}"), &me, None).await;
    assert_eq!(task["task"]["parts_waiting"], 0);
    let comments = ok(&app, "GET", &format!("/tasks/{fuel}"), &me, None).await["comments"].clone();
    let bodies: Vec<&str> =
        comments.as_array().unwrap().iter().map(|c| c["body"].as_str().unwrap()).collect();
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    assert!(bodies[0].starts_with("Parts arrived: all 2"), "{bodies:?}");
    // Fitting it later doesn't say so again.
    ok(&app, "POST", "/shopping/mark", &me, Some(json!({"row_ids": [pump], "stage": "fitted"}))).await;
    let task = ok(&app, "GET", &format!("/tasks/{fuel}"), &me, None).await;
    assert_eq!(task["comments"].as_array().unwrap().len(), 1);

    // The pump arrived today, so it shows under Arrived this month, with both dates.
    let list = ok(&app, "GET", "/shopping?tz=UTC", &me, None).await;
    let arrived: Vec<&str> =
        list["arrived"].as_array().unwrap().iter().map(|p| p["item"].as_str().unwrap()).collect();
    assert!(arrived.contains(&"Walbro 255 fuel pump"), "{arrived:?}");
    assert!(list["arrived"][0]["ordered_on"].is_string());

    // Today counts what's left to buy.
    let today = ok(&app, "GET", "/today?tz=UTC", &me, None).await;
    assert_eq!(today["shopping"]["need_count"], 1);
    assert_eq!(today["shopping"]["to_buy"], json!(9.99));

    // Rows outside Parts databases can't be marked.
    let plain =
        ok(&app, "POST", &format!("/projects/{z80}/databases"), &me, Some(json!({"name": "Notes"}))).await;
    let row =
        id(&ok(&app, "POST", &format!("/databases/{}/rows", id(&plain["database"])), &me, Some(json!({})))
            .await);
    let res = app
        .call("POST", "/api/v1/shopping/mark", Some(&me), Some(json!({"row_ids": [row], "stage": "ordered"})))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let _ = clips;

    // The owner sets the currency, and Parts databases follow.
    ok(&app, "PATCH", "/instance-settings", &me, Some(json!({"currency": "usd"}))).await;
    let db = ok(&app, "GET", &format!("/databases/{db_id}"), &me, None).await;
    let cost = db["columns"].as_array().unwrap().iter().find(|c| c["role"] == "unit_cost").unwrap();
    assert_eq!(cost["options"]["currency"], "USD");
    assert_eq!(ok(&app, "GET", "/shopping", &me, None).await["currency"], "USD");
    let bad =
        app.call("PATCH", "/api/v1/instance-settings", Some(&me), Some(json!({"currency": "dollars"}))).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}
