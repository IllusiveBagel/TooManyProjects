//! Quick add: one line into a project's board, its notes, or the Inbox, from
//! anywhere in the app (and from a phone's share sheet). The Inbox is simply
//! the vault's `Inbox/` folder, so captures are notes until they are filed.
//! Also the link preview that turns a pasted address into the page's title.

use std::{net::IpAddr, time::Duration};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, User},
    board,
    error::{AppError, AppResult},
    notes::{self, markdown, vault},
    projects::{editable_project, log_activity},
    tasks::{self, Task},
    time,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(quick_add)).routes(routes!(file_inbox)).routes(routes!(link_preview))
}

// ---------------------------------------------------------------------------
// Quick add

#[derive(Deserialize, ToSchema)]
pub struct QuickAddRequest {
    /// The line typed (or shared). Becomes the task or note title.
    pub text: String,
    /// `task`, `note` or `inbox`. Task and note need `project_id`.
    pub kind: String,
    pub project_id: Option<String>,
    /// A link that came with it, kept in the task's description or note's body.
    pub url: Option<String>,
    /// More text, such as what a share sheet sent besides the link.
    pub body: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct QuickAddResult {
    /// `task`, `note` or `inbox`.
    pub kind: String,
    /// The new task, when it was a task.
    pub task: Option<Task>,
    /// The new note (also for the Inbox).
    pub note_id: Option<String>,
    pub title: String,
    pub project_id: Option<String>,
    /// The column a task went into.
    pub column: Option<String>,
}

fn bad(code: &'static str, msg: impl Into<String>) -> AppError {
    AppError::BadRequest(code, msg.into())
}

/// The first line, trimmed and cut to a title's length.
fn title_from(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(200).collect();
    if out.chars().count() < line.chars().count() {
        out.push('…');
    }
    out
}

/// A link that is safe to write into markdown, or an error.
fn clean_url(url: &Option<String>) -> AppResult<Option<String>> {
    let Some(u) = url.as_deref().map(str::trim).filter(|u| !u.is_empty()) else { return Ok(None) };
    let parsed = reqwest::Url::parse(u).map_err(|_| bad("invalid_url", "That link isn't a web address."))?;
    if !matches!(parsed.scheme(), "http" | "https") || u.len() > 2000 {
        return Err(bad("invalid_url", "Links start with http:// or https://."));
    }
    Ok(Some(parsed.to_string()))
}

/// The markdown that goes with a capture: the extra text and the link.
fn capture_body(body: &Option<String>, url: &Option<String>, title: &str) -> String {
    let mut parts = vec![];
    if let Some(b) = body.as_deref().map(str::trim).filter(|b| !b.is_empty() && *b != title) {
        // A share sheet often sends the link inside the text too.
        let b = match url {
            Some(u) => b.replace(u.as_str(), "").trim().to_string(),
            None => b.to_string(),
        };
        if !b.is_empty() && b != title {
            parts.push(b);
        }
    }
    if let Some(u) = url {
        parts.push(u.clone());
    }
    parts.join("\n\n")
}

/// The board column quick add uses: the first that counts as to do, else the first.
pub async fn capture_column(conn: &mut SqliteConnection, project_id: &str) -> AppResult<board::Status> {
    let statuses = board::statuses_for(conn, project_id).await?;
    statuses
        .iter()
        .find(|s| s.category == "todo")
        .or_else(|| statuses.iter().find(|s| s.category != "done"))
        .or(statuses.first())
        .cloned()
        .ok_or_else(|| AppError::Conflict("no_columns", "Add a column to the board first.".into()))
}

/// Capture a line as a task, a note filed under a project, or into the Inbox.
#[utoipa::path(post, path = "/quick-add", tag = "capture", request_body = QuickAddRequest,
    responses((status = 201, body = QuickAddResult), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn quick_add(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<QuickAddRequest>,
) -> AppResult<(StatusCode, Json<QuickAddResult>)> {
    Ok((StatusCode::CREATED, Json(add(&state, &user, req).await?)))
}

pub async fn add(state: &AppState, user: &User, req: QuickAddRequest) -> AppResult<QuickAddResult> {
    let url = clean_url(&req.url)?;
    let mut title = title_from(&req.text);
    if title.is_empty() {
        title = match url.as_deref().and_then(|u| reqwest::Url::parse(u).ok()) {
            Some(u) => format!("Link from {}", u.host_str().unwrap_or("the web").trim_start_matches("www.")),
            None => return Err(bad("empty", "Type something to add.")),
        };
    }
    let body = capture_body(&req.body, &url, &title);
    match req.kind.as_str() {
        "task" => {
            let project_id =
                req.project_id.ok_or_else(|| bad("no_project", "Pick a project for the task."))?;
            let column = {
                let mut conn = state.db.acquire().await?;
                editable_project(&mut conn, &user.id, &project_id).await?;
                capture_column(&mut conn, &project_id).await?
            };
            let create = tasks::CreateTaskRequest {
                title: title.clone(),
                status_id: Some(column.id.clone()),
                description: (!body.is_empty()).then_some(body),
                priority: None,
                due_date: None,
                label_ids: vec![],
                assignee_id: None,
                at_top: false,
            };
            let (_, Json(task)) = tasks::create_task(
                State(state.clone()),
                CurrentUser(user.clone()),
                Path(project_id.clone()),
                Json(create),
            )
            .await?;
            Ok(QuickAddResult {
                kind: "task".into(),
                task: Some(task),
                note_id: None,
                title,
                project_id: Some(project_id),
                column: Some(column.name),
            })
        }
        "note" | "inbox" => {
            let inbox = req.kind == "inbox";
            let project_ids = if inbox {
                vec![]
            } else {
                vec![
                    req.project_id
                        .clone()
                        .ok_or_else(|| bad("no_project", "Pick a project for the note."))?,
                ]
            };
            let create = notes::CreateNoteRequest {
                title: Some(notes::safe_title(&title)),
                body: Some(body),
                folder: inbox.then(|| "inbox".to_string()),
                project_ids: project_ids.clone(),
                tags: vec![],
                task_id: None,
            };
            let (_, Json(note)) =
                notes::create_note(State(state.clone()), CurrentUser(user.clone()), Json(create)).await?;
            Ok(QuickAddResult {
                kind: req.kind,
                task: None,
                title: note.note.title.clone(),
                note_id: Some(note.note.id),
                project_id: project_ids.into_iter().next(),
                column: None,
            })
        }
        _ => Err(bad("invalid_kind", "Add it as a task, a note or to the Inbox.")),
    }
}

// ---------------------------------------------------------------------------
// Filing the Inbox

#[derive(Deserialize, ToSchema)]
pub struct FileInboxRequest {
    /// `task` or `note`.
    pub kind: String,
    pub project_id: String,
    /// A new title. Defaults to the note's.
    pub title: Option<String>,
}

/// Move something from the Inbox into a project: as a task (the note's text
/// becomes its description, its photos move to the task, and the note goes),
/// or as a note filed under the project.
#[utoipa::path(post, path = "/inbox/{id}/file", tag = "capture", params(("id" = String, Path)),
    request_body = FileInboxRequest,
    responses((status = 200, body = QuickAddResult), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn file_inbox(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<FileInboxRequest>,
) -> AppResult<Json<QuickAddResult>> {
    Ok(Json(file(&state, &user, &id, req).await?))
}

pub async fn file(
    state: &AppState,
    user: &User,
    id: &str,
    req: FileInboxRequest,
) -> AppResult<QuickAddResult> {
    let mut conn = state.db.acquire().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT path, title FROM notes WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(&user.id)
            .fetch_optional(&mut *conn)
            .await?;
    let (path, note_title) = row.ok_or(AppError::NotFound)?;
    let project = editable_project(&mut conn, &user.id, &req.project_id).await?;
    let title =
        req.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(title_from).unwrap_or(note_title);
    match req.kind.as_str() {
        "note" => {
            drop(conn);
            let update = notes::UpdateNoteRequest {
                title: Some(notes::safe_title(&title)),
                body: None,
                tags: None,
                project_ids: Some(vec![project.id.clone()]),
                folder: Some(format!("{}/{}", vault::PROJECTS, markdown::file_stem(&project.title))),
            };
            let Json(note) = notes::update_note(
                State(state.clone()),
                CurrentUser(user.clone()),
                Path(id.to_string()),
                Json(update),
            )
            .await?;
            Ok(QuickAddResult {
                kind: "note".into(),
                task: None,
                title: note.note.title,
                note_id: Some(id.to_string()),
                project_id: Some(project.id),
                column: None,
            })
        }
        "task" => {
            let root = vault::root(&state.config.data_dir, &user.id);
            let text = vault::read_file(&root, &path).await?;
            let (_, body) = markdown::split(&text);
            // Photo embeds don't render in a task; the photos move to the task instead.
            let description: String = body
                .lines()
                .filter(|l| !(l.trim_start().starts_with("![[") && l.trim_end().ends_with("]]")))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            let column = capture_column(&mut conn, &project.id).await?;
            drop(conn);
            let create = tasks::CreateTaskRequest {
                title: title.clone(),
                status_id: Some(column.id.clone()),
                description: (!description.is_empty()).then_some(description),
                priority: None,
                due_date: None,
                label_ids: vec![],
                assignee_id: None,
                at_top: false,
            };
            let (_, Json(task)) = tasks::create_task(
                State(state.clone()),
                CurrentUser(user.clone()),
                Path(project.id.clone()),
                Json(create),
            )
            .await?;
            move_note_files(state, user, id, &task, &project.owner_id).await?;
            let mut conn = state.db.acquire().await?;
            let _ = tokio::fs::remove_file(root.join(&path)).await;
            vault::prune_empty_dirs(&root, &path).await;
            vault::unindex(&mut conn, id).await?;
            sqlx::query("DELETE FROM task_notes WHERE note_id = ?").bind(id).execute(&mut *conn).await?;
            let task = tasks::visible_task(&mut conn, &user.id, &task.id).await?;
            Ok(QuickAddResult {
                kind: "task".into(),
                task: Some(task),
                note_id: None,
                title,
                project_id: Some(project.id),
                column: Some(column.name),
            })
        }
        _ => Err(bad("invalid_kind", "File it as a task or a note.")),
    }
}

/// Moves a note's photos and files out of the vault onto a task.
async fn move_note_files(
    state: &AppState,
    user: &User,
    note_id: &str,
    task: &Task,
    owner: &str,
) -> AppResult<()> {
    let mut conn = state.db.acquire().await?;
    let files: Vec<(String, String)> = sqlx::query_as("SELECT id, path FROM attachments WHERE note_id = ?")
        .bind(note_id)
        .fetch_all(&mut *conn)
        .await?;
    let month = &time::now()[..7];
    for (id, path) in files {
        let ext = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let file = if ext.is_empty() { id.clone() } else { format!("{id}.{}", ext.to_ascii_lowercase()) };
        let rel = format!("files/{owner}/{month}/{file}");
        let from = state.config.data_dir.join(&path);
        let to = state.config.data_dir.join(&rel);
        if let Some(dir) = to.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(|e| AppError::Internal(e.into()))?;
        }
        if tokio::fs::rename(&from, &to).await.is_err() {
            // Gone from the vault (say, deleted in Obsidian): drop the record.
            sqlx::query("DELETE FROM attachments WHERE id = ?").bind(&id).execute(&mut *conn).await?;
            continue;
        }
        sqlx::query(
            "UPDATE attachments SET note_id = NULL, task_id = ?, project_id = ?, owner_id = ?, path = ? WHERE id = ?",
        )
        .bind(&task.id)
        .bind(&task.project_id)
        .bind(owner)
        .bind(&rel)
        .bind(&id)
        .execute(&mut *conn)
        .await?;
    }
    log_activity(
        &mut conn,
        &task.project_id,
        &user.id,
        "task.filed",
        serde_json::json!({ "title": task.title }),
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Link preview

#[derive(Deserialize, IntoParams)]
pub struct PreviewQuery {
    pub url: String,
}

#[derive(Debug, Default, Serialize, ToSchema, PartialEq)]
pub struct LinkPreview {
    /// Where the page ended up, after redirects.
    pub url: String,
    pub title: Option<String>,
    /// The site's own name, such as Printables.
    pub site: Option<String>,
    /// The page's preview picture, an https address.
    pub image: Option<String>,
}

/// The title, site name and picture of a web page, for a pasted or shared
/// link. Only public addresses are fetched, and only the first 512 KB.
#[utoipa::path(get, path = "/link-preview", tag = "capture", params(PreviewQuery),
    responses((status = 200, body = LinkPreview), (status = 400, body = crate::error::ErrorBody)))]
pub(crate) async fn link_preview(
    CurrentUser(_user): CurrentUser,
    Query(q): Query<PreviewQuery>,
) -> AppResult<Json<LinkPreview>> {
    let url = clean_url(&Some(q.url))?.ok_or_else(|| bad("invalid_url", "Give a link."))?;
    Ok(Json(fetch_preview(&url).await.unwrap_or(LinkPreview { url, ..Default::default() })))
}

const PREVIEW_BYTES: usize = 512 * 1024;

/// Whether an address is on the public internet, so the server never fetches
/// something on its own network for a caller.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

async fn fetch_preview(start: &str) -> Option<LinkPreview> {
    let mut url = reqwest::Url::parse(start).ok()?;
    for _ in 0..5 {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?.trim_start_matches('[').trim_end_matches(']').to_string();
        let port = url.port_or_known_default()?;
        let addrs: Vec<_> =
            tokio::time::timeout(Duration::from_secs(4), tokio::net::lookup_host((host.as_str(), port)))
                .await
                .ok()?
                .ok()?
                .collect();
        let addr = *addrs.first()?;
        if addrs.iter().any(|a| !is_public(a.ip())) {
            return None;
        }
        // Pin the checked address, so a second lookup can't point elsewhere.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .resolve(&host, addr)
            .user_agent("Mozilla/5.0 (compatible; TooManyProjects link preview)")
            .build()
            .ok()?;
        let mut res =
            client.get(url.clone()).header("accept", "text/html,application/xhtml+xml").send().await.ok()?;
        if res.status().is_redirection() {
            let to = res.headers().get("location")?.to_str().ok()?;
            url = url.join(to).ok()?;
            continue;
        }
        if !res.status().is_success() {
            return None;
        }
        let html_type = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_none_or(|t| t.contains("html"));
        if !html_type {
            return Some(LinkPreview { url: url.to_string(), ..Default::default() });
        }
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = res.chunk().await {
            bytes.extend_from_slice(&chunk);
            if bytes.len() >= PREVIEW_BYTES {
                break;
            }
        }
        let mut preview = parse_preview(&String::from_utf8_lossy(&bytes), &url);
        preview.url = url.to_string();
        return Some(preview);
    }
    None
}

/// Reads the title, site name and picture out of a page's HTML.
pub fn parse_preview(html: &str, base: &reqwest::Url) -> LinkPreview {
    let lower = html.to_ascii_lowercase();
    let mut out = LinkPreview::default();
    let mut og_title = None;
    let mut twitter_title = None;
    let mut image = None;
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<meta") {
        let start = pos + i;
        let Some(end) = lower[start..].find('>').map(|e| start + e) else { break };
        let attrs = attributes(&html[start + 5..end]);
        let key =
            attrs.iter().find(|(k, _)| k == "property" || k == "name").map(|(_, v)| v.to_ascii_lowercase());
        let content = attrs.iter().find(|(k, _)| k == "content").map(|(_, v)| decode(v).trim().to_string());
        if let (Some(key), Some(content)) = (key, content.filter(|c| !c.is_empty())) {
            match key.as_str() {
                "og:title" => og_title = og_title.or(Some(content)),
                "twitter:title" => twitter_title = twitter_title.or(Some(content)),
                "og:site_name" => out.site = out.site.or(Some(content)),
                "og:image" | "og:image:url" | "twitter:image" => image = image.or(Some(content)),
                _ => {}
            }
        }
        pos = end;
    }
    let title_tag = lower.find("<title").and_then(|i| {
        let open = i + lower[i..].find('>')? + 1;
        let close = open + lower[open..].find("</title")?;
        Some(decode(&html[open..close]).split_whitespace().collect::<Vec<_>>().join(" "))
    });
    out.title = og_title
        .or(twitter_title)
        .or(title_tag)
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(200).collect());
    out.image =
        image.and_then(|i| base.join(&i).ok()).filter(|u| u.scheme() == "https").map(|u| u.to_string());
    out
}

/// `name="value"` pairs from inside a tag.
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let ks = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        let key = s[ks..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let value = if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                i += 1;
                let vs = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                let v = &s[vs..i];
                i += 1;
                v
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                &s[vs..i]
            };
            if !key.is_empty() {
                out.push((key, value.to_string()));
            }
        } else if key.is_empty() {
            i += 1;
        }
    }
    out
}

/// Decodes the HTML entities that turn up in titles.
fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") || e.starts_with("#X") => {
                u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
            }
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.10",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "151.101.1.1", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn reads_open_graph_tags() {
        let html = r#"<html><head><title>Fallback</title>
            <meta property="og:title" content="Articulated dragon, print-in-place &amp; more">
            <meta content='Printables' property='og:site_name'/>
            <meta property="og:image" content="/media/dragon.jpg">
            </head></html>"#;
        let base = reqwest::Url::parse("https://www.printables.com/model/1-dragon").unwrap();
        let p = parse_preview(html, &base);
        assert_eq!(p.title.as_deref(), Some("Articulated dragon, print-in-place & more"));
        assert_eq!(p.site.as_deref(), Some("Printables"));
        assert_eq!(p.image.as_deref(), Some("https://www.printables.com/media/dragon.jpg"));
    }

    #[test]
    fn falls_back_to_the_title_tag() {
        let base = reqwest::Url::parse("http://example.com/").unwrap();
        let p = parse_preview("<TITLE>\n  A  page &#8211; here\n</TITLE>", &base);
        assert_eq!(p.title.as_deref(), Some("A page – here"));
        assert_eq!(p.image, None);
    }

    #[test]
    fn titles_are_one_line() {
        assert_eq!(title_from("\n  first line \nsecond"), "first line");
        assert_eq!(title_from(&"x".repeat(250)).chars().count(), 201);
    }

    #[test]
    fn body_keeps_the_link_once() {
        let url = Some("https://a.example/x".to_string());
        assert_eq!(
            capture_body(&Some("Look at this https://a.example/x".into()), &url, "Dragon"),
            "Look at this\n\nhttps://a.example/x"
        );
        assert_eq!(capture_body(&None, &url, "Dragon"), "https://a.example/x");
    }
}
