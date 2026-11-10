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
    board, calendar, canvas, comments, dashboard,
    error::{AppError, AppResult},
    notes, oauth,
    projects::{self, Project, as_ai, visible_project},
    tasks::{self, Task},
    templates,
    tokens::{self, TokenCaller},
};

/// `/mcp`, behind bearer-token sign-in (OAuth or an API token).
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

/// A 401 that tells an MCP client where to sign in (RFC 9728), and with
/// `invalid_token` when it sent one that no longer works, so it refreshes.
fn unauthorized(base: &str, sent_token: bool, message: &str) -> Response {
    let mut res = (StatusCode::UNAUTHORIZED, Json(json!({ "code": "unauthorized", "message": message })))
        .into_response();
    let error = if sent_token { ", error=\"invalid_token\"" } else { "" };
    let value = format!(
        "Bearer realm=\"TooManyProjects\", resource_metadata=\"{}\"{error}",
        oauth::resource_metadata_url(base)
    );
    if let Ok(v) = HeaderValue::from_str(&value) {
        res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
    res
}

/// Finds the person behind `Authorization: Bearer …` (a connection made by
/// signing in, or an API token) and passes them on to the tools through the
/// request's extensions.
async fn require_token(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let base = oauth::base_url(&state, req.headers());
    let secret = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim().to_string());
    let Some(secret) = secret else {
        return unauthorized(
            &base,
            false,
            "Connect by signing in (add this address as a connector in Claude), or send an API token from Settings → AI access as `Authorization: Bearer <token>`.",
        );
    };
    let found = if secret.starts_with(oauth::ACCESS_PREFIX) {
        oauth::caller_for(&state, &secret).await
    } else {
        tokens::caller_for(&state, &secret).await
    };
    match found {
        Ok(Some(caller)) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Ok(None) => unauthorized(&base, true, "That token is unknown, expired or has been revoked."),
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
    /// Only projects whose cards sit inside this canvas group (its label, any case).
    pub group: Option<String>,
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
    /// Words to look for in projects, tasks, comments, notes and database rows.
    pub query: String,
    /// Most results per kind. Default 10, at most 50.
    pub limit: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TodayArgs {
    /// IANA time zone that says when today starts, such as Europe/London. Default: the zone the user last planned in.
    pub tz: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct MarkPartsArgs {
    /// Row ids from get_shopping_list (or query_database on a Parts database).
    pub rows: Vec<String>,
    /// need, ordered, shipped, arrived or fitted.
    pub stage: String,
    /// The day it happened, YYYY-MM-DD. Default: today.
    pub date: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct QuickAddArgs {
    /// The line to capture. Becomes the task or note title.
    pub text: String,
    /// Project id or exact title. Leave out to put it in the Inbox.
    pub project: Option<String>,
    /// "task" (default with a project), "note", or "inbox" (default without one).
    #[serde(rename = "as")]
    #[schemars(rename = "as")]
    pub r#as: Option<String>,
    /// A web link to keep with it.
    pub link: Option<String>,
    /// More text: the task's description or the note's body.
    pub details: Option<String>,
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
    /// true logs a task that is already finished: it goes in the first column that counts as done.
    pub done: Option<bool>,
    /// YYYY-MM-DD the finished task was done, for work logged after the fact. Default: now. Implies `done`.
    pub completed_on: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AddColumnArgs {
    /// Project id, or its exact title.
    pub project: String,
    /// Up to 40 characters.
    pub name: String,
    /// What its tasks count as on the global board: todo, in_progress, blocked or done.
    /// Default: the same as the column it goes after.
    pub counts_as: Option<String>,
    /// Put it after this column (name or id), or "start" for first. Default: at the end.
    pub after: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateColumnArgs {
    /// Project id, or its exact title.
    pub project: String,
    /// The column's current name, or its id.
    pub column: String,
    /// A new name.
    pub name: Option<String>,
    /// todo, in_progress, blocked or done. Moving a column into done marks its tasks finished today.
    pub counts_as: Option<String>,
    /// Move it after this column (name or id), or "start" for first.
    pub after: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateProjectArgs {
    /// Project id, or its exact title.
    pub project: String,
    pub title: Option<String>,
    /// One line shown on the canvas card. An empty string clears it.
    pub summary: Option<String>,
    /// Markdown. Replaces the description; an empty string clears it.
    pub description: Option<String>,
    /// idea, active, paused, done or archived.
    pub status: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateProjectArgs {
    pub title: String,
    /// One line.
    pub summary: Option<String>,
    /// Markdown.
    pub description: Option<String>,
    /// idea, active, paused or done. Default active.
    pub status: Option<String>,
    /// Start from a template, by name or id: Blank, Electronics / PCB,
    /// 3D print / prop, Car / restoration, Video, Software, or one the user saved.
    /// It brings board columns, labels, starter tasks, databases and notes.
    pub template: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateNoteArgs {
    /// Note id, or its exact title.
    pub note: String,
    /// Renames the note (and fixes links to it in other notes).
    pub title: Option<String>,
    /// Markdown. Replaces the whole body.
    pub body: Option<String>,
    /// Replace just the section under this heading (its text, without #s), up to the next heading of the same
    /// or a higher level. Needs `section_body`.
    pub section: Option<String>,
    /// The section's new markdown, without its heading line.
    pub section_body: Option<String>,
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
pub struct AttachmentsArgs {
    /// A project's photos and files: id or title. Give this or `task`.
    pub project: Option<String>,
    /// One task's photos and files: id, or `T-12` together with `project`.
    pub task: Option<String>,
    /// With a project: projects spawned from it too.
    #[serde(default)]
    pub include_children: bool,
    /// Photos only.
    #[serde(default)]
    pub images_only: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct AttachmentArgs {
    /// Attachment id, from list_attachments or get_task.
    pub id: String,
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
    /// Start from a template, by name or id: Blank, Electronics / PCB,
    /// 3D print / prop, Car / restoration, Video, Software, or one the user saved.
    /// It brings board columns, labels, starter tasks, databases and notes.
    pub template: Option<String>,
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

#[derive(Deserialize, JsonSchema)]
pub struct CalendarArgs {
    /// First day, YYYY-MM-DD. Default: today.
    pub from: Option<String>,
    /// How many days. Default 7, at most 31.
    pub days: Option<i64>,
    /// Only this project's sessions (id or exact title).
    pub project: Option<String>,
    /// Only what the user planned or is joining. Default false.
    pub mine: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct PlanTimeArgs {
    /// `session` (time on a project, the default) or `busy` (time the user can't give to projects).
    pub kind: Option<String>,
    /// For a session: project id or exact title.
    pub project: Option<String>,
    /// What the time is for, such as "Rear brakes" or "Work".
    pub title: Option<String>,
    /// Local start, YYYY-MM-DDTHH:MM, such as 2026-11-14T13:00.
    pub start: String,
    /// Length in minutes (15 to 1440). Give this or `end`.
    pub minutes: Option<i64>,
    /// Local end time HH:MM on the same day (or past midnight). Give this or `minutes`.
    pub end: Option<String>,
    /// Repeat weekly on these days: "weekdays", "weekends", "daily" or names like "Mon, Thu". Default: once.
    pub repeat: Option<String>,
    /// Last day a repeat may start on, YYYY-MM-DD.
    pub until: Option<String>,
    /// IANA time zone. Default: the zone the user last planned in, else UTC.
    pub tz: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers

fn text_reply(s: String) -> CallToolResult {
    text(s)
}

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
            "This connection is read-only. To change things, reconnect with Read and write, or use a read-write token (Settings → AI access).",
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

/// One column by its exact name (any case) or id.
fn find_column<'a>(statuses: &'a [board::Status], wanted: &str) -> AppResult<&'a board::Status> {
    let w = wanted.trim();
    statuses.iter().find(|s| s.id == w || s.name.eq_ignore_ascii_case(w)).ok_or_else(|| {
        let names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
        bad("unknown_column", format!("No column called “{w}”. The columns are: {}.", names.join(", ")))
    })
}

/// `after` for the column API: `None` leaves it, `Some(None)` is first, `Some(Some(id))` after that column.
fn column_after(
    statuses: &[board::Status],
    after: Option<&str>,
    moving: Option<&str>,
) -> AppResult<Option<Option<String>>> {
    match after.map(str::trim) {
        None | Some("") => Ok(None),
        Some(a) if a.eq_ignore_ascii_case("start") || a.eq_ignore_ascii_case("first") => Ok(Some(None)),
        Some(a) => {
            let col = find_column(statuses, a)?;
            if Some(col.id.as_str()) == moving {
                return Err(bad("invalid_after", "A column can't go after itself."));
            }
            Ok(Some(Some(col.id.clone())))
        }
    }
}

/// A category from what a model might say.
fn category_of(c: &str) -> AppResult<&'static str> {
    Ok(match c.trim().to_lowercase().replace([' ', '-'], "_").as_str() {
        "todo" | "to_do" => "todo",
        "in_progress" | "doing" => "in_progress",
        "blocked" => "blocked",
        "done" | "finished" => "done",
        other => {
            return Err(bad(
                "invalid_category",
                format!("“{other}” isn't a category: use todo, in_progress, blocked or done."),
            ));
        }
    })
}

/// Replace the body of the section under `heading` (up to the next heading
/// of the same or a higher level). `None` when there is no such heading.
fn replace_section(body: &str, heading: &str, text: &str) -> Option<String> {
    let want = heading.trim().trim_start_matches('#').trim().to_lowercase();
    let lines: Vec<&str> = body.lines().collect();
    let level = |l: &str| {
        let hashes = l.chars().take_while(|c| *c == '#').count();
        (hashes > 0 && hashes <= 6 && l[hashes..].starts_with(' ')).then_some(hashes)
    };
    let mut in_code = false;
    let mut start = None;
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate() {
        if l.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let Some(n) = level(l) else { continue };
        match start {
            None if l[n..].trim().to_lowercase() == want => start = Some((i, n)),
            Some((_, m)) if n <= m => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    let (at, _) = start?;
    let mut out: Vec<String> = lines[..=at].iter().map(|l| l.to_string()).collect();
    out.push(String::new());
    let text = text.trim();
    if !text.is_empty() {
        out.push(text.to_string());
        out.push(String::new());
    }
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    let mut joined = out.join("\n").trim_end().to_string();
    joined.push('\n');
    Some(joined)
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
spawned another), each with a kanban board of tasks, plus a vault of markdown notes. Start with get_today \
(what is in progress, blocked, due, quiet or waiting in the Inbox), list_projects or search, then \
get_project_context for a full briefing on one project. Capture a quick idea or link with quick_add. Notes link with [[Note title]] and \
[[@Project title]]. Projects can hold databases (tables with typed columns): read one with query_database. Parts databases track \
parts to buy (Need, Ordered, Shipped, Arrived, Fitted); get_shopping_list lists them all by supplier and mark_parts moves them along. Tasks keep a log of \
comments and can have notes, photos and files attached: read them with get_task, list a project's photos with \
list_attachments and look at one with get_attachment, and log progress or lessons with \
add_task_comment. A calendar holds work sessions on projects (which people a project is shared with can \
join) and the user's private busy time: read it with get_calendar, add to it with plan_time. Write tools \
(create_task, quick_add, update_task, add_task_comment, link_note_to_task, add_note, append_to_note, update_note, \
create_project, spawn_project, update_project, add_column, update_column, add_row, mark_parts, plan_time) need read-write access, and everything they change shows in the project's history as done via AI. Prefer adding to \
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

    /// The board's columns after a change, with any warning about Done.
    async fn columns_reply(&self, user: &User, project: &Project, done: &str) -> AppResult<String> {
        let _ = user;
        let statuses = board::statuses_for(&mut *self.conn().await?, &project.id).await?;
        let list: Vec<String> =
            statuses.iter().map(|s| format!("{} ({})", s.name, brief::category_label(&s.category))).collect();
        let mut out = format!("{done} {} columns, in order: {}", project.title, list.join(", "));
        if let Some(w) = board::done_warning(&statuses) {
            out.push_str(&format!("\nNote: {w}"));
        }
        Ok(out)
    }

    /// "Its board has Design, Ordered and Done columns, 2 starter tasks and a Parts database."
    async fn board_line(&self, project_id: &str) -> AppResult<String> {
        let mut conn = self.conn().await?;
        let columns: Vec<String> =
            crate::board::statuses_for(&mut conn, project_id).await?.into_iter().map(|s| s.name).collect();
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&mut *conn)
            .await?;
        let dbs: Vec<String> =
            sqlx::query_scalar("SELECT name FROM databases WHERE project_id = ? ORDER BY position")
                .bind(project_id)
                .fetch_all(&mut *conn)
                .await?;
        let mut line = format!("Its board has these columns: {}.", columns.join(", "));
        if tasks > 0 {
            line.push_str(&format!(" {tasks} starter task{}.", if tasks == 1 { "" } else { "s" }));
        }
        if !dbs.is_empty() {
            line.push_str(&format!(" Databases: {}.", dbs.join(", ")));
        }
        Ok(line)
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
        let now = chrono::Utc::now();
        let coming = calendar::occurrences_between(
            &mut conn,
            &user.id,
            now,
            now + chrono::Duration::days(14),
            Some(&id),
            false,
        )
        .await?;
        if !coming.is_empty() {
            out.push_str("\n## Coming up (work sessions, next 14 days)\n\n");
            for o in coming.iter().take(12) {
                out.push_str(&calendar::line(o));
                out.push('\n');
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
        let now = chrono::Utc::now();
        let mut conn = self.conn().await?;
        let past = calendar::occurrences_between(
            &mut conn,
            &user.id,
            now - chrono::Duration::days(7),
            now,
            None,
            true,
        )
        .await?;
        let next = calendar::occurrences_between(
            &mut conn,
            &user.id,
            now,
            now + chrono::Duration::days(7),
            None,
            true,
        )
        .await?;
        out.push('\n');
        out.push_str(
            &brief::calendar(&past, "the last 7 days")
                .replacen("# ", "## ", 1)
                .replace("\n## Planned", "\n### Planned"),
        );
        out.push('\n');
        out.push_str(
            &brief::calendar(&next, "the next 7 days")
                .replacen("# ", "## ", 1)
                .replace("\n## Planned", "\n### Planned"),
        );
        Ok(out)
    }
}

#[tool_router]
impl Mcp {
    #[tool(
        description = "List the user's projects with status, open, in-progress, overdue and done task counts, and ids, then the canvas groups they are arranged in (labelled areas such as Electronics or Cars). Filter by status, by a family (a project and everything spawned from it) or by a group."
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
                let groups = {
                    let mut conn = self.conn().await?;
                    canvas::groups_with_members(&mut conn, &user.id).await?
                };
                if let Some(g) = &args.group {
                    let Some(m) = groups
                        .iter()
                        .find(|m| m.group.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(g.trim())))
                    else {
                        let names: Vec<&str> =
                            groups.iter().filter_map(|m| m.group.label.as_deref()).collect();
                        return Ok(if names.is_empty() {
                            "There are no groups on the canvas.".to_string()
                        } else {
                            format!("No group is called \"{g}\". Groups: {}.", names.join(", "))
                        });
                    };
                    list.retain(|p| m.project_ids.contains(&p.id));
                }
                if list.is_empty() {
                    return Ok("No projects match.".to_string());
                }
                let mut out = format!("{} projects:\n\n", list.len());
                for p in &list {
                    out.push_str(&brief::project_line(p));
                    out.push('\n');
                }
                let titles: std::collections::HashMap<&str, &str> =
                    list.iter().map(|p| (p.id.as_str(), p.title.as_str())).collect();
                let lines: Vec<String> = groups
                    .iter()
                    .filter_map(|m| {
                        let inside: Vec<&str> =
                            m.project_ids.iter().filter_map(|id| titles.get(id.as_str()).copied()).collect();
                        (!inside.is_empty()).then(|| {
                            format!(
                                "- **{}**: {}",
                                m.group.label.as_deref().unwrap_or("Group"),
                                inside.join(", ")
                            )
                        })
                    })
                    .collect();
                if !lines.is_empty() {
                    out.push_str("\nCanvas groups:\n\n");
                    out.push_str(&lines.join("\n"));
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
        description = "Search everything the user can see for some words: projects (title, summary, description), tasks (title, description), comments in task logs, notes (full text) and database rows. Words match the start of words, so `brak` finds `brakes`."
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
                let hits = crate::search::run(&self.state, &user, q, args.limit.unwrap_or(10)).await?;
                Ok(brief::search(q, &hits))
            }
            .await,
        )
    }

    #[tool(
        description = "The user's Today view as markdown: tasks in progress and blocked (theirs, or nobody's on their own projects), tasks late or due in the next 7 days, today's work sessions, active projects with no activity for 14 days, and unfiled Inbox captures. A good first call for \"what should I work on?\"."
    )]
    async fn get_today(
        &self,
        Parameters(args): Parameters<TodayArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let today = crate::today::build(&self.state, &user, args.tz.as_deref()).await?;
                Ok(brief::today(&today))
            }
            .await,
        )
    }

    #[tool(
        description = "The Shopping list as markdown: every part in Need across the user's projects (rows of Parts databases), grouped by supplier with totals, then parts ordered or shipped, then parts that arrived this month. Each part shows its row id for mark_parts. Add parts with add_row on a project's Parts database."
    )]
    async fn get_shopping_list(
        &self,
        Parameters(args): Parameters<TodayArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let list =
                    crate::parts::build_shopping_list(&mut *self.conn().await?, &user.id, args.tz.as_deref())
                        .await?;
                Ok(brief::shopping(&list))
            }
            .await,
        )
    }

    #[tool(
        description = "Move parts along: need, ordered, shipped, arrived or fitted, filling in Ordered on and Arrived on. When the last part a task waits on arrives, the task's log gets a \"Parts arrived\" comment. Needs read-write access."
    )]
    async fn mark_parts(
        &self,
        Parameters(args): Parameters<MarkPartsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let mut tx = self.state.db.begin().await?;
                let date = match &args.date {
                    Some(d) => d.trim().to_string(),
                    None => crate::parts::today_for(&mut tx, &user.id).await?,
                };
                if chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
                    return Err(bad("invalid_date", "A date is YYYY-MM-DD."));
                }
                let stage = args.stage.trim().to_lowercase();
                let out = crate::parts::mark(&mut tx, &user.id, &args.rows, &stage, &date).await?;
                tx.commit().await?;
                let mut text = format!(
                    "Moved {} part{} to {stage} ({date}).",
                    out.moved,
                    if out.moved == 1 { "" } else { "s" }
                );
                for t in &out.tasks_ready {
                    text.push_str(&format!(
                        "\nAll parts are in for T-{} {}; its log says so.",
                        t.number, t.title
                    ));
                }
                Ok(text)
            })
            .await,
        )
    }

    #[tool(
        description = "Capture one line, like the app's Quick add: as a task in a project's first To do column, as a note filed under a project, or into the Inbox (no project) to file later. A link is kept with it. Needs read-write access."
    )]
    async fn quick_add(
        &self,
        Parameters(args): Parameters<QuickAddArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let project = match args.project.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                    Some(p) => Some(find_project(&mut *self.conn().await?, &user, p).await?),
                    None => None,
                };
                let kind = match (args.r#as.as_deref().map(str::trim), &project) {
                    (Some(k), _) if !k.is_empty() => k.to_ascii_lowercase(),
                    (_, Some(_)) => "task".to_string(),
                    (_, None) => "inbox".to_string(),
                };
                if kind == "inbox" && project.is_some() {
                    return Err(bad(
                        "invalid_kind",
                        "The Inbox has no project. Leave `project` out, or add it as a task or note.",
                    ));
                }
                let added = crate::capture::add(
                    &self.state,
                    &user,
                    crate::capture::QuickAddRequest {
                        text: args.text.clone(),
                        kind,
                        project_id: project.as_ref().map(|p| p.id.clone()),
                        url: args.link.clone(),
                        body: args.details.clone(),
                    },
                )
                .await?;
                Ok(match (added.kind.as_str(), &added.task, &project) {
                    ("task", Some(t), Some(p)) => format!(
                        "Added to {} ({}):\n{}",
                        p.title,
                        added.column.as_deref().unwrap_or("first column"),
                        brief::task_line(t)
                    ),
                    ("note", _, Some(p)) => format!("Added note \"{}\" under {}.", added.title, p.title),
                    _ => format!("Added \"{}\" to the Inbox.", added.title),
                })
            })
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

    #[tool(description = "Add a task to a project's board. Needs read-write access.")]
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
                let completed_on = match args.completed_on.as_deref().map(str::trim) {
                    None | Some("") => None,
                    Some(d) => Some(
                        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                            .map_err(|_| bad("invalid_date", "completed_on is a date like 2026-11-09."))?,
                    ),
                };
                let done = args.done.unwrap_or(false) || completed_on.is_some();
                let status_id = match (&args.column, done) {
                    (None, false) => None,
                    (None, true) => Some(
                        statuses.iter().find(|s| s.category == "done").map(|s| s.id.clone()).ok_or_else(|| {
                            bad(
                                "no_done_column",
                                "This board has no column that counts as done. Add one with add_column, or set one's counts_as to done with update_column.",
                            )
                        })?,
                    ),
                    (Some(c), _) => match pick_column(&statuses, c)? {
                        ColumnChoice::Status(id) => Some(id),
                        ColumnChoice::Category(cat) => {
                            statuses.iter().find(|s| s.category == cat).map(|s| s.id.clone())
                        }
                    },
                };
                if done {
                    let column = statuses.iter().find(|s| Some(&s.id) == status_id.as_ref());
                    if column.is_some_and(|s| s.category != "done") {
                        return Err(bad("not_a_done_column", "That column doesn't count as done, so the task can't be logged as finished there."));
                    }
                }
                drop(conn);
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "status_id": status_id,
                    "description": args.description,
                    "priority": args.priority,
                    "due_date": args.due_date,
                }))
                .map_err(|e| bad("invalid_task", e.to_string()))?;
                let (_, Json(mut task)) = tasks::create_task(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(project.id.clone()),
                    Json(req),
                )
                .await?;
                if let Some(day) = completed_on {
                    let at = crate::time::format(day.and_hms_opt(12, 0, 0).expect("noon exists").and_utc());
                    sqlx::query("UPDATE tasks SET completed_at = ? WHERE id = ?")
                        .bind(&at)
                        .bind(&task.id)
                        .execute(&self.state.db)
                        .await?;
                    task.completed_at = Some(at);
                }
                Ok(format!("Added to {}:\n{}", project.title, brief::task_line(&task)))
            })
            .await,
        )
    }

    #[tool(
        description = "Change a task: rename it, move it to another column, mark it done, or set its description, priority or due date. Needs read-write access."
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
        description = "List the photos and files on a project (its tasks, comments, database rows and linked notes included) or on one task, newest first, with ids, captions, sizes and where each is attached. Look at one with get_attachment."
    )]
    async fn list_attachments(
        &self,
        Parameters(args): Parameters<AttachmentsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let mut conn = self.conn().await?;
                let mut q = crate::attachments::ListQuery {
                    include_children: args.include_children,
                    images_only: args.images_only,
                    ..Default::default()
                };
                if let Some(task) = &args.task {
                    q.task_id = Some(find_task(&mut conn, &user, task, args.project.as_deref()).await?);
                } else if let Some(project) = &args.project {
                    q.project_id = Some(find_project(&mut conn, &user, project).await?.id);
                } else {
                    return Err(AppError::BadRequest("invalid_query", "Name a project or a task.".into()));
                }
                let list = crate::attachments::list(&mut conn, &user.id, &q).await?;
                Ok(brief::attachments(&list))
            }
            .await,
        )
    }

    #[tool(
        description = "Look at one photo or file. A photo comes back as an image (up to 1600 px) so you can see it; a small text file (CSV, markdown, code) comes back as text; anything else is described."
    )]
    async fn get_attachment(
        &self,
        Parameters(args): Parameters<AttachmentArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        let a = {
            let mut conn = self.conn().await.map_err(|e| McpError::internal_error(e.to_string(), None))?;
            match crate::attachments::visible_attachment(&mut conn, &user.id, &args.id).await {
                Ok(a) => a,
                Err(e) => return Ok(failed(e)),
            }
        };
        let about = brief::attachments(std::slice::from_ref(&a));
        let data_dir = &self.state.config.data_dir;
        if a.is_image
            && let Some(bytes) = crate::attachments::preview_bytes(data_dir, &a.id).await
        {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            return Ok(CallToolResult::success(vec![
                ContentBlock::text(about),
                ContentBlock::image(data, "image/jpeg"),
            ]));
        }
        if a.size <= 200 * 1024
            && let Ok(bytes) = crate::attachments::original_bytes(&self.state, &a.id).await
            && let Ok(text) = String::from_utf8(bytes)
        {
            return Ok(text_reply(format!("{about}\n```\n{text}\n```")));
        }
        Ok(text_reply(format!("{about}\nThis file can't be shown here; the person can open it in the app.")))
    }

    #[tool(
        description = "Log a comment on a task: progress, what was tried, a measurement, a decision. Comments build the task's work log. Needs read-write access."
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
        description = "Link an existing note to a task (or unlink it), so the note shows on the task and the task on the note. To write a new note for a task, use add_note with `task`. Needs read-write access."
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
        description = "Save a new markdown note: in the Inbox, under a project, or in a folder. Needs read-write access."
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

    #[tool(description = "Add markdown to the end of an existing note. Needs read-write access.")]
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
        description = "Add a row to a project database, with values by column name. Needs read-write access."
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
        description = "Add a column to a project's board, at the end or after a given column. Needs read-write access."
    )]
    async fn add_column(
        &self,
        Parameters(args): Parameters<AddColumnArgs>,
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
                drop(conn);
                let after = column_after(&statuses, args.after.as_deref(), None)?;
                let left = match &after {
                    Some(Some(id)) => statuses.iter().find(|s| &s.id == id),
                    Some(None) => None,
                    None => statuses.last(),
                };
                let category = match args.counts_as.as_deref() {
                    Some(c) => category_of(c)?.to_string(),
                    None => left.map(|s| s.category.clone()).unwrap_or_else(|| "todo".into()),
                };
                let mut body = json!({ "name": args.name, "category": category });
                if let Some(a) = after {
                    body["after_id"] = json!(a);
                }
                let req = serde_json::from_value(body).map_err(|e| bad("invalid_column", e.to_string()))?;
                let (_, Json(status)) = board::create_status(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(project.id.clone()),
                    Json(req),
                )
                .await?;
                self.columns_reply(&user, &project, &format!("Added the {} column.", status.name)).await
            })
            .await,
        )
    }

    #[tool(
        description = "Change a board column: rename it, change what it counts as (todo, in_progress, blocked, done), or move it after another column. Needs read-write access."
    )]
    async fn update_column(
        &self,
        Parameters(args): Parameters<UpdateColumnArgs>,
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
                drop(conn);
                let column = find_column(&statuses, &args.column)?;
                let mut body = serde_json::Map::new();
                if let Some(n) = &args.name {
                    body.insert("name".into(), json!(n));
                }
                if let Some(c) = &args.counts_as {
                    body.insert("category".into(), json!(category_of(c)?));
                }
                if let Some(a) = column_after(&statuses, args.after.as_deref(), Some(&column.id))? {
                    body.insert("after_id".into(), json!(a));
                }
                if body.is_empty() {
                    return Err(bad("nothing_to_change", "Say what to change: name, counts_as or after."));
                }
                let req = serde_json::from_value(serde_json::Value::Object(body))
                    .map_err(|e| bad("invalid_column", e.to_string()))?;
                let _ = board::update_status(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(column.id.clone()),
                    Json(req),
                )
                .await?;
                self.columns_reply(&user, &project, &format!("Updated the {} column.", column.name)).await
            })
            .await,
        )
    }

    #[tool(
        description = "Change a project's title, one-line summary, description or status (idea, active, paused, done, archived). Needs read-write access."
    )]
    async fn update_project(
        &self,
        Parameters(args): Parameters<UpdateProjectArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let project = find_project(&mut *self.conn().await?, &user, &args.project).await?;
                let blank_is_null = |v: &str| if v.trim().is_empty() { json!(null) } else { json!(v) };
                let mut body = serde_json::Map::new();
                if let Some(t) = &args.title {
                    body.insert("title".into(), json!(t));
                }
                if let Some(s) = &args.summary {
                    body.insert("summary".into(), blank_is_null(s));
                }
                if let Some(d) = &args.description {
                    body.insert("description".into(), blank_is_null(d));
                }
                if let Some(s) = &args.status {
                    body.insert("status".into(), json!(s.trim().to_lowercase()));
                }
                if body.is_empty() {
                    return Err(bad(
                        "nothing_to_change",
                        "Say what to change: title, summary, description or status.",
                    ));
                }
                let req = serde_json::from_value(serde_json::Value::Object(body))
                    .map_err(|e| bad("invalid_project", e.to_string()))?;
                let Json(p) = projects::update_project(
                    State(self.state.clone()),
                    CurrentUser(user.clone()),
                    Path(project.id.clone()),
                    Json(req),
                )
                .await?;
                Ok(format!(
                    "Updated “{}” · {} · id `{}`{}",
                    p.title,
                    p.status,
                    p.id,
                    p.summary.as_deref().map(|s| format!("\nSummary: {s}")).unwrap_or_default()
                ))
            })
            .await,
        )
    }

    #[tool(
        description = "Start a new top-level project (use spawn_project for one that grew out of another), optionally from a template such as Electronics / PCB or Video. Needs read-write access."
    )]
    async fn create_project(
        &self,
        Parameters(args): Parameters<CreateProjectArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let template = match &args.template {
                    Some(t) if !t.trim().is_empty() => {
                        Some(templates::find(&mut *self.conn().await?, &user.id, t).await?)
                    }
                    _ => None,
                };
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "summary": args.summary,
                    "description": args.description,
                    "status": args.status.map(|s| s.trim().to_lowercase()),
                    "template_id": template,
                }))
                .map_err(|e| bad("invalid_project", e.to_string()))?;
                let (_, Json(p)) =
                    projects::create_project(State(self.state.clone()), CurrentUser(user.clone()), Json(req))
                        .await?;
                let board = self.board_line(&p.id).await?;
                Ok(format!("Started “{}” (id `{}`). {board}", p.title, p.id))
            })
            .await,
        )
    }

    #[tool(
        description = "Edit a note: rename it, replace its whole body, or replace one section under a heading. Use append_to_note to add to the end. Needs read-write access."
    )]
    async fn update_note(
        &self,
        Parameters(args): Parameters<UpdateNoteArgs>,
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
                let mut body = serde_json::Map::new();
                match (&args.body, &args.section, &args.section_body) {
                    (Some(_), Some(_), _) => {
                        return Err(bad(
                            "pick_one",
                            "Send either body or section with section_body, not both.",
                        ));
                    }
                    (Some(b), None, _) => {
                        body.insert("body".into(), json!(b));
                    }
                    (None, Some(heading), Some(text)) => {
                        let Json(current) = notes::get_note(st(), me(), Path(id.clone())).await?;
                        let next = replace_section(&current.body, heading, text).ok_or_else(|| {
                            bad("unknown_section", format!("The note has no heading “{}”.", heading.trim()))
                        })?;
                        body.insert("body".into(), json!(next));
                    }
                    (None, Some(_), None) => {
                        return Err(bad("need_section_body", "Give section_body: the section's new text."));
                    }
                    (None, None, _) => {}
                }
                if let Some(t) = &args.title {
                    body.insert("title".into(), json!(t));
                }
                if body.is_empty() {
                    return Err(bad(
                        "nothing_to_change",
                        "Say what to change: title, body, or section with section_body.",
                    ));
                }
                let req = serde_json::from_value(serde_json::Value::Object(body))
                    .map_err(|e| bad("invalid_note", e.to_string()))?;
                let Json(note) = notes::update_note(st(), me(), Path(id), Json(req)).await?;
                Ok(format!("Saved “{}” · id `{}`", note.note.title, note.note.id))
            })
            .await,
        )
    }

    #[tool(
        description = "Start a new project spawned from an existing one, so it appears linked to its parent on the canvas. Needs read-write access."
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
                let template = match &args.template {
                    Some(t) if !t.trim().is_empty() => {
                        Some(templates::find(&mut *self.conn().await?, &user.id, t).await?)
                    }
                    _ => None,
                };
                let req = serde_json::from_value(json!({
                    "title": args.title,
                    "summary": args.summary,
                    "description": args.description,
                    "spawned_from": parent.id,
                    "template_id": template,
                }))
                .map_err(|e| bad("invalid_project", e.to_string()))?;
                let (_, Json(p)) =
                    projects::create_project(State(self.state.clone()), CurrentUser(user.clone()), Json(req))
                        .await?;
                let board = self.board_line(&p.id).await?;
                Ok(format!("Started “{}” (id `{}`), spawned from {}. {board}", p.title, p.id, parent.title))
            })
            .await,
        )
    }

    #[tool(
        description = "Read the user's calendar: work sessions planned on projects (with who is joining), and the user's own busy time (work, appointments). Default: the next 7 days. Ends with hours planned per project."
    )]
    async fn get_calendar(
        &self,
        Parameters(args): Parameters<CalendarArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = caller(&ctx)?.user;
        reply(
            async {
                let mut conn = self.conn().await?;
                let tz = calendar::parse_tz(&calendar::usual_tz(&mut conn, &user.id).await?)?;
                let first = match &args.from {
                    Some(d) => calendar::parse_date(d)?,
                    None => chrono::Utc::now().with_timezone(&tz).date_naive(),
                };
                let days = args.days.unwrap_or(7).clamp(1, 31);
                let from = calendar::to_utc(tz, first.and_hms_opt(0, 0, 0).unwrap_or_default());
                let to = calendar::to_utc(
                    tz,
                    (first + chrono::Duration::days(days)).and_hms_opt(0, 0, 0).unwrap_or_default(),
                );
                let project = match &args.project {
                    Some(p) => Some(find_project(&mut conn, &user, p).await?.id),
                    None => None,
                };
                let list = calendar::occurrences_between(
                    &mut conn,
                    &user.id,
                    from,
                    to,
                    project.as_deref(),
                    args.mine.unwrap_or(false),
                )
                .await?;
                Ok(brief::calendar(&list, &format!("{first}, {days} days ({})", tz.name())))
            }
            .await,
        )
    }

    #[tool(
        description = "Put time in the user's calendar: a work session on a project (seen by everyone the project is shared with, who can join it) or busy time (seen only by the user). Can repeat weekly. Needs read-write access."
    )]
    async fn plan_time(
        &self,
        Parameters(args): Parameters<PlanTimeArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user = match writer(&ctx)? {
            Ok(c) => c.user,
            Err(r) => return Ok(r),
        };
        reply(
            as_ai(async {
                let mut conn = self.conn().await?;
                let kind = args.kind.as_deref().unwrap_or("session").to_lowercase();
                let project = match (&kind[..], &args.project) {
                    ("session", Some(p)) => Some(find_project(&mut conn, &user, p).await?),
                    ("session", None) => {
                        return Err(bad("invalid_project", "Name the project for the session."));
                    }
                    _ => None,
                };
                let tz = match &args.tz {
                    Some(t) => t.clone(),
                    None => calendar::usual_tz(&mut conn, &user.id).await?,
                };
                drop(conn);
                let start = calendar::parse_local(&args.start)?;
                let minutes = match (args.minutes, &args.end) {
                    (Some(m), _) => m,
                    (None, Some(end)) => {
                        let end = chrono::NaiveTime::parse_from_str(end, "%H:%M")
                            .map_err(|_| bad("invalid_end", "End times look like 17:00."))?;
                        let mut m = (end - start.time()).num_minutes();
                        if m <= 0 {
                            m += 24 * 60;
                        }
                        m
                    }
                    (None, None) => return Err(bad("invalid_length", "Give `minutes` or an `end` time.")),
                };
                let repeat = match &args.repeat {
                    Some(r) => calendar::parse_days(r)?,
                    None => 0,
                };
                let req = serde_json::from_value(json!({
                    "kind": kind,
                    "project_id": project.as_ref().map(|p| p.id.clone()),
                    "title": args.title,
                    "start_local": args.start,
                    "minutes": minutes,
                    "tz": tz,
                    "repeat_days": repeat,
                    "repeat_until": args.until,
                }))
                .map_err(|e| bad("invalid_event", e.to_string()))?;
                let (_, Json(e)) =
                    calendar::create_event(State(self.state.clone()), CurrentUser(user.clone()), Json(req))
                        .await?;
                let what = match &project {
                    Some(p) => format!("a session on {}", p.title),
                    None => "busy time".into(),
                };
                let repeats = if e.repeat_days != 0 {
                    format!(", repeating {}", calendar::days_text(e.repeat_days))
                } else {
                    String::new()
                };
                Ok(format!(
                    "Planned {what} from {} for {} ({}){repeats} · event id `{}`",
                    e.start_local.replace('T', " "),
                    calendar::hours(e.minutes),
                    e.tz,
                    e.id
                ))
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
                 which stalled, what's overdue or blocked, and a short, realistic focus list for next week that fits the time planned in the calendar, and which projects got no time.",
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
