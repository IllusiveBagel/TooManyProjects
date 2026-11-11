//! Tasks, their checklists, and the global board across every project.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    board::{self, Label, Status, editable_status, insert_status, position_after, position_last},
    error::{AppError, AppResult},
    projects::{check_date, clean_text, editable_project, log_activity, require_edit, some, visible_project},
    time,
};

pub const PRIORITIES: &[&str] = &["low", "medium", "high", "urgent"];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_project_tasks, create_task))
        .routes(routes!(get_task, update_task, delete_task))
        .routes(routes!(global_tasks))
        .routes(routes!(add_checklist_item))
        .routes(routes!(update_checklist_item, delete_checklist_item))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, FromRow)]
struct TaskRow {
    id: String,
    project_id: String,
    number: i64,
    status_id: String,
    title: String,
    description: Option<String>,
    priority: Option<String>,
    due_date: Option<String>,
    position: String,
    assignee_id: Option<String>,
    assignee_name: Option<String>,
    completed_at: Option<String>,
    created_at: String,
    updated_at: String,
    checklist_done: i64,
    checklist_total: i64,
    comment_count: i64,
    note_count: i64,
    attachment_count: i64,
    parts_total: i64,
    parts_waiting: i64,
    category: String,
    status_name: String,
    project_title: String,
    project_color: String,
}

const TASK_SELECT: &str = "SELECT t.id, t.project_id, t.number, t.status_id, t.title, t.description,
        t.priority, t.due_date, t.position, t.assignee_id, au.name AS assignee_name,
        t.completed_at, t.created_at, t.updated_at,
        (SELECT COUNT(*) FROM task_checklist c WHERE c.task_id = t.id AND c.done = 1) AS checklist_done,
        (SELECT COUNT(*) FROM task_checklist c WHERE c.task_id = t.id) AS checklist_total,
        (SELECT COUNT(*) FROM task_comments tc WHERE tc.task_id = t.id) AS comment_count,
        (SELECT COUNT(*) FROM task_notes tn JOIN notes n ON n.id = tn.note_id WHERE tn.task_id = t.id) AS note_count,
        (SELECT COUNT(*) FROM attachments x WHERE x.task_id = t.id) AS attachment_count,
        (SELECT COUNT(*) FROM db_relations x JOIN db_columns fc ON fc.id = x.column_id AND fc.role = 'for_task'
            WHERE x.target_kind = 'task' AND x.target_id = t.id) AS parts_total,
        (SELECT COUNT(*) FROM db_relations x JOIN db_columns fc ON fc.id = x.column_id AND fc.role = 'for_task'
            JOIN db_rows r ON r.id = x.row_id
            JOIN db_columns sc ON sc.database_id = r.database_id AND sc.role = 'status'
            WHERE x.target_kind = 'task' AND x.target_id = t.id
              AND COALESCE(json_extract(r.data, '$.\"' || sc.id || '\"'), '') NOT IN ('arrived', 'fitted')) AS parts_waiting,
        s.category, s.name AS status_name, p.title AS project_title, p.color AS project_color
    FROM tasks t
    JOIN statuses s ON s.id = t.status_id
    JOIN projects p ON p.id = t.project_id
    LEFT JOIN users au ON au.id = t.assignee_id";

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    /// Per-project number, shown as T-12.
    pub number: i64,
    pub status_id: String,
    /// The column's category: `todo`, `in_progress`, `blocked` or `done`.
    pub category: String,
    /// The column's name, such as Doing.
    pub status_name: String,
    pub title: String,
    /// Markdown.
    pub description: Option<String>,
    /// `low`, `medium`, `high` or `urgent`.
    pub priority: Option<String>,
    pub due_date: Option<String>,
    pub position: String,
    /// Who is doing it, someone the project is shared with.
    pub assignee_id: Option<String>,
    pub assignee_name: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<Label>,
    pub checklist_done: i64,
    pub checklist_total: i64,
    /// Comments in the task's log.
    pub comment_count: i64,
    /// Notes linked to the task, including any not shared with you.
    pub note_count: i64,
    /// Photos and files on it, its comments' included.
    pub attachment_count: i64,
    /// Parts (rows of Parts databases) for this task.
    pub parts_total: i64,
    /// Of those, the ones not arrived or fitted yet.
    pub parts_waiting: i64,
    /// Open, with a due date before today.
    pub overdue: bool,
    pub project_title: String,
    pub project_color: String,
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub position: String,
}

/// One line of a project's or task's history.
#[derive(Serialize, FromRow, ToSchema)]
pub struct ActivityItem {
    pub id: String,
    /// Such as `task.created`, `task.moved`, `project.archived`.
    pub kind: String,
    pub task_id: Option<String>,
    pub user_name: Option<String>,
    /// Details for the kind, such as the task title or the columns moved between.
    #[sqlx(json)]
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    /// `web` or `ai`.
    pub via: String,
    pub created_at: String,
}

pub(crate) const ACTIVITY_SELECT: &str = "SELECT a.id, a.kind, a.task_id, u.name AS user_name, a.payload,
        a.via, a.created_at
    FROM activity a LEFT JOIN users u ON u.id = a.user_id";

#[derive(Serialize, ToSchema)]
pub struct TaskDetail {
    pub task: Task,
    pub checklist: Vec<ChecklistItem>,
    pub activity: Vec<ActivityItem>,
    /// The task's log, oldest first.
    pub comments: Vec<crate::comments::Comment>,
    /// Linked notes you can read, most recently updated first.
    pub notes: Vec<crate::notes::NoteSummary>,
    /// Linked notes that aren't shared with you.
    pub hidden_notes: i64,
    /// Photos and files on the task itself, oldest first. Those added with a
    /// comment are on the comment.
    pub attachments: Vec<crate::attachments::Attachment>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTaskRequest {
    pub title: String,
    /// Column to add it to. Defaults to the first column.
    pub status_id: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub due_date: Option<String>,
    #[serde(default)]
    pub label_ids: Vec<String>,
    /// Someone who can see the project, or `null` for nobody. Left out, the
    /// task is assigned to whoever creates it.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub assignee_id: Option<Option<String>>,
    /// Add at the top of the column instead of the bottom.
    #[serde(default)]
    pub at_top: bool,
}

/// Fields left out stay as they are. Send `null` to clear an optional field.
#[derive(Deserialize, ToSchema)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub priority: Option<Option<String>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub due_date: Option<Option<String>>,
    /// Replaces the task's labels.
    pub label_ids: Option<Vec<String>>,
    /// Someone who can see the project, or `null` for nobody.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub assignee_id: Option<Option<String>>,
    /// Move to this column.
    pub status_id: Option<String>,
    /// Move to the first column of this category, as the global board does.
    /// Adds the column if the project has none.
    pub category: Option<String>,
    /// Place the card after this task in its column, or at the top when `null`.
    /// Left out, a moved card goes to the bottom and an unmoved one stays put.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams)]
pub struct GlobalQuery {
    /// Only this project.
    pub project_id: Option<String>,
    /// This project and every project spawned from it, at any depth.
    pub family: Option<String>,
    /// Hide tasks completed more than this many days ago. Default 14.
    pub done_days: Option<i64>,
    /// Only tasks assigned to you.
    #[serde(default)]
    pub mine: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct ChecklistRequest {
    pub text: Option<String>,
    pub done: Option<bool>,
}

// ---------------------------------------------------------------------------
// Helpers

/// Fails unless `assignee` can see the project.
async fn check_assignee(
    conn: &mut SqliteConnection,
    project_id: &str,
    assignee: &Option<String>,
) -> AppResult<()> {
    if let Some(a) = assignee {
        let ok: Option<String> =
            sqlx::query_scalar("SELECT role FROM project_access WHERE project_id = ? AND user_id = ?")
                .bind(project_id)
                .bind(a)
                .fetch_optional(conn)
                .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest(
                "invalid_assignee",
                "Tasks can only be assigned to people the project is shared with.".into(),
            ));
        }
    }
    Ok(())
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

async fn attach_labels(conn: &mut SqliteConnection, rows: Vec<TaskRow>) -> AppResult<Vec<Task>> {
    let mut labels: HashMap<String, Vec<Label>> = HashMap::new();
    if !rows.is_empty() {
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT tl.task_id, l.id, l.project_id, l.name, l.color FROM task_labels tl
             JOIN labels l ON l.id = tl.label_id WHERE tl.task_id IN (",
        );
        let mut sep = q.separated(", ");
        for r in &rows {
            sep.push_bind(&r.id);
        }
        q.push(") ORDER BY l.name COLLATE NOCASE");
        let found: Vec<(String, String, String, String, Option<String>)> =
            q.build_query_as().fetch_all(&mut *conn).await?;
        for (task_id, id, project_id, name, color) in found {
            labels.entry(task_id).or_default().push(Label { id, project_id, name, color });
        }
    }
    let today = today();
    Ok(rows
        .into_iter()
        .map(|r| Task {
            overdue: r.category != "done" && r.due_date.as_deref().is_some_and(|d| d < today.as_str()),
            labels: labels.remove(&r.id).unwrap_or_default(),
            id: r.id,
            project_id: r.project_id,
            number: r.number,
            status_id: r.status_id,
            category: r.category,
            title: r.title,
            description: r.description,
            priority: r.priority,
            due_date: r.due_date,
            position: r.position,
            assignee_id: r.assignee_id,
            assignee_name: r.assignee_name,
            completed_at: r.completed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            checklist_done: r.checklist_done,
            checklist_total: r.checklist_total,
            comment_count: r.comment_count,
            note_count: r.note_count,
            attachment_count: r.attachment_count,
            parts_total: r.parts_total,
            parts_waiting: r.parts_waiting,
            status_name: r.status_name,
            project_title: r.project_title,
            project_color: r.project_color,
        })
        .collect())
}

/// A task the person can see, else 404.
pub(crate) async fn visible_task(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Task> {
    let row = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(format!(
        "{TASK_SELECT} WHERE t.id = ?
            AND EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = t.project_id AND a.user_id = ?)"
    )))
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(attach_labels(conn, vec![row]).await?.remove(0))
}

/// A task the person may change, else 404 or 403.
pub(crate) async fn editable_task(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Task> {
    let task = visible_task(conn, user_id, id).await?;
    require_edit(conn, user_id, &task.project_id).await?;
    Ok(task)
}

pub async fn project_tasks(conn: &mut SqliteConnection, project_id: &str) -> AppResult<Vec<Task>> {
    let rows = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(format!(
        "{TASK_SELECT} WHERE t.project_id = ? ORDER BY s.position, t.position"
    )))
    .bind(project_id)
    .fetch_all(&mut *conn)
    .await?;
    attach_labels(conn, rows).await
}

/// Open tasks for the Today screen: in progress, blocked, or due by `until`
/// (a YYYY-MM-DD date), on projects that aren't archived. Only tasks assigned
/// to `user_id`, or nobody's tasks on their own projects, so a friend's
/// shared board doesn't fill someone's day.
pub async fn today_tasks(conn: &mut SqliteConnection, user_id: &str, until: &str) -> AppResult<Vec<Task>> {
    let rows = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(format!(
        "{TASK_SELECT}
         WHERE EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = t.project_id AND a.user_id = ?1)
           AND p.status <> 'archived' AND s.category <> 'done'
           AND (t.assignee_id = ?1 OR (t.assignee_id IS NULL AND p.owner_id = ?1))
           AND (s.category IN ('in_progress', 'blocked') OR (t.due_date IS NOT NULL AND t.due_date <= ?2))
         ORDER BY CASE WHEN t.due_date IS NULL THEN 1 ELSE 0 END, t.due_date,
            CASE t.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END,
            t.updated_at DESC
         LIMIT 300"
    )))
    .bind(user_id)
    .bind(until)
    .fetch_all(&mut *conn)
    .await?;
    attach_labels(conn, rows).await
}

/// Tasks a note is linked to, among those `viewer` can see, open ones first.
pub async fn note_tasks(conn: &mut SqliteConnection, viewer: &str, note_id: &str) -> AppResult<Vec<Task>> {
    let rows = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(format!(
        "{TASK_SELECT} JOIN task_notes tn ON tn.task_id = t.id
         WHERE tn.note_id = ?
           AND EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = t.project_id AND a.user_id = ?)
         ORDER BY s.category = 'done', t.updated_at DESC"
    )))
    .bind(note_id)
    .bind(viewer)
    .fetch_all(&mut *conn)
    .await?;
    attach_labels(conn, rows).await
}

fn clean_title(title: &str) -> AppResult<String> {
    let t = title.trim();
    if t.is_empty() || t.chars().count() > 300 {
        return Err(AppError::BadRequest(
            "invalid_title",
            "Give the task a title of up to 300 characters.".into(),
        ));
    }
    Ok(t.to_string())
}

fn check_priority(p: &Option<String>) -> AppResult<()> {
    match p {
        Some(p) if !PRIORITIES.contains(&p.as_str()) => {
            Err(AppError::BadRequest("invalid_priority", "Priority is low, medium, high or urgent.".into()))
        }
        _ => Ok(()),
    }
}

async fn set_labels(
    conn: &mut SqliteConnection,
    project_id: &str,
    task_id: &str,
    label_ids: &[String],
) -> AppResult<()> {
    sqlx::query("DELETE FROM task_labels WHERE task_id = ?").bind(task_id).execute(&mut *conn).await?;
    for label_id in label_ids {
        let ok: Option<String> = sqlx::query_scalar("SELECT id FROM labels WHERE id = ? AND project_id = ?")
            .bind(label_id)
            .bind(project_id)
            .fetch_optional(&mut *conn)
            .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest(
                "invalid_label",
                "That label belongs to another project.".into(),
            ));
        }
        sqlx::query("INSERT OR IGNORE INTO task_labels (task_id, label_id) VALUES (?, ?)")
            .bind(task_id)
            .bind(label_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

const CATEGORY_NAMES: &[(&str, &str)] =
    &[("todo", "To do"), ("in_progress", "In progress"), ("blocked", "Blocked"), ("done", "Done")];

/// The first column of a category, made if the project has none.
async fn column_for_category(
    conn: &mut SqliteConnection,
    project_id: &str,
    category: &str,
) -> AppResult<Status> {
    board::CATEGORIES.contains(&category).then_some(()).ok_or_else(|| {
        AppError::BadRequest("invalid_category", "Category is todo, in_progress, blocked or done.".into())
    })?;
    let found = sqlx::query_as::<_, Status>(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM statuses WHERE project_id = ? AND category = ? ORDER BY position LIMIT 1",
        board::STATUS_COLUMNS
    )))
    .bind(project_id)
    .bind(category)
    .fetch_optional(&mut *conn)
    .await?;
    match found {
        Some(s) => Ok(s),
        None => {
            let name =
                CATEGORY_NAMES.iter().find(|(c, _)| *c == category).map(|(_, n)| *n).unwrap_or(category);
            insert_status(conn, project_id, name, category, None).await
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers

/// Every task in a project, column by column.
#[utoipa::path(get, path = "/projects/{id}/tasks", tag = "tasks", params(("id" = String, Path)),
    responses((status = 200, body = Vec<Task>), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn list_project_tasks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Task>>> {
    let mut conn = state.db.acquire().await?;
    visible_project(&mut conn, &user.id, &id).await?;
    Ok(Json(project_tasks(&mut conn, &id).await?))
}

/// Add a task to a project.
#[utoipa::path(post, path = "/projects/{id}/tasks", tag = "tasks", params(("id" = String, Path)),
    request_body = CreateTaskRequest,
    responses((status = 201, body = Task), (status = 400, body = crate::error::ErrorBody)))]
pub(crate) async fn create_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<(StatusCode, Json<Task>)> {
    let title = clean_title(&req.title)?;
    let description = clean_text(req.description, 100_000, "invalid_description")?;
    check_priority(&req.priority)?;
    check_date(&req.due_date)?;

    let mut tx = state.db.begin().await?;
    editable_project(&mut tx, &user.id, &id).await?;
    let assignee_id = req.assignee_id.clone().unwrap_or_else(|| Some(user.id.clone()));
    check_assignee(&mut tx, &id, &assignee_id).await?;
    let status = match &req.status_id {
        Some(s) => {
            let status = editable_status(&mut tx, &user.id, s).await?;
            if status.project_id != id {
                return Err(AppError::BadRequest(
                    "invalid_status",
                    "That column is on another board.".into(),
                ));
            }
            status
        }
        None => board::statuses_for(&mut tx, &id)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Conflict("no_columns", "Add a column to the board first.".into()))?,
    };
    let pos = if req.at_top {
        position_after(&mut tx, "tasks", "status_id", &status.id, None, None).await?
    } else {
        position_last(&mut tx, "tasks", "status_id", &status.id).await?
    };
    let number: i64 =
        sqlx::query_scalar("UPDATE projects SET next_task_number = next_task_number + 1 WHERE id = ? RETURNING next_task_number - 1")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
    let task_id = time::new_id();
    let now = time::now();
    let completed_at = (status.category == "done").then(|| now.clone());
    sqlx::query(
        "INSERT INTO tasks (id, project_id, number, status_id, title, description, priority, due_date, position,
            assignee_id, completed_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&task_id)
    .bind(&id)
    .bind(number)
    .bind(&status.id)
    .bind(&title)
    .bind(&description)
    .bind(&req.priority)
    .bind(&req.due_date)
    .bind(&pos)
    .bind(&assignee_id)
    .bind(&completed_at)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    set_labels(&mut tx, &id, &task_id, &req.label_ids).await?;
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    log_task(
        &mut tx,
        &id,
        &task_id,
        &user.id,
        "task.created",
        serde_json::json!({ "title": title, "to": status.name }),
    )
    .await?;
    let task = editable_task(&mut tx, &user.id, &task_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub(crate) async fn log_task(
    conn: &mut SqliteConnection,
    project_id: &str,
    task_id: &str,
    user_id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO activity (id, project_id, task_id, user_id, kind, payload, via, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(time::new_id())
    .bind(project_id)
    .bind(task_id)
    .bind(user_id)
    .bind(kind)
    .bind(payload.to_string())
    .bind(crate::projects::via())
    .bind(time::now())
    .execute(conn)
    .await?;
    Ok(())
}

/// One task with its checklist and history.
#[utoipa::path(get, path = "/tasks/{id}", tag = "tasks", params(("id" = String, Path)),
    responses((status = 200, body = TaskDetail), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn get_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<TaskDetail>> {
    let mut conn = state.db.acquire().await?;
    let task = visible_task(&mut conn, &user.id, &id).await?;
    let checklist = sqlx::query_as::<_, ChecklistItem>(
        "SELECT id, text, done, position FROM task_checklist WHERE task_id = ? ORDER BY position",
    )
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    let activity = sqlx::query_as::<_, ActivityItem>(sqlx::AssertSqlSafe(format!(
        "{ACTIVITY_SELECT} WHERE a.task_id = ? ORDER BY a.created_at DESC, a.rowid DESC LIMIT 30"
    )))
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    let comments = crate::comments::task_comments(&mut conn, &user.id, &task).await?;
    let (notes, hidden_notes) = crate::notes::task_notes(&mut conn, &user.id, &id).await?;
    let attachments = crate::attachments::for_task(&mut conn, &user.id, &id)
        .await?
        .into_iter()
        .filter(|a| a.comment_id.is_none())
        .collect();
    Ok(Json(TaskDetail { task, checklist, activity, comments, notes, hidden_notes, attachments }))
}

/// Edit a task, or move it between columns.
#[utoipa::path(patch, path = "/tasks/{id}", tag = "tasks", params(("id" = String, Path)),
    request_body = UpdateTaskRequest,
    responses((status = 200, body = Task), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin().await?;
    let current = editable_task(&mut tx, &user.id, &id).await?;

    let title = match &req.title {
        Some(t) => clean_title(t)?,
        None => current.title.clone(),
    };
    let description = match req.description.clone() {
        Some(d) => clean_text(d, 100_000, "invalid_description")?,
        None => current.description.clone(),
    };
    let priority = req.priority.clone().unwrap_or_else(|| current.priority.clone());
    check_priority(&priority)?;
    let due_date = req.due_date.clone().unwrap_or_else(|| current.due_date.clone());
    check_date(&due_date)?;
    let assignee_id = req.assignee_id.clone().unwrap_or_else(|| current.assignee_id.clone());
    if assignee_id != current.assignee_id {
        check_assignee(&mut tx, &current.project_id, &assignee_id).await?;
    }

    // Where the card ends up.
    let target = match (&req.status_id, &req.category) {
        (Some(s), _) => {
            let status = editable_status(&mut tx, &user.id, s).await?;
            if status.project_id != current.project_id {
                return Err(AppError::BadRequest(
                    "invalid_status",
                    "That column is on another board.".into(),
                ));
            }
            Some(status)
        }
        (None, Some(c)) if *c != current.category => {
            Some(column_for_category(&mut tx, &current.project_id, c).await?)
        }
        _ => None,
    };
    let moved_column = target.as_ref().is_some_and(|s| s.id != current.status_id);
    let (status_id, category, from_name, to_name) = match &target {
        Some(s) => {
            let from: String = sqlx::query_scalar("SELECT name FROM statuses WHERE id = ?")
                .bind(&current.status_id)
                .fetch_one(&mut *tx)
                .await?;
            (s.id.clone(), s.category.clone(), from, s.name.clone())
        }
        None => (current.status_id.clone(), current.category.clone(), String::new(), String::new()),
    };
    let pos = match &req.after_id {
        Some(after) => {
            position_after(&mut tx, "tasks", "status_id", &status_id, after.as_deref(), Some(&id)).await?
        }
        None if moved_column => position_last(&mut tx, "tasks", "status_id", &status_id).await?,
        None => current.position.clone(),
    };
    let now = time::now();
    let completed_at = match (current.category.as_str(), category.as_str()) {
        ("done", "done") => current.completed_at.clone(),
        (_, "done") => Some(now.clone()),
        _ => None,
    };

    sqlx::query(
        "UPDATE tasks SET title = ?, description = ?, priority = ?, due_date = ?, assignee_id = ?, status_id = ?,
            position = ?, completed_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&title)
    .bind(&description)
    .bind(&priority)
    .bind(&due_date)
    .bind(&assignee_id)
    .bind(&status_id)
    .bind(&pos)
    .bind(&completed_at)
    .bind(&now)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    if let Some(label_ids) = &req.label_ids {
        set_labels(&mut tx, &current.project_id, &id, label_ids).await?;
    }
    sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&current.project_id)
        .execute(&mut *tx)
        .await?;

    if moved_column {
        let kind =
            if category == "done" && current.category != "done" { "task.completed" } else { "task.moved" };
        log_task(
            &mut tx,
            &current.project_id,
            &id,
            &user.id,
            kind,
            serde_json::json!({ "title": title, "from": from_name, "to": to_name }),
        )
        .await?;
    } else if req.title.is_some()
        || req.description.is_some()
        || req.priority.is_some()
        || req.due_date.is_some()
        || req.label_ids.is_some()
        || req.assignee_id.is_some()
    {
        let mut fields = vec![];
        for (name, changed) in [
            ("title", req.title.is_some() && title != current.title),
            ("description", req.description.is_some() && description != current.description),
            ("priority", req.priority.is_some() && priority != current.priority),
            ("due date", req.due_date.is_some() && due_date != current.due_date),
            ("assignee", req.assignee_id.is_some() && assignee_id != current.assignee_id),
            ("labels", req.label_ids.is_some()),
        ] {
            if changed {
                fields.push(name);
            }
        }
        if !fields.is_empty() && fields != ["labels"] {
            log_task(
                &mut tx,
                &current.project_id,
                &id,
                &user.id,
                "task.updated",
                serde_json::json!({ "title": title, "fields": fields }),
            )
            .await?;
        }
    }
    let task = editable_task(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(task))
}

/// Delete a task.
#[utoipa::path(delete, path = "/tasks/{id}", tag = "tasks", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let task = editable_task(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM tasks WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    log_activity(
        &mut tx,
        &task.project_id,
        &user.id,
        "task.deleted",
        serde_json::json!({ "title": task.title }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Tasks across every project, for the global board. Archived projects are
/// left out, and so are tasks finished more than `done_days` ago.
#[utoipa::path(get, path = "/tasks", tag = "tasks", params(GlobalQuery),
    responses((status = 200, body = Vec<Task>)))]
pub(crate) async fn global_tasks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<GlobalQuery>,
) -> AppResult<Json<Vec<Task>>> {
    let mut conn = state.db.acquire().await?;
    let done_days = q.done_days.unwrap_or(14).clamp(0, 3650);
    let since = time::format(chrono::Utc::now() - chrono::Duration::days(done_days));
    let mut sql: QueryBuilder<Sqlite> = QueryBuilder::new("");
    if let Some(root) = &q.family {
        sql.push("WITH RECURSIVE fam(id) AS (SELECT ");
        sql.push_bind(root);
        sql.push(
            " UNION SELECT l.target_id FROM project_links l JOIN fam f ON l.source_id = f.id WHERE l.kind = 'spawned') ",
        );
    }
    sql.push(TASK_SELECT);
    sql.push(
        " WHERE EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = t.project_id AND a.user_id = ",
    );
    sql.push_bind(&user.id);
    sql.push(")");
    sql.push(" AND p.status <> 'archived' AND (s.category <> 'done' OR t.completed_at >= ");
    sql.push_bind(&since);
    sql.push(")");
    if q.mine {
        sql.push(" AND t.assignee_id = ");
        sql.push_bind(&user.id);
    }
    if let Some(p) = &q.project_id {
        sql.push(" AND t.project_id = ");
        sql.push_bind(p);
    }
    if q.family.is_some() {
        sql.push(" AND t.project_id IN (SELECT id FROM fam)");
    }
    sql.push(
        " ORDER BY CASE WHEN t.due_date IS NULL THEN 1 ELSE 0 END, t.due_date,
            CASE t.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END,
            t.updated_at DESC",
    );
    let rows: Vec<TaskRow> = sql.build_query_as().fetch_all(&mut *conn).await?;
    Ok(Json(attach_labels(&mut conn, rows).await?))
}

// ---------------------------------------------------------------------------
// Checklist

async fn visible_item(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<String> {
    let task_id: String = sqlx::query_scalar("SELECT task_id FROM task_checklist WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    editable_task(conn, user_id, &task_id).await?;
    Ok(task_id)
}

fn clean_item(text: &str) -> AppResult<String> {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > 300 {
        return Err(AppError::BadRequest("invalid_text", "Checklist items are 1 to 300 characters.".into()));
    }
    Ok(t.to_string())
}

/// Add a checklist item to the bottom of a task's list.
#[utoipa::path(post, path = "/tasks/{id}/checklist", tag = "tasks", params(("id" = String, Path)),
    request_body = ChecklistRequest, responses((status = 201, body = ChecklistItem)))]
async fn add_checklist_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ChecklistRequest>,
) -> AppResult<(StatusCode, Json<ChecklistItem>)> {
    let text = clean_item(req.text.as_deref().unwrap_or(""))?;
    let mut tx = state.db.begin().await?;
    editable_task(&mut tx, &user.id, &id).await?;
    let pos = position_last(&mut tx, "task_checklist", "task_id", &id).await?;
    let item = ChecklistItem { id: time::new_id(), text, done: req.done.unwrap_or(false), position: pos };
    sqlx::query("INSERT INTO task_checklist (id, task_id, text, done, position, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&item.id)
        .bind(&id)
        .bind(&item.text)
        .bind(item.done)
        .bind(&item.position)
        .bind(time::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// Tick, untick or reword a checklist item.
#[utoipa::path(patch, path = "/checklist/{id}", tag = "tasks", params(("id" = String, Path)),
    request_body = ChecklistRequest, responses((status = 200, body = ChecklistItem)))]
async fn update_checklist_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ChecklistRequest>,
) -> AppResult<Json<ChecklistItem>> {
    let mut tx = state.db.begin().await?;
    visible_item(&mut tx, &user.id, &id).await?;
    if let Some(t) = &req.text {
        sqlx::query("UPDATE task_checklist SET text = ? WHERE id = ?")
            .bind(clean_item(t)?)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(done) = req.done {
        sqlx::query("UPDATE task_checklist SET done = ? WHERE id = ?")
            .bind(done)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    let item = sqlx::query_as::<_, ChecklistItem>(
        "SELECT id, text, done, position FROM task_checklist WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(item))
}

/// Remove a checklist item.
#[utoipa::path(delete, path = "/checklist/{id}", tag = "tasks", params(("id" = String, Path)),
    responses((status = 204)))]
async fn delete_checklist_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    visible_item(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM task_checklist WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
