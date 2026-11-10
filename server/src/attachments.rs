//! Photos and files: attached to a project, a task, a comment in a task's log,
//! a database row or a note.
//!
//! A file follows the access of the thing it is attached to: whoever can see
//! the project (or the note) can see the file, and whoever can change it can
//! add, caption and remove files. Files are served only through
//! [`get_file`], never from a public URL.
//!
//! Storage lives on the data volume: `files/<owner>/<yyyy-mm>/<id>.<ext>`, with
//! previews in `files/thumbs/`. A note's files go in its vault's `attachments/`
//! folder instead, so Obsidian sees them. Uploads stream to disk in pieces, so
//! a 25 MB photo never sits whole in memory.

use std::{
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use anyhow::Context;
use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path as UrlPath, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    notes::{self, vault},
    projects::{editable_project, require_edit, via, visible_project},
    tasks::{editable_task, visible_task},
    time,
};

/// Preview sizes, the longest side in pixels.
pub const SMALL: u32 = 400;
pub const LARGE: u32 = 1600;
/// Longest caption, in characters.
const MAX_CAPTION: usize = 300;
/// Most files one listing returns.
const MAX_LIST: i64 = 2000;
/// Types a browser may show in place. Anything else downloads as bytes, so an
/// uploaded web page or SVG can never run in the app's origin.
const INLINE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp", "application/pdf"];

/// Most bytes one upload request may carry. Each file in it is held to
/// `TMP_MAX_UPLOAD_MB` as it streams in.
const MAX_REQUEST_BYTES: usize = 1024 * 1024 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_attachments))
        .routes(routes!(update_attachment, delete_attachment))
        .routes(routes!(get_file))
        .routes(routes!(note_file))
        .routes(routes!(set_cover))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(upload))
                .layer(axum::extract::DefaultBodyLimit::max(MAX_REQUEST_BYTES)),
        )
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Attachment {
    pub id: String,
    /// Missing only for a note's file.
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub task_id: Option<String>,
    /// Shown as T-12.
    pub task_number: Option<i64>,
    pub task_title: Option<String>,
    /// Set when it was added with a comment in the task's log.
    pub comment_id: Option<String>,
    pub row_id: Option<String>,
    pub note_id: Option<String>,
    pub note_title: Option<String>,
    /// The file's name. For a note's file, its name in the vault, as used in
    /// an `![[embed]]`.
    pub name: String,
    pub mime: String,
    /// Bytes.
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// When the photo was taken, from the camera (local time, no zone).
    pub taken_at: Option<String>,
    pub caption: Option<String>,
    /// An image with previews: ask for `?size=400` or `?size=1600`.
    pub is_image: bool,
    pub added_by: Option<String>,
    pub added_by_name: Option<String>,
    /// `web` or `ai`.
    pub via: String,
    pub created_at: String,
    /// You may caption or remove it.
    pub can_edit: bool,
}

#[derive(FromRow)]
struct AttachmentRow {
    id: String,
    project_id: Option<String>,
    project_title: Option<String>,
    task_id: Option<String>,
    task_number: Option<i64>,
    task_title: Option<String>,
    comment_id: Option<String>,
    row_id: Option<String>,
    note_id: Option<String>,
    note_title: Option<String>,
    name: String,
    mime: String,
    size: i64,
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<String>,
    caption: Option<String>,
    has_thumbs: bool,
    added_by: Option<String>,
    added_by_name: Option<String>,
    via: String,
    created_at: String,
    role: String,
    path: String,
}

impl AttachmentRow {
    fn into_api(self) -> Attachment {
        Attachment {
            can_edit: self.role != "viewer",
            id: self.id,
            project_id: self.project_id,
            project_title: self.project_title,
            task_id: self.task_id,
            task_number: self.task_number,
            task_title: self.task_title,
            comment_id: self.comment_id,
            row_id: self.row_id,
            note_id: self.note_id,
            note_title: self.note_title,
            name: self.name,
            mime: self.mime,
            size: self.size,
            width: self.width,
            height: self.height,
            taken_at: self.taken_at,
            caption: self.caption,
            is_image: self.has_thumbs,
            added_by: self.added_by,
            added_by_name: self.added_by_name,
            via: self.via,
            created_at: self.created_at,
        }
    }
}

/// Every attachment the viewer (bound twice, first) may see, with their role
/// on it. Wrap it and filter on its columns.
const VISIBLE: &str = "SELECT * FROM (
    SELECT a.id, a.project_id, p.title AS project_title, a.task_id, t.number AS task_number,
        t.title AS task_title, a.comment_id, a.row_id, a.note_id, n.title AS note_title, a.name,
        a.mime, a.size, a.width, a.height, a.taken_at, a.caption, a.has_thumbs, a.added_by,
        u.name AS added_by_name, a.via, a.created_at, a.path, a.owner_id,
        COALESCE(
            (SELECT pa.role FROM project_access pa WHERE pa.project_id = a.project_id AND pa.user_id = ?),
            (SELECT na.role FROM note_access na WHERE na.note_id = a.note_id AND na.user_id = ?)
        ) AS role,
        COALESCE(a.taken_at, a.created_at) AS sort_at
    FROM attachments a
    LEFT JOIN projects p ON p.id = a.project_id
    LEFT JOIN tasks t ON t.id = a.task_id
    LEFT JOIN notes n ON n.id = a.note_id
    LEFT JOIN users u ON u.id = a.added_by
) v WHERE v.role IS NOT NULL";

fn visible<'a>(viewer: &'a str) -> QueryBuilder<'a, Sqlite> {
    let mut q = QueryBuilder::new("");
    push_visible(&mut q, viewer);
    q
}

/// Pushes [`VISIBLE`] with the viewer bound in both places.
fn push_visible<'a>(q: &mut QueryBuilder<'a, Sqlite>, viewer: &'a str) {
    let (head, rest) = VISIBLE.split_once('?').unwrap();
    let (mid, tail) = rest.split_once('?').unwrap();
    q.push(head).push_bind(viewer).push(mid).push_bind(viewer).push(tail);
}

async fn visible_row(conn: &mut SqliteConnection, viewer: &str, id: &str) -> AppResult<AttachmentRow> {
    let mut q = visible(viewer);
    q.push(" AND v.id = ").push_bind(id);
    q.build_query_as::<AttachmentRow>().fetch_optional(conn).await?.ok_or(AppError::NotFound)
}

/// Every file on a task the viewer can see, comments' files included, in the
/// order they were added.
pub async fn for_task(
    conn: &mut SqliteConnection,
    viewer: &str,
    task_id: &str,
) -> AppResult<Vec<Attachment>> {
    let mut q = visible(viewer);
    q.push(" AND v.task_id = ").push_bind(task_id).push(" ORDER BY v.created_at, v.id");
    Ok(q.build_query_as::<AttachmentRow>().fetch_all(conn).await?.into_iter().map(|r| r.into_api()).collect())
}

/// One attachment as the viewer sees it, else 404.
pub async fn visible_attachment(
    conn: &mut SqliteConnection,
    viewer: &str,
    id: &str,
) -> AppResult<Attachment> {
    Ok(visible_row(conn, viewer, id).await?.into_api())
}

// ---------------------------------------------------------------------------
// Listing

#[derive(Deserialize, IntoParams, Default)]
pub struct ListQuery {
    /// A project's files: its own, its tasks', comments', rows' and those of
    /// notes linked to it.
    pub project_id: Option<String>,
    /// With `project_id`: projects spawned from it too, all the way down.
    #[serde(default)]
    pub include_children: bool,
    /// One task's files, its comments' included.
    pub task_id: Option<String>,
    pub row_id: Option<String>,
    pub note_id: Option<String>,
    /// Images only.
    #[serde(default)]
    pub images_only: bool,
}

/// Files, newest first (by when the photo was taken, else when it was added).
#[utoipa::path(get, path = "/attachments", tag = "attachments", params(ListQuery),
    responses((status = 200, body = Vec<Attachment>), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn list_attachments(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Attachment>>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(list(&mut conn, &user.id, &q).await?))
}

pub async fn list(conn: &mut SqliteConnection, viewer: &str, q: &ListQuery) -> AppResult<Vec<Attachment>> {
    let mut sql = QueryBuilder::<Sqlite>::new("");
    if let Some(project) = &q.project_id {
        visible_project(conn, viewer, project).await?;
        sql.push("WITH RECURSIVE fam (id) AS (SELECT ").push_bind(project.clone());
        if q.include_children {
            sql.push(
                " UNION SELECT l.target_id FROM fam JOIN project_links l ON l.source_id = fam.id AND l.kind = 'spawned'",
            );
        }
        sql.push(") ");
        push_visible(&mut sql, viewer);
        sql.push(
            " AND (v.project_id IN (SELECT id FROM fam)
                OR v.note_id IN (SELECT note_id FROM note_projects WHERE project_id IN (SELECT id FROM fam)))",
        );
    } else if let Some(task) = &q.task_id {
        visible_task(conn, viewer, task).await?;
        push_visible(&mut sql, viewer);
        sql.push(" AND v.task_id = ").push_bind(task.clone());
    } else if let Some(row) = &q.row_id {
        push_visible(&mut sql, viewer);
        sql.push(" AND v.row_id = ").push_bind(row.clone());
    } else if let Some(note) = &q.note_id {
        push_visible(&mut sql, viewer);
        sql.push(" AND v.note_id = ").push_bind(note.clone());
    } else {
        return Err(AppError::BadRequest(
            "invalid_query",
            "Say whose files: project_id, task_id, row_id or note_id.".into(),
        ));
    }
    finish_list(conn, sql, q).await
}

async fn finish_list(
    conn: &mut SqliteConnection,
    mut sql: QueryBuilder<'_, Sqlite>,
    q: &ListQuery,
) -> AppResult<Vec<Attachment>> {
    if q.images_only {
        sql.push(" AND v.has_thumbs = 1");
    }
    sql.push(" ORDER BY v.sort_at DESC, v.id DESC LIMIT ").push_bind(MAX_LIST);
    Ok(sql
        .build_query_as::<AttachmentRow>()
        .fetch_all(conn)
        .await?
        .into_iter()
        .map(|r| r.into_api())
        .collect())
}

// ---------------------------------------------------------------------------
// Upload

/// Exactly one of these says what the files are attached to.
#[derive(Deserialize, IntoParams, Default, Clone)]
pub struct Target {
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    /// A comment of yours in a task's log.
    pub comment_id: Option<String>,
    pub row_id: Option<String>,
    pub note_id: Option<String>,
}

/// Where new files go, once the person is known to be allowed.
#[derive(Clone)]
pub struct Place {
    project_id: Option<String>,
    task_id: Option<String>,
    comment_id: Option<String>,
    row_id: Option<String>,
    note_id: Option<String>,
    /// Whose storage holds it.
    owner_id: String,
    /// For a note: its vault-relative folder, so the file lands beside the vault.
    note_vault: bool,
}

/// Checks the person may add files there and works out where they go.
pub async fn resolve(conn: &mut SqliteConnection, user_id: &str, t: &Target) -> AppResult<Place> {
    let given = [&t.project_id, &t.task_id, &t.comment_id, &t.row_id, &t.note_id];
    if given.iter().filter(|v| v.is_some()).count() != 1 {
        return Err(AppError::BadRequest(
            "invalid_target",
            "Attach files to one thing: a project, task, comment, row or note.".into(),
        ));
    }
    let mut place = Place {
        project_id: None,
        task_id: None,
        comment_id: None,
        row_id: None,
        note_id: None,
        owner_id: String::new(),
        note_vault: false,
    };
    let project_id = if let Some(id) = &t.project_id {
        id.clone()
    } else if let Some(id) = &t.task_id {
        let task = editable_task(conn, user_id, id).await?;
        place.task_id = Some(task.id);
        task.project_id
    } else if let Some(id) = &t.comment_id {
        let found: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT task_id, user_id FROM task_comments WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        let (task_id, author) = found.ok_or(AppError::NotFound)?;
        let task = editable_task(conn, user_id, &task_id).await?;
        if author.as_deref() != Some(user_id) {
            return Err(AppError::Forbidden("You can only add files to your own comments.".into()));
        }
        place.task_id = Some(task.id);
        place.comment_id = Some(id.clone());
        task.project_id
    } else if let Some(id) = &t.row_id {
        let project: Option<String> = sqlx::query_scalar(
            "SELECT d.project_id FROM db_rows r JOIN databases d ON d.id = r.database_id WHERE r.id = ?",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        let project = project.ok_or(AppError::NotFound)?;
        visible_project(conn, user_id, &project).await?;
        place.row_id = Some(id.clone());
        project
    } else {
        let id = t.note_id.as_ref().unwrap();
        let owner = notes::editable_note_owner(conn, user_id, id).await?;
        place.note_id = Some(id.clone());
        place.owner_id = owner;
        place.note_vault = true;
        return Ok(place);
    };
    let project = editable_project(conn, user_id, &project_id).await?;
    place.owner_id = project.owner_id;
    place.project_id = Some(project_id);
    Ok(place)
}

/// One file to upload.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct UploadForm {
    /// One or more files, each in a part named `file`.
    #[schema(value_type = Vec<String>, format = Binary)]
    file: Vec<Vec<u8>>,
}

/// Add photos or files. The body is `multipart/form-data` with one or more
/// parts named `file`. Each may be up to the server's limit (25 MB unless
/// `TMP_MAX_UPLOAD_MB` says otherwise).
#[utoipa::path(post, path = "/attachments", tag = "attachments", params(Target),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 201, body = Vec<Attachment>), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody),
        (status = 413, body = crate::error::ErrorBody)))]
pub(crate) async fn upload(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(target): Query<Target>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<Vec<Attachment>>)> {
    let place = {
        let mut conn = state.db.acquire().await?;
        resolve(&mut conn, &user.id, &target).await?
    };
    let mut added = vec![];
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return Err(bad_upload(e)),
        };
        if field.name() != Some("file") {
            continue;
        }
        let name = clean_name(field.file_name().unwrap_or("file"));
        let client_mime = field.content_type().map(str::to_string);
        let mut field = field;
        let incoming = Incoming::create(&state).await?;
        let mut writer = incoming.writer().await?;
        let mut hasher = Sha256::new();
        let mut size: usize = 0;
        loop {
            let chunk = match field.chunk().await {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(e) => {
                    incoming.discard().await;
                    return Err(bad_upload(e));
                }
            };
            size += chunk.len();
            if size > state.config.max_upload_bytes {
                incoming.discard().await;
                return Err(too_big(state.config.max_upload_bytes));
            }
            hasher.update(&chunk);
            if let Err(e) = writer.write_all(&chunk).await {
                incoming.discard().await;
                return Err(AppError::Internal(anyhow::Error::new(e).context("writing upload")));
            }
        }
        writer.flush().await.context("writing upload")?;
        drop(writer);
        if size == 0 {
            incoming.discard().await;
            continue;
        }
        let sha256 = hex::encode(hasher.finalize());
        let a =
            store(&state, &user.id, &place, incoming, &name, client_mime.as_deref(), size, &sha256).await?;
        added.push(a);
    }
    if added.is_empty() {
        return Err(AppError::BadRequest("no_file", "Choose a file to attach.".into()));
    }
    Ok((StatusCode::CREATED, Json(added)))
}

fn bad_upload(e: axum::extract::multipart::MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return AppError::BadRequest("too_large", "That upload is too large.".into());
    }
    AppError::BadRequest("invalid_upload", "That upload didn't arrive whole. Try again.".into())
}

fn too_big(max: usize) -> AppError {
    AppError::BadRequest("too_large", format!("Files can be up to {} MB.", max / (1024 * 1024)))
}

/// A file part on its way to disk.
pub struct Incoming {
    id: String,
    tmp: PathBuf,
}

impl Incoming {
    pub async fn create(state: &AppState) -> AppResult<Self> {
        let dir = state.config.data_dir.join("files").join("tmp");
        tokio::fs::create_dir_all(&dir).await.context("making the upload folder")?;
        let id = time::new_id();
        Ok(Self { tmp: dir.join(format!("{id}.part")), id })
    }

    async fn writer(&self) -> AppResult<tokio::io::BufWriter<tokio::fs::File>> {
        let f = tokio::fs::File::create(&self.tmp).await.context("starting upload")?;
        Ok(tokio::io::BufWriter::new(f))
    }

    pub async fn write_all(&self, bytes: &[u8]) -> AppResult<()> {
        tokio::fs::write(&self.tmp, bytes).await.context("writing upload")?;
        Ok(())
    }

    async fn discard(&self) {
        let _ = tokio::fs::remove_file(&self.tmp).await;
    }
}

/// Moves an uploaded file into place, makes its previews and records it.
#[allow(clippy::too_many_arguments)]
pub async fn store(
    state: &AppState,
    user_id: &str,
    place: &Place,
    incoming: Incoming,
    name: &str,
    client_mime: Option<&str>,
    size: usize,
    sha256: &str,
) -> AppResult<Attachment> {
    let data_dir = state.config.data_dir.clone();
    let id = incoming.id.clone();
    let ext = extension(name);
    // Where the original goes, relative to the data folder.
    let (rel, final_name) = if place.note_vault {
        let root = vault::root(&data_dir, &place.owner_id);
        let file = free_name(&root.join("attachments"), name).await;
        (format!("vaults/{}/attachments/{file}", place.owner_id), file)
    } else {
        let month = &time::now()[..7];
        let file = if ext.is_empty() { id.clone() } else { format!("{id}.{ext}") };
        (format!("files/{}/{month}/{file}", place.owner_id), name.to_string())
    };
    let dest = data_dir.join(&rel);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.context("making the file folder")?;
    }
    tokio::fs::rename(&incoming.tmp, &dest).await.context("moving the upload into place")?;

    let thumbs = thumb_paths(&data_dir, &id);
    let client_mime = client_mime.map(str::to_string);
    let src = dest.clone();
    let info = tokio::task::spawn_blocking(move || analyse(&src, client_mime.as_deref(), &thumbs))
        .await
        .context("reading the file")?;

    let now = time::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO attachments (id, project_id, task_id, comment_id, row_id, note_id, owner_id, path, name,
            mime, size, width, height, taken_at, sha256, has_thumbs, added_by, via, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&place.project_id)
    .bind(&place.task_id)
    .bind(&place.comment_id)
    .bind(&place.row_id)
    .bind(&place.note_id)
    .bind(&place.owner_id)
    .bind(&rel)
    .bind(&final_name)
    .bind(&info.mime)
    .bind(size as i64)
    .bind(info.width.map(i64::from))
    .bind(info.height.map(i64::from))
    .bind(&info.taken_at)
    .bind(sha256)
    .bind(info.thumbs)
    .bind(user_id)
    .bind(via())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    if let Some(project) = &place.project_id {
        sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(project)
            .execute(&mut *tx)
            .await?;
        log(
            &mut tx,
            project,
            place.task_id.as_deref(),
            user_id,
            "attachment.added",
            serde_json::json!({ "name": final_name, "image": info.thumbs }),
        )
        .await?;
    }
    let a = visible_attachment(&mut tx, user_id, &id).await?;
    tx.commit().await?;
    Ok(a)
}

/// Notes the change in the project's history (and the task's, when on one).
async fn log(
    conn: &mut SqliteConnection,
    project: &str,
    task: Option<&str>,
    user_id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    match task {
        Some(task) => crate::tasks::log_task(conn, project, task, user_id, kind, payload).await,
        None => crate::projects::log_activity(conn, project, user_id, kind, payload).await,
    }
}

/// A file name made safe to keep: no folders, no control characters, not
/// hidden, at most 200 characters.
pub fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|' | '[' | ']' | '#' | '^') { '_' } else { c }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    let mut out: String = cleaned.chars().take(200).collect();
    if out.is_empty() {
        out = "file".into();
    }
    out
}

/// The lower-case extension, letters and digits only, or empty.
fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && !ext.is_empty()
                && ext.len() <= 10
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            ext.to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

/// `name`, or `name 2`, `name 3`… so it doesn't replace a file in `dir`.
async fn free_name(dir: &Path, name: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 1.. {
        let candidate = if n == 1 { name.to_string() } else { format!("{stem} {n}{ext}") };
        if tokio::fs::metadata(dir.join(&candidate)).await.is_err() {
            return candidate;
        }
    }
    unreachable!()
}

fn thumb_paths(data_dir: &Path, id: &str) -> [PathBuf; 2] {
    let dir = data_dir.join("files").join("thumbs");
    [dir.join(format!("{id}-{SMALL}.jpg")), dir.join(format!("{id}-{LARGE}.jpg"))]
}

/// What the server learnt from a file's bytes.
#[derive(Debug, Default)]
pub struct Info {
    pub mime: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub taken_at: Option<String>,
    pub thumbs: bool,
}

/// Works out a file's type from its bytes (never trusting the name or the
/// browser for anything shown in place), reads when a photo was taken, and
/// writes its previews. Previews are re-encoded, so they carry no EXIF data
/// (and so no location); the original is left untouched.
pub fn analyse(path: &Path, client_mime: Option<&str>, thumbs: &[PathBuf; 2]) -> Info {
    let mut head = [0u8; 32];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    let head = &head[..n];
    let sniffed = match image::guess_format(head) {
        Ok(image::ImageFormat::Jpeg) => Some("image/jpeg"),
        Ok(image::ImageFormat::Png) => Some("image/png"),
        Ok(image::ImageFormat::Gif) => Some("image/gif"),
        Ok(image::ImageFormat::WebP) => Some("image/webp"),
        _ if head.starts_with(b"%PDF-") => Some("application/pdf"),
        _ => None,
    };
    let mime = match sniffed {
        Some(m) => m.to_string(),
        None => client_mime
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .filter(|m| {
                m.len() <= 100
                    && !INLINE_TYPES.contains(&m.as_str())
                    && m.split_once('/').is_some_and(|(a, b)| {
                        !a.is_empty()
                            && !b.is_empty()
                            && m.chars().all(|c| c.is_ascii_alphanumeric() || "/.+-".contains(c))
                    })
            })
            .unwrap_or_else(|| "application/octet-stream".into()),
    };
    let mut info = Info { mime, ..Default::default() };
    if !info.mime.starts_with("image/") {
        return info;
    }
    info.taken_at = taken_at(path);
    match make_thumbs(path, thumbs) {
        Ok((w, h)) => {
            info.width = Some(w);
            info.height = Some(h);
            info.thumbs = true;
        }
        Err(e) => tracing::warn!(error = %e, file = %path.display(), "making previews"),
    }
    info
}

/// The camera's DateTimeOriginal, as `2026-11-09T16:20:00`.
fn taken_at(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let exif = exif::Reader::new().read_from_container(&mut BufReader::new(file)).ok()?;
    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;
    let exif::Value::Ascii(ref parts) = field.value else { return None };
    let dt = exif::DateTime::from_ascii(parts.first()?).ok()?;
    (dt.year > 1900 && (1..=12).contains(&dt.month) && (1..=31).contains(&dt.day)).then(|| {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
        )
    })
}

/// Writes the two JPEG previews, turned upright. Returns the upright size.
fn make_thumbs(path: &Path, out: &[PathBuf; 2]) -> anyhow::Result<(u32, u32)> {
    use image::{DynamicImage, ImageDecoder, ImageReader, Limits, codecs::jpeg::JpegEncoder};
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    let (w, h) = (img.width(), img.height());
    if let Some(dir) = out[0].parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Large first, then the small one from it: quicker than from the original.
    let large =
        if w.max(h) > LARGE { img.resize(LARGE, LARGE, image::imageops::FilterType::Triangle) } else { img };
    let small = if w.max(h) > SMALL {
        large.resize(SMALL, SMALL, image::imageops::FilterType::Triangle)
    } else {
        large.clone()
    };
    for (img, dest, quality) in [(&small, &out[0], 78), (&large, &out[1], 84)] {
        let rgb = img.to_rgb8();
        let tmp = dest.with_extension("jpg.tmp");
        let mut file = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        JpegEncoder::new_with_quality(&mut file, quality).encode_image(&rgb)?;
        std::io::Write::flush(&mut file)?;
        drop(file);
        std::fs::rename(&tmp, dest)?;
    }
    Ok((w, h))
}

// ---------------------------------------------------------------------------
// Reading

#[derive(Deserialize, IntoParams)]
pub struct FileQuery {
    /// `400` or `1600` for an image's preview (a JPEG); left out, the original.
    pub size: Option<u32>,
    /// Offer it as a download instead of showing it.
    #[serde(default)]
    pub download: bool,
}

/// The file itself, or a preview of an image.
#[utoipa::path(get, path = "/attachments/{id}/file", tag = "attachments", params(("id" = String, Path), FileQuery),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn get_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<FileQuery>,
    req: Request,
) -> AppResult<Response> {
    let row = {
        let mut conn = state.db.acquire().await?;
        visible_row(&mut conn, &user.id, &id).await?
    };
    serve(&state.config.data_dir, &row.path, &row.id, &row.name, &row.mime, row.has_thumbs, &q, req).await
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    data_dir: &Path,
    rel: &str,
    id: &str,
    name: &str,
    mime: &str,
    has_thumbs: bool,
    q: &FileQuery,
    req: Request,
) -> AppResult<Response> {
    let (path, mime, name) = match q.size {
        Some(size) if has_thumbs && !q.download => {
            let [small, large] = thumb_paths(data_dir, id);
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
            (if size <= SMALL { small } else { large }, "image/jpeg", format!("{stem}.jpg"))
        }
        _ => (data_dir.join(rel), mime, name.to_string()),
    };
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(AppError::NotFound);
    }
    let res = ServeFile::new(&path).oneshot(req).await.context("reading the file")?;
    let (mut parts, body) = res.into_parts();
    if !parts.status.is_success() && parts.status != StatusCode::NOT_MODIFIED {
        return Ok(Response::from_parts(parts, Body::new(body)));
    }
    let inline = !q.download && INLINE_TYPES.contains(&mime);
    let headers = &mut parts.headers;
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(if inline { mime } else { "application/octet-stream" }).unwrap(),
    );
    let disposition = notes::transfer::attachment(&name);
    let disposition = if inline { disposition.replacen("attachment", "inline", 1) } else { disposition };
    headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap());
    // Each id's bytes never change, so the browser may keep them; `private`
    // keeps shared caches out of it.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    if mime.starts_with("image/") {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; sandbox",
            ),
        );
    }
    Ok(Response::from_parts(parts, Body::new(body)))
}

#[derive(Deserialize, IntoParams)]
pub struct NoteFileQuery {
    /// What the note embeds, as written: `photo.jpg`, `attachments/photo.jpg`
    /// or a path relative to the note.
    pub name: String,
    pub size: Option<u32>,
    #[serde(default)]
    pub download: bool,
}

/// A file a note embeds, such as `![[photo.jpg]]`, found the way Obsidian
/// finds it: beside the note, from the vault's top, in `attachments/`, then
/// anywhere in the vault by name.
#[utoipa::path(get, path = "/notes/{id}/file", tag = "notes", params(("id" = String, Path), NoteFileQuery),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn note_file(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<NoteFileQuery>,
    req: Request,
) -> AppResult<Response> {
    let mut conn = state.db.acquire().await?;
    let (owner, note_path) = notes::readable_note_owner(&mut conn, &user.id, &id).await?;
    let root = vault::root(&state.config.data_dir, &owner);
    let wanted = percent_decode(q.name.trim().split(['#', '|']).next().unwrap_or(""));
    let base = wanted.rsplit('/').next().unwrap_or("").to_string();
    if base.is_empty() || base.to_lowercase().ends_with(".md") {
        return Err(AppError::NotFound);
    }
    let folder = note_path.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
    let mut candidates = vec![];
    for c in [format!("{folder}/{wanted}"), wanted.clone(), format!("attachments/{base}")] {
        if let Some(rel) = vault::clean_rel(&c) {
            candidates.push(rel);
        }
    }
    let mut found = None;
    for rel in candidates {
        if tokio::fs::metadata(root.join(&rel)).await.is_ok_and(|m| m.is_file()) {
            found = Some(rel);
            break;
        }
    }
    if found.is_none() {
        let search_root = root.clone();
        let base = base.clone();
        found = tokio::task::spawn_blocking(move || find_by_name(&search_root, &base))
            .await
            .context("searching the vault")?;
    }
    let rel = found.ok_or(AppError::NotFound)?;
    let full_rel = format!("vaults/{owner}/{rel}");
    // A file uploaded to a note has a record, and so previews.
    let known: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT id, mime, has_thumbs FROM attachments WHERE owner_id = ? AND path = ? LIMIT 1",
    )
    .bind(&owner)
    .bind(&full_rel)
    .fetch_optional(&mut *conn)
    .await?;
    drop(conn);
    let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
    let fq = FileQuery { size: q.size, download: q.download };
    match known {
        Some((aid, mime, thumbs)) => {
            serve(&state.config.data_dir, &full_rel, &aid, &name, &mime, thumbs, &fq, req).await
        }
        None => {
            let mime = mime_for_ext(&extension(&name));
            serve(&state.config.data_dir, &full_rel, "", &name, mime, false, &fq, req).await
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a vault file with this extension is, for showing it in place.
fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// The first file under `root` called `name`, skipping hidden folders.
fn find_by_name(root: &Path, name: &str) -> Option<String> {
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > 50_000 {
                return None;
            }
            let file_name = e.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.') {
                continue;
            }
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                stack.push(e.path());
            } else if t.is_file() && file_name == name {
                let rel = e.path().strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
                return Some(rel);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Changing

#[derive(Deserialize, ToSchema)]
pub struct UpdateAttachmentRequest {
    /// A few words under the photo. Empty clears it.
    pub caption: Option<String>,
}

/// Caption a photo or file.
#[utoipa::path(patch, path = "/attachments/{id}", tag = "attachments", params(("id" = String, Path)),
    request_body = UpdateAttachmentRequest,
    responses((status = 200, body = Attachment), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_attachment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(req): Json<UpdateAttachmentRequest>,
) -> AppResult<Json<Attachment>> {
    let mut tx = state.db.begin().await?;
    let current = visible_attachment(&mut tx, &user.id, &id).await?;
    if !current.can_edit {
        return Err(AppError::Forbidden("This is shared with you to look at, not to change.".into()));
    }
    if let Some(caption) = req.caption {
        let caption = caption.trim();
        if caption.chars().count() > MAX_CAPTION {
            return Err(AppError::BadRequest(
                "invalid_caption",
                format!("Captions are up to {MAX_CAPTION} characters."),
            ));
        }
        sqlx::query("UPDATE attachments SET caption = ? WHERE id = ?")
            .bind((!caption.is_empty()).then_some(caption))
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    let a = visible_attachment(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(a))
}

/// Remove a photo or file for good.
#[utoipa::path(delete, path = "/attachments/{id}", tag = "attachments", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn delete_attachment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(id): UrlPath<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let current = visible_attachment(&mut tx, &user.id, &id).await?;
    if !current.can_edit {
        return Err(AppError::Forbidden("This is shared with you to look at, not to change.".into()));
    }
    sqlx::query("DELETE FROM attachments WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    if let Some(project) = &current.project_id {
        log(
            &mut tx,
            project,
            current.task_id.as_deref(),
            &user.id,
            "attachment.removed",
            serde_json::json!({ "name": current.name }),
        )
        .await?;
    }
    tx.commit().await?;
    empty_trash(&state.db, &state.config.data_dir).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct CoverRequest {
    /// A photo on this project, or `null` for no cover.
    pub attachment_id: Option<String>,
}

/// Choose the photo on the project's canvas card.
#[utoipa::path(put, path = "/projects/{id}/cover", tag = "projects", params(("id" = String, Path)),
    request_body = CoverRequest,
    responses((status = 204), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn set_cover(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Json(req): Json<CoverRequest>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    require_edit(&mut tx, &user.id, &id).await?;
    if let Some(a) = &req.attachment_id {
        let ok: Option<bool> =
            sqlx::query_scalar("SELECT has_thumbs FROM attachments WHERE id = ? AND project_id = ?")
                .bind(a)
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?;
        if ok != Some(true) {
            return Err(AppError::BadRequest("invalid_cover", "Pick a photo from this project.".into()));
        }
    }
    sqlx::query("UPDATE projects SET cover_id = ? WHERE id = ?")
        .bind(&req.attachment_id)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Housekeeping

/// Removes the files of deleted records from disk. Runs after a delete and at
/// start-up, so a cascade (a deleted project or account) is cleaned up too.
pub async fn empty_trash(db: &SqlitePool, data_dir: &Path) {
    let rows: Vec<(String, String)> =
        match sqlx::query_as("SELECT id, path FROM attachment_trash LIMIT 5000").fetch_all(db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "reading attachment trash");
                return;
            }
        };
    for (id, rel) in rows {
        // Only ever remove from inside the data folder.
        if let Some(rel) = vault::clean_rel(&rel) {
            let _ = tokio::fs::remove_file(data_dir.join(rel)).await;
        }
        for t in thumb_paths(data_dir, &id) {
            let _ = tokio::fs::remove_file(t).await;
        }
        let _ = sqlx::query("DELETE FROM attachment_trash WHERE id = ?").bind(&id).execute(db).await;
    }
    // Uploads cut off half way.
    if let Ok(mut dir) = tokio::fs::read_dir(data_dir.join("files").join("tmp")).await {
        let day = std::time::Duration::from_secs(24 * 60 * 60);
        while let Ok(Some(e)) = dir.next_entry().await {
            let old = e
                .metadata()
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > day);
            if old {
                let _ = tokio::fs::remove_file(e.path()).await;
            }
        }
    }
}

/// Moves files someone just uploaded to a task onto their new comment.
pub async fn claim_for_comment(
    conn: &mut SqliteConnection,
    user_id: &str,
    task_id: &str,
    comment_id: &str,
    ids: &[String],
) -> AppResult<usize> {
    let mut moved = 0;
    for id in ids {
        moved += sqlx::query(
            "UPDATE attachments SET comment_id = ? WHERE id = ? AND task_id = ? AND comment_id IS NULL AND added_by = ?",
        )
        .bind(comment_id)
        .bind(id)
        .bind(task_id)
        .bind(user_id)
        .execute(&mut *conn)
        .await?
        .rows_affected() as usize;
    }
    Ok(moved)
}

/// Response wrapper so tests and the MCP server can read a preview's bytes.
pub async fn preview_bytes(data_dir: &Path, id: &str) -> Option<Vec<u8>> {
    tokio::fs::read(&thumb_paths(data_dir, id)[1]).await.ok()
}

/// The original file's bytes, for a caller already allowed to see it.
pub async fn original_bytes(state: &AppState, id: &str) -> AppResult<Vec<u8>> {
    let rel: String = sqlx::query_scalar("SELECT path FROM attachments WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let rel = vault::clean_rel(&rel).ok_or(AppError::NotFound)?;
    Ok(tokio::fs::read(state.config.data_dir.join(rel)).await.context("reading the file")?)
}

impl IntoResponse for Attachment {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name("C:\\photos\\IMG_1.JPG"), "IMG_1.JPG");
        assert_eq!(clean_name(".hidden"), "hidden");
        assert_eq!(clean_name("a[b]#c.png"), "a_b__c.png");
        assert_eq!(clean_name(""), "file");
        assert_eq!(extension("Photo.JPG"), "jpg");
        assert_eq!(extension("noext"), "");
        assert_eq!(extension(".bashrc"), "");
        assert_eq!(percent_decode("my%20photo.jpg"), "my photo.jpg");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn untrusted_types_never_show_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.html");
        std::fs::write(&path, b"<script>alert(1)</script>").unwrap();
        let thumbs = thumb_paths(dir.path(), "x");
        assert_eq!(analyse(&path, Some("image/png"), &thumbs).mime, "application/octet-stream");
        assert_eq!(analyse(&path, Some("text/html"), &thumbs).mime, "text/html");
        assert_eq!(analyse(&path, Some("model/stl"), &thumbs).mime, "model/stl");
    }
}
