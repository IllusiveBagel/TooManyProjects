mod common;

use std::io::{Read, Write};

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

async fn user_id(app: &TestApp, cookie: &str) -> String {
    let res = app.call("GET", "/api/v1/auth/status", Some(cookie), None).await;
    res.body["user"]["id"].as_str().unwrap().to_string()
}

async fn note(app: &TestApp, cookie: &str, body: Value) -> Value {
    let res = app.call("POST", "/api/v1/notes", Some(cookie), Some(body)).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body
}

async fn project(app: &TestApp, cookie: &str, title: &str) -> String {
    let res = app.call("POST", "/api/v1/projects", Some(cookie), Some(json!({"title": title}))).await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    res.body["id"].as_str().unwrap().to_string()
}

fn titles(list: &Value) -> Vec<&str> {
    list.as_array().unwrap().iter().map(|n| n["title"].as_str().unwrap()).collect()
}

async fn invite(app: &TestApp, owner: &str) -> String {
    let invite = app.call("POST", "/api/v1/invites", Some(owner), Some(json!({}))).await;
    app.call(
        "POST",
        "/api/v1/auth/accept-invite",
        None,
        Some(json!({"token": invite.body["token"], "name": "Sam", "email": "sam@example.com", "password": "sam's password"})),
    )
    .await
    .cookie
    .unwrap()
}

#[tokio::test]
async fn notes_are_markdown_files_with_front_matter() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let uid = user_id(&app, &me).await;
    let car = project(&app, &me, "Car restoration").await;

    let created = note(
        &app,
        &me,
        json!({"title": "Bleeding brakes", "body": "Use DOT 4.\n", "tags": ["howto"], "project_ids": [car]}),
    )
    .await;
    let n = &created["note"];
    assert_eq!(n["path"], "Projects/Car restoration/Bleeding brakes.md");
    assert_eq!(n["folder"], "Projects");
    assert_eq!(n["projects"][0]["title"], "Car restoration");
    assert_eq!(created["body"], "Use DOT 4.\n");

    let file =
        std::fs::read_to_string(app.data_dir.join("vaults").join(&uid).join(n["path"].as_str().unwrap()))
            .unwrap();
    assert!(file.starts_with("---\n"), "{file}");
    assert!(file.contains("projects:") && file.contains("Car restoration"), "{file}");
    assert!(file.contains(&format!("id: {}", n["id"].as_str().unwrap())), "{file}");
    assert!(file.ends_with("Use DOT 4.\n"), "{file}");

    let inbox = note(&app, &me, json!({"folder": "inbox", "body": "buy bleed kit"})).await;
    assert!(inbox["note"]["path"].as_str().unwrap().starts_with("Inbox/"));
    let plain = note(&app, &me, json!({"title": "Torque specs"})).await;
    assert_eq!(plain["note"]["path"], "Knowledge/Torque specs.md");
    let again = note(&app, &me, json!({"title": "Torque specs"})).await;
    assert_eq!(again["note"]["path"], "Knowledge/Torque specs 2.md", "names never clash");

    let download = app
        .call_raw("GET", &format!("/api/v1/notes/{}/download", n["id"].as_str().unwrap()), &me, "", vec![])
        .await;
    assert_eq!(download.0, StatusCode::OK);
    assert_eq!(String::from_utf8(download.1).unwrap(), file);
}

#[tokio::test]
async fn search_tags_and_filters() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    note(
        &app,
        &me,
        json!({"title": "Bleeding brakes", "body": "Pump the pedal slowly.", "tags": ["car", "howto"]}),
    )
    .await;
    note(&app, &me, json!({"title": "Render settings", "body": "Cycles, 128 samples.", "tags": ["blender"]}))
        .await;
    note(&app, &me, json!({"folder": "inbox", "body": "pedal box idea", "tags": ["car"]})).await;

    let hits = app.call("GET", "/api/v1/notes?q=pedal", Some(&me), None).await;
    assert_eq!(hits.body.as_array().unwrap().len(), 2);
    assert!(hits.body[0]["snippet"].as_str().unwrap().contains("[pedal"), "{:?}", hits.body[0]);
    let prefix = app.call("GET", "/api/v1/notes?q=brak", Some(&me), None).await;
    assert_eq!(titles(&prefix.body), ["Bleeding brakes"], "prefix search, title matches");
    let odd = app.call("GET", "/api/v1/notes?q=%22%28*", Some(&me), None).await;
    assert_eq!(odd.status, StatusCode::OK, "punctuation can't break the query");

    let tagged = app.call("GET", "/api/v1/notes?tag=car", Some(&me), None).await;
    assert_eq!(tagged.body.as_array().unwrap().len(), 2);
    let inbox = app.call("GET", "/api/v1/notes?folder=Inbox", Some(&me), None).await;
    assert_eq!(inbox.body.as_array().unwrap().len(), 1);

    let tags = app.call("GET", "/api/v1/notes/tags", Some(&me), None).await;
    assert_eq!(tags.body[0], json!({"tag": "car", "count": 2}));
}

#[tokio::test]
async fn backlinks_and_renames_follow_links() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let car = project(&app, &me, "Car restoration").await;
    let target = note(&app, &me, json!({"title": "Brake fluid"})).await;
    let tid = target["note"]["id"].as_str().unwrap().to_string();
    let source = note(
        &app,
        &me,
        json!({"title": "Weekend", "body": "Read [[Brake fluid|the fluid note]] and [[Missing]] for [[@Car restoration]].\n"}),
    )
    .await;
    let sid = source["note"]["id"].as_str().unwrap().to_string();

    let links = &source["links"];
    let link = |t: &str| links.as_array().unwrap().iter().find(|l| l["title"] == t).unwrap().clone();
    assert_eq!(link("Brake fluid")["id"], tid);
    assert_eq!(link("Missing")["id"], Value::Null);
    assert_eq!(link("Car restoration")["kind"], "project");
    assert_eq!(link("Car restoration")["id"], car);

    let detail = app.call("GET", &format!("/api/v1/notes/{tid}"), Some(&me), None).await;
    assert_eq!(titles(&detail.body["backlinks"]), ["Weekend"]);

    let mentions = app.call("GET", &format!("/api/v1/projects/{car}/notes"), Some(&me), None).await;
    assert_eq!(titles(&mentions.body["mentions"]), ["Weekend"]);
    assert_eq!(mentions.body["notes"].as_array().unwrap().len(), 0);

    // Renaming a note rewrites the links to it, keeping the alias.
    let renamed = app
        .call("PATCH", &format!("/api/v1/notes/{tid}"), Some(&me), Some(json!({"title": "DOT 4 fluid"})))
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.body);
    assert_eq!(renamed.body["note"]["path"], "Knowledge/DOT 4 fluid.md");
    assert_eq!(renamed.body["note"]["id"], tid, "ids survive a rename");
    let source = app.call("GET", &format!("/api/v1/notes/{sid}"), Some(&me), None).await;
    assert!(
        source.body["body"].as_str().unwrap().contains("[[DOT 4 fluid|the fluid note]]"),
        "{:?}",
        source.body["body"]
    );
    assert_eq!(titles(&renamed.body["backlinks"]), ["Weekend"]);

    // Renaming a project rewrites mentions and front matter.
    let listed = note(&app, &me, json!({"title": "Parts", "project_ids": [car]})).await;
    let res = app
        .call("PATCH", &format!("/api/v1/projects/{car}"), Some(&me), Some(json!({"title": "Mustang"})))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let source = app.call("GET", &format!("/api/v1/notes/{sid}"), Some(&me), None).await;
    assert!(source.body["body"].as_str().unwrap().contains("[[@Mustang]]"), "{:?}", source.body["body"]);
    let parts = app
        .call("GET", &format!("/api/v1/notes/{}", listed["note"]["id"].as_str().unwrap()), Some(&me), None)
        .await;
    assert_eq!(parts.body["note"]["projects"][0]["title"], "Mustang");
    let project_notes = app.call("GET", &format!("/api/v1/projects/{car}/notes"), Some(&me), None).await;
    assert_eq!(titles(&project_notes.body["notes"]), ["Parts"]);
    assert_eq!(titles(&project_notes.body["mentions"]), ["Weekend"]);
}

#[tokio::test]
async fn edits_moves_and_deletes() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let uid = user_id(&app, &me).await;
    let root = app.data_dir.join("vaults").join(&uid);
    let n = note(&app, &me, json!({"folder": "inbox", "title": "Idea", "body": "one"})).await;
    let id = n["note"]["id"].as_str().unwrap();

    let res = app
        .call(
            "PATCH",
            &format!("/api/v1/notes/{id}"),
            Some(&me),
            Some(json!({"body": "two", "tags": ["x"], "folder": "Knowledge/Ideas"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["note"]["path"], "Knowledge/Ideas/Idea.md");
    assert_eq!(res.body["body"], "two\n", "files end with a newline");
    assert!(!root.join("Inbox").exists(), "empty folders are tidied away");

    let res = app
        .call("PATCH", &format!("/api/v1/notes/{id}"), Some(&me), Some(json!({"folder": "../../escape"})))
        .await;
    assert_eq!(res.body["note"]["path"], "escape/Idea.md", "{:?}", res.body);
    let res = app
        .call("PATCH", &format!("/api/v1/notes/{id}"), Some(&me), Some(json!({"title": "a/../../b"})))
        .await;
    if res.status == StatusCode::OK {
        let path = res.body["note"]["path"].as_str().unwrap();
        assert!(path.split('/').all(|seg| seg != ".." && seg != "."), "{path}");
    }
    assert!(!app.data_dir.join("vaults").join("escape").exists());

    let res = app.call("DELETE", &format!("/api/v1/notes/{id}"), Some(&me), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.call("GET", &format!("/api/v1/notes/{id}"), Some(&me), None).await.status,
        StatusCode::NOT_FOUND
    );
    let list = app.call("GET", "/api/v1/notes", Some(&me), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn files_changed_outside_the_app_are_picked_up() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let uid = user_id(&app, &me).await;
    let root = app.data_dir.join("vaults").join(&uid);
    let n = note(&app, &me, json!({"title": "Brakes", "body": "old"})).await;
    let id = n["note"]["id"].as_str().unwrap();

    // Another editor rewrites one file and adds another without front matter.
    let path = root.join("Knowledge/Brakes.md");
    let text = std::fs::read_to_string(&path).unwrap().replace("old", "new text from Obsidian");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&path, text).unwrap();
    std::fs::create_dir_all(root.join("Knowledge/Shop")).unwrap();
    std::fs::write(root.join("Knowledge/Shop/Lathe.md"), "#tools and [[Brakes]]\n").unwrap();
    std::fs::create_dir_all(root.join(".obsidian")).unwrap();
    std::fs::write(root.join(".obsidian/ignored.md"), "settings").unwrap();

    let res = app.call("POST", "/api/v1/notes/rescan", Some(&me), None).await;
    assert_eq!(res.body["indexed"], 2, "{:?}", res.body);
    let detail = app.call("GET", &format!("/api/v1/notes/{id}"), Some(&me), None).await;
    assert_eq!(detail.body["body"], "new text from Obsidian\n");
    assert_eq!(titles(&detail.body["backlinks"]), ["Lathe"]);
    let lathe = std::fs::read_to_string(root.join("Knowledge/Shop/Lathe.md")).unwrap();
    assert!(lathe.starts_with("---\nid: "), "a new file gets an id written back: {lathe}");

    std::fs::remove_file(root.join("Knowledge/Shop/Lathe.md")).unwrap();
    toomanyprojects::notes::vault::scan_all(&app.state.db, &app.data_dir).await.unwrap();
    let list = app.call("GET", "/api/v1/notes", Some(&me), None).await;
    assert_eq!(titles(&list.body), ["Brakes"]);
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[tokio::test]
async fn imports_obsidian_and_notion_and_exports() {
    let app = TestApp::new().await;
    let me = app.setup_owner().await;
    let uid = user_id(&app, &me).await;
    let root = app.data_dir.join("vaults").join(&uid);
    note(&app, &me, json!({"title": "Brakes", "folder": "Garage"})).await;

    let obsidian = zip_of(&[
        ("My vault/Garage/Brakes.md", b"---\ntags: [car]\n---\nFrom Obsidian, see [[Lathe]]\n"),
        ("My vault/Garage/Lathe.md", b"Spindle speeds\n"),
        ("My vault/Garage/lathe.png", b"\x89PNG"),
        ("My vault/.obsidian/app.json", b"{}"),
        ("My vault/../../evil.md", b"nope"),
    ]);
    let (status, body) =
        app.call_raw("POST", "/api/v1/notes/import?source=obsidian", &me, "application/zip", obsidian).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result, json!({"notes": 2, "attachments": 1, "skipped": 2, "renamed": 1}));
    assert!(root.join("Garage/Brakes 2.md").exists(), "an existing note is never overwritten");
    assert!(root.join("Garage/lathe.png").exists());
    assert!(!app.data_dir.join("evil.md").exists() && !root.join("evil.md").exists());

    let id = "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d";
    let page = format!("# Car\n\nSee [Parts](Car%20{id}/Parts%20{id}.md) ![p](Car%20{id}/photo.jpg)\n");
    let notion = zip_of(&[
        (&format!("Car {id}.md"), page.as_bytes()),
        (&format!("Car {id}/Parts {id}.md"), b"# Parts\n\nBolts\n"),
        (&format!("Car {id}/photo.jpg"), b"jpg"),
        (&format!("Car {id}/Table {id}.csv"), b"a,b"),
    ]);
    let (status, body) = app
        .call_raw("POST", "/api/v1/notes/import?source=notion&folder=Notion", &me, "application/zip", notion)
        .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["notes"], 2);
    assert_eq!(result["skipped"], 1);
    let car = std::fs::read_to_string(root.join("Notion/Car.md")).unwrap();
    assert!(car.contains("See [[Parts]] ![p](Car/photo.jpg)"), "{car}");
    assert!(!car.contains("# Car"), "the title line becomes the file name: {car}");
    assert!(root.join("Notion/Car/photo.jpg").exists());

    let list = app.call("GET", "/api/v1/notes", Some(&me), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 5);
    let parts = app.call("GET", "/api/v1/notes?q=bolts", Some(&me), None).await;
    let parts_id = parts.body[0]["id"].as_str().unwrap();
    let parts = app.call("GET", &format!("/api/v1/notes/{parts_id}"), Some(&me), None).await;
    assert_eq!(titles(&parts.body["backlinks"]), ["Car"]);

    let (status, bad) = app
        .call_raw("POST", "/api/v1/notes/import?source=notion", &me, "application/zip", b"not a zip".to_vec())
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", String::from_utf8_lossy(&bad));

    let (status, zip) = app.call_raw("GET", "/api/v1/notes/export", &me, "", vec![]).await;
    assert_eq!(status, StatusCode::OK);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut names: Vec<String> = archive.file_names().map(|n| n.unwrap().to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Garage/Brakes 2.md",
            "Garage/Brakes.md",
            "Garage/Lathe.md",
            "Garage/lathe.png",
            "Notion/Car.md",
            "Notion/Car/Parts.md",
            "Notion/Car/photo.jpg"
        ]
    );
    let mut text = String::new();
    archive.by_name("Garage/Lathe.md").unwrap().read_to_string(&mut text).unwrap();
    assert!(text.contains("Spindle speeds"));
}

#[tokio::test]
async fn people_only_see_their_own_notes() {
    let app = TestApp::new().await;
    let owner = app.setup_owner().await;
    let sam = invite(&app, &owner).await;
    let car = project(&app, &owner, "Car restoration").await;
    let n = note(&app, &owner, json!({"title": "Private", "body": "secret pedal"})).await;
    let id = n["note"]["id"].as_str().unwrap();

    assert_eq!(
        app.call("GET", &format!("/api/v1/notes/{id}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call("PATCH", &format!("/api/v1/notes/{id}"), Some(&sam), Some(json!({"body": "x"})))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call("DELETE", &format!("/api/v1/notes/{id}"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.call("GET", "/api/v1/notes?q=pedal", Some(&sam), None).await.body, json!([]));
    assert_eq!(
        app.call("GET", &format!("/api/v1/projects/{car}/notes"), Some(&sam), None).await.status,
        StatusCode::NOT_FOUND
    );
    let res = app
        .call("POST", "/api/v1/notes", Some(&sam), Some(json!({"title": "Mine", "project_ids": [car]})))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "can't file a note under someone else's project");
    assert_eq!(app.call("GET", "/api/v1/notes", None, None).await.status, StatusCode::UNAUTHORIZED);
}
