//! Projects: the cards on the canvas.
//!
//! Who sees what comes from the `project_access` view: a person's own
//! projects plus whatever has been shared with them. [`visible_project`],
//! [`editable_project`] and [`owned_project`] are the checks every handler uses.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    canvas,
    error::{AppError, AppResult},
    links, notes, time,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_projects, create_project)).routes(routes!(
        get_project,
        update_project,
        delete_project
    ))
}

// ---------------------------------------------------------------------------
// Types

pub const STATUSES: &[&str] = &["idea", "active", "paused", "done", "archived"];

/// Numbers shown on a project's card.
#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct ProjectStats {
    pub total_tasks: i64,
    pub done_tasks: i64,
    pub open_tasks: i64,
    pub in_progress_tasks: i64,
    pub overdue_tasks: i64,
    pub child_count: i64,
    /// Latest activity, or the last edit when there is none.
    pub last_active_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct Project {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub summary: Option<String>,
    /// Markdown.
    pub description: Option<String>,
    /// `idea`, `active`, `paused`, `done` or `archived`.
    pub status: String,
    /// A theme slot (`slot-1` to `slot-8`) or a one-off `#rrggbb`.
    pub color: String,
    pub icon: Option<String>,
    pub started_at: Option<String>,
    pub target_date: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// What you can do with it: `owner`, `editor` or `viewer`.
    pub role: String,
    pub owner_name: String,
    /// The photo shown on its canvas card.
    pub cover_id: Option<String>,
    #[sqlx(flatten)]
    pub stats: ProjectStats,
}

pub(crate) const PROJECT_SELECT: &str = "SELECT p.id, p.owner_id, p.title, p.summary, p.description,
        p.status, p.color, p.icon, p.started_at, p.target_date, p.archived_at, p.created_at, p.updated_at,
        s.total_tasks, s.done_tasks, s.open_tasks, s.in_progress_tasks, s.overdue_tasks, s.child_count,
        s.last_active_at, a.role, o.name AS owner_name, p.cover_id
    FROM projects p JOIN project_stats s ON s.project_id = p.id
        JOIN project_access a ON a.project_id = p.id
        JOIN users o ON o.id = p.owner_id";

/// A parent or child, as shown in a project's breadcrumbs.
#[derive(Serialize, FromRow, ToSchema)]
pub struct ProjectRef {
    pub id: String,
    pub title: String,
    pub color: String,
    pub status: String,
    /// The link joining the two projects.
    pub link_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectDetail {
    pub project: Project,
    /// Projects this one was spawned from.
    pub parents: Vec<ProjectRef>,
    /// Projects spawned from this one.
    pub children: Vec<ProjectRef>,
}

#[derive(Deserialize, ToSchema)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub title: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    /// Defaults to `active`.
    pub status: Option<String>,
    /// Defaults to the next unused-looking slot.
    pub color: Option<String>,
    pub target_date: Option<String>,
    /// Draw a spawned link from this project to the new one.
    pub spawned_from: Option<String>,
    /// Where to put the card on your home canvas. Defaults to beside the others.
    pub position: Option<Position>,
}

/// Fields left out stay as they are. Send `null` to clear an optional field.
#[derive(Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub summary: Option<Option<String>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    /// Setting `archived` stamps `archived_at`; any other status clears it.
    pub status: Option<String>,
    pub color: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub icon: Option<Option<String>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub started_at: Option<Option<String>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub target_date: Option<Option<String>>,
}

/// Tells "field sent as null" (`Some(None)`) apart from "field left out" (`None`).
pub fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// Include archived projects. Default false.
    #[serde(default)]
    pub include_archived: bool,
}

// ---------------------------------------------------------------------------
// Validation

fn bad(code: &'static str, msg: &str) -> AppError {
    AppError::BadRequest(code, msg.to_string())
}

fn clean_title(title: &str) -> AppResult<String> {
    let t = title.trim();
    if t.is_empty() || t.chars().count() > 200 {
        return Err(bad("invalid_title", "Give the project a title of up to 200 characters."));
    }
    Ok(t.to_string())
}

pub fn clean_text(value: Option<String>, max: usize, code: &'static str) -> AppResult<Option<String>> {
    match value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        Some(v) if v.chars().count() > max => Err(bad(code, &format!("Keep it under {max} characters."))),
        other => Ok(other),
    }
}

fn check_status(status: &str) -> AppResult<()> {
    if STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(bad("invalid_status", "Status is idea, active, paused, done or archived."))
    }
}

pub fn check_color(color: &str) -> AppResult<()> {
    let slot = color
        .strip_prefix("slot-")
        .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8"));
    let hex = color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit());
    if slot || hex {
        Ok(())
    } else {
        Err(bad("invalid_color", "Colour is a theme slot (slot-1 to slot-8) or #rrggbb."))
    }
}

pub fn check_date(date: &Option<String>) -> AppResult<()> {
    match date {
        Some(d) if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() => {
            Err(bad("invalid_date", "Dates look like 2026-11-06."))
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers

/// The project if the signed-in person may see it, else 404.
pub async fn visible_project(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Project> {
    sqlx::query_as::<_, Project>(&format!("{PROJECT_SELECT} WHERE p.id = ? AND a.user_id = ?"))
        .bind(id)
        .bind(user_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

/// The project if the signed-in person may change it (owner or editor).
/// A viewer gets 403; someone it isn't shared with gets 404.
pub async fn editable_project(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Project> {
    let project = visible_project(conn, user_id, id).await?;
    if project.role == "viewer" {
        return Err(AppError::Forbidden(format!(
            "{} is shared with you to view, not to change.",
            project.title
        )));
    }
    Ok(project)
}

/// The project if the signed-in person owns it.
pub async fn owned_project(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Project> {
    let project = visible_project(conn, user_id, id).await?;
    if project.role != "owner" {
        return Err(AppError::Forbidden(format!(
            "Only {} can do that to {}.",
            project.owner_name, project.title
        )));
    }
    Ok(project)
}

/// Fails unless the person can change the project, without loading it.
pub async fn require_edit(conn: &mut SqliteConnection, user_id: &str, project_id: &str) -> AppResult<()> {
    editable_project(conn, user_id, project_id).await.map(|_| ())
}

tokio::task_local! {
    static VIA: &'static str;
}

/// Where the current change came from, for the activity feed: `web`, or `ai`
/// inside [`as_ai`].
pub fn via() -> &'static str {
    VIA.try_with(|v| *v).unwrap_or("web")
}

/// Runs `f` with every activity it logs marked as made through the MCP server.
pub async fn as_ai<F: std::future::Future>(f: F) -> F::Output {
    VIA.scope("ai", f).await
}

pub async fn log_activity(
    conn: &mut SqliteConnection,
    project_id: &str,
    user_id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO activity (id, project_id, user_id, kind, payload, via, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(time::new_id())
    .bind(project_id)
    .bind(user_id)
    .bind(kind)
    .bind(payload.to_string())
    .bind(via())
    .bind(time::now())
    .execute(conn)
    .await?;
    Ok(())
}

/// Board columns every new project starts with.
async fn create_default_statuses(conn: &mut SqliteConnection, project_id: &str) -> AppResult<()> {
    let now = time::now();
    let positions = crate::position::spread(3);
    for (i, (name, category)) in
        [("To do", "todo"), ("Doing", "in_progress"), ("Done", "done")].into_iter().enumerate()
    {
        sqlx::query(
            "INSERT INTO statuses (id, project_id, name, category, position, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(time::new_id())
        .bind(project_id)
        .bind(name)
        .bind(category)
        .bind(&positions[i])
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers

/// Your projects and the ones shared with you, with their card stats, oldest first.
#[utoipa::path(get, path = "/projects", tag = "projects", params(ListQuery),
    responses((status = 200, body = Vec<Project>)))]
pub(crate) async fn list_projects(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Project>>> {
    let archived = if q.include_archived { "" } else { " AND p.status <> 'archived'" };
    let projects = sqlx::query_as::<_, Project>(&format!(
        "{PROJECT_SELECT} WHERE a.user_id = ?{archived} ORDER BY p.created_at"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(projects))
}

/// Make a project, optionally spawned from another and placed on the canvas.
#[utoipa::path(post, path = "/projects", tag = "projects", request_body = CreateProjectRequest,
    responses((status = 201, body = Project), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody, description = "spawned_from not found")))]
pub(crate) async fn create_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateProjectRequest>,
) -> AppResult<(StatusCode, Json<Project>)> {
    let title = clean_title(&req.title)?;
    let summary = clean_text(req.summary, 500, "invalid_summary")?;
    let description = clean_text(req.description, 100_000, "invalid_description")?;
    let status = req.status.unwrap_or_else(|| "active".into());
    check_status(&status)?;
    check_date(&req.target_date)?;
    if let Some(c) = &req.color {
        check_color(c)?;
    }

    let mut tx = state.db.begin().await?;
    // A project spawned from a shared one joins its parent's family: the
    // parent's owner owns it, so it is shared the same way.
    let owner_id = match &req.spawned_from {
        Some(parent) => editable_project(&mut tx, &user.id, parent).await?.owner_id,
        None => user.id.clone(),
    };
    let color = match req.color {
        Some(c) => c,
        None => {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE owner_id = ?")
                .bind(&owner_id)
                .fetch_one(&mut *tx)
                .await?;
            format!("slot-{}", count % 8 + 1)
        }
    };
    let id = time::new_id();
    let now = time::now();
    let archived_at = (status == "archived").then(|| now.clone());
    sqlx::query(
        "INSERT INTO projects (id, owner_id, title, summary, description, status, color, target_date,
            archived_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&owner_id)
    .bind(&title)
    .bind(&summary)
    .bind(&description)
    .bind(&status)
    .bind(&color)
    .bind(&req.target_date)
    .bind(&archived_at)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    create_default_statuses(&mut tx, &id).await?;
    if let Some(parent) = &req.spawned_from {
        links::insert_link(&mut tx, parent, &id, "spawned", None).await?;
    }
    let canvas_id = canvas::home_canvas_id(&mut tx, &user.id).await?;
    let (x, y) = match req.position {
        Some(p) if p.x.is_finite() && p.y.is_finite() => (p.x, p.y),
        _ => canvas::next_free_spot(&mut tx, &canvas_id).await?,
    };
    canvas::place_project(&mut tx, &canvas_id, &id, x, y).await?;
    if owner_id != user.id {
        // Whoever spawned it keeps editing it, even if their share of the
        // parent doesn't take in children.
        let covered: Option<String> =
            sqlx::query_scalar("SELECT role FROM project_access WHERE project_id = ? AND user_id = ?")
                .bind(&id)
                .bind(&user.id)
                .fetch_optional(&mut *tx)
                .await?;
        if covered.is_none() {
            crate::sharing::insert_project_share(&mut tx, &id, &user.id, "editor", false).await?;
        }
    }
    log_activity(
        &mut tx,
        &id,
        &user.id,
        "project.created",
        serde_json::json!({ "spawned_from": req.spawned_from }),
    )
    .await?;
    let project = visible_project(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// One project with its parents and children.
#[utoipa::path(get, path = "/projects/{id}", tag = "projects", params(("id" = String, Path)),
    responses((status = 200, body = ProjectDetail), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn get_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<ProjectDetail>> {
    let mut conn = state.db.acquire().await?;
    let project = visible_project(&mut conn, &user.id, &id).await?;
    let parents = sqlx::query_as::<_, ProjectRef>(
        "SELECT p.id, p.title, p.color, p.status, l.id AS link_id
         FROM project_links l JOIN projects p ON p.id = l.source_id
         JOIN project_access a ON a.project_id = p.id
         WHERE l.target_id = ? AND l.kind = 'spawned' AND a.user_id = ? ORDER BY p.title",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_all(&mut *conn)
    .await?;
    let children = sqlx::query_as::<_, ProjectRef>(
        "SELECT p.id, p.title, p.color, p.status, l.id AS link_id
         FROM project_links l JOIN projects p ON p.id = l.target_id
         JOIN project_access a ON a.project_id = p.id
         WHERE l.source_id = ? AND l.kind = 'spawned' AND a.user_id = ? ORDER BY p.title",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(ProjectDetail { project, parents, children }))
}

/// Edit a project. Archive it by setting `status` to `archived`.
#[utoipa::path(patch, path = "/projects/{id}", tag = "projects", params(("id" = String, Path)),
    request_body = UpdateProjectRequest,
    responses((status = 200, body = Project), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> AppResult<Json<Project>> {
    let mut tx = state.db.begin().await?;
    let current = editable_project(&mut tx, &user.id, &id).await?;

    let title = match req.title {
        Some(t) => clean_title(&t)?,
        None => current.title.clone(),
    };
    let summary = match req.summary {
        Some(s) => clean_text(s, 500, "invalid_summary")?,
        None => current.summary.clone(),
    };
    let description = match req.description {
        Some(d) => clean_text(d, 100_000, "invalid_description")?,
        None => current.description.clone(),
    };
    let status = req.status.unwrap_or_else(|| current.status.clone());
    check_status(&status)?;
    let color = req.color.unwrap_or_else(|| current.color.clone());
    check_color(&color)?;
    let icon = match req.icon {
        Some(i) => clean_text(i, 32, "invalid_icon")?,
        None => current.icon.clone(),
    };
    let started_at = req.started_at.unwrap_or_else(|| current.started_at.clone());
    check_date(&started_at)?;
    let target_date = req.target_date.unwrap_or_else(|| current.target_date.clone());
    check_date(&target_date)?;
    let now = time::now();
    let archived_at = match (status.as_str(), &current.archived_at) {
        ("archived", Some(at)) => Some(at.clone()),
        ("archived", None) => Some(now.clone()),
        _ => None,
    };

    sqlx::query(
        "UPDATE projects SET title = ?, summary = ?, description = ?, status = ?, color = ?, icon = ?,
            started_at = ?, target_date = ?, archived_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&title)
    .bind(&summary)
    .bind(&description)
    .bind(&status)
    .bind(&color)
    .bind(&icon)
    .bind(&started_at)
    .bind(&target_date)
    .bind(&archived_at)
    .bind(&now)
    .bind(&id)
    .execute(&mut *tx)
    .await?;

    let kind = match (current.status.as_str(), status.as_str()) {
        (a, "archived") if a != "archived" => "project.archived",
        ("archived", b) if b != "archived" => "project.restored",
        (a, b) if a != b => "project.status_changed",
        _ => "project.updated",
    };
    log_activity(&mut tx, &id, &user.id, kind, serde_json::json!({ "from": current.status, "to": status }))
        .await?;
    let project = visible_project(&mut tx, &user.id, &id).await?;
    let people: Vec<String> = sqlx::query_scalar("SELECT user_id FROM project_access WHERE project_id = ?")
        .bind(&id)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    if title != current.title {
        // Notes name projects by title; every vault that can name this one
        // follows the rename.
        for person in people {
            notes::project_renamed(&state, &person, &current.title, &title).await?;
        }
    }
    Ok(Json(project))
}

/// Delete a project, its tasks and its links for good. Owner only.
#[utoipa::path(delete, path = "/projects/{id}", tag = "projects", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    owned_project(&mut tx, &user.id, &id).await?;
    // Tasks first: their status column is ON DELETE RESTRICT, and the cascade
    // from projects would otherwise reach statuses before tasks.
    sqlx::query("DELETE FROM tasks WHERE project_id = ?").bind(&id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM projects WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    crate::attachments::empty_trash(&state.db, &state.config.data_dir).await;
    Ok(StatusCode::NO_CONTENT)
}
