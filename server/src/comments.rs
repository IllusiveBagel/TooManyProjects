//! Comments on a task: a running log of what was done, tried and learnt.
//!
//! Anyone who can see a task reads its comments. People who may change the
//! project (owner and editors) write them. You edit only your own; the
//! project's owner may also delete anyone's.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    projects::via,
    tasks::{Task, editable_task, log_task, visible_task},
    time,
};

/// Longest comment, in characters.
pub const MAX_COMMENT: usize = 20_000;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(add_comment)).routes(routes!(update_comment, delete_comment))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Comment {
    pub id: String,
    pub task_id: String,
    /// Missing when the author's account is gone.
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    /// Markdown.
    pub body: String,
    /// `web` or `ai`.
    pub via: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    /// You wrote it and can still change the project.
    pub can_edit: bool,
    /// You wrote it, or you own the project.
    pub can_delete: bool,
    /// Photos and files added with it.
    pub attachments: Vec<crate::attachments::Attachment>,
}

#[derive(FromRow)]
struct CommentRow {
    id: String,
    task_id: String,
    user_id: Option<String>,
    user_name: Option<String>,
    body: String,
    via: String,
    created_at: String,
    edited_at: Option<String>,
}

const COMMENT_SELECT: &str = "SELECT c.id, c.task_id, c.user_id, u.name AS user_name, c.body, c.via,
        c.created_at, c.edited_at
    FROM task_comments c LEFT JOIN users u ON u.id = c.user_id";

#[derive(Deserialize, ToSchema)]
pub struct CommentRequest {
    /// Markdown, up to 20,000 characters. May be empty when adding photos.
    #[serde(default)]
    pub body: String,
    /// Files you just uploaded to the task, to show with this comment
    /// instead. Only read when adding a comment.
    #[serde(default)]
    pub attachment_ids: Vec<String>,
}

async fn role(conn: &mut SqliteConnection, user_id: &str, project_id: &str) -> AppResult<String> {
    Ok(sqlx::query_scalar("SELECT role FROM project_access WHERE project_id = ? AND user_id = ?")
        .bind(project_id)
        .bind(user_id)
        .fetch_optional(conn)
        .await?
        .unwrap_or_else(|| "viewer".into()))
}

fn finish(row: CommentRow, viewer: &str, role: &str) -> Comment {
    let mine = row.user_id.as_deref() == Some(viewer);
    Comment {
        can_edit: mine && role != "viewer",
        can_delete: (mine && role != "viewer") || role == "owner",
        id: row.id,
        task_id: row.task_id,
        user_id: row.user_id,
        user_name: row.user_name,
        body: row.body,
        via: row.via,
        created_at: row.created_at,
        edited_at: row.edited_at,
        attachments: vec![],
    }
}

/// A task's comments, oldest first, as `viewer` (who can see the task) sees them.
pub async fn task_comments(
    conn: &mut SqliteConnection,
    viewer: &str,
    task: &Task,
) -> AppResult<Vec<Comment>> {
    let role = role(conn, viewer, &task.project_id).await?;
    let rows = sqlx::query_as::<_, CommentRow>(&format!(
        "{COMMENT_SELECT} WHERE c.task_id = ? ORDER BY c.created_at, c.rowid"
    ))
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    let mut files = crate::attachments::for_task(conn, viewer, &task.id).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let mut c = finish(r, viewer, &role);
            let (mine, rest): (Vec<_>, Vec<_>) =
                files.drain(..).partition(|a| a.comment_id.as_deref() == Some(c.id.as_str()));
            c.attachments = mine;
            files = rest;
            c
        })
        .collect())
}

fn clean_body(body: &str, may_be_empty: bool) -> AppResult<String> {
    let b = body.trim();
    if (b.is_empty() && !may_be_empty) || b.chars().count() > MAX_COMMENT {
        return Err(AppError::BadRequest(
            "invalid_comment",
            format!("Comments are 1 to {MAX_COMMENT} characters."),
        ));
    }
    Ok(b.to_string())
}

/// One comment as `viewer` sees it, with the task it is on. 404 when the
/// viewer can't see the task.
async fn visible_comment(conn: &mut SqliteConnection, viewer: &str, id: &str) -> AppResult<(Comment, Task)> {
    let row = sqlx::query_as::<_, CommentRow>(&format!("{COMMENT_SELECT} WHERE c.id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    let task = visible_task(conn, viewer, &row.task_id).await?;
    let role = role(conn, viewer, &task.project_id).await?;
    let mut comment = finish(row, viewer, &role);
    comment.attachments = crate::attachments::for_task(conn, viewer, &task.id)
        .await?
        .into_iter()
        .filter(|a| a.comment_id.as_deref() == Some(id))
        .collect();
    Ok((comment, task))
}

/// Add a comment to a task's log.
#[utoipa::path(post, path = "/tasks/{id}/comments", tag = "tasks", params(("id" = String, Path)),
    request_body = CommentRequest,
    responses((status = 201, body = Comment), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn add_comment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CommentRequest>,
) -> AppResult<(StatusCode, Json<Comment>)> {
    let body = clean_body(&req.body, !req.attachment_ids.is_empty())?;
    let mut tx = state.db.begin().await?;
    let task = editable_task(&mut tx, &user.id, &id).await?;
    let comment_id = time::new_id();
    let now = time::now();
    sqlx::query(
        "INSERT INTO task_comments (id, task_id, user_id, body, via, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&comment_id)
    .bind(&id)
    .bind(&user.id)
    .bind(&body)
    .bind(via())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let claimed =
        crate::attachments::claim_for_comment(&mut tx, &user.id, &id, &comment_id, &req.attachment_ids)
            .await?;
    if body.is_empty() && claimed == 0 {
        return Err(AppError::BadRequest(
            "invalid_comment",
            format!("Comments are 1 to {MAX_COMMENT} characters."),
        ));
    }
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&task.project_id)
        .execute(&mut *tx)
        .await?;
    log_task(
        &mut tx,
        &task.project_id,
        &id,
        &user.id,
        "task.commented",
        serde_json::json!({ "title": task.title }),
    )
    .await?;
    let (comment, _) = visible_comment(&mut tx, &user.id, &comment_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

/// Reword your own comment.
#[utoipa::path(patch, path = "/comments/{id}", tag = "tasks", params(("id" = String, Path)),
    request_body = CommentRequest,
    responses((status = 200, body = Comment), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_comment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CommentRequest>,
) -> AppResult<Json<Comment>> {
    let mut tx = state.db.begin().await?;
    let (current, _) = visible_comment(&mut tx, &user.id, &id).await?;
    // A photo comment may have no words.
    let body = clean_body(&req.body, !current.attachments.is_empty())?;
    if !current.can_edit {
        return Err(AppError::Forbidden("You can only edit your own comments.".into()));
    }
    if body != current.body {
        sqlx::query("UPDATE task_comments SET body = ?, edited_at = ? WHERE id = ?")
            .bind(&body)
            .bind(time::now())
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    let (comment, _) = visible_comment(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(comment))
}

/// Delete a comment: your own, or any on a project you own.
#[utoipa::path(delete, path = "/comments/{id}", tag = "tasks", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn delete_comment(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let (current, _) = visible_comment(&mut tx, &user.id, &id).await?;
    if !current.can_delete {
        return Err(AppError::Forbidden(
            "Only the person who wrote a comment, or the project's owner, can delete it.".into(),
        ));
    }
    sqlx::query("DELETE FROM task_comments WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
