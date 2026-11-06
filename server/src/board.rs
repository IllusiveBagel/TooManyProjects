//! Board columns (statuses) and labels, both per project.
//!
//! Each column has a category (`todo`, `in_progress`, `blocked`, `done`) that
//! places its tasks on the global board and decides when a task counts as done.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    position,
    projects::{check_color, log_activity, some, visible_project},
    time,
};

pub const CATEGORIES: &[&str] = &["todo", "in_progress", "blocked", "done"];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_statuses, create_status))
        .routes(routes!(update_status, delete_status))
        .routes(routes!(list_labels, create_label))
        .routes(routes!(update_label, delete_label))
}

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct Status {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// `todo`, `in_progress`, `blocked` or `done`.
    pub category: String,
    pub color: Option<String>,
    pub position: String,
}

pub(crate) const STATUS_COLUMNS: &str = "id, project_id, name, category, color, position";

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct Label {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateStatusRequest {
    pub name: String,
    pub category: String,
    pub color: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateStatusRequest {
    pub name: Option<String>,
    pub category: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub color: Option<Option<String>>,
    /// Move the column: put it after this column, or first when `null`.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams)]
pub struct DeleteStatusQuery {
    /// Move the column's tasks here first. Needed when it has any.
    pub move_to: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LabelRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub color: Option<Option<String>>,
}

// ---------------------------------------------------------------------------
// Helpers

fn clean_name(name: &str, what: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 40 {
        return Err(AppError::BadRequest(
            "invalid_name",
            format!("Give the {what} a name up to 40 characters."),
        ));
    }
    Ok(n.to_string())
}

fn check_category(c: &str) -> AppResult<()> {
    if CATEGORIES.contains(&c) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "invalid_category",
            "Category is todo, in_progress, blocked or done.".into(),
        ))
    }
}

pub async fn statuses_for(conn: &mut SqliteConnection, project_id: &str) -> AppResult<Vec<Status>> {
    Ok(sqlx::query_as::<_, Status>(&format!(
        "SELECT {STATUS_COLUMNS} FROM statuses WHERE project_id = ? ORDER BY position"
    ))
    .bind(project_id)
    .fetch_all(conn)
    .await?)
}

/// A column the person can see, else 404.
pub async fn visible_status(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Status> {
    let status = sqlx::query_as::<_, Status>(&format!("SELECT {STATUS_COLUMNS} FROM statuses WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    visible_project(conn, user_id, &status.project_id).await?;
    Ok(status)
}

/// The key for a row placed right after `after` (or first when `None`) in a
/// list ordered by `position`, skipping `moving` itself.
pub async fn position_after(
    conn: &mut SqliteConnection,
    table: &str,
    scope_column: &str,
    scope: &str,
    after: Option<&str>,
    moving: Option<&str>,
) -> AppResult<String> {
    let moving = moving.unwrap_or("");
    let a: Option<String> = match after {
        Some(id) => Some(
            sqlx::query_scalar(&format!("SELECT position FROM {table} WHERE id = ? AND {scope_column} = ?"))
                .bind(id)
                .bind(scope)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or_else(|| {
                    AppError::BadRequest("invalid_after", "That neighbour isn't in this list.".into())
                })?,
        ),
        None => None,
    };
    let b: Option<String> = sqlx::query_scalar(&format!(
        "SELECT position FROM {table} WHERE {scope_column} = ? AND id <> ? AND position > ? ORDER BY position LIMIT 1"
    ))
    .bind(scope)
    .bind(moving)
    .bind(a.as_deref().unwrap_or(""))
    .fetch_optional(&mut *conn)
    .await?;
    Ok(position::between(a.as_deref(), b.as_deref()))
}

/// The key for the end of a list.
pub async fn position_last(
    conn: &mut SqliteConnection,
    table: &str,
    scope_column: &str,
    scope: &str,
) -> AppResult<String> {
    let last: Option<String> =
        sqlx::query_scalar(&format!("SELECT MAX(position) FROM {table} WHERE {scope_column} = ?"))
            .bind(scope)
            .fetch_one(&mut *conn)
            .await?;
    Ok(position::between(last.as_deref(), None))
}

pub async fn insert_status(
    conn: &mut SqliteConnection,
    project_id: &str,
    name: &str,
    category: &str,
    color: Option<&str>,
) -> AppResult<Status> {
    let id = time::new_id();
    let pos = position_last(conn, "statuses", "project_id", project_id).await?;
    sqlx::query(
        "INSERT INTO statuses (id, project_id, name, category, color, position, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(name)
    .bind(category)
    .bind(color)
    .bind(&pos)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    Ok(Status {
        id,
        project_id: project_id.into(),
        name: name.into(),
        category: category.into(),
        color: color.map(Into::into),
        position: pos,
    })
}

// ---------------------------------------------------------------------------
// Status handlers

/// A project's board columns, left to right.
#[utoipa::path(get, path = "/projects/{id}/statuses", tag = "board", params(("id" = String, Path)),
    responses((status = 200, body = Vec<Status>), (status = 404, body = crate::error::ErrorBody)))]
async fn list_statuses(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Status>>> {
    let mut conn = state.db.acquire().await?;
    visible_project(&mut conn, &user.id, &id).await?;
    Ok(Json(statuses_for(&mut conn, &id).await?))
}

/// Add a column at the right end of the board.
#[utoipa::path(post, path = "/projects/{id}/statuses", tag = "board", params(("id" = String, Path)),
    request_body = CreateStatusRequest,
    responses((status = 201, body = Status), (status = 400, body = crate::error::ErrorBody)))]
async fn create_status(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateStatusRequest>,
) -> AppResult<(StatusCode, Json<Status>)> {
    let name = clean_name(&req.name, "column")?;
    check_category(&req.category)?;
    if let Some(c) = &req.color {
        check_color(c)?;
    }
    let mut tx = state.db.begin().await?;
    visible_project(&mut tx, &user.id, &id).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM statuses WHERE project_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= 20 {
        return Err(AppError::BadRequest("too_many_columns", "A board can have up to 20 columns.".into()));
    }
    let status = insert_status(&mut tx, &id, &name, &req.category, req.color.as_deref()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(status)))
}

/// Rename, recategorise, recolour or move a column.
#[utoipa::path(patch, path = "/statuses/{id}", tag = "board", params(("id" = String, Path)),
    request_body = UpdateStatusRequest,
    responses((status = 200, body = Status), (status = 404, body = crate::error::ErrorBody)))]
async fn update_status(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateStatusRequest>,
) -> AppResult<Json<Status>> {
    let mut tx = state.db.begin().await?;
    let current = visible_status(&mut tx, &user.id, &id).await?;
    let name = match &req.name {
        Some(n) => clean_name(n, "column")?,
        None => current.name.clone(),
    };
    let category = req.category.clone().unwrap_or_else(|| current.category.clone());
    check_category(&category)?;
    let color = req.color.clone().unwrap_or_else(|| current.color.clone());
    if let Some(c) = &color {
        check_color(c)?;
    }
    let pos = match &req.after_id {
        Some(after) if after.as_deref() != Some(id.as_str()) => {
            position_after(
                &mut tx,
                "statuses",
                "project_id",
                &current.project_id,
                after.as_deref(),
                Some(&id),
            )
            .await?
        }
        _ => current.position.clone(),
    };
    sqlx::query("UPDATE statuses SET name = ?, category = ?, color = ?, position = ? WHERE id = ?")
        .bind(&name)
        .bind(&category)
        .bind(&color)
        .bind(&pos)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    // A column moving in or out of `done` changes whether its tasks are complete.
    if category != current.category {
        let now = time::now();
        sqlx::query(
            "UPDATE tasks SET completed_at = CASE WHEN ? = 'done' THEN COALESCE(completed_at, ?) ELSE NULL END
             WHERE status_id = ?",
        )
        .bind(&category)
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    }
    let status = visible_status(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(status))
}

/// Remove a column. Its tasks must be moved first, with `move_to`.
#[utoipa::path(delete, path = "/statuses/{id}", tag = "board", params(("id" = String, Path), DeleteStatusQuery),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody, description = "Last column, or tasks left in it")))]
async fn delete_status(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<DeleteStatusQuery>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let status = visible_status(&mut tx, &user.id, &id).await?;
    let columns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM statuses WHERE project_id = ?")
        .bind(&status.project_id)
        .fetch_one(&mut *tx)
        .await?;
    if columns <= 1 {
        return Err(AppError::Conflict("last_column", "A board needs at least one column.".into()));
    }
    let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE status_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    if tasks > 0 {
        let Some(target) = q.move_to.filter(|t| *t != id) else {
            return Err(AppError::Conflict(
                "column_not_empty",
                "Move this column's tasks to another column first.".into(),
            ));
        };
        let dest = visible_status(&mut tx, &user.id, &target).await?;
        if dest.project_id != status.project_id {
            return Err(AppError::BadRequest("invalid_move_to", "Pick a column on the same board.".into()));
        }
        // Append the moved tasks after the destination's last card, keeping their order.
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE status_id = ? ORDER BY position")
                .bind(&id)
                .fetch_all(&mut *tx)
                .await?;
        let now = time::now();
        for task_id in ids {
            let pos = position_last(&mut tx, "tasks", "status_id", &dest.id).await?;
            sqlx::query(
                "UPDATE tasks SET status_id = ?, position = ?, updated_at = ?,
                    completed_at = CASE WHEN ? = 'done' THEN COALESCE(completed_at, ?) ELSE NULL END
                 WHERE id = ?",
            )
            .bind(&dest.id)
            .bind(&pos)
            .bind(&now)
            .bind(&dest.category)
            .bind(&now)
            .bind(&task_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("DELETE FROM statuses WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    log_activity(
        &mut tx,
        &status.project_id,
        &user.id,
        "status.deleted",
        serde_json::json!({ "name": status.name }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Label handlers

async fn visible_label(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Label> {
    let label = sqlx::query_as::<_, Label>("SELECT id, project_id, name, color FROM labels WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    visible_project(conn, user_id, &label.project_id).await?;
    Ok(label)
}

/// A project's labels, by name.
#[utoipa::path(get, path = "/projects/{id}/labels", tag = "board", params(("id" = String, Path)),
    responses((status = 200, body = Vec<Label>)))]
async fn list_labels(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Label>>> {
    let mut conn = state.db.acquire().await?;
    visible_project(&mut conn, &user.id, &id).await?;
    let labels = sqlx::query_as::<_, Label>(
        "SELECT id, project_id, name, color FROM labels WHERE project_id = ? ORDER BY name COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(labels))
}

/// Make a label for this project's tasks.
#[utoipa::path(post, path = "/projects/{id}/labels", tag = "board", params(("id" = String, Path)),
    request_body = LabelRequest,
    responses((status = 201, body = Label), (status = 409, body = crate::error::ErrorBody)))]
async fn create_label(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<LabelRequest>,
) -> AppResult<(StatusCode, Json<Label>)> {
    let name = clean_name(req.name.as_deref().unwrap_or(""), "label")?;
    let color = req.color.flatten().unwrap_or_else(|| "slot-1".into());
    check_color(&color)?;
    let mut tx = state.db.begin().await?;
    visible_project(&mut tx, &user.id, &id).await?;
    let taken: Option<String> =
        sqlx::query_scalar("SELECT id FROM labels WHERE project_id = ? AND name = ? COLLATE NOCASE")
            .bind(&id)
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await?;
    if taken.is_some() {
        return Err(AppError::Conflict("duplicate_label", "This project already has that label.".into()));
    }
    let label = Label { id: time::new_id(), project_id: id, name, color: Some(color) };
    sqlx::query("INSERT INTO labels (id, project_id, name, color) VALUES (?, ?, ?, ?)")
        .bind(&label.id)
        .bind(&label.project_id)
        .bind(&label.name)
        .bind(&label.color)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(label)))
}

/// Rename or recolour a label.
#[utoipa::path(patch, path = "/labels/{id}", tag = "board", params(("id" = String, Path)),
    request_body = LabelRequest, responses((status = 200, body = Label)))]
async fn update_label(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<LabelRequest>,
) -> AppResult<Json<Label>> {
    let mut tx = state.db.begin().await?;
    let mut label = visible_label(&mut tx, &user.id, &id).await?;
    if let Some(n) = &req.name {
        label.name = clean_name(n, "label")?;
    }
    if let Some(c) = req.color {
        if let Some(c) = &c {
            check_color(c)?;
        }
        label.color = c;
    }
    sqlx::query("UPDATE labels SET name = ?, color = ? WHERE id = ?")
        .bind(&label.name)
        .bind(&label.color)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                AppError::Conflict("duplicate_label", "This project already has that label.".into())
            }
            _ => e.into(),
        })?;
    tx.commit().await?;
    Ok(Json(label))
}

/// Delete a label. Tasks keep everything else.
#[utoipa::path(delete, path = "/labels/{id}", tag = "board", params(("id" = String, Path)),
    responses((status = 204)))]
async fn delete_label(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    visible_label(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM labels WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
