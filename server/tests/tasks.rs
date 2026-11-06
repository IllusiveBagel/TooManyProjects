//! Columns, labels, tasks, checklists, the global board and dashboards.

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

async fn project(app: &TestApp, cookie: &str, title: &str) -> String {
    ok(app, "POST", "/projects", cookie, Some(json!({ "title": title }))).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn titles(tasks: &Value) -> Vec<&str> {
    tasks.as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect()
}

fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap()
}

#[tokio::test]
async fn tasks_move_between_columns_and_finish() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car restoration").await;

    let columns = ok(&app, "GET", &format!("/projects/{car}/statuses"), &me, None).await;
    let names: Vec<_> = columns.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["To do", "Doing", "Done"]);
    let (todo, doing, done) = (id(&columns[0]), id(&columns[1]), id(&columns[2]));

    let a =
        ok(&app, "POST", &format!("/projects/{car}/tasks"), &me, Some(json!({"title": "Strip paint"}))).await;
    assert_eq!(a["number"], 1);
    assert_eq!(a["status_id"], todo, "new tasks go in the first column");
    assert_eq!(a["category"], "todo");
    let b = ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Order seals", "priority": "high", "due_date": "2020-01-01"})),
    )
    .await;
    assert_eq!(b["number"], 2);
    assert_eq!(b["overdue"], true);
    let c = ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Weld floor", "at_top": true})),
    )
    .await;
    let list = ok(&app, "GET", &format!("/projects/{car}/tasks"), &me, None).await;
    assert_eq!(titles(&list), ["Weld floor", "Strip paint", "Order seals"]);

    // Reorder within the column: put Weld floor after Order seals.
    ok(&app, "PATCH", &format!("/tasks/{}", id(&c)), &me, Some(json!({"after_id": id(&b)}))).await;
    let list = ok(&app, "GET", &format!("/projects/{car}/tasks"), &me, None).await;
    assert_eq!(titles(&list), ["Strip paint", "Order seals", "Weld floor"]);

    // Move to Doing at the top, then to Done.
    let moved = ok(
        &app,
        "PATCH",
        &format!("/tasks/{}", id(&b)),
        &me,
        Some(json!({"status_id": doing, "after_id": null})),
    )
    .await;
    assert_eq!(moved["category"], "in_progress");
    assert_eq!(moved["completed_at"], Value::Null);
    let finished =
        ok(&app, "PATCH", &format!("/tasks/{}", id(&b)), &me, Some(json!({"status_id": done}))).await;
    assert_eq!(finished["category"], "done");
    assert!(finished["completed_at"].is_string());
    assert_eq!(finished["overdue"], false, "done tasks are never late");

    // Moving back out clears completed_at.
    let reopened =
        ok(&app, "PATCH", &format!("/tasks/{}", id(&b)), &me, Some(json!({"status_id": todo}))).await;
    assert_eq!(reopened["completed_at"], Value::Null);

    // The task's history is kept.
    let detail = ok(&app, "GET", &format!("/tasks/{}", id(&b)), &me, None).await;
    let kinds: Vec<_> =
        detail["activity"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["task.moved", "task.completed", "task.moved", "task.created"]);
    assert_eq!(detail["activity"][1]["payload"]["to"], "Done");

    // Project stats follow.
    let p = ok(&app, "GET", &format!("/projects/{car}"), &me, None).await;
    assert_eq!(p["project"]["stats"]["total_tasks"], 3);

    // Edits and validation.
    let edited = ok(
        &app,
        "PATCH",
        &format!("/tasks/{}", id(&a)),
        &me,
        Some(json!({"title": "Strip all paint", "description": "Wire wheel first", "priority": "urgent"})),
    )
    .await;
    assert_eq!(edited["title"], "Strip all paint");
    assert_eq!(edited["priority"], "urgent");
    let cleared =
        ok(&app, "PATCH", &format!("/tasks/{}", id(&a)), &me, Some(json!({"priority": null}))).await;
    assert_eq!(cleared["priority"], Value::Null);
    assert_eq!(cleared["description"], "Wire wheel first");
    assert_eq!(
        status(&app, "PATCH", &format!("/tasks/{}", id(&a)), &me, Some(json!({"priority": "asap"}))).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(&app, "POST", &format!("/projects/{car}/tasks"), &me, Some(json!({"title": "  "}))).await,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        status(&app, "DELETE", &format!("/tasks/{}", id(&a)), &me, None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(status(&app, "GET", &format!("/tasks/{}", id(&a)), &me, None).await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn columns_can_be_added_moved_and_removed() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car").await;
    let columns = ok(&app, "GET", &format!("/projects/{car}/statuses"), &me, None).await;
    let (todo, doing, done) = (id(&columns[0]), id(&columns[1]), id(&columns[2]));

    let parts = ok(
        &app,
        "POST",
        &format!("/projects/{car}/statuses"),
        &me,
        Some(json!({"name": "Waiting on parts", "category": "blocked"})),
    )
    .await;
    // Move it between Doing and Done.
    ok(&app, "PATCH", &format!("/statuses/{}", id(&parts)), &me, Some(json!({"after_id": doing}))).await;
    let names: Vec<_> = ok(&app, "GET", &format!("/projects/{car}/statuses"), &me, None)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["To do", "Doing", "Waiting on parts", "Done"]);

    // A column's category decides whether its tasks count as done.
    let t = ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Wait for seals", "status_id": id(&parts)})),
    )
    .await;
    ok(&app, "PATCH", &format!("/statuses/{}", id(&parts)), &me, Some(json!({"category": "done"}))).await;
    let t2 = ok(&app, "GET", &format!("/tasks/{}", id(&t)), &me, None).await;
    assert!(t2["task"]["completed_at"].is_string());

    // A column with tasks needs somewhere to send them.
    assert_eq!(
        status(&app, "DELETE", &format!("/statuses/{}", id(&parts)), &me, None).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/statuses/{}?move_to={todo}", id(&parts)), &me, None).await,
        StatusCode::NO_CONTENT
    );
    let t3 = ok(&app, "GET", &format!("/tasks/{}", id(&t)), &me, None).await;
    assert_eq!(t3["task"]["status_id"], todo);
    assert_eq!(t3["task"]["completed_at"], Value::Null);

    assert_eq!(status(&app, "DELETE", &format!("/statuses/{done}"), &me, None).await, StatusCode::NO_CONTENT);
    assert_eq!(
        status(&app, "DELETE", &format!("/statuses/{doing}"), &me, None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/statuses/{todo}?move_to={todo}"), &me, None).await,
        StatusCode::CONFLICT,
        "the last column stays"
    );
    assert_eq!(
        status(
            &app,
            "POST",
            &format!("/projects/{car}/statuses"),
            &me,
            Some(json!({"name": "X", "category": "nope"}))
        )
        .await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn labels_and_checklists() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car").await;
    let other = project(&app, &me, "Videos").await;

    let rust =
        ok(&app, "POST", &format!("/projects/{car}/labels"), &me, Some(json!({"name": "Bodywork"}))).await;
    let engine = ok(
        &app,
        "POST",
        &format!("/projects/{car}/labels"),
        &me,
        Some(json!({"name": "Engine", "color": "slot-3"})),
    )
    .await;
    assert_eq!(
        status(&app, "POST", &format!("/projects/{car}/labels"), &me, Some(json!({"name": "bodywork"})))
            .await,
        StatusCode::CONFLICT
    );
    let foreign =
        ok(&app, "POST", &format!("/projects/{other}/labels"), &me, Some(json!({"name": "Edit"}))).await;

    let t = ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Fix sills", "label_ids": [id(&rust), id(&engine)]})),
    )
    .await;
    let names: Vec<_> = t["labels"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Bodywork", "Engine"]);
    assert_eq!(
        status(&app, "PATCH", &format!("/tasks/{}", id(&t)), &me, Some(json!({"label_ids": [id(&foreign)]})))
            .await,
        StatusCode::BAD_REQUEST
    );
    let t = ok(&app, "PATCH", &format!("/tasks/{}", id(&t)), &me, Some(json!({"label_ids": [id(&engine)]})))
        .await;
    assert_eq!(t["labels"].as_array().unwrap().len(), 1);

    let a =
        ok(&app, "POST", &format!("/tasks/{}/checklist", id(&t)), &me, Some(json!({"text": "Cut out rust"})))
            .await;
    ok(&app, "POST", &format!("/tasks/{}/checklist", id(&t)), &me, Some(json!({"text": "Weld patch"}))).await;
    ok(&app, "PATCH", &format!("/checklist/{}", id(&a)), &me, Some(json!({"done": true}))).await;
    let detail = ok(&app, "GET", &format!("/tasks/{}", id(&t)), &me, None).await;
    assert_eq!(detail["task"]["checklist_done"], 1);
    assert_eq!(detail["task"]["checklist_total"], 2);
    assert_eq!(detail["checklist"][0]["text"], "Cut out rust");
    assert_eq!(detail["checklist"][0]["done"], true);
    assert_eq!(
        status(&app, "DELETE", &format!("/checklist/{}", id(&a)), &me, None).await,
        StatusCode::NO_CONTENT
    );

    // Deleting a label takes it off tasks.
    ok(&app, "DELETE", &format!("/labels/{}", id(&engine)), &me, None).await;
    let detail = ok(&app, "GET", &format!("/tasks/{}", id(&t)), &me, None).await;
    assert_eq!(detail["task"]["labels"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn global_board_filters_by_project_and_family() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car").await;
    let videos = project(&app, &me, "Videos").await;
    let blender = project(&app, &me, "Blender").await;
    let garden = project(&app, &me, "Garden").await;
    for (s, t) in [(&car, &videos), (&videos, &blender)] {
        ok(&app, "POST", "/links", &me, Some(json!({"source_id": s, "target_id": t, "kind": "spawned"})))
            .await;
    }
    for (p, title) in [(&car, "Weld"), (&videos, "Film"), (&blender, "Animate"), (&garden, "Dig")] {
        ok(&app, "POST", &format!("/projects/{p}/tasks"), &me, Some(json!({"title": title}))).await;
    }

    let all = ok(&app, "GET", "/tasks", &me, None).await;
    assert_eq!(all.as_array().unwrap().len(), 4);
    assert!(all[0]["project_title"].is_string());
    let fam = ok(&app, "GET", &format!("/tasks?family={videos}"), &me, None).await;
    let mut t = titles(&fam);
    t.sort();
    assert_eq!(t, ["Animate", "Film"]);
    let one = ok(&app, "GET", &format!("/tasks?project_id={garden}"), &me, None).await;
    assert_eq!(titles(&one), ["Dig"]);

    // Dropping on the global board's Blocked column makes a Blocked column.
    let dig = id(&one[0]).to_string();
    let blocked =
        ok(&app, "PATCH", &format!("/tasks/{dig}"), &me, Some(json!({"category": "blocked"}))).await;
    assert_eq!(blocked["category"], "blocked");
    let columns = ok(&app, "GET", &format!("/projects/{garden}/statuses"), &me, None).await;
    assert_eq!(columns.as_array().unwrap().len(), 4);
    assert_eq!(columns[3]["name"], "Blocked");
    // And again reuses it.
    ok(&app, "PATCH", &format!("/tasks/{dig}"), &me, Some(json!({"category": "done"}))).await;
    ok(&app, "PATCH", &format!("/tasks/{dig}"), &me, Some(json!({"category": "blocked"}))).await;
    let columns = ok(&app, "GET", &format!("/projects/{garden}/statuses"), &me, None).await;
    assert_eq!(columns.as_array().unwrap().len(), 4);

    // Archived projects drop off.
    ok(&app, "PATCH", &format!("/projects/{garden}"), &me, Some(json!({"status": "archived"}))).await;
    assert_eq!(ok(&app, "GET", "/tasks", &me, None).await.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn dashboard_counts_columns_weeks_and_activity() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car").await;
    let columns = ok(&app, "GET", &format!("/projects/{car}/statuses"), &me, None).await;
    let done = id(&columns[2]).to_string();
    for title in ["One", "Two", "Three"] {
        ok(&app, "POST", &format!("/projects/{car}/tasks"), &me, Some(json!({"title": title}))).await;
    }
    ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Shipped", "status_id": done})),
    )
    .await;
    ok(
        &app,
        "POST",
        &format!("/projects/{car}/tasks"),
        &me,
        Some(json!({"title": "Late", "due_date": "2020-05-01"})),
    )
    .await;

    let d = ok(&app, "GET", &format!("/projects/{car}/dashboard"), &me, None).await;
    let counts: Vec<_> =
        d["columns"].as_array().unwrap().iter().map(|c| c["count"].as_i64().unwrap()).collect();
    assert_eq!(counts, [4, 0, 1]);
    assert_eq!(d["weekly"].as_array().unwrap().len(), 8);
    assert_eq!(d["weekly"][7]["done"], 1);
    assert_eq!(d["up_next"][0]["title"], "Late", "late tasks first");
    assert_eq!(d["up_next"].as_array().unwrap().len(), 4, "done tasks aren't up next");
    assert_eq!(d["activity"][0]["kind"], "task.created");
    assert_eq!(d["activity"][0]["user_name"], "Logan");
}

#[tokio::test]
async fn people_only_see_their_own_tasks() {
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
    let car = project(&app, &owner, "Car").await;
    let t = ok(&app, "POST", &format!("/projects/{car}/tasks"), &owner, Some(json!({"title": "Weld"}))).await;
    let tid = id(&t);
    let columns = ok(&app, "GET", &format!("/projects/{car}/statuses"), &owner, None).await;

    assert_eq!(status(&app, "GET", &format!("/tasks/{tid}"), &sam, None).await, StatusCode::NOT_FOUND);
    assert_eq!(
        status(&app, "PATCH", &format!("/tasks/{tid}"), &sam, Some(json!({"title": "Mine"}))).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(&app, "GET", &format!("/projects/{car}/tasks"), &sam, None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(&app, "GET", &format!("/projects/{car}/dashboard"), &sam, None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(&app, "POST", &format!("/projects/{car}/tasks"), &sam, Some(json!({"title": "Sneak"}))).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status(&app, "DELETE", &format!("/statuses/{}", id(&columns[0])), &sam, None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(ok(&app, "GET", "/tasks", &sam, None).await.as_array().unwrap().len(), 0);

    // Sam can't drop their task into Logan's column either.
    let mine = project(&app, &sam, "Sam's").await;
    let st = ok(&app, "POST", &format!("/projects/{mine}/tasks"), &sam, Some(json!({"title": "Mine"}))).await;
    assert_eq!(
        status(
            &app,
            "PATCH",
            &format!("/tasks/{}", id(&st)),
            &sam,
            Some(json!({"status_id": id(&columns[0])}))
        )
        .await,
        StatusCode::NOT_FOUND
    );
}
