//! Whiteboards: freeform boards of shapes, arrows, sticky notes, text, pen
//! strokes and photos, kept with a project (T-30).
//!
//! A board follows its project's access: whoever can see the project looks
//! at its boards, and the owner and editors draw on them.
//!
//! The server stores each element as JSON it doesn't look inside, beyond its
//! kind. Browsers save only the elements that changed, each with a version
//! they count up; the higher version wins, so two people drawing on different
//! things never overwrite each other. Every save bumps the board's `seq`, and
//! an open board asks for what changed since the `seq` it last saw. Where
//! everyone's cursor is lives only in memory.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    board::{position_after, position_last},
    error::{AppError, AppResult},
    projects::{Project, editable_project, log_activity, some, visible_project},
    time,
};

/// Most boards in one project.
const MAX_BOARDS: i64 = 100;
/// Most elements on one board, deleted ones not counted.
pub const MAX_ELEMENTS: i64 = 10_000;
/// Most elements one save may change.
const MAX_CHANGES: usize = 2_000;
/// Biggest element, as JSON. A long pen stroke is the usual big one.
const MAX_ELEMENT_BYTES: usize = 200_000;
/// Biggest save request.
const MAX_SAVE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TITLE: usize = 80;
/// A cursor not moved for this long has left the board.
const CURSOR_TTL: Duration = Duration::from_secs(20);

pub const KINDS: &[&str] = &["box", "ellipse", "diamond", "sticky", "text", "arrow", "pen", "image"];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_whiteboards, create_whiteboard))
        .routes(routes!(get_whiteboard, update_whiteboard, delete_whiteboard))
        .routes(routes!(set_cursor))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(get_elements, save_elements))
                .layer(DefaultBodyLimit::max(MAX_SAVE_BYTES)),
        )
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Whiteboard {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub position: String,
    /// Elements on it now.
    pub element_count: i64,
    /// Goes up with every save.
    pub seq: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Who saved it last. Missing when their account is gone.
    pub updated_by_name: Option<String>,
    /// You may draw on it (you own or edit the project).
    pub can_edit: bool,
}

#[derive(FromRow)]
struct BoardRow {
    id: String,
    project_id: String,
    title: String,
    position: String,
    element_count: i64,
    seq: i64,
    created_at: String,
    updated_at: String,
    updated_by_name: Option<String>,
}

const BOARD_SELECT: &str = "SELECT w.id, w.project_id, w.title, w.position, w.seq, w.created_at,
        w.updated_at, u.name AS updated_by_name,
        (SELECT COUNT(*) FROM whiteboard_elements e WHERE e.board_id = w.id AND e.deleted = 0) AS element_count
    FROM whiteboards w LEFT JOIN users u ON u.id = w.updated_by";

fn finish(r: BoardRow, project: &Project) -> Whiteboard {
    Whiteboard {
        id: r.id,
        project_id: r.project_id,
        title: r.title,
        position: r.position,
        element_count: r.element_count,
        seq: r.seq,
        created_at: r.created_at,
        updated_at: r.updated_at,
        updated_by_name: r.updated_by_name,
        can_edit: project.role != "viewer",
    }
}

/// One thing on a board.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Element {
    /// Chosen by whoever drew it.
    pub id: String,
    /// `box`, `ellipse`, `diamond`, `sticky`, `text`, `arrow`, `pen` or `image`.
    pub kind: String,
    /// Counted up with each change; the higher version wins.
    pub version: i64,
    /// Removed. Only sent to someone asking for changes `since` a save.
    pub deleted: bool,
    /// Everything else about it: where it is, colour, words. Empty when deleted.
    #[schema(value_type = Object)]
    pub data: Value,
    pub updated_by_name: Option<String>,
    pub updated_at: String,
}

#[derive(FromRow)]
struct ElementRow {
    id: String,
    kind: String,
    version: i64,
    deleted: bool,
    data: String,
    updated_by_name: Option<String>,
    updated_at: String,
}

impl From<ElementRow> for Element {
    fn from(r: ElementRow) -> Self {
        Element {
            id: r.id,
            kind: r.kind,
            version: r.version,
            deleted: r.deleted,
            data: serde_json::from_str(&r.data).unwrap_or(Value::Object(Default::default())),
            updated_by_name: r.updated_by_name,
            updated_at: r.updated_at,
        }
    }
}

const ELEMENT_SELECT: &str = "SELECT e.id, e.kind, e.version, e.deleted, e.data, u.name AS updated_by_name,
        e.updated_at
    FROM whiteboard_elements e LEFT JOIN users u ON u.id = e.updated_by";

/// Someone else looking at the board.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Cursor {
    pub user_id: String,
    pub name: String,
    /// Board coordinates.
    pub x: f64,
    pub y: f64,
}

/// A board's elements, or what changed on it.
#[derive(Serialize, ToSchema)]
pub struct BoardElements {
    /// Ask with this as `since` next time.
    pub seq: i64,
    pub elements: Vec<Element>,
    /// Where everyone else on the board is pointing.
    pub cursors: Vec<Cursor>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateWhiteboardRequest {
    pub title: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateWhiteboardRequest {
    pub title: Option<String>,
    /// Move it after this board; `null` for first.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams)]
pub struct ElementsQuery {
    /// Only what changed after this save, deleted elements included.
    pub since: Option<i64>,
}

/// A new or changed element.
#[derive(Deserialize, ToSchema)]
pub struct ElementChange {
    pub id: String,
    pub kind: String,
    /// One more than the version it was changed from (1 when new).
    pub version: i64,
    #[serde(default)]
    pub deleted: bool,
    /// The whole element apart from id, kind and version. Ignored when deleted.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub data: Value,
}

#[derive(Deserialize, ToSchema)]
pub struct SaveElementsRequest {
    pub elements: Vec<ElementChange>,
}

#[derive(Serialize, ToSchema)]
pub struct SaveElementsResponse {
    /// The board's save number after this one.
    pub seq: i64,
    /// Changes that lost to a newer version someone else saved, as the
    /// server has them now. Show these instead.
    pub rejected: Vec<Element>,
}

#[derive(Deserialize, ToSchema)]
pub struct CursorRequest {
    pub x: f64,
    pub y: f64,
}

// ---------------------------------------------------------------------------
// Helpers

/// The board and its project if the person may see it, else 404.
pub async fn visible_board(
    conn: &mut SqliteConnection,
    user_id: &str,
    id: &str,
) -> AppResult<(Whiteboard, Project)> {
    let row = sqlx::query_as::<_, BoardRow>(sqlx::AssertSqlSafe(format!("{BOARD_SELECT} WHERE w.id = ?")))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    let project = visible_project(conn, user_id, &row.project_id).await?;
    Ok((finish(row, &project), project))
}

/// The board and its project if the person may draw on it.
pub async fn editable_board(
    conn: &mut SqliteConnection,
    user_id: &str,
    id: &str,
) -> AppResult<(Whiteboard, Project)> {
    let (board, project) = visible_board(conn, user_id, id).await?;
    editable_project(conn, user_id, &project.id).await?;
    Ok((board, project))
}

fn clean_title(title: &str) -> AppResult<String> {
    let t = title.trim();
    if t.is_empty() || t.chars().count() > MAX_TITLE {
        return Err(AppError::BadRequest(
            "invalid_title",
            format!("Whiteboard names are 1 to {MAX_TITLE} characters."),
        ));
    }
    Ok(t.to_string())
}

fn check_change(c: &ElementChange) -> AppResult<String> {
    let id_ok = !c.id.is_empty()
        && c.id.len() <= 64
        && c.id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if !id_ok {
        return Err(AppError::BadRequest(
            "invalid_element",
            "Element ids are 1 to 64 letters, digits, - or _.".into(),
        ));
    }
    if !KINDS.contains(&c.kind.as_str()) {
        return Err(AppError::BadRequest("invalid_element", format!("{} isn't a kind of element.", c.kind)));
    }
    if c.version < 1 {
        return Err(AppError::BadRequest("invalid_element", "Versions start at 1.".into()));
    }
    if c.deleted {
        return Ok("{}".into());
    }
    if !c.data.is_object() {
        return Err(AppError::BadRequest("invalid_element", "An element's data is a JSON object.".into()));
    }
    let json = c.data.to_string();
    if json.len() > MAX_ELEMENT_BYTES {
        return Err(AppError::BadRequest("element_too_big", "That element is too big to save.".into()));
    }
    Ok(json)
}

/// The elements on a board now, or what changed after `since`.
pub async fn elements(
    conn: &mut SqliteConnection,
    board_id: &str,
    since: Option<i64>,
) -> AppResult<Vec<Element>> {
    let rows = match since {
        Some(s) => {
            sqlx::query_as::<_, ElementRow>(sqlx::AssertSqlSafe(format!(
                "{ELEMENT_SELECT} WHERE e.board_id = ? AND e.seq > ? ORDER BY e.seq, e.rowid"
            )))
            .bind(board_id)
            .bind(s)
            .fetch_all(conn)
            .await?
        }
        None => {
            sqlx::query_as::<_, ElementRow>(sqlx::AssertSqlSafe(format!(
                "{ELEMENT_SELECT} WHERE e.board_id = ? AND e.deleted = 0 ORDER BY e.rowid"
            )))
            .bind(board_id)
            .fetch_all(conn)
            .await?
        }
    };
    Ok(rows.into_iter().map(Element::from).collect())
}

/// Saves changes to a board as `user_id`, who may draw on it. Returns the
/// new save number and the changes that lost to a newer version.
pub async fn apply_changes(
    conn: &mut SqliteConnection,
    board_id: &str,
    project_id: &str,
    user_id: &str,
    changes: &[ElementChange],
) -> AppResult<SaveElementsResponse> {
    if changes.len() > MAX_CHANGES {
        return Err(AppError::BadRequest(
            "too_many_changes",
            format!("Save up to {MAX_CHANGES} elements at a time."),
        ));
    }
    let checked = changes.iter().map(check_change).collect::<AppResult<Vec<_>>>()?;
    let now = time::now();
    let seq: i64 = sqlx::query_scalar(
        "UPDATE whiteboards SET seq = seq + 1, updated_at = ?, updated_by = ? WHERE id = ? RETURNING seq",
    )
    .bind(&now)
    .bind(user_id)
    .bind(board_id)
    .fetch_one(&mut *conn)
    .await?;
    let mut rejected = vec![];
    for (c, data) in changes.iter().zip(checked) {
        let current: Option<i64> =
            sqlx::query_scalar("SELECT version FROM whiteboard_elements WHERE board_id = ? AND id = ?")
                .bind(board_id)
                .bind(&c.id)
                .fetch_optional(&mut *conn)
                .await?;
        match current {
            Some(v) if v > c.version => {
                let row = sqlx::query_as::<_, ElementRow>(sqlx::AssertSqlSafe(format!(
                    "{ELEMENT_SELECT} WHERE e.board_id = ? AND e.id = ?"
                )))
                .bind(board_id)
                .bind(&c.id)
                .fetch_one(&mut *conn)
                .await?;
                rejected.push(row.into());
            }
            _ => {
                sqlx::query(
                    "INSERT INTO whiteboard_elements (board_id, id, kind, data, version, seq, deleted, updated_by, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT (board_id, id) DO UPDATE SET kind = excluded.kind, data = excluded.data,
                        version = excluded.version, seq = excluded.seq, deleted = excluded.deleted,
                        updated_by = excluded.updated_by, updated_at = excluded.updated_at",
                )
                .bind(board_id)
                .bind(&c.id)
                .bind(&c.kind)
                .bind(&data)
                .bind(c.version)
                .bind(seq)
                .bind(c.deleted)
                .bind(user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    let live: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM whiteboard_elements WHERE board_id = ? AND deleted = 0")
            .bind(board_id)
            .fetch_one(&mut *conn)
            .await?;
    if live > MAX_ELEMENTS {
        return Err(AppError::BadRequest(
            "too_many_elements",
            format!("A whiteboard holds up to {MAX_ELEMENTS} things. Start another board."),
        ));
    }
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(project_id)
        .execute(&mut *conn)
        .await?;
    Ok(SaveElementsResponse { seq, rejected })
}

struct Seen {
    name: String,
    x: f64,
    y: f64,
    at: Instant,
}

/// Board id to user id to where they last pointed.
static CURSORS: LazyLock<Mutex<HashMap<String, HashMap<String, Seen>>>> = LazyLock::new(Default::default);

fn cursors_except(board_id: &str, user_id: &str) -> Vec<Cursor> {
    let mut all = CURSORS.lock().unwrap();
    let Some(board) = all.get_mut(board_id) else { return vec![] };
    board.retain(|_, s| s.at.elapsed() < CURSOR_TTL);
    let mut out: Vec<Cursor> = board
        .iter()
        .filter(|(id, _)| id.as_str() != user_id)
        .map(|(id, s)| Cursor { user_id: id.clone(), name: s.name.clone(), x: s.x, y: s.y })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    if board.is_empty() {
        all.remove(board_id);
    }
    out
}

// ---------------------------------------------------------------------------
// Handlers

/// A project's whiteboards, in order.
#[utoipa::path(get, path = "/projects/{id}/whiteboards", tag = "whiteboards", params(("id" = String, Path)),
    responses((status = 200, body = Vec<Whiteboard>), (status = 404, body = crate::error::ErrorBody)))]
async fn list_whiteboards(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Whiteboard>>> {
    let mut conn = state.db.acquire().await?;
    let project = visible_project(&mut conn, &user.id, &id).await?;
    let rows = sqlx::query_as::<_, BoardRow>(sqlx::AssertSqlSafe(format!(
        "{BOARD_SELECT} WHERE w.project_id = ? ORDER BY w.position, w.id"
    )))
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(rows.into_iter().map(|r| finish(r, &project)).collect()))
}

/// Start a new, empty whiteboard at the end of the project's list.
#[utoipa::path(post, path = "/projects/{id}/whiteboards", tag = "whiteboards", params(("id" = String, Path)),
    request_body = CreateWhiteboardRequest,
    responses((status = 201, body = Whiteboard), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
async fn create_whiteboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateWhiteboardRequest>,
) -> AppResult<(StatusCode, Json<Whiteboard>)> {
    let title = clean_title(&req.title)?;
    let mut tx = state.db.begin().await?;
    let board = create(&mut tx, &user.id, &id, &title).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(board)))
}

/// Makes a board in a project `user_id` may change.
pub async fn create(
    conn: &mut SqliteConnection,
    user_id: &str,
    project_id: &str,
    title: &str,
) -> AppResult<Whiteboard> {
    editable_project(conn, user_id, project_id).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM whiteboards WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(&mut *conn)
        .await?;
    if count >= MAX_BOARDS {
        return Err(AppError::BadRequest(
            "too_many_whiteboards",
            format!("A project can have up to {MAX_BOARDS} whiteboards."),
        ));
    }
    let board_id = time::new_id();
    let now = time::now();
    let position = position_last(conn, "whiteboards", "project_id", project_id).await?;
    sqlx::query(
        "INSERT INTO whiteboards (id, project_id, title, position, created_by, created_at, updated_at, updated_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&board_id)
    .bind(project_id)
    .bind(title)
    .bind(&position)
    .bind(user_id)
    .bind(&now)
    .bind(&now)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(project_id)
        .execute(&mut *conn)
        .await?;
    log_activity(conn, project_id, user_id, "whiteboard.created", serde_json::json!({ "title": title }))
        .await?;
    Ok(visible_board(conn, user_id, &board_id).await?.0)
}

/// One whiteboard (without its elements).
#[utoipa::path(get, path = "/whiteboards/{id}", tag = "whiteboards", params(("id" = String, Path)),
    responses((status = 200, body = Whiteboard), (status = 404, body = crate::error::ErrorBody)))]
async fn get_whiteboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Whiteboard>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(visible_board(&mut conn, &user.id, &id).await?.0))
}

/// Rename a whiteboard or move it in the project's list.
#[utoipa::path(patch, path = "/whiteboards/{id}", tag = "whiteboards", params(("id" = String, Path)),
    request_body = UpdateWhiteboardRequest,
    responses((status = 200, body = Whiteboard), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
async fn update_whiteboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateWhiteboardRequest>,
) -> AppResult<Json<Whiteboard>> {
    let mut tx = state.db.begin().await?;
    let (board, project) = editable_board(&mut tx, &user.id, &id).await?;
    if let Some(title) = &req.title {
        let title = clean_title(title)?;
        if title != board.title {
            sqlx::query("UPDATE whiteboards SET title = ? WHERE id = ?")
                .bind(&title)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            log_activity(
                &mut tx,
                &project.id,
                &user.id,
                "whiteboard.renamed",
                serde_json::json!({ "from": board.title, "title": title }),
            )
            .await?;
        }
    }
    if let Some(after) = &req.after {
        if after.as_deref() == Some(id.as_str()) {
            return Err(AppError::BadRequest("invalid_after", "A board can't go after itself.".into()));
        }
        let position =
            position_after(&mut tx, "whiteboards", "project_id", &project.id, after.as_deref(), Some(&id))
                .await?;
        sqlx::query("UPDATE whiteboards SET position = ? WHERE id = ?")
            .bind(&position)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    let (board, _) = visible_board(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(board))
}

/// Delete a whiteboard and everything on it. Photos stay with the project.
#[utoipa::path(delete, path = "/whiteboards/{id}", tag = "whiteboards", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn delete_whiteboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let (board, project) = editable_board(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM whiteboards WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    log_activity(
        &mut tx,
        &project.id,
        &user.id,
        "whiteboard.deleted",
        serde_json::json!({ "title": board.title }),
    )
    .await?;
    tx.commit().await?;
    CURSORS.lock().unwrap().remove(&id);
    Ok(StatusCode::NO_CONTENT)
}

/// Everything on a board, or with `since` only what changed after that save
/// (deleted elements included, so you can take them off). Also says where
/// everyone else on the board is pointing.
#[utoipa::path(get, path = "/whiteboards/{id}/elements", tag = "whiteboards",
    params(("id" = String, Path), ElementsQuery),
    responses((status = 200, body = BoardElements), (status = 404, body = crate::error::ErrorBody)))]
async fn get_elements(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<ElementsQuery>,
) -> AppResult<Json<BoardElements>> {
    let mut conn = state.db.acquire().await?;
    let (board, _) = visible_board(&mut conn, &user.id, &id).await?;
    // Nothing new: skip the query.
    let elements = match q.since {
        Some(s) if s >= board.seq => vec![],
        since => elements(&mut conn, &id, since).await?,
    };
    Ok(Json(BoardElements { seq: board.seq, elements, cursors: cursors_except(&id, &user.id) }))
}

/// Save new, changed and deleted elements. Each wins unless the server
/// already has a higher version of it; those come back in `rejected`.
#[utoipa::path(patch, path = "/whiteboards/{id}/elements", tag = "whiteboards", params(("id" = String, Path)),
    request_body = SaveElementsRequest,
    responses((status = 200, body = SaveElementsResponse), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
async fn save_elements(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<SaveElementsRequest>,
) -> AppResult<Json<SaveElementsResponse>> {
    let mut tx = state.db.begin().await?;
    let (_, project) = editable_board(&mut tx, &user.id, &id).await?;
    let out = apply_changes(&mut tx, &id, &project.id, &user.id, &req.elements).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Say where your pointer is on the board, so others see it. Viewers count
/// too. Forgotten after 20 seconds without a move.
#[utoipa::path(put, path = "/whiteboards/{id}/cursor", tag = "whiteboards", params(("id" = String, Path)),
    request_body = CursorRequest,
    responses((status = 204), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn set_cursor(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CursorRequest>,
) -> AppResult<StatusCode> {
    if !req.x.is_finite() || !req.y.is_finite() {
        return Err(AppError::BadRequest("invalid_cursor", "x and y are numbers.".into()));
    }
    let mut conn = state.db.acquire().await?;
    visible_board(&mut conn, &user.id, &id).await?;
    CURSORS
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .insert(user.id.clone(), Seen { name: user.name.clone(), x: req.x, y: req.y, at: Instant::now() });
    Ok(StatusCode::NO_CONTENT)
}
