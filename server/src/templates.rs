//! Project templates (phase 4e-2): what a new project starts with.
//!
//! A template carries board columns, labels, starter tasks, database schemas
//! (no rows) and note skeletons. The built-ins live here in code, so they get
//! better with each release; the ones people save from their own projects
//! live in `project_templates` as JSON. A saved template is private unless its
//! owner shares it, and then only the people they share everything with see it.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, User},
    board,
    databases::{self, ColumnOptions, ViewConfig},
    error::{AppError, AppResult},
    notes, position,
    projects::{self, editable_project, log_activity, visible_project},
    time,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_templates, create_template))
        .routes(routes!(update_template, delete_template))
        .routes(routes!(apply_template))
}

const MAX_TEMPLATES: i64 = 100;
const MAX_TASKS: usize = 200;
const BUILTIN_PREFIX: &str = "builtin-";

// ---------------------------------------------------------------------------
// The template body

/// A board column a template brings.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct TemplateColumn {
    pub name: String,
    /// `todo`, `in_progress`, `blocked` or `done`.
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateLabel {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TemplateTask {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Index into the template's columns; the first column when out of range.
    #[serde(default)]
    pub column: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checklist: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateDbColumn {
    /// The column's id where it was saved from; views refer to columns by it.
    pub key: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub options: ColumnOptions,
    #[serde(default)]
    pub is_title: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateView {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateDatabase {
    pub name: String,
    /// `parts` makes a Parts database (phase 4d) and ignores the columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<TemplateDbColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<TemplateView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateNote {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TemplateBody {
    #[serde(default)]
    pub columns: Vec<TemplateColumn>,
    #[serde(default)]
    pub labels: Vec<TemplateLabel>,
    #[serde(default)]
    pub tasks: Vec<TemplateTask>,
    #[serde(default)]
    pub databases: Vec<TemplateDatabase>,
    #[serde(default)]
    pub notes: Vec<TemplateNote>,
}

// ---------------------------------------------------------------------------
// The built-ins

fn col(name: &str, category: &str) -> TemplateColumn {
    TemplateColumn { name: name.into(), category: category.into(), color: None }
}

fn labels(names: &[(&str, &str)]) -> Vec<TemplateLabel> {
    names.iter().map(|(n, c)| TemplateLabel { name: (*n).into(), color: Some((*c).into()) }).collect()
}

fn parts_db() -> TemplateDatabase {
    TemplateDatabase {
        name: "Parts".into(),
        template: Some(crate::parts::TEMPLATE.into()),
        columns: vec![],
        views: vec![],
    }
}

struct Builtin {
    key: &'static str,
    name: &'static str,
    blurb: &'static str,
    body: fn() -> TemplateBody,
}

const BUILTINS: &[Builtin] = &[
    Builtin { key: "blank", name: "Blank", blurb: "Today's default board", body: blank },
    Builtin {
        key: "pcb",
        name: "Electronics / PCB",
        blurb: "Parts database, labels Schematic, Layout, Firmware",
        body: pcb,
    },
    Builtin {
        key: "prop",
        name: "3D print / prop",
        blurb: "Parts database, finishing checklist",
        body: prop,
    },
    Builtin { key: "car", name: "Car / restoration", blurb: "Parts database, phase labels", body: car },
    Builtin { key: "video", name: "Video", blurb: "Labels Episode, Short, B-roll", body: video },
    Builtin { key: "software", name: "Software", blurb: "Labels Bug, Feature", body: software },
];

/// Today's default board: To do, Doing, Done.
pub fn blank() -> TemplateBody {
    TemplateBody {
        columns: vec![col("To do", "todo"), col("Doing", "in_progress"), col("Done", "done")],
        ..Default::default()
    }
}

fn pcb() -> TemplateBody {
    TemplateBody {
        columns: vec![
            col("Design", "todo"),
            col("Ready to order", "in_progress"),
            col("Ordered", "in_progress"),
            col("Assembly", "in_progress"),
            col("Testing", "in_progress"),
            col("Done", "done"),
        ],
        labels: labels(&[("Schematic", "slot-2"), ("Layout", "slot-3"), ("Firmware", "slot-4")]),
        databases: vec![parts_db()],
        ..Default::default()
    }
}

fn prop() -> TemplateBody {
    let steps = [
        "Print all parts",
        "Clean up supports and glue seams",
        "Fill and sand",
        "Prime",
        "Sand smooth",
        "Mask",
        "Base coat",
        "Detail paint",
        "Weather",
        "Clear coat",
    ];
    TemplateBody {
        columns: vec![
            col("Print", "todo"),
            col("Fill", "in_progress"),
            col("Prime", "in_progress"),
            col("Mask", "in_progress"),
            col("Paint", "in_progress"),
            col("Weather", "in_progress"),
            col("Done", "done"),
        ],
        tasks: vec![TemplateTask {
            title: "Finishing checklist".into(),
            description: Some("Tick each step off as the pieces go through it.".into()),
            checklist: steps.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        }],
        databases: vec![parts_db()],
        ..Default::default()
    }
}

fn car() -> TemplateBody {
    TemplateBody {
        columns: vec![
            col("To do", "todo"),
            col("Doing", "in_progress"),
            col("Waiting on parts", "blocked"),
            col("Done", "done"),
        ],
        labels: labels(&[("Disassembly", "slot-1"), ("Bodywork", "slot-3"), ("Engine", "slot-5")]),
        databases: vec![parts_db()],
        ..Default::default()
    }
}

fn video() -> TemplateBody {
    TemplateBody {
        columns: vec![
            col("Idea", "todo"),
            col("Script", "in_progress"),
            col("Shoot", "in_progress"),
            col("Edit", "in_progress"),
            col("Published", "done"),
        ],
        labels: labels(&[("Episode", "slot-2"), ("Short", "slot-6"), ("B-roll", "slot-7")]),
        ..Default::default()
    }
}

fn software() -> TemplateBody {
    TemplateBody {
        columns: vec![
            col("Backlog", "todo"),
            col("Doing", "in_progress"),
            col("Review", "in_progress"),
            col("Done", "done"),
        ],
        labels: labels(&[("Bug", "slot-1"), ("Feature", "slot-2")]),
        ..Default::default()
    }
}

fn builtin(id: &str) -> Option<&'static Builtin> {
    let key = id.strip_prefix(BUILTIN_PREFIX)?;
    BUILTINS.iter().find(|b| b.key == key)
}

// ---------------------------------------------------------------------------
// API types

/// A template as the picker shows it.
#[derive(Serialize, ToSchema)]
pub struct ProjectTemplate {
    /// `builtin-pcb` and so on for the built-ins.
    pub id: String,
    pub name: String,
    /// One line on what else it brings.
    pub blurb: String,
    pub builtin: bool,
    /// You saved it, so you can rename, share or delete it.
    pub mine: bool,
    /// Shown to the people its owner shares everything with.
    pub shared: bool,
    /// Who saved it, when that isn't you.
    pub owner_name: Option<String>,
    /// The project it was saved from.
    pub source_title: Option<String>,
    pub columns: Vec<TemplateColumn>,
    pub label_names: Vec<String>,
    pub task_count: i64,
    pub database_names: Vec<String>,
    pub note_titles: Vec<String>,
    pub created_at: Option<String>,
}

/// Save a project as a template, or copy another template.
#[derive(Deserialize, ToSchema)]
pub struct CreateTemplateRequest {
    pub name: String,
    /// Save this project's set-up.
    pub project_id: Option<String>,
    /// Or copy this template (a built-in or one you can see).
    pub from_template_id: Option<String>,
    /// From a project: its board columns. Default true.
    #[serde(default = "yes")]
    pub columns: bool,
    /// From a project: its labels. Default true.
    #[serde(default = "yes")]
    pub labels: bool,
    /// From a project: its open tasks, as starter tasks. Default true.
    #[serde(default = "yes")]
    pub tasks: bool,
    /// From a project: its databases' columns and views, without rows. Default true.
    #[serde(default = "yes")]
    pub databases: bool,
    /// From a project: its notes' headings, as skeletons. Default false.
    #[serde(default)]
    pub notes: bool,
    #[serde(default)]
    pub shared: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub shared: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ApplyTemplateRequest {
    pub template_id: String,
}

#[derive(FromRow)]
struct TemplateRow {
    id: String,
    owner_id: String,
    name: String,
    body: String,
    shared: bool,
    source_title: Option<String>,
    created_at: String,
    owner_name: String,
}

const TEMPLATE_SELECT: &str =
    "SELECT t.id, t.owner_id, t.name, t.body, t.shared, t.source_title, t.created_at,
        u.name AS owner_name
    FROM project_templates t JOIN users u ON u.id = t.owner_id
    WHERE (t.owner_id = ? OR (t.shared = 1 AND EXISTS (
        SELECT 1 FROM shares s WHERE s.owner_id = t.owner_id AND s.grantee_id = ?)))";

fn summarise(
    id: String,
    name: String,
    blurb: String,
    body: &TemplateBody,
    builtin: bool,
    row: Option<&TemplateRow>,
    me: &str,
) -> ProjectTemplate {
    ProjectTemplate {
        id,
        name,
        blurb,
        builtin,
        mine: row.is_some_and(|r| r.owner_id == me),
        shared: row.is_some_and(|r| r.shared),
        owner_name: row.filter(|r| r.owner_id != me).map(|r| r.owner_name.clone()),
        source_title: row.and_then(|r| r.source_title.clone()),
        columns: if body.columns.is_empty() { blank().columns } else { body.columns.clone() },
        label_names: body.labels.iter().map(|l| l.name.clone()).collect(),
        task_count: body.tasks.len() as i64,
        database_names: body.databases.iter().map(|d| d.name.clone()).collect(),
        note_titles: body.notes.iter().map(|n| n.title.clone()).collect(),
        created_at: row.map(|r| r.created_at.clone()),
    }
}

/// "Saved from Z80 Computer, 6 starter tasks".
fn saved_blurb(row: &TemplateRow, body: &TemplateBody) -> String {
    let mut parts = vec![];
    if let Some(t) = &row.source_title {
        parts.push(format!("Saved from {t}"));
    }
    let n = body.tasks.len();
    if n > 0 {
        parts.push(format!("{n} starter task{}", if n == 1 { "" } else { "s" }));
    }
    for d in &body.databases {
        parts.push(format!("{} database", d.name));
    }
    if parts.is_empty() {
        parts.push(format!("{} columns", body.columns.len()));
    }
    parts.join(", ")
}

fn parse(row: &TemplateRow) -> TemplateBody {
    serde_json::from_str(&row.body).unwrap_or_default()
}

fn clean_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 80 {
        return Err(AppError::BadRequest(
            "invalid_name",
            "Give the template a name of up to 80 characters.".into(),
        ));
    }
    Ok(n.to_string())
}

async fn visible_row(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<TemplateRow> {
    sqlx::query_as::<_, TemplateRow>(sqlx::AssertSqlSafe(format!("{TEMPLATE_SELECT} AND t.id = ?")))
        .bind(user_id)
        .bind(user_id)
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

async fn owned_row(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<TemplateRow> {
    let row = visible_row(conn, user_id, id).await?;
    if row.owner_id != user_id {
        return Err(AppError::Forbidden(format!("Only {} can change {}.", row.owner_name, row.name)));
    }
    Ok(row)
}

/// Every template a person can start from: the built-ins, then their own and
/// the ones shared with them, newest first.
pub async fn visible_templates(
    conn: &mut SqliteConnection,
    user_id: &str,
) -> AppResult<Vec<ProjectTemplate>> {
    let mut out: Vec<ProjectTemplate> = BUILTINS
        .iter()
        .map(|b| {
            summarise(
                format!("{BUILTIN_PREFIX}{}", b.key),
                b.name.into(),
                b.blurb.into(),
                &(b.body)(),
                true,
                None,
                user_id,
            )
        })
        .collect();
    let rows = sqlx::query_as::<_, TemplateRow>(sqlx::AssertSqlSafe(format!(
        "{TEMPLATE_SELECT} ORDER BY t.created_at DESC"
    )))
    .bind(user_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    for r in &rows {
        let body = parse(r);
        out.push(summarise(
            r.id.clone(),
            r.name.clone(),
            saved_blurb(r, &body),
            &body,
            false,
            Some(r),
            user_id,
        ));
    }
    Ok(out)
}

/// A template's name and body, if the person may use it.
pub async fn load(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<(String, TemplateBody)> {
    if let Some(b) = builtin(id) {
        return Ok((b.name.into(), (b.body)()));
    }
    if id.starts_with(BUILTIN_PREFIX) {
        return Err(AppError::NotFound);
    }
    let row = visible_row(conn, user_id, id).await?;
    Ok((row.name.clone(), parse(&row)))
}

/// Find a template by id or by name (any case), for the MCP server.
pub async fn find(conn: &mut SqliteConnection, user_id: &str, wanted: &str) -> AppResult<String> {
    let wanted = wanted.trim();
    let all = visible_templates(conn, user_id).await?;
    let norm = |s: &str| s.to_lowercase().replace([' ', '/', '-', '_'], "");
    let hit = all
        .iter()
        .find(|t| t.id == wanted)
        .or_else(|| all.iter().find(|t| t.name.eq_ignore_ascii_case(wanted)))
        .or_else(|| {
            all.iter().find(|t| t.builtin && t.id == format!("{BUILTIN_PREFIX}{}", wanted.to_lowercase()))
        })
        .or_else(|| all.iter().find(|t| norm(&t.name).contains(&norm(wanted)) && !norm(wanted).is_empty()));
    match hit {
        Some(t) => Ok(t.id.clone()),
        None => Err(AppError::BadRequest(
            "unknown_template",
            format!(
                "No template called {wanted}. Templates: {}.",
                all.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Applying a template

/// Board columns for a new project: the template's, or To do, Doing, Done.
pub async fn insert_columns(
    conn: &mut SqliteConnection,
    project_id: &str,
    body: &TemplateBody,
) -> AppResult<()> {
    let columns = if body.columns.is_empty() { blank().columns } else { body.columns.clone() };
    let positions = position::spread(columns.len());
    for (c, pos) in columns.iter().zip(positions) {
        let category =
            if board::CATEGORIES.contains(&c.category.as_str()) { c.category.as_str() } else { "todo" };
        let name: String = c.name.trim().chars().take(60).collect();
        let name = if name.is_empty() { "Column".to_string() } else { name };
        board::insert_status_at(conn, project_id, &name, category, c.color.as_deref(), pos).await?;
    }
    Ok(())
}

/// Adds a template's labels, tasks and databases to a project whose columns
/// are already in place. Notes are written after the transaction commits:
/// see [`write_notes`].
pub async fn apply_rest(conn: &mut SqliteConnection, project_id: &str, body: &TemplateBody) -> AppResult<()> {
    // Labels, keeping any the project already has.
    let mut label_ids: HashMap<String, String> = HashMap::new();
    for l in &body.labels {
        let name: String = l.name.trim().chars().take(40).collect();
        if name.is_empty() {
            continue;
        }
        let existing: Option<String> =
            sqlx::query_scalar("SELECT id FROM labels WHERE project_id = ? AND name = ? COLLATE NOCASE")
                .bind(project_id)
                .bind(&name)
                .fetch_optional(&mut *conn)
                .await?;
        let id = match existing {
            Some(id) => id,
            None => {
                let id = time::new_id();
                sqlx::query("INSERT INTO labels (id, project_id, name, color) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(project_id)
                    .bind(&name)
                    .bind(&l.color)
                    .execute(&mut *conn)
                    .await?;
                id
            }
        };
        label_ids.insert(name.to_lowercase(), id);
    }

    // Starter tasks, each at the bottom of its column.
    let statuses = board::statuses_for(conn, project_id).await?;
    for t in body.tasks.iter().take(MAX_TASKS) {
        let title: String = t.title.trim().chars().take(300).collect();
        let Some(status) = statuses.get(t.column).or(statuses.first()) else { break };
        if title.is_empty() {
            continue;
        }
        let pos = board::position_last(conn, "tasks", "status_id", &status.id).await?;
        let number: i64 = sqlx::query_scalar(
            "UPDATE projects SET next_task_number = next_task_number + 1 WHERE id = ? RETURNING next_task_number - 1",
        )
        .bind(project_id)
        .fetch_one(&mut *conn)
        .await?;
        let task_id = time::new_id();
        let now = time::now();
        let priority = t.priority.as_deref().filter(|p| matches!(*p, "low" | "medium" | "high" | "urgent"));
        let completed_at = (status.category == "done").then(|| now.clone());
        sqlx::query(
            "INSERT INTO tasks (id, project_id, number, status_id, title, description, priority, position,
                completed_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&task_id)
        .bind(project_id)
        .bind(number)
        .bind(&status.id)
        .bind(&title)
        .bind(&t.description)
        .bind(priority)
        .bind(&pos)
        .bind(&completed_at)
        .bind(&now)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
        for name in &t.labels {
            if let Some(label) = label_ids.get(&name.trim().to_lowercase()) {
                sqlx::query("INSERT OR IGNORE INTO task_labels (task_id, label_id) VALUES (?, ?)")
                    .bind(&task_id)
                    .bind(label)
                    .execute(&mut *conn)
                    .await?;
            }
        }
        let items: Vec<&String> = t.checklist.iter().filter(|s| !s.trim().is_empty()).take(100).collect();
        for (text, pos) in items.iter().zip(position::spread(items.len())) {
            sqlx::query(
                "INSERT INTO task_checklist (id, task_id, text, done, position, created_at) VALUES (?, ?, ?, 0, ?, ?)",
            )
            .bind(time::new_id())
            .bind(&task_id)
            .bind(text.trim())
            .bind(&pos)
            .bind(&now)
            .execute(&mut *conn)
            .await?;
        }
    }

    // Databases, schemas only. A name already taken gets a number.
    for d in &body.databases {
        let name = free_database_name(conn, project_id, &d.name).await?;
        if d.template.as_deref() == Some(crate::parts::TEMPLATE) {
            crate::parts::create_parts_database(conn, project_id, &name).await?;
            continue;
        }
        let db = databases::insert_database(conn, project_id, &name).await?;
        let mut ids: Vec<(String, String)> = vec![];
        let mut columns = d.columns.clone();
        if !columns.iter().any(|c| c.is_title) {
            columns.insert(
                0,
                TemplateDbColumn {
                    key: String::new(),
                    name: "Name".into(),
                    kind: "text".into(),
                    options: ColumnOptions::default(),
                    is_title: true,
                },
            );
        }
        // The title column first, as a database expects.
        columns.sort_by_key(|c| !c.is_title);
        let mut title_done = false;
        for c in columns.iter().take(60) {
            let is_title = c.is_title && !title_done;
            title_done |= is_title;
            let kind = if is_title { "text" } else { c.kind.as_str() };
            let mut options = c.options.clone();
            if options.target.as_deref() == Some("row") {
                // Rows of a database that isn't coming along: leave the column out.
                continue;
            }
            options.target_database_id = None;
            let new = databases::insert_column(conn, &db, &c.name, kind, &options, is_title).await?;
            if !c.key.is_empty() {
                ids.push((c.key.clone(), new.id));
            }
        }
        let remap = |config: &Value| -> ViewConfig {
            let mut text = config.to_string();
            for (old, new) in &ids {
                text = text.replace(old.as_str(), new);
            }
            serde_json::from_str(&text).unwrap_or_default()
        };
        for (i, v) in d.views.iter().take(20).enumerate() {
            let config = remap(&v.config);
            if i == 0 {
                // insert_database made an All view; the first view takes its place.
                sqlx::query("UPDATE db_views SET name = ?, kind = ?, config = ? WHERE database_id = ?")
                    .bind(&v.name)
                    .bind(&v.kind)
                    .bind(serde_json::to_string(&config).unwrap_or_else(|_| "{}".into()))
                    .bind(&db)
                    .execute(&mut *conn)
                    .await?;
            } else {
                databases::insert_view(conn, &db, &v.name, &v.kind, &config).await?;
            }
        }
    }
    Ok(())
}

async fn free_database_name(
    conn: &mut SqliteConnection,
    project_id: &str,
    wanted: &str,
) -> AppResult<String> {
    let base: String = wanted.trim().chars().take(55).collect();
    let base = if base.is_empty() { "Database".to_string() } else { base };
    let mut name = base.clone();
    for n in 2..100 {
        let taken: Option<String> =
            sqlx::query_scalar("SELECT id FROM databases WHERE project_id = ? AND name = ? COLLATE NOCASE")
                .bind(project_id)
                .bind(&name)
                .fetch_optional(&mut *conn)
                .await?;
        if taken.is_none() {
            break;
        }
        name = format!("{base} {n}");
    }
    Ok(name)
}

/// Writes a template's note skeletons into the person's vault, filed under the
/// project. Runs after the project exists, outside its transaction.
pub async fn write_notes(
    state: &AppState,
    user: &User,
    project_id: &str,
    notes: &[TemplateNote],
) -> AppResult<()> {
    for n in notes.iter().take(50) {
        let req = notes::CreateNoteRequest {
            title: Some(n.title.clone()),
            body: Some(n.body.clone()),
            folder: None,
            project_ids: vec![project_id.to_string()],
            tags: vec![],
            task_id: None,
        };
        let _ = notes::create_note(State(state.clone()), CurrentUser(user.clone()), Json(req)).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Saving a project as a template

/// The headings of a note, so a new project gets its structure but none of
/// the old project's words.
pub fn skeleton(body: &str) -> String {
    let heads: Vec<&str> = body.lines().filter(|l| l.trim_start().starts_with('#')).map(str::trim).collect();
    heads.join("\n\n")
}

/// id, title, description, priority, status_id.
type OpenTaskRow = (String, String, Option<String>, Option<String>, String);

async fn body_from_project(
    state: &AppState,
    conn: &mut SqliteConnection,
    user_id: &str,
    project_id: &str,
    req: &CreateTemplateRequest,
) -> AppResult<TemplateBody> {
    visible_project(conn, user_id, project_id).await?;
    let mut body = TemplateBody::default();
    let statuses = board::statuses_for(conn, project_id).await?;
    if req.columns {
        body.columns = statuses
            .iter()
            .map(|s| TemplateColumn {
                name: s.name.clone(),
                category: s.category.clone(),
                color: s.color.clone(),
            })
            .collect();
    }
    if req.labels {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT name, color FROM labels WHERE project_id = ? ORDER BY name COLLATE NOCASE",
        )
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?;
        body.labels = rows.into_iter().map(|(name, color)| TemplateLabel { name, color }).collect();
    }
    if req.tasks {
        let column_of: HashMap<&str, usize> =
            statuses.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
        let rows: Vec<OpenTaskRow> = sqlx::query_as(
            "SELECT t.id, t.title, t.description, t.priority, t.status_id FROM tasks t
             JOIN statuses s ON s.id = t.status_id
             WHERE t.project_id = ? AND s.category <> 'done' ORDER BY s.position, t.position LIMIT ?",
        )
        .bind(project_id)
        .bind(MAX_TASKS as i64)
        .fetch_all(&mut *conn)
        .await?;
        for (id, title, description, priority, status_id) in rows {
            let labels: Vec<String> = if req.labels {
                sqlx::query_scalar(
                    "SELECT l.name FROM task_labels tl JOIN labels l ON l.id = tl.label_id WHERE tl.task_id = ?",
                )
                .bind(&id)
                .fetch_all(&mut *conn)
                .await?
            } else {
                vec![]
            };
            let checklist: Vec<String> =
                sqlx::query_scalar("SELECT text FROM task_checklist WHERE task_id = ? ORDER BY position")
                    .bind(&id)
                    .fetch_all(&mut *conn)
                    .await?;
            body.tasks.push(TemplateTask {
                title,
                description,
                // Without the project's own columns, every task starts in the first one.
                column: if req.columns { column_of.get(status_id.as_str()).copied().unwrap_or(0) } else { 0 },
                priority,
                labels,
                checklist,
            });
        }
    }
    if req.databases {
        let dbs: Vec<(String, String, Option<String>)> =
            sqlx::query_as("SELECT id, name, template FROM databases WHERE project_id = ? ORDER BY position")
                .bind(project_id)
                .fetch_all(&mut *conn)
                .await?;
        for (id, name, template) in dbs {
            if template.as_deref() == Some(crate::parts::TEMPLATE) {
                body.databases.push(TemplateDatabase { name, template, columns: vec![], views: vec![] });
                continue;
            }
            let columns = databases::columns_of(conn, &id)
                .await?
                .into_iter()
                .filter(|c| c.options.target.as_deref() != Some("row"))
                .map(|c| TemplateDbColumn {
                    key: c.id,
                    name: c.name,
                    kind: c.kind,
                    options: c.options,
                    is_title: c.is_title,
                })
                .collect();
            let views: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT name, kind, config FROM db_views WHERE database_id = ? ORDER BY position",
            )
            .bind(&id)
            .fetch_all(&mut *conn)
            .await?;
            let views = views
                .into_iter()
                .map(|(name, kind, config)| TemplateView {
                    name,
                    kind,
                    config: serde_json::from_str(&config).unwrap_or(Value::Null),
                })
                .collect();
            body.databases.push(TemplateDatabase { name, template: None, columns, views });
        }
    }
    if req.notes {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT n.title, n.path, n.user_id FROM notes n
             JOIN note_projects np ON np.note_id = n.id
             JOIN note_access na ON na.note_id = n.id AND na.user_id = ?
             WHERE np.project_id = ? ORDER BY n.title COLLATE NOCASE LIMIT 50",
        )
        .bind(user_id)
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?;
        for (title, path, owner) in rows {
            let root = notes::vault::root(&state.config.data_dir, &owner);
            let text = notes::vault::read_file(&root, &path).await.unwrap_or_default();
            let (_, note_body) = notes::markdown::split(&text);
            body.notes.push(TemplateNote { title, body: skeleton(&note_body) });
        }
    }
    Ok(body)
}

// ---------------------------------------------------------------------------
// Handlers

/// Templates you can start a project from: the built-ins, yours, and the ones
/// shared with you.
#[utoipa::path(get, path = "/templates", tag = "templates",
    responses((status = 200, body = Vec<ProjectTemplate>)))]
async fn list_templates(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<ProjectTemplate>>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(visible_templates(&mut conn, &user.id).await?))
}

/// Save a project as a template, or copy one (built-ins can't be edited, but
/// a copy can).
#[utoipa::path(post, path = "/templates", tag = "templates", request_body = CreateTemplateRequest,
    responses((status = 201, body = ProjectTemplate), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn create_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateTemplateRequest>,
) -> AppResult<(StatusCode, Json<ProjectTemplate>)> {
    let name = clean_name(&req.name)?;
    let mut conn = state.db.acquire().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_templates WHERE owner_id = ?")
        .bind(&user.id)
        .fetch_one(&mut *conn)
        .await?;
    if count >= MAX_TEMPLATES {
        return Err(AppError::BadRequest("too_many_templates", "You can keep up to 100 templates.".into()));
    }
    let (body, source_title) = match (&req.project_id, &req.from_template_id) {
        (Some(p), None) => {
            let project = visible_project(&mut conn, &user.id, p).await?;
            (body_from_project(&state, &mut conn, &user.id, p, &req).await?, Some(project.title))
        }
        (None, Some(t)) => (load(&mut conn, &user.id, t).await?.1, None),
        _ => {
            return Err(AppError::BadRequest(
                "invalid_source",
                "Save a template from a project_id or copy a from_template_id.".into(),
            ));
        }
    };
    let id = time::new_id();
    let now = time::now();
    sqlx::query(
        "INSERT INTO project_templates (id, owner_id, name, body, shared, source_title, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&name)
    .bind(serde_json::to_string(&body).map_err(|e| AppError::Internal(e.into()))?)
    .bind(req.shared)
    .bind(&source_title)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await?;
    let row = visible_row(&mut conn, &user.id, &id).await?;
    let blurb = saved_blurb(&row, &body);
    Ok((StatusCode::CREATED, Json(summarise(id, name, blurb, &body, false, Some(&row), &user.id))))
}

/// Rename or share one of your templates.
#[utoipa::path(patch, path = "/templates/{id}", tag = "templates", params(("id" = String, Path)),
    request_body = UpdateTemplateRequest,
    responses((status = 200, body = ProjectTemplate), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn update_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTemplateRequest>,
) -> AppResult<Json<ProjectTemplate>> {
    let mut conn = state.db.acquire().await?;
    let row = owned_row(&mut conn, &user.id, &id).await?;
    let name = match &req.name {
        Some(n) => clean_name(n)?,
        None => row.name.clone(),
    };
    sqlx::query("UPDATE project_templates SET name = ?, shared = ?, updated_at = ? WHERE id = ?")
        .bind(&name)
        .bind(req.shared.unwrap_or(row.shared))
        .bind(time::now())
        .bind(&id)
        .execute(&mut *conn)
        .await?;
    let row = visible_row(&mut conn, &user.id, &id).await?;
    let body = parse(&row);
    Ok(Json(summarise(id, name, saved_blurb(&row, &body), &body, false, Some(&row), &user.id)))
}

/// Delete one of your templates. Projects made from it keep everything.
#[utoipa::path(delete, path = "/templates/{id}", tag = "templates", params(("id" = String, Path)),
    responses((status = 204), (status = 403, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn delete_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut conn = state.db.acquire().await?;
    owned_row(&mut conn, &user.id, &id).await?;
    sqlx::query("DELETE FROM project_templates WHERE id = ?").bind(&id).execute(&mut *conn).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Give a project with an empty board a template's columns, labels, starter
/// tasks, databases and notes. A board that has tasks can't switch.
#[utoipa::path(post, path = "/projects/{id}/apply-template", tag = "templates", params(("id" = String, Path)),
    request_body = ApplyTemplateRequest,
    responses((status = 200, body = projects::Project), (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody, description = "The board has tasks")))]
async fn apply_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<ApplyTemplateRequest>,
) -> AppResult<Json<projects::Project>> {
    let mut tx = state.db.begin().await?;
    editable_project(&mut tx, &user.id, &id).await?;
    let (name, body) = load(&mut tx, &user.id, &req.template_id).await?;
    let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE project_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    if tasks > 0 {
        return Err(AppError::Conflict(
            "board_not_empty",
            "Only a board with no tasks can switch to a template's columns.".into(),
        ));
    }
    if !body.columns.is_empty() {
        sqlx::query("DELETE FROM statuses WHERE project_id = ?").bind(&id).execute(&mut *tx).await?;
        insert_columns(&mut tx, &id, &body).await?;
    }
    apply_rest(&mut tx, &id, &body).await?;
    log_activity(&mut tx, &id, &user.id, "project.template_applied", serde_json::json!({ "template": name }))
        .await?;
    tx.commit().await?;
    write_notes(&state, &user, &id, &body.notes).await?;
    let mut conn = state.db.acquire().await?;
    Ok(Json(visible_project(&mut conn, &user.id, &id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_a_done_column_and_valid_categories() {
        for b in BUILTINS {
            let body = (b.body)();
            assert!(body.columns.iter().any(|c| c.category == "done"), "{}", b.name);
            assert!(
                body.columns.iter().all(|c| board::CATEGORIES.contains(&c.category.as_str())),
                "{}",
                b.name
            );
            assert!(body.tasks.iter().all(|t| t.column < body.columns.len()), "{}", b.name);
        }
    }

    #[test]
    fn skeleton_keeps_only_headings() {
        let body = "# Plan\nSome words\n\n## Parts\n- a thing\n### Notes";
        assert_eq!(skeleton(body), "# Plan\n\n## Parts\n\n### Notes");
    }
}
