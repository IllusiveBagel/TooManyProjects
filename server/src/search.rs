//! One search over everything a person can see: projects, tasks, task
//! comments, notes and database rows. The Ctrl+K palette and the MCP `search`
//! tool both use it.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, User},
    error::AppResult,
    notes,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search))
}

#[derive(Deserialize, IntoParams)]
pub struct SearchQuery {
    /// The words to look for. Each word matches the start of a word, so
    /// `brak` finds `brakes`.
    pub q: String,
    /// Most results of each kind. Default 8, at most 50.
    pub limit: Option<i64>,
}

/// One thing that matched.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SearchHit {
    /// `project`, `task`, `comment`, `note` or `row`.
    pub kind: String,
    /// The project, task, comment, note or row id.
    pub id: String,
    /// The name to show: a title, or for a comment the words around the match.
    pub title: String,
    /// The matching words in context, with `[` and `]` around each hit.
    pub snippet: Option<String>,
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub project_color: Option<String>,
    /// The task a comment is on, or the task itself.
    pub task_id: Option<String>,
    /// The task's number, shown as T-12.
    pub task_number: Option<i64>,
    /// The column a task is in, a note's folder, or a row's database.
    pub context: Option<String>,
    /// A row's database.
    pub database_id: Option<String>,
    /// A finished task, or a project that is done or archived.
    pub done: bool,
    pub updated_at: String,
}

/// Everything that matched, kind by kind, best matches first within each.
#[utoipa::path(get, path = "/search", tag = "search", params(SearchQuery),
    responses((status = 200, body = Vec<SearchHit>)))]
pub(crate) async fn search(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<SearchHit>>> {
    Ok(Json(run(&state, &user, &q.q, q.limit.unwrap_or(8)).await?))
}

/// `%word%` for LIKE, with the wildcards in the words themselves escaped.
fn like_pattern(q: &str) -> String {
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

pub async fn run(state: &AppState, user: &User, q: &str, limit: i64) -> AppResult<Vec<SearchHit>> {
    let q = q.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 50);
    let fts = notes::fts_query(q);
    if fts.is_empty() {
        return Ok(vec![]);
    }
    let like = like_pattern(q);
    let mut conn = state.db.acquire().await?;
    let mut out = vec![];
    out.extend(tasks(&mut conn, &user.id, &fts, limit).await?);
    out.extend(comments(&mut conn, &user.id, &fts, limit).await?);
    let Json(found) = notes::list_notes(
        State(state.clone()),
        CurrentUser(user.clone()),
        Query(notes::ListNotesQuery {
            q: Some(q.to_string()),
            tag: None,
            project_id: None,
            folder: None,
            shared: None,
            limit: Some(limit),
        }),
    )
    .await?;
    for n in found {
        let first = n.projects.first();
        out.push(SearchHit {
            kind: "note".into(),
            id: n.id,
            title: n.title,
            snippet: n.snippet,
            project_id: first.map(|p| p.id.clone()),
            project_title: first.map(|p| p.title.clone()),
            project_color: first.map(|p| p.color.clone()),
            task_id: None,
            task_number: None,
            context: Some(n.path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or(n.folder)),
            database_id: None,
            done: false,
            updated_at: n.updated_at,
        });
    }
    out.extend(rows(&mut conn, &user.id, &like, limit).await?);
    out.extend(projects(&mut conn, &user.id, &like, limit).await?);
    Ok(out)
}

type TaskHit = (String, i64, String, String, String, String, String, String, String);

async fn tasks(
    conn: &mut SqliteConnection,
    user_id: &str,
    fts: &str,
    limit: i64,
) -> AppResult<Vec<SearchHit>> {
    let found: Vec<TaskHit> = sqlx::query_as(
        "SELECT t.id, t.number, t.title, p.id, p.title, p.color, s.name, s.category, t.updated_at
         FROM tasks_fts f JOIN tasks t ON t.id = f.task_id
         JOIN projects p ON p.id = t.project_id JOIN statuses s ON s.id = t.status_id
         WHERE tasks_fts MATCH ?2
           AND EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = p.id AND a.user_id = ?1)
         ORDER BY s.category = 'done', p.status = 'archived', bm25(tasks_fts, 0, 5, 1)
         LIMIT ?3",
    )
    .bind(user_id)
    .bind(fts)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(found
        .into_iter()
        .map(|(id, number, title, pid, ptitle, pcolor, column, category, updated_at)| SearchHit {
            kind: "task".into(),
            task_id: Some(id.clone()),
            id,
            title,
            snippet: None,
            project_id: Some(pid),
            project_title: Some(ptitle),
            project_color: Some(pcolor),
            task_number: Some(number),
            context: Some(column),
            database_id: None,
            done: category == "done",
            updated_at,
        })
        .collect())
}

type CommentHit = (String, String, String, i64, String, String, String, String, String);

async fn comments(
    conn: &mut SqliteConnection,
    user_id: &str,
    fts: &str,
    limit: i64,
) -> AppResult<Vec<SearchHit>> {
    let found: Vec<CommentHit> = sqlx::query_as(
        "SELECT c.id, snippet(task_comments_fts, 1, '[', ']', '…', 10), t.id, t.number, t.title,
            p.id, p.title, p.color, c.created_at
         FROM task_comments_fts f JOIN task_comments c ON c.id = f.comment_id
         JOIN tasks t ON t.id = c.task_id JOIN projects p ON p.id = t.project_id
         WHERE task_comments_fts MATCH ?2
           AND EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = p.id AND a.user_id = ?1)
         ORDER BY bm25(task_comments_fts), c.created_at DESC
         LIMIT ?3",
    )
    .bind(user_id)
    .bind(fts)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(found
        .into_iter()
        .map(|(id, snippet, task_id, number, task_title, pid, ptitle, pcolor, at)| SearchHit {
            kind: "comment".into(),
            id,
            title: task_title,
            snippet: Some(snippet),
            project_id: Some(pid),
            project_title: Some(ptitle),
            project_color: Some(pcolor),
            task_id: Some(task_id),
            task_number: Some(number),
            context: None,
            database_id: None,
            done: false,
            updated_at: at,
        })
        .collect())
}

type RowHit = (String, Option<String>, String, String, String, String, String, String);

async fn rows(
    conn: &mut SqliteConnection,
    user_id: &str,
    like: &str,
    limit: i64,
) -> AppResult<Vec<SearchHit>> {
    // A row's title is its title column's value. Other text columns are
    // matched through the stored JSON, which holds the values as typed.
    let found: Vec<RowHit> = sqlx::query_as(
        "SELECT r.id,
            (SELECT json_extract(r.data, '$.\"' || c.id || '\"') FROM db_columns c
              WHERE c.database_id = d.id AND c.is_title = 1 LIMIT 1) AS title,
            d.id, d.name, p.id, p.title, p.color, r.updated_at
         FROM db_rows r JOIN databases d ON d.id = r.database_id JOIN projects p ON p.id = d.project_id
         WHERE r.data LIKE ?2 ESCAPE '\\'
           AND EXISTS (SELECT 1 FROM project_access a WHERE a.project_id = p.id AND a.user_id = ?1)
         ORDER BY title LIKE ?2 ESCAPE '\\' DESC, r.updated_at DESC
         LIMIT ?3",
    )
    .bind(user_id)
    .bind(like)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(found
        .into_iter()
        .map(|(id, title, db_id, db_name, pid, ptitle, pcolor, at)| SearchHit {
            kind: "row".into(),
            id,
            title: title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Untitled row".into()),
            snippet: None,
            project_id: Some(pid),
            project_title: Some(ptitle),
            project_color: Some(pcolor),
            task_id: None,
            task_number: None,
            context: Some(db_name),
            database_id: Some(db_id),
            done: false,
            updated_at: at,
        })
        .collect())
}

type ProjectHit = (String, String, String, String, Option<String>, String);

async fn projects(
    conn: &mut SqliteConnection,
    user_id: &str,
    like: &str,
    limit: i64,
) -> AppResult<Vec<SearchHit>> {
    let found: Vec<ProjectHit> = sqlx::query_as(
        "SELECT p.id, p.title, p.color, p.status, p.summary, p.updated_at FROM projects p
         JOIN project_access a ON a.project_id = p.id AND a.user_id = ?1
         WHERE p.title LIKE ?2 ESCAPE '\\' OR p.summary LIKE ?2 ESCAPE '\\' OR p.description LIKE ?2 ESCAPE '\\'
         ORDER BY p.title LIKE ?2 ESCAPE '\\' DESC, p.status IN ('done', 'archived'), p.updated_at DESC
         LIMIT ?3",
    )
    .bind(user_id)
    .bind(like)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(found
        .into_iter()
        .map(|(id, title, color, status, summary, at)| SearchHit {
            kind: "project".into(),
            project_id: Some(id.clone()),
            project_title: Some(title.clone()),
            project_color: Some(color),
            id,
            title,
            snippet: summary.filter(|s| !s.is_empty()),
            task_id: None,
            task_number: None,
            context: Some(status.clone()),
            database_id: None,
            done: status == "done" || status == "archived",
            updated_at: at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::like_pattern;

    #[test]
    fn like_escapes_wildcards() {
        assert_eq!(like_pattern("50%_off"), "%50\\%\\_off%");
    }
}
