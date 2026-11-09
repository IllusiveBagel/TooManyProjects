//! The weekly calendar: sessions, busy time, repeats, joining and who sees what.

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

/// The week of Monday 9 November 2026, in UTC.
const WEEK: &str = "/calendar?from=2026-11-09T00:00:00Z&to=2026-11-16T00:00:00Z";

fn kinds(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!("{} {}", o["occurrence"].as_str().unwrap(), o["event"]["title"].as_str().unwrap_or(""))
        })
        .collect()
}

#[tokio::test]
async fn busy_time_repeats_and_stays_private() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let car = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Beetle restoration"}))).await);
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "role": "viewer"})),
    )
    .await;

    let work = ok(
        &app,
        "POST",
        "/calendar/events",
        &logan,
        Some(json!({"kind": "busy", "title": "Work", "start_local": "2026-11-02T09:00", "minutes": 480,
                    "tz": "Europe/London", "repeat_days": 31})),
    )
    .await;
    assert_eq!(work["can_edit"], true);
    let week = ok(&app, "GET", WEEK, &logan, None).await;
    assert_eq!(week.as_array().unwrap().len(), 5);
    assert_eq!(week[0]["start_at"], "2026-11-09T09:00:00Z");
    assert_eq!(week[0]["end_at"], "2026-11-09T17:00:00Z");
    assert_eq!(week[0]["can_join"], false);

    // Sam can see the whole account but never Logan's busy time.
    assert_eq!(ok(&app, "GET", WEEK, &sam, None).await, json!([]));
    assert_eq!(
        status(&app, "GET", &format!("/calendar/events/{}", id(&work)), &sam, None).await,
        StatusCode::NOT_FOUND
    );

    // Not this Wednesday: one week out of the repeat.
    ok(&app, "DELETE", &format!("/calendar/events/{}?occurrence=2026-11-11", id(&work)), &logan, None).await;
    let week = ok(&app, "GET", WEEK, &logan, None).await;
    assert!(!kinds(&week).contains(&"2026-11-11 Work".to_string()));
    assert_eq!(week.as_array().unwrap().len(), 4);
    // A day it doesn't happen on can't be taken out.
    assert_eq!(
        status(
            &app,
            "DELETE",
            &format!("/calendar/events/{}?occurrence=2026-11-14", id(&work)),
            &logan,
            None
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    // Busy time can't belong to a project, and sessions need one.
    let bad = |body: Value| async { status(&app, "POST", "/calendar/events", &logan, Some(body)).await };
    assert_eq!(
        bad(json!({"kind": "busy", "project_id": car, "start_local": "2026-11-09T09:00", "minutes": 60, "tz": "UTC"})).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        bad(json!({"kind": "session", "start_local": "2026-11-09T09:00", "minutes": 60, "tz": "UTC"})).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        bad(json!({"kind": "busy", "start_local": "2026-11-09T09:00", "minutes": 60, "tz": "Nowhere"})).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(&app, "GET", "/calendar?from=2026-01-01T00:00:00Z&to=2026-12-01T00:00:00Z", &logan, None)
            .await,
        StatusCode::BAD_REQUEST
    );

    // The whole series goes.
    ok(&app, "DELETE", &format!("/calendar/events/{}", id(&work)), &logan, None).await;
    assert_eq!(ok(&app, "GET", WEEK, &logan, None).await, json!([]));
}

#[tokio::test]
async fn friends_see_sessions_and_join_them() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let lee = join(&app, &logan, "Lee").await;
    let car = id(&ok(&app, "POST", "/projects", &logan, Some(json!({"title": "Beetle restoration"}))).await);
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "project_id": car, "role": "viewer"})),
    )
    .await;

    // Saturday is car day.
    let session = ok(
        &app,
        "POST",
        "/calendar/events",
        &logan,
        Some(json!({"kind": "session", "project_id": car, "title": "Rear brakes", "start_local": "2026-11-14T13:00",
                    "minutes": 240, "tz": "UTC", "repeat_days": 32})),
    )
    .await;
    let sid = id(&session);
    assert_eq!(session["project_title"], "Beetle restoration");

    // Sam sees it, can join it, but as a viewer can't plan or change sessions.
    let week = ok(&app, "GET", WEEK, &sam, None).await;
    assert_eq!(week.as_array().unwrap().len(), 1);
    assert_eq!(week[0]["can_join"], true);
    assert_eq!(week[0]["event"]["can_edit"], false);
    assert_eq!(
        status(&app, "POST", "/calendar/events", &sam,
            Some(json!({"kind": "session", "project_id": car, "start_local": "2026-11-15T10:00", "minutes": 60, "tz": "UTC"})))
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, "PATCH", &format!("/calendar/events/{sid}"), &sam, Some(json!({"minutes": 60}))).await,
        StatusCode::FORBIDDEN
    );
    // Lee isn't shared the project, so the session doesn't exist for them.
    assert_eq!(ok(&app, "GET", WEEK, &lee, None).await, json!([]));
    assert_eq!(
        status(&app, "PUT", &format!("/calendar/events/{sid}/joins/2026-11-14"), &lee, None).await,
        StatusCode::NOT_FOUND
    );

    let joined = ok(&app, "PUT", &format!("/calendar/events/{sid}/joins/2026-11-14"), &sam, None).await;
    assert_eq!(joined["joined"], true);
    assert_eq!(joined["joiners"][0]["name"], "Sam");
    // Joining twice is the same as once; a Friday isn't a session day.
    ok(&app, "PUT", &format!("/calendar/events/{sid}/joins/2026-11-14"), &sam, None).await;
    assert_eq!(
        status(&app, "PUT", &format!("/calendar/events/{sid}/joins/2026-11-13"), &sam, None).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(&app, "PUT", &format!("/calendar/events/{sid}/joins/2026-11-14"), &logan, None).await,
        StatusCode::BAD_REQUEST
    );

    // Logan sees who's coming that week only, and the activity says so.
    let next = "/calendar?from=2026-11-09T00:00:00Z&to=2026-11-23T00:00:00Z";
    let two = ok(&app, "GET", next, &logan, None).await;
    assert_eq!(two[0]["joiners"][0]["name"], "Sam");
    assert_eq!(two[1]["joiners"], json!([]));
    let dash = ok(&app, "GET", &format!("/projects/{car}/dashboard"), &logan, None).await;
    let kinds: Vec<&str> =
        dash["activity"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"session.joined"), "{kinds:?}");
    assert!(kinds.contains(&"session.planned"), "{kinds:?}");
    assert_eq!(dash["activity"][0]["payload"]["when"], "Sat 14 Nov, 13:00");

    // "Just mine" for Sam: the session they're joining.
    let mine = ok(&app, "GET", &format!("{next}&mine=true"), &sam, None).await;
    assert_eq!(mine.as_array().unwrap().len(), 1);

    // Taking it back.
    let left = ok(&app, "DELETE", &format!("/calendar/events/{sid}/joins/2026-11-14"), &sam, None).await;
    assert_eq!(left["joined"], false);

    // Moving the series keeps it one event; a project filter narrows the list.
    let moved = ok(
        &app,
        "PATCH",
        &format!("/calendar/events/{sid}"),
        &logan,
        Some(json!({"start_local": "2026-11-14T10:00", "title": null})),
    )
    .await;
    assert_eq!(moved["title"], Value::Null);
    let week = ok(&app, "GET", &format!("{WEEK}&project_id={car}"), &logan, None).await;
    assert_eq!(week[0]["start_at"], "2026-11-14T10:00:00Z");

    // Deleting the project takes its sessions with it.
    ok(&app, "DELETE", &format!("/projects/{car}"), &logan, None).await;
    assert_eq!(ok(&app, "GET", WEEK, &logan, None).await, json!([]));
}
