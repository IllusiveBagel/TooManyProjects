//! Photos and files: uploading, previews, who may see them, comments, notes,
//! rows, covers and clean-up.

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

const BOUNDARY: &str = "----tmp-test-boundary";

fn multipart(files: &[(&str, &str, &[u8])]) -> Vec<u8> {
    let mut body = vec![];
    for (name, mime, bytes) in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: {mime}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn upload(
    app: &TestApp,
    cookie: &str,
    query: &str,
    files: &[(&str, &str, &[u8])],
) -> (StatusCode, Value) {
    let (status, bytes) = app
        .call_raw(
            "POST",
            &format!("/api/v1/attachments?{query}"),
            cookie,
            &format!("multipart/form-data; boundary={BOUNDARY}"),
            multipart(files),
        )
        .await;
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn get_bytes(app: &TestApp, cookie: &str, path: &str) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let req = Request::builder()
        .uri(path)
        .header("host", "localhost:8080")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    (status, res.into_body().collect().await.unwrap().to_bytes().to_vec(), headers)
}

/// A 40 × 20 JPEG whose EXIF says "turn me 90° clockwise" (so it is really
/// 20 × 40) and that it was taken on 9 November 2026 at 16:20.
fn phone_photo() -> Vec<u8> {
    let img = image::RgbImage::from_fn(40, 20, |x, _| image::Rgb([(x * 6) as u8, 120, 60]));
    let mut jpeg = vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&img).unwrap();

    // TIFF, little-endian: IFD0 (Orientation, ExifIFD pointer), then the Exif
    // IFD (DateTimeOriginal), then the date string.
    let mut tiff: Vec<u8> = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let entry = |tag: u16, kind: u16, count: u32, value: u32| {
        let mut e = tag.to_le_bytes().to_vec();
        e.extend_from_slice(&kind.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&value.to_le_bytes());
        e
    };
    let ifd0_len = 2 + 2 * 12 + 4;
    let exif_ifd = 8 + ifd0_len as u32;
    let exif_len = 2 + 12 + 4;
    let date_at = exif_ifd + exif_len;
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend(entry(0x0112, 3, 1, 6));
    tiff.extend(entry(0x8769, 4, 1, exif_ifd));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend(entry(0x9003, 2, 20, date_at));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(b"2026:11:09 16:20:00\0");

    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut out = jpeg[..2].to_vec(); // SOI
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend(app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// A project with one task, as (project id, task id).
async fn seed(app: &TestApp, me: &str) -> (String, String) {
    let car = id(&ok(app, "POST", "/projects", me, Some(json!({"title": "Supra build"}))).await);
    let task =
        id(&ok(app, "POST", &format!("/projects/{car}/tasks"), me, Some(json!({"title": "Strip interior"})))
            .await);
    (car, task)
}

#[tokio::test]
async fn photos_on_a_task_get_previews_and_a_date() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, task) = seed(&app, &me).await;
    let photo = phone_photo();

    let (status, added) = upload(
        &app,
        &me,
        &format!("task_id={task}"),
        &[("IMG_0042.JPG", "image/jpeg", &photo), ("A70 trim.pdf", "application/pdf", b"%PDF-1.4 hello")],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{added:?}");
    let pic = &added[0];
    assert_eq!(pic["name"], "IMG_0042.JPG");
    assert_eq!(pic["is_image"], true);
    assert_eq!(pic["mime"], "image/jpeg");
    // Turned upright from the EXIF orientation.
    assert_eq!((pic["width"].as_i64(), pic["height"].as_i64()), (Some(20), Some(40)));
    assert_eq!(pic["taken_at"], "2026-11-09T16:20:00");
    assert_eq!(pic["task_number"], 1);
    assert_eq!(pic["project_id"], car.as_str());
    assert_eq!(added[1]["mime"], "application/pdf");
    assert_eq!(added[1]["is_image"], false);

    // The preview is an upright JPEG with no EXIF left in it.
    let (status, preview, headers) =
        get_bytes(&app, &me, &format!("/api/v1/attachments/{}/file?size=400", id(pic))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert!(headers["cache-control"].to_str().unwrap().starts_with("private"));
    let img = image::load_from_memory(&preview).unwrap();
    assert_eq!((img.width(), img.height()), (20, 40));
    assert!(!preview.windows(6).any(|w| w == b"Exif\0\0"));

    // The original is kept as it came.
    let (_, original, headers) = get_bytes(&app, &me, &format!("/api/v1/attachments/{}/file", id(pic))).await;
    assert_eq!(original, photo);
    assert!(headers["content-disposition"].to_str().unwrap().starts_with("inline"));
    let (_, _, headers) =
        get_bytes(&app, &me, &format!("/api/v1/attachments/{}/file?download=true", id(pic))).await;
    assert!(headers["content-disposition"].to_str().unwrap().starts_with("attachment"));

    // The task shows them and counts them.
    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["attachments"].as_array().unwrap().len(), 2);
    assert_eq!(detail["task"]["attachment_count"], 2);

    // The project's Photos tab, photos only.
    let photos = ok(&app, "GET", &format!("/attachments?project_id={car}&images_only=true"), &me, None).await;
    assert_eq!(photos.as_array().unwrap().len(), 1);

    // A caption, then a cover for the canvas card.
    let captioned =
        ok(&app, "PATCH", &format!("/attachments/{}", id(pic)), &me, Some(json!({"caption": " Seats out "})))
            .await;
    assert_eq!(captioned["caption"], "Seats out");
    ok(&app, "PUT", &format!("/projects/{car}/cover"), &me, Some(json!({"attachment_id": id(pic)}))).await;
    let project = ok(&app, "GET", &format!("/projects/{car}"), &me, None).await;
    assert_eq!(project["project"]["cover_id"], id(pic).as_str());
    // A PDF can't be a cover.
    let res = app
        .call(
            "PUT",
            &format!("/api/v1/projects/{car}/cover"),
            Some(&me),
            Some(json!({"attachment_id": id(&added[1])})),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Removing the task keeps its photos on the project.
    ok(&app, "DELETE", &format!("/tasks/{task}"), &me, None).await;
    let all = ok(&app, "GET", &format!("/attachments?project_id={car}"), &me, None).await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert!(all[0]["task_id"].is_null());
}

#[tokio::test]
async fn untrusted_files_download_and_never_run() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let (status, added) = upload(
        &app,
        &me,
        &format!("project_id={car}"),
        &[
            ("evil.svg", "image/svg+xml", b"<svg onload=alert(1)/>"),
            ("x.png", "image/png", b"<html>not a png"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(added[0]["mime"], "image/svg+xml");
    assert_eq!(added[1]["mime"], "application/octet-stream");
    for a in added.as_array().unwrap() {
        assert_eq!(a["is_image"], false);
        let (_, _, headers) = get_bytes(&app, &me, &format!("/api/v1/attachments/{}/file", id(a))).await;
        assert_eq!(headers["content-type"], "application/octet-stream");
        assert!(headers["content-disposition"].to_str().unwrap().starts_with("attachment"));
    }
}

#[tokio::test]
async fn uploads_over_the_limit_are_refused() {
    let app = TestApp::with_config(|c| c.max_upload_bytes = 1024).await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let big = vec![7u8; 4096];
    let (status, body) = upload(&app, &me, &format!("project_id={car}"), &[("big.bin", "x/y", &big)]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "too_large");
    let list = ok(&app, "GET", &format!("/attachments?project_id={car}"), &me, None).await;
    assert!(list.as_array().unwrap().is_empty());
    // Nothing half-written is left behind as a file.
    let leftovers = std::fs::read_dir(app.data_dir.join("files/tmp")).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftovers, 0);

    // One target, exactly.
    let (status, _) = upload(&app, &me, "", &[("a.txt", "text/plain", b"hi")]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn files_follow_the_project_they_are_on() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let sam = join(&app, &logan, "Sam").await;
    let lee = join(&app, &logan, "Lee").await;
    let (car, task) = seed(&app, &logan).await;
    ok(
        &app,
        "POST",
        "/shares",
        &logan,
        Some(json!({"grantee_id": user_id(&app, &sam).await, "project_id": car, "role": "viewer"})),
    )
    .await;
    let (_, added) =
        upload(&app, &logan, &format!("task_id={task}"), &[("p.jpg", "image/jpeg", &phone_photo())]).await;
    let pic = id(&added[0]);

    // Sam can look but not change.
    let (status, _, _) = get_bytes(&app, &sam, &format!("/api/v1/attachments/{pic}/file?size=1600")).await;
    assert_eq!(status, StatusCode::OK);
    let seen = ok(&app, "GET", &format!("/attachments?task_id={task}"), &sam, None).await;
    assert_eq!(seen[0]["can_edit"], false);
    let res = app.call("DELETE", &format!("/api/v1/attachments/{pic}"), Some(&sam), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let (status, _) = upload(&app, &sam, &format!("task_id={task}"), &[("s.txt", "text/plain", b"hi")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Lee sees nothing at all.
    let (status, _, _) = get_bytes(&app, &lee, &format!("/api/v1/attachments/{pic}/file")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let res = app.call("GET", &format!("/api/v1/attachments?project_id={car}"), Some(&lee), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Deleting removes the file and its previews from disk.
    ok(&app, "DELETE", &format!("/attachments/{pic}"), &logan, None).await;
    let thumbs = app.data_dir.join("files/thumbs");
    assert_eq!(std::fs::read_dir(&thumbs).unwrap().count(), 0);
    let (status, _, _) = get_bytes(&app, &logan, &format!("/api/v1/attachments/{pic}/file")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_photo_comment_carries_its_photos() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (_, task) = seed(&app, &me).await;
    let (_, added) =
        upload(&app, &me, &format!("task_id={task}"), &[("rear floor.jpg", "image/jpeg", &phone_photo())])
            .await;
    let pic = id(&added[0]);

    // Words are optional when photos come with it.
    let comment = ok(
        &app,
        "POST",
        &format!("/tasks/{task}/comments"),
        &me,
        Some(json!({"body": "", "attachment_ids": [pic]})),
    )
    .await;
    assert_eq!(comment["attachments"][0]["id"], pic.as_str());
    let res = app
        .call("POST", &format!("/api/v1/tasks/{task}/comments"), Some(&me), Some(json!({"body": ""})))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["comments"][0]["attachments"].as_array().unwrap().len(), 1);
    // On the comment, not loose on the task.
    assert!(detail["attachments"].as_array().unwrap().is_empty());
    assert_eq!(detail["task"]["attachment_count"], 1);

    // Deleting the comment leaves the photo on the task.
    ok(&app, "DELETE", &format!("/comments/{}", id(&comment)), &me, None).await;
    let detail = ok(&app, "GET", &format!("/tasks/{task}"), &me, None).await;
    assert_eq!(detail["attachments"][0]["id"], pic.as_str());
}

#[tokio::test]
async fn note_files_live_in_the_vault() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let note = ok(&app, "POST", "/notes", &me, Some(json!({"title": "Wiring", "project_ids": [car]}))).await;
    let note_id = id(&note["note"]);
    let photo = phone_photo();
    let (status, added) =
        upload(&app, &me, &format!("note_id={note_id}"), &[("loom.jpg", "image/jpeg", &photo)]).await;
    assert_eq!(status, StatusCode::CREATED, "{added:?}");
    assert_eq!(added[0]["name"], "loom.jpg");
    let (_, again) =
        upload(&app, &me, &format!("note_id={note_id}"), &[("loom.jpg", "image/jpeg", &photo)]).await;
    assert_eq!(again[0]["name"], "loom 2.jpg");

    let me_id = user_id(&app, &me).await;
    let on_disk = app.data_dir.join(format!("vaults/{me_id}/attachments/loom.jpg"));
    assert_eq!(std::fs::read(on_disk).unwrap(), photo);

    // An ![[embed]] finds it by name, with previews.
    let (status, bytes, headers) =
        get_bytes(&app, &me, &format!("/api/v1/notes/{note_id}/file?name=loom.jpg&size=400")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert_ne!(bytes, photo);
    // A file dropped into the vault some other way is found too.
    std::fs::create_dir_all(app.data_dir.join(format!("vaults/{me_id}/Knowledge/img"))).unwrap();
    std::fs::write(app.data_dir.join(format!("vaults/{me_id}/Knowledge/img/old%20one.png")), b"png?")
        .unwrap();
    std::fs::write(app.data_dir.join(format!("vaults/{me_id}/Knowledge/img/plan.png")), b"png!").unwrap();
    let (status, bytes, _) =
        get_bytes(&app, &me, &format!("/api/v1/notes/{note_id}/file?name=plan.png")).await;
    assert_eq!((status, bytes), (StatusCode::OK, b"png!".to_vec()));
    let (status, _, _) =
        get_bytes(&app, &me, &format!("/api/v1/notes/{note_id}/file?name=..%2F..%2Ftoomanyprojects.db"))
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The project's Photos tab includes photos from its notes.
    let photos = ok(&app, "GET", &format!("/attachments?project_id={car}"), &me, None).await;
    assert_eq!(photos.as_array().unwrap().len(), 2);
    assert_eq!(photos[0]["note_title"], "Wiring");
}

#[tokio::test]
async fn rows_have_files_and_projects_clean_up() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let (car, _) = seed(&app, &me).await;
    let db =
        ok(&app, "POST", &format!("/projects/{car}/databases"), &me, Some(json!({"name": "Parts"}))).await;
    let db_id = id(&db["database"]);
    let title = db["columns"][0]["id"].as_str().unwrap().to_string();
    let row = ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/rows"),
        &me,
        Some(json!({"values": {title: "Seat rails"}})),
    )
    .await;
    let (status, added) =
        upload(&app, &me, &format!("row_id={}", id(&row)), &[("rails.jpg", "image/jpeg", &phone_photo())])
            .await;
    assert_eq!(status, StatusCode::CREATED);
    let rows = ok(&app, "GET", &format!("/databases/{db_id}/rows"), &me, None).await;
    let first = rows.as_array().map(|r| r[0].clone()).unwrap_or_else(|| rows["rows"][0].clone());
    assert_eq!(first["file_count"], 1);
    assert_eq!(first["cover_id"], id(&added[0]).as_str());

    // A gallery view is a kind of view now.
    ok(
        &app,
        "POST",
        &format!("/databases/{db_id}/views"),
        &me,
        Some(json!({"name": "Photos", "kind": "gallery"})),
    )
    .await;

    // Deleting the project takes its files off the disk.
    ok(&app, "DELETE", &format!("/projects/{car}"), &me, None).await;
    let thumbs = app.data_dir.join("files/thumbs");
    assert_eq!(std::fs::read_dir(&thumbs).unwrap().count(), 0);
    let me_id = user_id(&app, &me).await;
    let month_dirs: usize = std::fs::read_dir(app.data_dir.join(format!("files/{me_id}")))
        .unwrap()
        .map(|d| std::fs::read_dir(d.unwrap().path()).unwrap().count())
        .sum();
    assert_eq!(month_dirs, 0);
}
