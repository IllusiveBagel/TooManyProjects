//! The MCP server at `/mcp`: lets an AI assistant read your projects, tasks
//! and notes, and (with a write token) add tasks, notes and projects. It runs
//! the same handlers as the web app, as the person who owns the token, so it
//! can see and change exactly what they can.

pub mod brief;
mod data;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, GetPromptRequestParams, GetPromptResponse, GetPromptResult,
        Implementation, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        PaginatedRequestParams, Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Role,
        ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
    },
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use sqlx::SqliteConnection;

use crate::{
    AppState,
    auth::{CurrentUser, User},
    board, comments, dashboard,
    error::{AppError, AppResult},
    notes,
    projects::{self, Project, as_ai, visible_project},
    tasks::{self, Task},
    tokens::{self, TokenCaller},
};

/// `/mcp`, behind bearer-token sign-in.
pub fn router(state: AppState) -> Router<AppState> {
    let service_state = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(Mcp::new(service_state.clone())),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            // One request, one answer: nothing to keep between calls.
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            // Every call needs a bearer token, which a page in a browser can't
            // send on someone's behalf, so the DNS-rebinding host check isn't
            // needed and would refuse the app's own public address.
            .disable_allowed_hosts(),
    );
    Router::new().nest_service("/mcp", service).layer(middleware::from_fn_with_state(state, require_token))
}

fn unauthorized(message: &str) -> Response {
    let mut res = (StatusCode::UNAUTHORIZED, Json(json!({ "code": "unauthorized", "message": message })))
        .into_response();
    res.headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"TooManyProjects\""));
    res
}

/// Finds the person behind `Authorization: Bearer tmp_…` and passes them on
/// to the tools through the request's extensions.
async fn require_token(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let secret = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim().to_string());
    let Some(secret) = secret else {
        return unauthorized(
            "Send an API token from Settings → AI access as `Authorization: Bearer <token>`.",
        );
    };
    match tokens::caller_for(&state, &secret).await {
        Ok(Some(caller)) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Ok(None) => unauthorized("That token is unknown or has been revoked."),
        Err(e) => e.into_response(),
    }
}

// ---------------------------------------------------------------------------
// Tool inputs

#[derive(Deserialize, JsonSchema)]
pub struct ListProjectsArgs {
    /// Only projects with this status: idea, active, paused, done or archived.
    pub status: Option<String>,
    /// Only this project and everything spawned from it (id or exact title).
    pub family: Option<String>,
    /// Include archived projects. Default false.
    pub include_archived: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProjectArgs {
    /// Project id, or its exact title.
    pub project: String,
    /// Also brief every project spawned from it, at any depth. Default false.
    pub include_family: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Words to look for in projects, tasks and notes.
    pub query: String,
    /// Most results per kind. Default 10, at most 50.
    pub limit: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListNotesArgs {
    /// Full-text search over titles and bodies.
    pub query: Option<String>,
    /// Only notes with this tag (without the #).
    pub tag: Option<String>,
    /// Only notes filed under this project (id or exact title).
    pub project: Option<String>,
    /// Only notes in this top folder, such as Inbox, Knowledge or Projects.
    pub folder: Option<String>,
    /// Default 30, at most 200.
    pub limit: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteArgs {
    /// Note id, or its exact title.
    pub note: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateTaskArgs {
    /// Project id, or its exact title.
    pub project: String,
    pub title: String,
    /// Column name (such as "Doing") or category (todo, in_progress, blocked, done). Default: the first column.
    pub column: Option<String>,
    /// Markdown.
    pub description: Option<String>,
    /// low, medium, high or urgent.
    pub priority: Option<String>,
    /// YYYY-MM-DD.
    pub due_date: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateTaskArgs {
    /// Task id, or `T-12` together with `project`.
    pub task: String,
    /// Needed only when `task` is a number like T-12.
    pub project: Option<String>,
    pub title: Option<String>,
    /// Move to this column name or category (todo, in_progress, blocked, done).
    pub column: Option<String>,
    /// true moves it to the first done column; false back to the first to-do column.
    pub done: Option<bool>,
    /// Markdown. Replaces the description.
    pub description: Option<String>,
    /// low, medium, high or urgent.
    pub priority: Option<String>,
    /// YYYY-MM-DD.
    pub due_date: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TaskArgs {
    /// Task id, or `T-12` together with `project`.
    pub task: String,
    /// Needed only when `task` is a number like T-12.
    pub project: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CommentArgs {
    /// Task id, or `T-12` together with `project`.
    pub task: String,
    /// Needed only when `task` is a number like T-12.
    pub project: Option<String>,
    /// Markdown: what was done, tried, found or decided.
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LinkNoteArgs {
    /// Task id, or `T-12` together with `project`.
    pub task: String,
    /// Needed only when `task` is a number like T-12.
    pub project: Option<String>,
    /// Note id, or its exact title.
    pub note: String,
    /// true removes the link instead. Default false.
    pub unlink: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddNoteArgs {
    /// Defaults to the date and time.
    pub title: Option<String>,
    /// Markdown. Link notes with [[Title]] and projects with [[@Project]].
    pub body: String,
    /// File it under this project (id or exact title). Without one it goes to the Inbox.
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    /// A folder such as Knowledge/Cars instead of the Inbox or the project's folder.
    pub folder: Option<String>,
    /// Link the note to this task: its id, or `T-12` together with `project`.
    /// Without a folder, the note is then filed under the task's project.
    pub task: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AppendArgs {
    /// Note id, or its exact title.
    pub note: String,
    /// Markdown to add at the end.
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SpawnArgs {
    /// The parent: project id or exact title.
    pub parent: String,
    pub title: String,
    /// One line.
    pub summary: Option<String>,
    /// Markdown.
    pub description: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct QueryDatabaseArgs {
    /// Database id, or its name (any case).
    pub database: String,
    /// The project it's in (id or title), when two projects have databases with that name.
    pub project: Option<String>,
    /// A saved view's name, to use its filters, sorts and columns.
    pub view: Option<String>,
    /// Only rows with this text in any cell.
    pub search: Option<String>,
    /// Most rows to show. Default 50, at most 200.
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddRowArgs {
    /// Database id, or its name (any case).
    pub database: String,
    /// The project it's in (id or title), when two projects have databases with that name.
    pub project: Option<String>,
    /// Column name to value. Selects take a choice's name, multi-selects a list of names,
    /// dates YYYY-MM-DD, people a name or email, relations titles (tasks also `T-12`).
    pub values: std::collections::HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Helpers

fn text(s: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(s)])
}

/// A failure the model should read and can fix.
fn failed(e: AppError) -> CallToolResult {
    let message = match &e {
        AppError::NotFound => "Not found, or not shared with you.".to_string(),
        AppError::Internal(err) => {
            tracing::error!(error = ?err, "mcp tool failed");
            "Something went wrong on the server.".to_string()
        }
        other => other.to_string(),
    };
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn reply(r: AppResult<String>) -> Result<CallToolResult, McpError> {
    Ok(match r {
        Ok(s) => text(s),
        Err(e) => failed(e),
    })
}

fn caller(ctx: &RequestContext<RoleServer>) -> Result<TokenCaller, McpError> {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .and_then(|p| p.extensions.get::<TokenCaller>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("Not signed in.", None))
}

fn writer(ctx: &RequestContext<RoleServer>) -> Result<Result<TokenCaller, CallToolResult>, McpError> {
    let c = caller(ctx)?;
    Ok(if c.can_write {
        Ok(c)
    } else {
        Err(CallToolResult::error(vec![ContentBlock::text(
            "This token is read-only. Make a read-write token in Settings → AI access to change things.",
        )]))
    })
}

fn bad(code: &'static str, msg: impl Into<String>) -> AppError {
    AppError::BadRequest(code, msg.into())
}

/// A project by id, or by exact title (any case) among those the person can
/// see, their own first.
async fn find_project(conn: &mut SqliteConnection, user: &User, key: &str) -> AppResult<Project> {
    let key = key.trim();
    if let Ok(p) = visible_project(conn, &user.id, key).await {
        return Ok(p);
    }
    let id: Option<String> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN project_access a ON a.project_id = p.id
             WHERE a.user_id = ?1 AND p.title = ?2 COLLATE NOCASE ORDER BY p.owner_id = ?1 DESC LIMIT 1",
    )
    .bind(&user.id)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    match id {
        Some(id) => visible_project(conn, &user.id, &id).await,
        None => {
            let titles: Vec<String> = sqlx::query_scalar(
                "SELECT p.title FROM projects p JOIN project_access a ON a.project_id = p.id
                     WHERE a.user_id = ? AND p.status <> 'archived'",
            )
            .bind(&user.id)
            .fetch_all(&mut *conn)
            .await?;
            let like = close_titles(key, &titles);
            let hint = if like.is_empty() {
                "Use list_projects to see them.".to_string()
            } else {
                format!("Did you mean: {}?", like.join(", "))
            };
            Err(bad("unknown_project", format!("No project called “{key}”. {hint}")))
        }
    }
}

/// Titles that contain `key`, or have a word starting like it, for "did you mean".
fn close_titles(key: &str, titles: &[String]) -> Vec<String> {
    let key = key.to_lowercase();
    let stem: String = key.chars().take(3).collect();
    titles
        .iter()
        .filter(|t| {
            let t = t.to_lowercase();
            t.contains(&key)
                || (stem.chars().count() == 3 && t.split_whitespace().any(|w| w.starts_with(&stem)))
        })
        .take(5)
        .cloned()
        .collect()
}

/// A note id, from an id or an exact title (any case).
async fn find_note(conn: &mut SqliteConnection, user: &User, key: &str) -> AppResult<String> {
    let key = key.trim();
    let id: Option<String> = sqlx::query_scalar(
        "SELECT n.id FROM notes n JOIN note_access na ON na.note_id = n.id AND na.user_id = ?1
         WHERE n.id = ?2 OR n.title = ?2 COLLATE NOCASE
         ORDER BY n.id = ?2 DESC, n.user_id = ?1 DESC, n.updated_at DESC LIMIT 1",
    )
    .bind(&user.id)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    id.ok_or_else(|| {
        bad("unknown_note", format!("No note called “{key}”. Use list_notes or search to find it."))
    })
}

/// A task id, from an id or `T-12` with the project it's in.
async fn find_task(
    conn: &mut SqliteConnection,
    user: &User,
    key: &str,
    project: Option<&str>,
) -> AppResult<String> {
    let key = key.trim();
    let number =
        key.strip_prefix("T-").or_else(|| key.strip_prefix("t-")).and_then(|n| n.parse::<i64>().ok());
    match (number, project) {
        (Some(n), Some(p)) => {
            let project = find_project(conn, user, p).await?;
            sqlx::query_scalar("SELECT id FROM tasks WHERE project_id = ? AND number = ?")
                .bind(&project.id)
                .bind(n)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or_else(|| bad("unknown_task", format!("{} has no T-{n}.", project.title)))
        }
        (Some(_), None) => Err(bad("need_project", "Give the project too when naming a task as T-12.")),
        _ => Ok(key.to_string()),
    }
}

/// A column's id from a name or a category, for create and move.
fn pick_column(statuses: &[board::Status], wanted: &str) -> Result<ColumnChoice, AppError> {
    let w = wanted.trim();
    if let Some(s) = statuses.iter().find(|s| s.name.eq_ignore_ascii_case(w) || s.id == w) {
        return Ok(ColumnChoice::Status(s.id.clone()));
    }
    let category = match w.to_lowercase().replace([' ', '-'], "_").as_str() {
        "todo" | "to_do" | "backlog" => "todo",
        "in_progress" | "doing" | "started" => "in_progress",
        "blocked" | "waiting" => "blocked",
        "done" | "finished" | "complete" | "completed" => "done",
        _ => {
            let names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
            return Err(bad(
                "unknown_column",
                format!("No column called “{w}”. The columns are: {}.", names.join(", ")),
            ));
        }
    };
    Ok(ColumnChoice::Category(category))
}

enum ColumnChoice {
    Status(String),
    Category(&'static str),
}

/// Projects spawned from `root`, at any depth, not counting `root`.
async fn descendants(conn: &mut SqliteConnection, user: &User, root: &str) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE fam(id) AS (SELECT ? UNION SELECT l.target_id FROM project_links l JOIN fam f ON l.source_id = f.id WHERE l.kind = 'spawned')
         SELECT p.id FROM fam JOIN projects p ON p.id = fam.id
         JOIN project_access a ON a.project_id = p.id
         WHERE a.user_id = ? AND p.id <> ? ORDER BY p.created_at",
    )
    .bind(root)
    .bind(&user.id)
    .bind(root)
    .fetch_all(&mut *conn)
    .await?)
}

const INSTRUCTIONS: &str = "TooManyProjects holds the user's projects (cards on a canvas, linked when one project \
spawned another), each with a kanban board of tasks, plus a vault of markdown notes. Start with list_projects \
or search, then get_project_context for a full briefing on one project. Notes link with [[Note title]] and \
[[@Project title]]. Projects can hold databases (tables with typed columns): read one with query_database. Tasks keep a log of \
comments and can have notes linked to them: read both with get_task, and log progress or lessons with \
add_task_comment. Write tools (create_task, update_task, add_task_comment, link_note_to_task, add_note, \
append_to_note, spawn_project, add_row) need a read-write token, and everything they change shows in the project's history as done via AI. Prefer adding to \
an existing note over making a near-duplicate. Projects other people have shared with the user are included; \
a project shared as viewer can be read but not changed.";

// ---------------------------------------------------------------------------
// The server

#[derive(Clone)]
pub struct Mcp {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl Mcp {
    pub fn new(state: AppState) -> Self {
        Self { state, tool_router: Self::tool_router() }
    }

    async fn conn(&self) -> AppResult<sqlx::pool::PoolConnection<sqlx::Sqlite>> {
        Ok(self.state.db.acquire().await?)
    }

    async fn project_context(&self, user: &User, key: &str, family: bool) -> AppResult<String> {
        let mut conn = self.conn().await?;
        let project = find_project(&mut conn, user, key).await?;
        let id = project.id.clone();
        let st = || State(self.state.clone());
        let me = || CurrentUser(user.clone());
        let Json(detail) = projects::get_project(st(), me(), Path(id.clone())).await?;
        let Json(dash) = dashboard::project_dashboard(st(), me(), Path(id.clone())).await?;
        let Json(notes) = notes::project_notes(st(), me(), Path(id.clone())).await?;
        let statuses = board::statuses_for(&mut conn, &id).await?;
        let task_list = tasks::project_tasks(&mut conn, &id).await?;
        let other_links: Vec<(String, bool, String)> = sqlx::query_as(
            "SELECT l.kind, l.source_id = ?1, p.title FROM project_links l
             JOIN projects p ON p.id = CASE WHEN l.source_id = ?1 THEN l.target_id ELSE l.source_id END
             JOIN project_access a ON a.project_id = p.id
             WHERE (l.source_id = ?1 OR l.target_id = ?1) AND l.kind <> 'spawned' AND a.user_id = ?2",
        )
        .bind(&id)
        .bind(&user.id)
        .fetch_all(&mut *conn)
        .await?;
        let mut out = brief::project(&brief::ProjectBrief {
            detail: &detail,
            statuses: &statuses,
            tasks: &task_list,
            dashboard: &dash,
            notes: &notes,
            other_links: &other_links,
        });
        let dbs: Vec<(String, i64)> = sqlx::query_as(
            "SELECT d.name, (SELECT COUNT(*) FROM db_rows r WHERE r.database_id = d.id) FROM databases d
             WHERE d.project_id = ? ORDER BY d.position",
        )
        .bind(&id)
        .fetch_all(&mut *conn)
        .await?;
        if !dbs.is_empty() {
            out.push_str("\n## Databases\n\n");
            for (name, rows) in dbs {
                out.push_str(&format!(
                    "- {name}: {rows} row{} (read with query_database)\n",
                    if rows == 1 { "" } else { "s" }
                ));
            }
        }
        if family {
            let kin = descendants(&mut conn, user, &id).await?;
            if !kin.is_empty() {
                out.push_str("\n## Spawned projects\n\n");
                for k in kin.iter().take(20) {
                    let p = visible_project(&mut conn, &user.id, k).await?;
                    let t = tasks::project_tasks(&mut conn, k).await?;
                    out.push_str(&brief::relative(&p, &t));
                    out.push('\n');
                }
            }
        }
        Ok(out)
    }

    async fn note_text(&self, user: &User, key: &str) -> AppResult<String> {
        let mut conn = self.conn().await?;
        let id = find_note(&mut conn, user, key).await?;
        let Json(detail) =
            notes::get_note(State(self.state.clone()), CurrentUser(user.clone()), Path(id)).await?;
        Ok(brief::note(&detail))
    }

    async fn weekly_review(&self, user: &User) -> AppResult<String> {
        let Json(projects) = projects::list_projects(
            State(self.state.clone()),
            CurrentUser(user.clone()),
            Query(projects::ListQuery { include_archived: false }),
        )
        .await?;
        let Json(all) = tasks::global_tasks(
            State(self.state.clone()),
            CurrentUser(user.clone()),
            Query(tasks::GlobalQuery { project_id: None, family: None, done_days: Some(7), mine: false }),
        )
        .await?;
        let mut out = String::from("# The week across every project\n\n## Projects\n\n");
        for p in &projects {
            out.push_str(&brief::project_line(p));
            out.push('\n');
        }
        let section = |title: &str, list: Vec<&Task>| {
            let mut s = format!("\n## {title} ({})\n\n", list.len());
            for t in list.iter().take(40) {
                s.push_str(&format!("{} · {}\n", brief::task_line(t), t.project_title));
            }
            s
        };
        out.push_str(&section("Finished this week", all.iter().filter(|t| t.category == "done").collect()));
        out.push_str(&section("Overdue", all.iter().filter(|t| t.overdue).collect()));
        out.push_str(&section("In progress", all.iter().filter(|t| t.category == "in_progress").collect()));
        out.push_str(&section("Blocked", all.iter().filter(|t| t.category == "blocked").collect()));
        Ok(out)
    }
}

#[tool_router]
impl Mcp {
    #[tool(
        description = "List the user's projects with status, open, in-progress, overdue and done task counts, and ids. Filter by status or by a family (a project and everything spawned from it)."
    )]
    async fn list_projects(
        &self,
        Parameters(args): Parameters<ListProjectsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let include_archived =
                    args.include_archived.unwrap_or(false) || args.status.as_deref() == Some("archived");
                let Json(mut list) = projects::list_projects(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Query(projects::ListQuery { include_archived }),
                )
                .await?;
                if let Some(s) = &args.status {
                    list.retain(|p| p.status.eq_ignore_ascii_case(s));
                }
                if let Some(f) = &args.family {
                    let mut conn = self.conn().await?;
                    let root = find_project(&mut conn, &user, f).await?;
                    let mut ids = descendants(&mut conn, &user, &root.id).await?;
                    ids.push(root.id);
                    list.retain(|p| ids.contains(&p.id));
                }
                if list.is_empty() {
                    return Ok("No projects match.".to_string());
                }
                let mut out = format!("{} projects:\n\n", list.len());
                for p in &list {
                    out.push_str(&brief::project_line(p));
                    out.push('\n');
                }
                Ok(out)
            }
            .await,
        )
    }

    #[tool(
        description = "A markdown briefing on one project: summary, status, parent and spawned projects, open tasks by column with due dates, recently finished tasks, linked notes and recent activity. Set include_family to also brief everything spawned from it."
    )]
    async fn get_project_context(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(self.project_context(&user, &args.project, args.include_family.unwrap_or(false)).await)
    }

    #[tool(
        description = "Search projects (title, summary, description), tasks (title, description, comments) and notes (full text) for some words."
    )]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let q = args.query.trim();
                if q.is_empty() {
                    return Err(bad("empty_query", "Give some words to search for."));
                }
                let limit = args.limit.unwrap_or(10).clamp(1, 50);
                let like = format!("%{}%", q.replace(['%', '_'], ""));
                let mut conn = self.conn().await?;
                let found_projects: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
                    "SELECT p.id, p.title, p.status, p.summary FROM projects p
                     JOIN project_access a ON a.project_id = p.id AND a.user_id = ?1
                     WHERE p.title LIKE ?2 OR p.summary LIKE ?2 OR p.description LIKE ?2
                     ORDER BY p.updated_at DESC LIMIT ?3",
                )
                .bind(&user.id)
                .bind(&like)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?;
                let found_tasks: Vec<(String, i64, String, String, String)> = sqlx::query_as(
                    "SELECT t.id, t.number, t.title, p.title, s.name FROM tasks t
                     JOIN projects p ON p.id = t.project_id JOIN statuses s ON s.id = t.status_id
                     JOIN project_access a ON a.project_id = p.id AND a.user_id = ?1
                     WHERE (t.title LIKE ?2 OR t.description LIKE ?2
                        OR EXISTS (SELECT 1 FROM task_comments c WHERE c.task_id = t.id AND c.body LIKE ?2))
                     ORDER BY s.category = 'done', t.updated_at DESC LIMIT ?3",
                )
                .bind(&user.id)
                .bind(&like)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?;
                let Json(found_notes) = notes::list_notes(
                    State(self.state.clone()),
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
                let mut out = format!("Results for “{q}”:\n\n## Projects ({})\n\n", found_projects.len());
                for (id, title, status, summary) in &found_projects {
                    out.push_str(&format!("- **{title}** ({status}) · id `{id}`"));
                    if let Some(s) = summary.as_deref().filter(|s| !s.is_empty()) {
                        out.push_str(&format!(" · {s}"));
                    }
                    out.push('\n');
                }
                out.push_str(&format!("\n## Tasks ({})\n\n", found_tasks.len()));
                for (id, number, title, project, column) in &found_tasks {
                    out.push_str(&format!("- T-{number} {title} · {project} · {column} · id `{id}`\n"));
                }
                out.push_str(&format!("\n## Notes ({})\n\n", found_notes.len()));
                for n in &found_notes {
                    out.push_str(&brief::note_line(n));
                    out.push('\n');
                }
                Ok(out)
            }
            .await,
        )
    }

    #[tool(
        description = "List notes, newest first, or the best matches for a query. Filter by tag, project or top folder (Inbox, Knowledge, Projects)."
    )]
    async fn list_notes(
        &self,
        Parameters(args): Parameters<ListNotesArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let project_id = match &args.project {
                    Some(p) => Some(find_project(&mut *self.conn().await?, &user, p).await?.id),
                    None => None,
                };
                let Json(list) = notes::list_notes(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Query(notes::ListNotesQuery {
                        q: args.query.clone(),
                        tag: args.tag.clone().map(|t| t.trim_start_matches('#').to_string()),
                        project_id,
                        folder: args.folder.clone(),
                        shared: None,
                        limit: Some(args.limit.unwrap_or(30).clamp(1, 200)),
                    }),
                )
                .await?;
                if list.is_empty() {
                    return Ok("No notes match.".to_string());
                }
                let mut out = format!("{} notes:\n\n", list.len());
                for n in &list {
                    out.push_str(&brief::note_line(n));
                    out.push('\n');
                }
                Ok(out)
            }
            .await,
        )
    }

    #[tool(description = "Read one note in full, with its tags, projects and the notes that link to it.")]
    async fn get_note(
        &self,
        Parameters(args): Parameters<NoteArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(self.note_text(&user, &args.note).await)
    }

    #[tool(description = "Add a task to a project's board. Needs a read-write token.")]
    async fn create_task(
        &self,
        Parameters(args): Parameters<CreateTaskArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let mut conn = self.conn().await?;
                let project = find_project(&mut conn, &user, &args.project).await?;
                let statuses = board::statuses_for(&mut conn, &project.id).await?;
                let status_id = match &args.column {
                    None => None,
                    Some(c) => match pick_column(&statuses, c)? {
                        ColumnChoice::Status(id) => Some(id),
                        ColumnChoice::Category(cat) => {
                            statuses.iter().find(|s| s.category == cat).map(|s| s.id.clone())
                        }
                    },
                };
                drop(conn);
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "status_id": status_id,
                    "description": args.description,
                    "priority": args.priority,
                    "due_date": args.due_date,
                }))
                .map_err(|e| bad("invalid_task", e.to_string()))?;
                let (_, Json(task)) = tasks::create_task(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(project.id.clone()),
                    Json(req),
                )
                .await?;
                Ok(format!("Added to {}:\n{}", project.title, brief::task_line(&task)))
            })
            .await,
        )
    }

    #[tool(
        description = "Change a task: rename it, move it to another column, mark it done, or set its description, priority or due date. Needs a read-write token."
    )]
    async fn update_task(
        &self,
        Parameters(args): Parameters<UpdateTaskArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let mut conn = self.conn().await?;
                let task_id = find_task(&mut conn, &user, &args.task, args.project.as_deref()).await?;
                let Json(current) = tasks::get_task(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(task_id.clone()),
                )
                .await?;
                let statuses = board::statuses_for(&mut conn, &current.task.project_id).await?;
                drop(conn);
                let mut body = serde_json::Map::new();
                if let Some(t) = &args.title {
                    body.insert("title".into(), json!(t));
                }
                if let Some(d) = &args.description {
                    body.insert("description".into(), json!(d));
                }
                if let Some(p) = &args.priority {
                    body.insert("priority".into(), json!(p));
                }
                if let Some(d) = &args.due_date {
                    body.insert("due_date".into(), json!(d));
                }
                let column = match (&args.column, args.done) {
                    (Some(c), _) => Some(pick_column(&statuses, c)?),
                    (None, Some(true)) => Some(ColumnChoice::Category("done")),
                    (None, Some(false)) => Some(ColumnChoice::Category("todo")),
                    (None, None) => None,
                };
                match column {
                    Some(ColumnChoice::Status(id)) => {
                        body.insert("status_id".into(), json!(id));
                    }
                    Some(ColumnChoice::Category(cat)) => {
                        body.insert("category".into(), json!(cat));
                    }
                    None => {}
                }
                if body.is_empty() {
                    return Err(bad(
                        "nothing_to_change",
                        "Say what to change: title, column, done, description, priority or due_date.",
                    ));
                }
                let req = serde_json::from_value(serde_json::Value::Object(body))
                    .map_err(|e| bad("invalid_task", e.to_string()))?;
                let Json(task) = tasks::update_task(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(task_id),
                    Json(req),
                )
                .await?;
                let column =
                    statuses.iter().find(|s| s.id == task.status_id).map(|s| s.name.as_str()).unwrap_or("?");
                Ok(format!("Updated in {} ({column}):\n{}", task.project_title, brief::task_line(&task)))
            })
            .await,
        )
    }

    #[tool(
        description = "Read one task in full: its details, checklist, the comments logged on it (what was done and learnt), the notes linked to it, and recent history."
    )]
    async fn get_task(
        &self,
        Parameters(args): Parameters<TaskArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let mut conn = self.conn().await?;
                let id = find_task(&mut conn, &user, &args.task, args.project.as_deref()).await?;
                drop(conn);
                let Json(detail) =
                    tasks::get_task(State(self.state.clone()), CurrentUser(user.clone()), Path(id)).await?;
                let statuses = board::statuses_for(&mut *self.conn().await?, &detail.task.project_id).await?;
                let column = statuses
                    .iter()
                    .find(|s| s.id == detail.task.status_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                Ok(brief::task(&detail, &column))
            }
            .await,
        )
    }

    #[tool(
        description = "Log a comment on a task: progress, what was tried, a measurement, a decision. Comments build the task's work log. Needs a read-write token."
    )]
    async fn add_task_comment(
        &self,
        Parameters(args): Parameters<CommentArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let id =
                    find_task(&mut *self.conn().await?, &user, &args.task, args.project.as_deref()).await?;
                let req = serde_json::from_value(json!({ "body": args.text }))
                    .map_err(|e| bad("invalid_comment", e.to_string()))?;
                let (_, Json(c)) = comments::add_comment(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(id.clone()),
                    Json(req),
                )
                .await?;
                let Json(detail) =
                    tasks::get_task(State(self.state.clone()), CurrentUser(user.clone()), Path(id)).await?;
                Ok(format!(
                    "Logged on T-{} {} ({} comment{}) · comment id `{}`",
                    detail.task.number,
                    detail.task.title,
                    detail.comments.len(),
                    if detail.comments.len() == 1 { "" } else { "s" },
                    c.id
                ))
            })
            .await,
        )
    }

    #[tool(
        description = "Link an existing note to a task (or unlink it), so the note shows on the task and the task on the note. To write a new note for a task, use add_note with `task`. Needs a read-write token."
    )]
    async fn link_note_to_task(
        &self,
        Parameters(args): Parameters<LinkNoteArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let mut conn = self.conn().await?;
                let task_id = find_task(&mut conn, &user, &args.task, args.project.as_deref()).await?;
                let note_id = find_note(&mut conn, &user, &args.note).await?;
                drop(conn);
                let st = || State(self.state.clone());
                let me = || CurrentUser(user.clone());
                let Json(detail) = tasks::get_task(st(), me(), Path(task_id.clone())).await?;
                let t = &detail.task;
                if args.unlink.unwrap_or(false) {
                    notes::unlink_task_note(st(), me(), Path((task_id, note_id))).await?;
                    Ok(format!("Unlinked the note from T-{} {}", t.number, t.title))
                } else {
                    let Json(n) = notes::link_task_note(st(), me(), Path((task_id, note_id))).await?;
                    Ok(format!("Linked “{}” to T-{} {}", n.title, t.number, t.title))
                }
            })
            .await,
        )
    }

    #[tool(
        description = "Save a new markdown note: in the Inbox, under a project, or in a folder. Needs a read-write token."
    )]
    async fn add_note(
        &self,
        Parameters(args): Parameters<AddNoteArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let task_id = match &args.task {
                    Some(t) => {
                        Some(find_task(&mut *self.conn().await?, &user, t, args.project.as_deref()).await?)
                    }
                    None => None,
                };
                let project_ids = match (&args.project, &task_id) {
                    (Some(p), _) => vec![find_project(&mut *self.conn().await?, &user, p).await?.id],
                    (None, Some(t)) => {
                        let Json(d) = tasks::get_task(
                            State(self.state.clone()),
                            CurrentUser(user.clone()),
                            Path(t.clone()),
                        )
                        .await?;
                        vec![d.task.project_id]
                    }
                    (None, None) => vec![],
                };
                let folder = match (&args.folder, project_ids.is_empty()) {
                    (Some(f), _) => Some(f.clone()),
                    (None, true) => Some("inbox".to_string()),
                    (None, false) => None,
                };
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "body": args.body,
                    "folder": folder,
                    "project_ids": project_ids,
                    "tags": args.tags.clone().unwrap_or_default(),
                    "task_id": task_id,
                }))
                .map_err(|e| bad("invalid_note", e.to_string()))?;
                let (_, Json(note)) =
                    notes::create_note(State(self.state.clone()), CurrentUser(user.clone()), Json(req))
                        .await?;
                let linked = match note.tasks.first() {
                    Some(t) if task_id.is_some() => format!(" · linked to T-{} {}", t.number, t.title),
                    _ => String::new(),
                };
                Ok(format!(
                    "Saved “{}” at `{}` · id `{}`{linked}",
                    note.note.title, note.note.path, note.note.id
                ))
            })
            .await,
        )
    }

    #[tool(description = "Add markdown to the end of an existing note. Needs a read-write token.")]
    async fn append_to_note(
        &self,
        Parameters(args): Parameters<AppendArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let id = find_note(&mut *self.conn().await?, &user, &args.note).await?;
                let st = || State(self.state.clone());
                let me = || CurrentUser(user.clone());
                let Json(current) = notes::get_note(st(), me(), Path(id.clone())).await?;
                let mut body = current.body.trim_end().to_string();
                if !body.is_empty() {
                    body.push_str("\n\n");
                }
                body.push_str(args.text.trim());
                body.push('\n');
                let req = serde_json::from_value(json!({ "body": body }))
                    .map_err(|e| bad("invalid_note", e.to_string()))?;
                let Json(note) = notes::update_note(st(), me(), Path(id), Json(req)).await?;
                Ok(format!("Added to “{}” · id `{}`", note.note.title, note.note.id))
            })
            .await,
        )
    }

    #[tool(
        description = "Read a project database (a table such as a parts list or episode tracker) as a markdown table, optionally through one of its saved views or with a search. Lists the columns and their choices, and totals number columns."
    )]
    async fn query_database(
        &self,
        Parameters(args): Parameters<QueryDatabaseArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let mut conn = self.conn().await?;
                data::query(
                    &mut conn,
                    &user,
                    &args.database,
                    args.project.as_deref(),
                    args.view.as_deref(),
                    args.search.as_deref(),
                    args.limit,
                )
                .await
            }
            .await,
        )
    }

    #[tool(
        description = "Add a row to a project database, with values by column name. Needs a read-write token."
    )]
    async fn add_row(
        &self,
        Parameters(args): Parameters<AddRowArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            async {
                let mut tx = self.state.db.begin().await?;
                let out =
                    data::add_row(&mut tx, &user, &args.database, args.project.as_deref(), &args.values)
                        .await?;
                tx.commit().await?;
                Ok(out)
            }
            .await,
        )
    }

    #[tool(
        description = "Start a new project spawned from an existing one, so it appears linked to its parent on the canvas. Needs a read-write token."
    )]
    async fn spawn_project(
        &self,
        Parameters(args): Parameters<SpawnArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let parent = find_project(&mut *self.conn().await?, &user, &args.parent).await?;
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "summary": args.summary,
                    "description": args.description,
                    "spawned_from": parent.id,
                }))
                .map_err(|e| bad("invalid_project", e.to_string()))?;
                let (_, Json(p)) =
                    projects::create_project(State(self.state.clone()), CurrentUser(user.clone()), Json(req))
                        .await?;
                Ok(format!("Started “{}” (id `{}`), spawned from {}.", p.title, p.id, parent.title))
            })
            .await,
        )
    }
}

const PROJECT_URI: &str = "tmp://projects/";
const NOTE_URI: &str = "tmp://notes/";

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build(),
        )
        .with_server_info(
            Implementation::new("toomanyprojects", env!("CARGO_PKG_VERSION")).with_title("TooManyProjects"),
        )
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let user = caller(&ctx)?.user;
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT p.id, p.title, p.summary FROM projects p JOIN project_access a ON a.project_id = p.id
             WHERE a.user_id = ? AND p.status <> 'archived' ORDER BY p.title",
        )
        .bind(&user.id)
        .fetch_all(&self.state.db)
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(ListResourcesResult::with_all_items(
            rows.into_iter()
                .map(|(id, title, summary)| {
                    let r = Resource::new(format!("{PROJECT_URI}{id}"), title.clone())
                        .with_title(format!("Project: {title}"));
                    match summary.filter(|s| !s.is_empty()) {
                        Some(s) => r.with_description(s),
                        None => r,
                    }
                })
                .collect(),
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(format!("{PROJECT_URI}{{id}}"), "project")
                .with_title("Project briefing")
                .with_description("The same briefing as get_project_context."),
            ResourceTemplate::new(format!("{NOTE_URI}{{id}}"), "note")
                .with_title("Note")
                .with_description("A note in full, as get_note returns it."),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let user = caller(&ctx)?.user;
        let uri = request.uri.clone();
        let result = if let Some(id) = uri.strip_prefix(PROJECT_URI) {
            self.project_context(&user, id, false).await
        } else if let Some(id) = uri.strip_prefix(NOTE_URI) {
            self.note_text(&user, id).await
        } else {
            Err(AppError::NotFound)
        };
        match result {
            Ok(text) => Ok(ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                uri,
                mime_type: Some("text/markdown".into()),
                text,
                meta: None,
            }])
            .into()),
            Err(_) => Err(McpError::resource_not_found(format!("No resource at {uri}"), None)),
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(vec![
            Prompt::new(
                "project_briefing",
                Some("Catch me up on a project: where it stands and what to do next."),
                Some(vec![
                    PromptArgument::new("project")
                        .with_description("Project id or exact title")
                        .with_required(true),
                ]),
            ),
            Prompt::new(
                "weekly_review",
                Some("Review the week across every project: what got done, what's late, what's stuck."),
                None::<Vec<PromptArgument>>,
            ),
        ]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let user = caller(&ctx)?.user;
        let fail = |e: AppError| McpError::invalid_params(e.to_string(), None);
        let (description, context, ask) = match request.name.as_str() {
            "project_briefing" => {
                let project = request
                    .arguments
                    .as_ref()
                    .and_then(|a| a.get("project"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("Name the project.", None))?;
                let context = self.project_context(&user, project, true).await.map_err(fail)?;
                (
                    "Catch up on a project",
                    context,
                    "Using the briefing above, catch me up on this project in a few short paragraphs: where it \
                     stands, what moved recently, anything overdue or blocked, and the two or three things I \
                     should do next. Point out tasks that look stale or duplicated.",
                )
            }
            "weekly_review" => (
                "Weekly review",
                self.weekly_review(&user).await.map_err(fail)?,
                "Using the summary above, give me a weekly review: what got done, which projects moved and \
                 which stalled, what's overdue or blocked, and a short, realistic focus list for next week.",
            ),
            other => return Err(McpError::invalid_params(format!("No prompt called {other}"), None)),
        };
        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, context),
            PromptMessage::new_text(Role::User, ask),
        ])
        .with_description(description)
        .into())
    }
}

impl std::fmt::Debug for Mcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Mcp")
    }
}
