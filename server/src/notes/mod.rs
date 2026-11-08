//! Notes: a markdown vault per person, with wiki links, backlinks, tags,
//! project links and full-text search. See `vault` for the files and index,
//! `markdown` for the text handling and `transfer` for zip import and export.

pub mod markdown;
pub mod transfer;
pub mod vault;

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    projects::{editable_project, visible_project},
    time,
};
use markdown::FrontMatter;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_notes, create_note))
        .routes(routes!(get_note, update_note, delete_note))
        .routes(routes!(list_tags))
        .routes(routes!(project_notes))
        .routes(routes!(download_note))
        .routes(routes!(rescan))
        .routes(routes!(link_task_note, unlink_task_note))
        .routes(routes!(transfer::export_vault))
        .routes(routes!(transfer::import_vault))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Serialize, ToSchema)]
pub struct NoteProject {
    pub id: String,
    pub title: String,
    pub color: String,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct NoteSummary {
    pub id: String,
    /// The file name without `.md`.
    pub title: String,
    /// Path inside the vault, such as `Knowledge/Cars/Bleeding brakes.md`.
    pub path: String,
    /// First folder of the path: `Inbox`, `Knowledge`, `Projects` or another.
    pub folder: String,
    pub tags: Vec<String>,
    pub projects: Vec<NoteProject>,
    /// The first line of text.
    pub excerpt: String,
    /// Search results only: the matching words in context, with `[` and `]`
    /// around each hit.
    pub snippet: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Whose vault it is in, when that isn't yours.
    pub owner_name: Option<String>,
    /// False when it is shared with you to read only.
    pub can_edit: bool,
}

#[derive(FromRow)]
struct NoteRow {
    id: String,
    /// Whose vault holds the file.
    user_id: String,
    title: String,
    path: String,
    folder: String,
    tags: String,
    excerpt: String,
    created_at: String,
    updated_at: String,
    #[sqlx(default)]
    snippet: Option<String>,
}

const NOTE_COLUMNS: &str =
    "n.id, n.user_id, n.title, n.path, n.folder, n.tags, n.excerpt, n.created_at, n.updated_at";

/// Where a `[[link]]` in the note leads.
#[derive(Serialize, ToSchema)]
pub struct NoteLink {
    /// `note` or `project`.
    pub kind: String,
    pub title: String,
    /// Missing when nothing has that title yet.
    pub id: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct NoteDetail {
    pub note: NoteSummary,
    /// Markdown, without the front matter.
    pub body: String,
    /// Notes that link here.
    pub backlinks: Vec<NoteSummary>,
    pub links: Vec<NoteLink>,
    /// Tasks this note is linked to, among those you can see.
    pub tasks: Vec<crate::tasks::Task>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateNoteRequest {
    /// Defaults to "Untitled" (or the date, for the Inbox).
    pub title: Option<String>,
    pub body: Option<String>,
    /// `inbox`, `knowledge`, or a folder path. Defaults to the first
    /// project's folder when `project_ids` is set, else Knowledge.
    pub folder: Option<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Link the new note to this task (you need to be able to change it).
    /// Without `project_ids` or `folder`, the note is filed under the task's project.
    pub task_id: Option<String>,
}

/// Fields left out stay as they are.
#[derive(Deserialize, ToSchema)]
pub struct UpdateNoteRequest {
    /// Renames the file and updates links to it in other notes.
    pub title: Option<String>,
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
    pub project_ids: Option<Vec<String>>,
    /// Moves the file to this folder.
    pub folder: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct ListNotesQuery {
    /// Full-text search over titles and bodies.
    pub q: Option<String>,
    pub tag: Option<String>,
    pub project_id: Option<String>,
    /// First folder of the path, such as `Inbox`. Your own notes only.
    pub folder: Option<String>,
    /// True: only notes others have shared with you. False: only yours.
    /// Left out: both.
    pub shared: Option<bool>,
    /// Default 200, at most 500.
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectNotes {
    /// Notes that list the project in their front matter.
    pub notes: Vec<NoteSummary>,
    /// Other notes that mention it with `[[@Project]]`.
    pub mentions: Vec<NoteSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct ScanResult {
    /// Files read again because they were new or had changed.
    pub indexed: usize,
}

// ---------------------------------------------------------------------------
// Helpers

/// Turns index rows into summaries, with each note's projects (those the
/// viewer can see) and what the viewer may do with it.
async fn summaries(
    conn: &mut SqliteConnection,
    viewer: &str,
    rows: Vec<NoteRow>,
) -> AppResult<Vec<NoteSummary>> {
    let mut projects: HashMap<String, Vec<NoteProject>> = HashMap::new();
    let mut access: HashMap<String, String> = HashMap::new();
    let mut owners: HashMap<String, String> = HashMap::new();
    if !rows.is_empty() {
        let mut q: QueryBuilder<Sqlite> =
            QueryBuilder::new("SELECT note_id, role FROM note_access WHERE user_id = ");
        q.push_bind(viewer);
        q.push(" AND note_id IN (");
        let mut sep = q.separated(", ");
        for r in &rows {
            sep.push_bind(&r.id);
        }
        q.push(")");
        access = q.build_query_as::<(String, String)>().fetch_all(&mut *conn).await?.into_iter().collect();
        for r in rows.iter().filter(|r| r.user_id != viewer) {
            if !owners.contains_key(&r.user_id) {
                let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = ?")
                    .bind(&r.user_id)
                    .fetch_one(&mut *conn)
                    .await?;
                owners.insert(r.user_id.clone(), name);
            }
        }
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT np.note_id, p.id, p.title, p.color FROM note_projects np JOIN projects p ON p.id = np.project_id
             JOIN project_access a ON a.project_id = p.id AND a.user_id = ",
        );
        q.push_bind(viewer);
        q.push(" WHERE np.note_id IN (");
        let mut sep = q.separated(", ");
        for r in &rows {
            sep.push_bind(&r.id);
        }
        q.push(") ORDER BY p.title COLLATE NOCASE");
        let found: Vec<(String, String, String, String)> = q.build_query_as().fetch_all(&mut *conn).await?;
        for (note_id, id, title, color) in found {
            projects.entry(note_id).or_default().push(NoteProject { id, title, color });
        }
    }
    Ok(rows
        .into_iter()
        .map(|r| NoteSummary {
            owner_name: owners.get(&r.user_id).filter(|_| r.user_id != viewer).cloned(),
            can_edit: access.get(&r.id).is_some_and(|role| role != "viewer"),
            projects: projects.remove(&r.id).unwrap_or_default(),
            tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            id: r.id,
            title: r.title,
            path: r.path,
            folder: r.folder,
            excerpt: r.excerpt,
            snippet: r.snippet,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// A note the person may read, else 404.
async fn note_row(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<NoteRow> {
    sqlx::query_as::<_, NoteRow>(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes n
         WHERE n.id = ? AND EXISTS (SELECT 1 FROM note_access na WHERE na.note_id = n.id AND na.user_id = ?)"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// A note the person may change, else 404 or 403.
async fn editable_note(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<NoteRow> {
    let row = note_row(conn, user_id, id).await?;
    let role: String = sqlx::query_scalar("SELECT role FROM note_access WHERE note_id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await?;
    if role == "viewer" {
        return Err(AppError::Forbidden(format!(
            "“{}” is shared with you to read, not to change.",
            row.title
        )));
    }
    Ok(row)
}

fn clean_tags(tags: &[String]) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for t in tags {
        let t = t.trim().trim_start_matches('#').replace(' ', "-");
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > 60 || t.contains([',', '[', ']', '"']) {
            return Err(AppError::BadRequest(
                "invalid_tag",
                "Tags are up to 60 characters, without commas.".into(),
            ));
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    Ok(out)
}

fn clean_title(title: &str) -> AppResult<String> {
    let t = title.trim();
    if t.chars().count() > 120 {
        return Err(AppError::BadRequest("invalid_title", "Keep note titles under 120 characters.".into()));
    }
    Ok(markdown::file_stem(t))
}

fn check_body(body: &str) -> AppResult<()> {
    if body.len() as u64 > vault::MAX_NOTE_BYTES {
        return Err(AppError::BadRequest("note_too_large", "Notes can be up to 2 MB.".into()));
    }
    Ok(())
}

/// Titles of the given projects. Filing a note under a project shares it
/// with everyone on that project, so a project not already on the note
/// (`keep`) needs edit rights.
async fn project_titles(
    conn: &mut SqliteConnection,
    user_id: &str,
    ids: &[String],
    keep: &[String],
) -> AppResult<Vec<String>> {
    let mut titles = vec![];
    for id in ids {
        let found = if keep.contains(id) {
            visible_project(conn, user_id, id).await
        } else {
            editable_project(conn, user_id, id).await
        };
        let p = found.map_err(|e| match e {
            AppError::Forbidden(m) => AppError::Forbidden(m),
            _ => AppError::BadRequest("invalid_project", "One of those projects doesn't exist.".into()),
        })?;
        if !titles.contains(&p.title) {
            titles.push(p.title);
        }
    }
    Ok(titles)
}

fn folder_for(choice: Option<&str>, first_project: Option<&str>) -> String {
    match choice.map(str::trim) {
        Some(f) if f.eq_ignore_ascii_case("inbox") => vault::INBOX.into(),
        Some(f) if f.eq_ignore_ascii_case("knowledge") => vault::KNOWLEDGE.into(),
        Some(f) if !f.is_empty() => vault::clean_folder(f),
        _ => match first_project {
            Some(p) => format!("{}/{}", vault::PROJECTS, markdown::file_stem(p)),
            None => vault::KNOWLEDGE.into(),
        },
    }
}

/// Rewrites every note that links to `old` so it links to `new` instead.
async fn rewrite_links(
    conn: &mut SqliteConnection,
    root: &std::path::Path,
    user_id: &str,
    kind: &str,
    old: &str,
    new: &str,
) -> AppResult<()> {
    let prefix = if kind == "project" { "@" } else { "" };
    let linking: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT n.id, n.path FROM note_links l JOIN notes n ON n.id = l.note_id
         WHERE n.user_id = ? AND l.target_kind = ? AND l.target_title = ?",
    )
    .bind(user_id)
    .bind(kind)
    .bind(old)
    .fetch_all(&mut *conn)
    .await?;
    for (_, path) in linking {
        let Ok(text) = vault::read_file(root, &path).await else { continue };
        let (fm, body) = markdown::split(&text);
        let new_body = vault::rename_links(&body, prefix, old, new);
        if new_body != body {
            vault::write_file(root, &path, &markdown::render(&fm, &new_body)).await?;
            vault::index_file(conn, root, user_id, &path).await?;
        }
    }
    Ok(())
}

/// Called after a project is renamed: notes that list it or link to it follow.
pub async fn project_renamed(state: &AppState, user_id: &str, old: &str, new: &str) -> AppResult<()> {
    if old == new {
        return Ok(());
    }
    let root = vault::root(&state.config.data_dir, user_id);
    let mut conn = state.db.acquire().await?;
    let listed: Vec<String> = sqlx::query_scalar(
        "SELECT n.path FROM notes n JOIN note_projects np ON np.note_id = n.id JOIN projects p ON p.id = np.project_id
         WHERE n.user_id = ? AND p.title = ?",
    )
    .bind(user_id)
    .bind(new)
    .fetch_all(&mut *conn)
    .await?;
    for path in listed {
        let Ok(text) = vault::read_file(&root, &path).await else { continue };
        let (mut fm, body) = markdown::split(&text);
        let mut changed = false;
        for p in fm.projects.iter_mut() {
            if p.eq_ignore_ascii_case(old) {
                *p = new.to_string();
                changed = true;
            }
        }
        if changed {
            vault::write_file(&root, &path, &markdown::render(&fm, &body)).await?;
            vault::index_file(&mut conn, &root, user_id, &path).await?;
        }
    }
    rewrite_links(&mut conn, &root, user_id, "project", old, new).await
}

// ---------------------------------------------------------------------------
// Handlers

/// Your notes and those shared with you, newest first, or the best matches for `q`.
#[utoipa::path(get, path = "/notes", tag = "notes", params(ListNotesQuery),
    responses((status = 200, body = Vec<NoteSummary>)))]
pub(crate) async fn list_notes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ListNotesQuery>,
) -> AppResult<Json<Vec<NoteSummary>>> {
    let mut conn = state.db.acquire().await?;
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(fts_query);
    let mut sql: QueryBuilder<Sqlite> = QueryBuilder::new(format!("SELECT {NOTE_COLUMNS}"));
    if search.is_some() {
        sql.push(", snippet(notes_fts, 2, '[', ']', '…', 12) AS snippet FROM notes_fts f JOIN notes n ON n.id = f.note_id");
    } else {
        sql.push(" FROM notes n");
    }
    sql.push(" WHERE EXISTS (SELECT 1 FROM note_access na WHERE na.note_id = n.id AND na.user_id = ");
    sql.push_bind(&user.id);
    sql.push(")");
    match q.shared {
        Some(true) => {
            sql.push(" AND n.user_id <> ");
            sql.push_bind(&user.id);
        }
        Some(false) => {
            sql.push(" AND n.user_id = ");
            sql.push_bind(&user.id);
        }
        None => {}
    }
    if let Some(s) = &search {
        sql.push(" AND notes_fts MATCH ");
        sql.push_bind(s.clone());
    }
    if let Some(tag) = q.tag.as_deref().filter(|t| !t.is_empty()) {
        sql.push(" AND EXISTS (SELECT 1 FROM json_each(n.tags) WHERE value = ");
        sql.push_bind(tag.trim_start_matches('#').to_string());
        sql.push(" COLLATE NOCASE)");
    }
    if let Some(p) = &q.project_id {
        sql.push(" AND EXISTS (SELECT 1 FROM note_projects np WHERE np.note_id = n.id AND np.project_id = ");
        sql.push_bind(p);
        sql.push(")");
    }
    if let Some(f) = q.folder.as_deref().filter(|f| !f.is_empty()) {
        sql.push(" AND n.user_id = ");
        sql.push_bind(&user.id);
        sql.push(" AND n.folder = ");
        sql.push_bind(f.to_string());
        sql.push(" COLLATE NOCASE");
    }
    sql.push(if search.is_some() {
        " ORDER BY bm25(notes_fts, 0, 5, 1)"
    } else {
        " ORDER BY n.updated_at DESC"
    });
    sql.push(" LIMIT ");
    sql.push_bind(limit);
    let rows: Vec<NoteRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(Json(summaries(&mut conn, &user.id, rows).await?))
}

/// Each word as a quoted prefix term, so punctuation can't break the query
/// and `brak` finds `brakes`.
fn fts_query(q: &str) -> String {
    q.split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write a new note to the vault.
#[utoipa::path(post, path = "/notes", tag = "notes", request_body = CreateNoteRequest,
    responses((status = 201, body = NoteDetail), (status = 400, body = crate::error::ErrorBody)))]
pub(crate) async fn create_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateNoteRequest>,
) -> AppResult<(StatusCode, Json<NoteDetail>)> {
    let inbox = req.folder.as_deref().is_some_and(|f| f.eq_ignore_ascii_case("inbox"));
    let title = match req.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => clean_title(t)?,
        None if inbox => chrono::Utc::now().format("%Y-%m-%d %H.%M").to_string(),
        None => "Untitled".into(),
    };
    let body = req.body.unwrap_or_default();
    check_body(&body)?;
    let tags = clean_tags(&req.tags)?;
    let mut conn = state.db.acquire().await?;
    let task = match &req.task_id {
        Some(t) => Some(crate::tasks::editable_task(&mut conn, &user.id, t).await?),
        None => None,
    };
    let project_ids = match &task {
        Some(t) if req.project_ids.is_empty() && req.folder.is_none() => vec![t.project_id.clone()],
        _ => req.project_ids.clone(),
    };
    let projects = project_titles(&mut conn, &user.id, &project_ids, &[]).await?;
    let folder = folder_for(req.folder.as_deref(), projects.first().map(String::as_str));

    let root = vault::root(&state.config.data_dir, &user.id);
    let rel = vault::free_path(&root, &folder, &title, None);
    let now = time::now();
    let fm = FrontMatter {
        id: Some(time::new_id()),
        tags,
        projects,
        created: Some(now.clone()),
        updated: Some(now),
        other: vec![],
    };
    vault::write_file(&root, &rel, &markdown::render(&fm, &body)).await?;
    let id = vault::index_file(&mut conn, &root, &user.id, &rel).await?;
    if let Some(task) = &task {
        add_task_link(&mut conn, &user.id, task, &id).await?;
    }
    Ok((StatusCode::CREATED, Json(detail(&mut conn, &state.config.data_dir, &user.id, &id).await?)))
}

/// A note as `viewer` sees it. Its links resolve inside the vault it lives in,
/// and only to notes and projects the viewer can see.
async fn detail(
    conn: &mut SqliteConnection,
    data_dir: &std::path::Path,
    viewer: &str,
    id: &str,
) -> AppResult<NoteDetail> {
    let row = note_row(conn, viewer, id).await?;
    let root = vault::root(data_dir, &row.user_id);
    let vault_owner = row.user_id.clone();
    let note = summaries(conn, viewer, vec![row]).await?.remove(0);
    let text = vault::read_file(&root, &note.path).await?;
    let (_, body) = markdown::split(&text);
    let backlinks = sqlx::query_as::<_, NoteRow>(&format!(
        "SELECT DISTINCT {NOTE_COLUMNS} FROM note_links l JOIN notes n ON n.id = l.note_id
         JOIN note_access na ON na.note_id = n.id AND na.user_id = ?
         WHERE n.user_id = ? AND l.target_kind = 'note' AND l.target_title = ? AND n.id <> ?
         ORDER BY n.updated_at DESC"
    ))
    .bind(viewer)
    .bind(&vault_owner)
    .bind(&note.title)
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let backlinks = summaries(conn, viewer, backlinks).await?;
    let mut links = vec![];
    for l in markdown::wiki_links(&body) {
        let kind = if l.project { "project" } else { "note" };
        let found: Option<String> = if l.project {
            sqlx::query_scalar(
                "SELECT p.id FROM projects p JOIN project_access a ON a.project_id = p.id
                 WHERE a.user_id = ? AND p.title = ? COLLATE NOCASE
                 ORDER BY p.owner_id = ? DESC LIMIT 1",
            )
            .bind(viewer)
            .bind(&l.title)
            .bind(&vault_owner)
        } else {
            sqlx::query_scalar(
                "SELECT n.id FROM notes n JOIN note_access na ON na.note_id = n.id AND na.user_id = ?
                 WHERE n.user_id = ? AND n.title = ? COLLATE NOCASE ORDER BY n.updated_at DESC LIMIT 1",
            )
            .bind(viewer)
            .bind(&vault_owner)
            .bind(&l.title)
        }
        .fetch_optional(&mut *conn)
        .await?;
        links.push(NoteLink { kind: kind.into(), title: l.title, id: found });
    }
    let tasks = crate::tasks::note_tasks(conn, viewer, id).await?;
    Ok(NoteDetail { note, body, backlinks, links, tasks })
}

/// One note with its body, backlinks and resolved links.
#[utoipa::path(get, path = "/notes/{id}", tag = "notes", params(("id" = String, Path)),
    responses((status = 200, body = NoteDetail), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn get_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<NoteDetail>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(detail(&mut conn, &state.config.data_dir, &user.id, &id).await?))
}

/// Edit a note. A new title renames the file and fixes links to it.
#[utoipa::path(patch, path = "/notes/{id}", tag = "notes", params(("id" = String, Path)),
    request_body = UpdateNoteRequest,
    responses((status = 200, body = NoteDetail), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> AppResult<Json<NoteDetail>> {
    let mut conn = state.db.acquire().await?;
    let current = editable_note(&mut conn, &user.id, &id).await?;
    // An editor of a shared project changes the file where it lives.
    let owner = current.user_id.clone();
    let root = vault::root(&state.config.data_dir, &owner);
    let text = vault::read_file(&root, &current.path).await?;
    let (mut fm, mut body) = markdown::split(&text);

    if let Some(b) = req.body {
        check_body(&b)?;
        body = b;
    }
    if let Some(tags) = &req.tags {
        fm.tags = clean_tags(tags)?;
    }
    if let Some(ids) = &req.project_ids {
        let keep: Vec<String> = sqlx::query_scalar("SELECT project_id FROM note_projects WHERE note_id = ?")
            .bind(&id)
            .fetch_all(&mut *conn)
            .await?;
        fm.projects = project_titles(&mut conn, &user.id, ids, &keep).await?;
    }
    fm.id = Some(id.clone());
    fm.updated = Some(time::now());
    if fm.created.is_none() {
        fm.created = Some(current.created_at.clone());
    }

    let title = match req.title.as_deref().map(str::trim) {
        Some("") => return Err(AppError::BadRequest("invalid_title", "Give the note a title.".into())),
        Some(t) => clean_title(t)?,
        None => current.title.clone(),
    };
    let folder = match &req.folder {
        Some(f) => folder_for(Some(f), None),
        None => current.path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default(),
    };
    let same_place =
        title == current.title && folder == current.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let rel = if same_place {
        current.path.clone()
    } else if title.eq_ignore_ascii_case(&current.title)
        && folder == current.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    {
        // Only the case changed: keep the name free of a " 2" suffix.
        let f = if folder.is_empty() { String::new() } else { format!("{folder}/") };
        format!("{f}{title}.md")
    } else {
        vault::free_path(&root, &folder, &title, Some(&current.path))
    };

    vault::write_file(&root, &current.path, &markdown::render(&fm, &body)).await?;
    if rel != current.path {
        let to = root.join(&rel);
        if let Some(dir) = to.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(|e| AppError::Internal(e.into()))?;
        }
        tokio::fs::rename(root.join(&current.path), &to).await.map_err(|e| AppError::Internal(e.into()))?;
        vault::prune_empty_dirs(&root, &current.path).await;
    }
    vault::index_file(&mut conn, &root, &owner, &rel).await?;
    if title != current.title {
        rewrite_links(&mut conn, &root, &owner, "note", &current.title, &title).await?;
    }
    Ok(Json(detail(&mut conn, &state.config.data_dir, &user.id, &id).await?))
}

/// Delete a note's file for good. Only from your own vault.
#[utoipa::path(delete, path = "/notes/{id}", tag = "notes", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let root = vault::root(&state.config.data_dir, &user.id);
    let mut conn = state.db.acquire().await?;
    let row = note_row(&mut conn, &user.id, &id).await?;
    if row.user_id != user.id {
        return Err(AppError::Forbidden("Only the person whose note it is can delete it.".into()));
    }
    let _ = tokio::fs::remove_file(root.join(&row.path)).await;
    vault::prune_empty_dirs(&root, &row.path).await;
    vault::unindex(&mut conn, &id).await?;
    sqlx::query("DELETE FROM task_notes WHERE note_id = ?").bind(&id).execute(&mut *conn).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Every tag in use, most used first.
#[utoipa::path(get, path = "/notes/tags", tag = "notes", responses((status = 200, body = Vec<TagCount>)))]
pub(crate) async fn list_tags(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<TagCount>>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.value, COUNT(*) FROM notes n, json_each(n.tags) t WHERE n.user_id = ?
         GROUP BY t.value COLLATE NOCASE ORDER BY COUNT(*) DESC, t.value COLLATE NOCASE",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(|(tag, count)| TagCount { tag, count }).collect()))
}

/// A project's notes, and other notes that mention it.
#[utoipa::path(get, path = "/projects/{id}/notes", tag = "notes", params(("id" = String, Path)),
    responses((status = 200, body = ProjectNotes), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn project_notes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<ProjectNotes>> {
    let mut conn = state.db.acquire().await?;
    let project = visible_project(&mut conn, &user.id, &id).await?;
    let notes = sqlx::query_as::<_, NoteRow>(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes n JOIN note_projects np ON np.note_id = n.id
         JOIN note_access na ON na.note_id = n.id
         WHERE na.user_id = ? AND np.project_id = ? ORDER BY n.updated_at DESC"
    ))
    .bind(&user.id)
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    let mentions = sqlx::query_as::<_, NoteRow>(&format!(
        "SELECT DISTINCT {NOTE_COLUMNS} FROM note_links l JOIN notes n ON n.id = l.note_id
         JOIN note_access na ON na.note_id = n.id
         WHERE na.user_id = ? AND l.target_kind = 'project' AND l.target_title = ?
           AND NOT EXISTS (SELECT 1 FROM note_projects np WHERE np.note_id = n.id AND np.project_id = ?)
         ORDER BY n.updated_at DESC"
    ))
    .bind(&user.id)
    .bind(&project.title)
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(ProjectNotes {
        notes: summaries(&mut conn, &user.id, notes).await?,
        mentions: summaries(&mut conn, &user.id, mentions).await?,
    }))
}

/// The note's file as it is on disk, front matter included.
#[utoipa::path(get, path = "/notes/{id}/download", tag = "notes", params(("id" = String, Path)),
    responses((status = 200, content_type = "text/markdown", body = String),
        (status = 404, body = crate::error::ErrorBody)))]
async fn download_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let mut conn = state.db.acquire().await?;
    let row = note_row(&mut conn, &user.id, &id).await?;
    let root = vault::root(&state.config.data_dir, &row.user_id);
    let text = vault::read_file(&root, &row.path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, transfer::attachment(&format!("{}.md", row.title))),
        ],
        text,
    )
        .into_response())
}

/// Re-read the vault now, picking up files changed outside the app.
#[utoipa::path(post, path = "/notes/rescan", tag = "notes", responses((status = 200, body = ScanResult)))]
async fn rescan(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<ScanResult>> {
    let indexed = vault::scan(&state.db, &state.config.data_dir, &user.id).await?;
    Ok(Json(ScanResult { indexed }))
}

// ---------------------------------------------------------------------------
// Notes linked to tasks

/// The notes linked to a task that `viewer` can read, newest first, and how
/// many more are linked that aren't shared with them.
pub async fn task_notes(
    conn: &mut SqliteConnection,
    viewer: &str,
    task_id: &str,
) -> AppResult<(Vec<NoteSummary>, i64)> {
    let rows = sqlx::query_as::<_, NoteRow>(&format!(
        "SELECT {NOTE_COLUMNS} FROM task_notes tn JOIN notes n ON n.id = tn.note_id
         WHERE tn.task_id = ?
           AND EXISTS (SELECT 1 FROM note_access na WHERE na.note_id = n.id AND na.user_id = ?)
         ORDER BY n.updated_at DESC"
    ))
    .bind(task_id)
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_notes tn JOIN notes n ON n.id = tn.note_id WHERE tn.task_id = ?",
    )
    .bind(task_id)
    .fetch_one(&mut *conn)
    .await?;
    let hidden = total - rows.len() as i64;
    Ok((summaries(conn, viewer, rows).await?, hidden))
}

/// Links a note `user_id` can read to a task they may change. Linking twice is fine.
async fn add_task_link(
    conn: &mut SqliteConnection,
    user_id: &str,
    task: &crate::tasks::Task,
    note_id: &str,
) -> AppResult<()> {
    note_row(conn, user_id, note_id).await?;
    let added = sqlx::query(
        "INSERT OR IGNORE INTO task_notes (task_id, note_id, created_by, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&task.id)
    .bind(note_id)
    .bind(user_id)
    .bind(time::now())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if added > 0 {
        crate::tasks::log_task(
            conn,
            &task.project_id,
            &task.id,
            user_id,
            "task.note_linked",
            serde_json::json!({ "title": task.title }),
        )
        .await?;
    }
    Ok(())
}

/// Link a note to a task, so each shows on the other. Needs edit rights on
/// the task's project and read access to the note. Linking doesn't share the
/// note: people on the project who can't read it see only that one is linked.
#[utoipa::path(put, path = "/tasks/{id}/notes/{note_id}", tag = "tasks",
    params(("id" = String, Path), ("note_id" = String, Path)),
    responses((status = 200, body = NoteSummary), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn link_task_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, note_id)): Path<(String, String)>,
) -> AppResult<Json<NoteSummary>> {
    let mut tx = state.db.begin().await?;
    let task = crate::tasks::editable_task(&mut tx, &user.id, &id).await?;
    add_task_link(&mut tx, &user.id, &task, &note_id).await?;
    let row = note_row(&mut tx, &user.id, &note_id).await?;
    let note = summaries(&mut tx, &user.id, vec![row]).await?.remove(0);
    tx.commit().await?;
    Ok(Json(note))
}

/// Unlink a note from a task. The note itself is kept.
#[utoipa::path(delete, path = "/tasks/{id}/notes/{note_id}", tag = "tasks",
    params(("id" = String, Path), ("note_id" = String, Path)),
    responses((status = 204), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn unlink_task_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, note_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    crate::tasks::editable_task(&mut tx, &user.id, &id).await?;
    let gone = sqlx::query("DELETE FROM task_notes WHERE task_id = ? AND note_id = ?")
        .bind(&id)
        .bind(&note_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
