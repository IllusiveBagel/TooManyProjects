//! Project databases: tables with typed columns, rows and saved views, like
//! Notion databases. A database belongs to a project and is shared with it.
//!
//! Rows keep their values as JSON keyed by column id (see [`values`]).
//! Relations to tasks, notes, projects or rows of another database live in
//! `db_relations` so a task can find the rows that point at it.

pub mod csv;
pub mod transfer;
pub mod values;

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{FromRow, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    board::{position_after, position_last},
    error::{AppError, AppResult},
    projects::{Project, editable_project, some, visible_project},
    time,
};

const MAX_DATABASES: i64 = 50;
const MAX_COLUMNS: i64 = 60;
const MAX_VIEWS: i64 = 20;
pub const MAX_ROWS: i64 = 10_000;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_all_databases))
        .routes(routes!(list_databases, create_database))
        .routes(routes!(get_database, update_database, delete_database))
        .routes(routes!(create_column))
        .routes(routes!(update_column, delete_column))
        .routes(routes!(create_view))
        .routes(routes!(update_view, delete_view))
        .routes(routes!(list_rows, create_row))
        .routes(routes!(update_row, delete_row))
        .routes(routes!(task_rows))
        .routes(routes!(transfer::export_database))
        .merge(
            OpenApiRouter::new()
                .routes(routes!(transfer::import_database))
                .layer(axum::extract::DefaultBodyLimit::max(transfer::MAX_CSV_BYTES + 1024)),
        )
}

// ---------------------------------------------------------------------------
// Types

/// One option of a select or multi-select column.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct Choice {
    /// Left empty when adding a choice; the server gives it one.
    #[serde(default)]
    #[schema(required = true)]
    pub id: String,
    pub name: String,
    /// A theme slot, `slot-1` to `slot-8`.
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ColumnOptions {
    /// Select and multi-select: the choices, in order.
    #[serde(default)]
    #[schema(required = true)]
    pub choices: Vec<Choice>,
    /// Currency: an ISO code such as `GBP`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Relation: `task`, `note`, `project` or `row`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Relation to rows: the database they're in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_database_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Column {
    pub id: String,
    pub database_id: String,
    pub name: String,
    /// `text`, `number`, `currency`, `select`, `multi_select`, `date`,
    /// `checkbox`, `url`, `person` or `relation`.
    pub kind: String,
    pub options: ColumnOptions,
    /// The row's name. Every database has exactly one, and it stays text.
    pub is_title: bool,
    pub position: String,
    /// What a template column is for, such as `status` or `unit_cost` in a
    /// Parts database. Such a column keeps its type.
    pub role: Option<String>,
}

#[derive(FromRow)]
struct ColumnRow {
    id: String,
    database_id: String,
    name: String,
    kind: String,
    options: String,
    is_title: bool,
    position: String,
    role: Option<String>,
}

impl From<ColumnRow> for Column {
    fn from(r: ColumnRow) -> Self {
        Column {
            id: r.id,
            database_id: r.database_id,
            name: r.name,
            kind: r.kind,
            options: serde_json::from_str(&r.options).unwrap_or_default(),
            is_title: r.is_title,
            position: r.position,
            role: r.role,
        }
    }
}

const COLUMN_SELECT: &str =
    "SELECT id, database_id, name, kind, options, is_title, position, role FROM db_columns";

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Filter {
    pub column_id: String,
    /// `is`, `is_not`, `contains`, `empty`, `not_empty`, `gt` or `lt`.
    pub op: String,
    #[serde(default)]
    #[schema(value_type = Option<Value>)]
    pub value: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct Sort {
    pub column_id: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ViewConfig {
    /// Every filter must pass.
    #[serde(default)]
    #[schema(required = true)]
    pub filters: Vec<Filter>,
    /// First sort first. With none, rows keep their own order.
    #[serde(default)]
    #[schema(required = true)]
    pub sorts: Vec<Sort>,
    /// Columns the view leaves out.
    #[serde(default)]
    #[schema(required = true)]
    pub hidden: Vec<String>,
    /// Board: the select column that makes the lanes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Calendar: the date column rows sit on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_column: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct View {
    pub id: String,
    pub database_id: String,
    pub name: String,
    /// `table`, `board`, `calendar` or `gallery` (cards with each row's first photo).
    pub kind: String,
    pub config: ViewConfig,
    /// Shown on the project's overview.
    pub pinned: bool,
    pub position: String,
}

#[derive(FromRow)]
struct ViewRow {
    id: String,
    database_id: String,
    name: String,
    kind: String,
    config: String,
    pinned: bool,
    position: String,
}

impl From<ViewRow> for View {
    fn from(r: ViewRow) -> Self {
        View {
            id: r.id,
            database_id: r.database_id,
            name: r.name,
            kind: r.kind,
            config: serde_json::from_str(&r.config).unwrap_or_default(),
            pinned: r.pinned,
            position: r.position,
        }
    }
}

const VIEW_SELECT: &str = "SELECT id, database_id, name, kind, config, pinned, position FROM db_views";

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct Database {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub position: String,
    pub row_count: i64,
    /// `parts` for a Parts database, which the Shopping list reads.
    pub template: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const DATABASE_SELECT: &str = "SELECT d.id, d.project_id, d.name, d.position,
    (SELECT COUNT(*) FROM db_rows r WHERE r.database_id = d.id) AS row_count, d.template, d.created_at, d.updated_at
    FROM databases d";

/// A database with its columns and views.
#[derive(Serialize, ToSchema)]
pub struct DatabaseDetail {
    pub database: Database,
    pub project_title: String,
    pub columns: Vec<Column>,
    pub views: Vec<View>,
    /// False when the project is shared with you to view.
    pub can_edit: bool,
}

/// A database somewhere you can see, for pickers and embeds.
#[derive(Clone, Serialize, ToSchema, FromRow)]
pub struct DatabaseRef {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub project_title: String,
    pub project_color: String,
    #[sqlx(skip)]
    pub views: Vec<View>,
}

/// Where a relation cell points.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RelationTarget {
    /// `task`, `note`, `project` or `row`.
    pub kind: String,
    pub id: String,
}

/// A relation cell's link, with what's needed to show and open it.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RelationRef {
    pub kind: String,
    pub id: String,
    pub title: String,
    /// Tasks and rows: the project they're in.
    pub project_id: Option<String>,
    /// Rows: their database.
    pub database_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Row {
    pub id: String,
    pub database_id: String,
    /// Column id to value. See the column kinds for each value's shape.
    #[schema(value_type = HashMap<String, Value>)]
    pub values: Map<String, Value>,
    /// Relation column id to its links, in order.
    pub relations: HashMap<String, Vec<RelationRef>>,
    pub position: String,
    pub created_at: String,
    pub updated_at: String,
    /// Photos and files attached to the row.
    pub file_count: i64,
    /// Its first photo, for the gallery view.
    pub cover_id: Option<String>,
}

#[derive(FromRow)]
struct RowRecord {
    id: String,
    database_id: String,
    data: String,
    position: String,
    created_at: String,
    updated_at: String,
    file_count: i64,
    cover_id: Option<String>,
}

/// Rows of one database that link to a task.
#[derive(Serialize, ToSchema)]
pub struct LinkedRows {
    pub database: DatabaseRef,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// The relation column that points at the task.
    pub column_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateDatabaseRequest {
    pub name: String,
    /// `parts` starts it with the Parts columns and views.
    pub template: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateDatabaseRequest {
    pub name: Option<String>,
    /// Put it after this database, or first when `null`.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateColumnRequest {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub options: ColumnOptions,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateColumnRequest {
    pub name: Option<String>,
    /// Change the type. Values are converted where they can be, else cleared.
    pub kind: Option<String>,
    pub options: Option<ColumnOptions>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateViewRequest {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub config: ViewConfig,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateViewRequest {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub config: Option<ViewConfig>,
    pub pinned: Option<bool>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct CreateRowRequest {
    /// Column id to value.
    #[serde(default)]
    #[schema(value_type = HashMap<String, Value>)]
    pub values: Map<String, Value>,
    /// Relation column id to its links.
    #[serde(default)]
    pub relations: HashMap<String, Vec<RelationTarget>>,
    /// Put it after this row; at the end when left out.
    pub after_id: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRowRequest {
    /// Only the columns named change; `null` clears a cell.
    #[schema(value_type = Option<HashMap<String, Value>>)]
    pub values: Option<Map<String, Value>>,
    /// Replaces the links of each relation column named.
    pub relations: Option<HashMap<String, Vec<RelationTarget>>>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub after_id: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams)]
pub struct RowsQuery {
    /// Apply this view's filters and sorts.
    pub view: Option<String>,
    /// Only rows with this text in any cell.
    pub q: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers

fn clean_name(name: &str, what: &str, max: usize) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > max {
        return Err(AppError::BadRequest(
            "invalid_name",
            format!("Give the {what} a name up to {max} characters."),
        ));
    }
    Ok(n.to_string())
}

pub(crate) fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

pub(crate) struct DbRow {
    pub id: String,
    pub project_id: String,
}

async fn database_row(conn: &mut SqliteConnection, id: &str) -> AppResult<DbRow> {
    let (id, project_id): (String, String) =
        sqlx::query_as("SELECT id, project_id FROM databases WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok(DbRow { id, project_id })
}

/// The database and its project if you can see it, else 404.
async fn visible_database(
    conn: &mut SqliteConnection,
    user_id: &str,
    id: &str,
) -> AppResult<(DbRow, Project)> {
    let db = database_row(conn, id).await?;
    let project = visible_project(conn, user_id, &db.project_id).await?;
    Ok((db, project))
}

/// The database if you can change it (403 for viewers).
pub(crate) async fn editable_database(
    conn: &mut SqliteConnection,
    user_id: &str,
    id: &str,
) -> AppResult<(DbRow, Project)> {
    let db = database_row(conn, id).await?;
    let project = editable_project(conn, user_id, &db.project_id).await?;
    Ok((db, project))
}

async fn touch(conn: &mut SqliteConnection, database_id: &str) -> AppResult<()> {
    sqlx::query("UPDATE databases SET updated_at = ? WHERE id = ?")
        .bind(time::now())
        .bind(database_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub(crate) async fn load_database(conn: &mut SqliteConnection, id: &str) -> AppResult<Database> {
    Ok(sqlx::query_as::<_, Database>(sqlx::AssertSqlSafe(format!("{DATABASE_SELECT} WHERE d.id = ?")))
        .bind(id)
        .fetch_one(conn)
        .await?)
}

pub(crate) async fn columns_of(conn: &mut SqliteConnection, database_id: &str) -> AppResult<Vec<Column>> {
    let rows = sqlx::query_as::<_, ColumnRow>(sqlx::AssertSqlSafe(format!(
        "{COLUMN_SELECT} WHERE database_id = ? ORDER BY position"
    )))
    .bind(database_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(Column::from).collect())
}

async fn views_of(conn: &mut SqliteConnection, database_id: &str) -> AppResult<Vec<View>> {
    let rows = sqlx::query_as::<_, ViewRow>(sqlx::AssertSqlSafe(format!(
        "{VIEW_SELECT} WHERE database_id = ? ORDER BY position"
    )))
    .bind(database_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(View::from).collect())
}

async fn column(conn: &mut SqliteConnection, id: &str) -> AppResult<Column> {
    let row = sqlx::query_as::<_, ColumnRow>(sqlx::AssertSqlSafe(format!("{COLUMN_SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.into())
}

async fn view(conn: &mut SqliteConnection, id: &str) -> AppResult<View> {
    let row = sqlx::query_as::<_, ViewRow>(sqlx::AssertSqlSafe(format!("{VIEW_SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.into())
}

pub(crate) async fn detail(
    conn: &mut SqliteConnection,
    id: &str,
    project: &Project,
) -> AppResult<DatabaseDetail> {
    Ok(DatabaseDetail {
        database: load_database(&mut *conn, id).await?,
        project_title: project.title.clone(),
        columns: columns_of(&mut *conn, id).await?,
        views: views_of(&mut *conn, id).await?,
        can_edit: project.role != "viewer",
    })
}

/// Check a column's kind and options, filling in choice ids and colours.
async fn check_column(
    conn: &mut SqliteConnection,
    user_id: &str,
    kind: &str,
    mut options: ColumnOptions,
) -> AppResult<ColumnOptions> {
    if !values::KINDS.contains(&kind) {
        return Err(AppError::BadRequest(
            "invalid_kind",
            format!("A column type is one of {}.", values::KINDS.join(", ")),
        ));
    }
    if matches!(kind, "select" | "multi_select") {
        if options.choices.len() > 100 {
            return Err(AppError::BadRequest(
                "too_many_choices",
                "A column can have up to 100 choices.".into(),
            ));
        }
        let mut seen: Vec<String> = vec![];
        for (i, c) in options.choices.iter_mut().enumerate() {
            c.name = clean_name(&c.name, "choice", 60)?;
            let key = c.name.to_lowercase();
            if seen.contains(&key) {
                return Err(AppError::BadRequest(
                    "duplicate_choice",
                    format!("There are two choices called {}.", c.name),
                ));
            }
            seen.push(key);
            if c.id.is_empty() {
                c.id = time::new_id();
            }
            match &c.color {
                Some(s) if s.starts_with("slot-") => {}
                _ => c.color = Some(format!("slot-{}", i % 8 + 1)),
            }
        }
    } else {
        options.choices.clear();
    }
    if kind == "currency" {
        let code = options.currency.take().unwrap_or_else(|| "GBP".into()).trim().to_uppercase();
        if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(AppError::BadRequest(
                "invalid_currency",
                "A currency is a 3-letter code such as GBP.".into(),
            ));
        }
        options.currency = Some(code);
    } else {
        options.currency = None;
    }
    if kind == "relation" {
        let target = options.target.clone().unwrap_or_default();
        if !values::RELATION_TARGETS.contains(&target.as_str()) {
            return Err(AppError::BadRequest(
                "invalid_target",
                "A relation links to tasks, notes, projects or rows.".into(),
            ));
        }
        if target == "row" {
            let other = options.target_database_id.clone().ok_or_else(|| {
                AppError::BadRequest("invalid_target", "Say which database the rows come from.".into())
            })?;
            visible_database(conn, user_id, &other).await?;
        } else {
            options.target_database_id = None;
        }
    } else {
        options.target = None;
        options.target_database_id = None;
    }
    Ok(options)
}

pub(crate) async fn insert_column(
    conn: &mut SqliteConnection,
    database_id: &str,
    name: &str,
    kind: &str,
    options: &ColumnOptions,
    is_title: bool,
) -> AppResult<Column> {
    let id = time::new_id();
    let pos = position_last(conn, "db_columns", "database_id", database_id).await?;
    sqlx::query(
        "INSERT INTO db_columns (id, database_id, name, kind, options, is_title, position, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(database_id)
    .bind(name)
    .bind(kind)
    .bind(serde_json::to_string(options).unwrap_or_else(|_| "{}".into()))
    .bind(is_title)
    .bind(&pos)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    column(conn, &id).await
}

pub(crate) async fn insert_view(
    conn: &mut SqliteConnection,
    database_id: &str,
    name: &str,
    kind: &str,
    config: &ViewConfig,
) -> AppResult<View> {
    let id = time::new_id();
    let pos = position_last(conn, "db_views", "database_id", database_id).await?;
    sqlx::query(
        "INSERT INTO db_views (id, database_id, name, kind, config, position, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(database_id)
    .bind(name)
    .bind(kind)
    .bind(serde_json::to_string(config).unwrap_or_else(|_| "{}".into()))
    .bind(&pos)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    view(conn, &id).await
}

/// Make a database with a `Name` column and an `All` table view.
pub(crate) async fn insert_database(
    conn: &mut SqliteConnection,
    project_id: &str,
    name: &str,
) -> AppResult<String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM databases WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(&mut *conn)
        .await?;
    if count >= MAX_DATABASES {
        return Err(AppError::BadRequest(
            "too_many_databases",
            "A project can have up to 50 databases.".into(),
        ));
    }
    let taken: Option<String> =
        sqlx::query_scalar("SELECT id FROM databases WHERE project_id = ? AND name = ? COLLATE NOCASE")
            .bind(project_id)
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
    if taken.is_some() {
        return Err(AppError::Conflict(
            "name_taken",
            format!("This project already has a database called {name}."),
        ));
    }
    let id = time::new_id();
    let now = time::now();
    let pos = position_last(conn, "databases", "project_id", project_id).await?;
    sqlx::query("INSERT INTO databases (id, project_id, name, position, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(project_id)
        .bind(name)
        .bind(&pos)
        .bind(&now)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    insert_view(conn, &id, "All", "table", &ViewConfig::default()).await?;
    Ok(id)
}

fn check_view(kind: &str, config: &ViewConfig, columns: &[Column]) -> AppResult<()> {
    if !values::VIEW_KINDS.contains(&kind) {
        return Err(AppError::BadRequest("invalid_kind", "A view is a table, board or calendar.".into()));
    }
    for f in &config.filters {
        values::check_filter(f, columns)?;
    }
    let has = |id: &Option<String>, kinds: &[&str]| match id {
        None => true,
        Some(id) => columns.iter().any(|c| &c.id == id && kinds.contains(&c.kind.as_str())),
    };
    if !has(&config.group_by, &["select"]) {
        return Err(AppError::BadRequest("invalid_group", "A board groups by a select column.".into()));
    }
    if !has(&config.date_column, &["date"]) {
        return Err(AppError::BadRequest("invalid_date_column", "A calendar uses a date column.".into()));
    }
    Ok(())
}

/// Check that `user` may point a relation at each target, and that each
/// target fits the column.
async fn check_targets(
    conn: &mut SqliteConnection,
    user_id: &str,
    column: &Column,
    targets: &[RelationTarget],
) -> AppResult<()> {
    if column.kind != "relation" {
        return Err(AppError::BadRequest(
            "invalid_value",
            format!("{} isn't a relation column.", column.name),
        ));
    }
    if targets.len() > 100 {
        return Err(AppError::BadRequest(
            "too_many_links",
            "A relation cell can hold up to 100 links.".into(),
        ));
    }
    let want = column.options.target.as_deref().unwrap_or("");
    for t in targets {
        if t.kind != want {
            return Err(AppError::BadRequest("invalid_value", format!("{} links to {want}s.", column.name)));
        }
        let ok: Option<i64> = match want {
            "task" => sqlx::query_scalar(
                "SELECT 1 FROM tasks t JOIN project_access a ON a.project_id = t.project_id WHERE t.id = ? AND a.user_id = ?",
            )
            .bind(&t.id)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?,
            "note" => sqlx::query_scalar("SELECT 1 FROM note_access WHERE note_id = ? AND user_id = ?")
                .bind(&t.id)
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await?,
            "project" => sqlx::query_scalar("SELECT 1 FROM project_access WHERE project_id = ? AND user_id = ?")
                .bind(&t.id)
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await?,
            _ => sqlx::query_scalar(
                "SELECT 1 FROM db_rows r JOIN databases d ON d.id = r.database_id
                 JOIN project_access a ON a.project_id = d.project_id
                 WHERE r.id = ? AND r.database_id = ? AND a.user_id = ?",
            )
            .bind(&t.id)
            .bind(column.options.target_database_id.as_deref().unwrap_or(""))
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?,
        };
        if ok.is_none() {
            return Err(AppError::BadRequest(
                "invalid_value",
                format!("{}: one of those {want}s isn't there, or isn't shared with you.", column.name),
            ));
        }
    }
    Ok(())
}

async fn set_relations(
    conn: &mut SqliteConnection,
    row_id: &str,
    column_id: &str,
    targets: &[RelationTarget],
) -> AppResult<()> {
    sqlx::query("DELETE FROM db_relations WHERE row_id = ? AND column_id = ?")
        .bind(row_id)
        .bind(column_id)
        .execute(&mut *conn)
        .await?;
    for (i, t) in targets.iter().enumerate() {
        sqlx::query(
            "INSERT OR IGNORE INTO db_relations (row_id, column_id, target_kind, target_id, position) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(row_id)
        .bind(column_id)
        .bind(&t.kind)
        .bind(&t.id)
        .bind(i as i64)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Check and clean the values for a set of columns. People must have access
/// to the project.
async fn clean_values(
    conn: &mut SqliteConnection,
    project_id: &str,
    columns: &[Column],
    input: &Map<String, Value>,
    into: &mut Map<String, Value>,
) -> AppResult<()> {
    for (key, value) in input {
        let column = columns.iter().find(|c| &c.id == key).ok_or_else(|| {
            AppError::BadRequest("invalid_column", "A value names a column that isn't there.".into())
        })?;
        match values::clean(column, value)? {
            Some(v) => {
                if column.kind == "person" {
                    let ok: Option<i64> = sqlx::query_scalar(
                        "SELECT 1 FROM project_access WHERE user_id = ? AND project_id = ?",
                    )
                    .bind(v.as_str().unwrap_or(""))
                    .bind(project_id)
                    .fetch_optional(&mut *conn)
                    .await?;
                    if ok.is_none() {
                        return Err(AppError::BadRequest(
                            "invalid_value",
                            format!("{}: pick someone who can see this project.", column.name),
                        ));
                    }
                }
                into.insert(key.clone(), v);
            }
            None => {
                into.remove(key);
            }
        }
    }
    Ok(())
}

/// Add a row. Used by the API and the MCP server.
pub(crate) async fn insert_row(
    conn: &mut SqliteConnection,
    user_id: &str,
    project_id: &str,
    database_id: &str,
    req: &CreateRowRequest,
) -> AppResult<String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM db_rows WHERE database_id = ?")
        .bind(database_id)
        .fetch_one(&mut *conn)
        .await?;
    if count >= MAX_ROWS {
        return Err(AppError::BadRequest("too_many_rows", "A database can have up to 10,000 rows.".into()));
    }
    let columns = columns_of(&mut *conn, database_id).await?;
    let mut data = Map::new();
    clean_values(&mut *conn, project_id, &columns, &req.values, &mut data).await?;
    // A new part starts in Need, and gets its dates if it starts further on.
    let parts = crate::parts::database_roles(&mut *conn, database_id).await?;
    if let Some(roles) = &parts {
        if let Some(status) = roles.get("status")
            && !data.contains_key(status)
            && columns.iter().any(|c| &c.id == status && c.options.choices.iter().any(|ch| ch.id == "need"))
        {
            data.insert(status.clone(), Value::String("need".into()));
        }
        let today = crate::parts::today_for(&mut *conn, user_id).await?;
        crate::parts::fill_dates(roles, "need", &mut data, &today);
    }
    let pos = match &req.after_id {
        Some(after) => position_after(conn, "db_rows", "database_id", database_id, Some(after), None).await?,
        None => position_last(conn, "db_rows", "database_id", database_id).await?,
    };
    let id = time::new_id();
    let now = time::now();
    sqlx::query(
        "INSERT INTO db_rows (id, database_id, data, position, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(database_id)
    .bind(Value::Object(data).to_string())
    .bind(&pos)
    .bind(user_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await?;
    for (column_id, targets) in &req.relations {
        let column = columns.iter().find(|c| &c.id == column_id).ok_or_else(|| {
            AppError::BadRequest("invalid_column", "A relation names a column that isn't there.".into())
        })?;
        check_targets(conn, user_id, column, targets).await?;
        set_relations(conn, &id, column_id, targets).await?;
    }
    touch(conn, database_id).await?;
    Ok(id)
}

/// Titles and links for relation targets, leaving out what `user` can't see.
async fn resolve_targets(
    conn: &mut SqliteConnection,
    user_id: &str,
    targets: &[(String, String)],
) -> AppResult<HashMap<(String, String), RelationRef>> {
    let mut out = HashMap::new();
    for kind in values::RELATION_TARGETS {
        let ids: Vec<&String> = targets.iter().filter(|(k, _)| k == kind).map(|(_, id)| id).collect();
        if ids.is_empty() {
            continue;
        }
        for chunk in ids.chunks(500) {
            let ph = placeholders(chunk.len());
            let sql = match *kind {
                "task" => format!(
                    "SELECT t.id, 'T-' || t.number || ' ' || t.title, t.project_id, NULL FROM tasks t
                     JOIN project_access a ON a.project_id = t.project_id AND a.user_id = ? WHERE t.id IN ({ph})"
                ),
                "note" => format!(
                    "SELECT n.id, n.title, NULL, NULL FROM notes n
                     JOIN note_access a ON a.note_id = n.id AND a.user_id = ? WHERE n.id IN ({ph})"
                ),
                "project" => format!(
                    "SELECT p.id, p.title, p.id, NULL FROM projects p
                     JOIN project_access a ON a.project_id = p.id AND a.user_id = ? WHERE p.id IN ({ph})"
                ),
                _ => format!(
                    "SELECT r.id, COALESCE(json_extract(r.data, '$.\"' || c.id || '\"'), 'Untitled'), d.project_id, d.id
                     FROM db_rows r JOIN databases d ON d.id = r.database_id
                     JOIN project_access a ON a.project_id = d.project_id AND a.user_id = ?
                     LEFT JOIN db_columns c ON c.database_id = r.database_id AND c.is_title = 1
                     WHERE r.id IN ({ph})"
                ),
            };
            let mut q = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
                sqlx::AssertSqlSafe(sql),
            )
            .bind(user_id);
            for id in chunk {
                q = q.bind(*id);
            }
            for (id, title, project_id, database_id) in q.fetch_all(&mut *conn).await? {
                out.insert(
                    (kind.to_string(), id.clone()),
                    RelationRef { kind: kind.to_string(), id, title, project_id, database_id },
                );
            }
        }
    }
    Ok(out)
}

/// Raw relations for some rows: row id to column id to targets, in order.
async fn relations_of(
    conn: &mut SqliteConnection,
    row_ids: &[String],
) -> AppResult<HashMap<String, HashMap<String, Vec<(String, String)>>>> {
    let mut out: HashMap<String, HashMap<String, Vec<(String, String)>>> = HashMap::new();
    for chunk in row_ids.chunks(500) {
        let sql = format!(
            "SELECT row_id, column_id, target_kind, target_id FROM db_relations WHERE row_id IN ({})
             ORDER BY row_id, column_id, position",
            placeholders(chunk.len())
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String)>(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            q = q.bind(id);
        }
        for (row, col, kind, id) in q.fetch_all(&mut *conn).await? {
            out.entry(row).or_default().entry(col).or_default().push((kind, id));
        }
    }
    Ok(out)
}

/// Build API rows from records, resolving relations for `user`.
async fn build_rows(
    conn: &mut SqliteConnection,
    user_id: &str,
    records: Vec<RowRecord>,
) -> AppResult<Vec<Row>> {
    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let raw = relations_of(&mut *conn, &ids).await?;
    let all: Vec<(String, String)> = raw
        .values()
        .flat_map(|cols| cols.values().flatten().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let refs = resolve_targets(conn, user_id, &all).await?;
    Ok(records
        .into_iter()
        .map(|r| {
            let relations = raw
                .get(&r.id)
                .map(|cols| {
                    cols.iter()
                        .map(|(col, targets)| {
                            (
                                col.clone(),
                                targets.iter().filter_map(|t| refs.get(t).cloned()).collect::<Vec<_>>(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            Row {
                values: serde_json::from_str::<Map<String, Value>>(&r.data).unwrap_or_default(),
                id: r.id,
                database_id: r.database_id,
                relations,
                position: r.position,
                created_at: r.created_at,
                updated_at: r.updated_at,
                file_count: r.file_count,
                cover_id: r.cover_id,
            }
        })
        .collect())
}

const ROW_SELECT: &str = "SELECT id, database_id, data, position, created_at, updated_at,
        (SELECT COUNT(*) FROM attachments x WHERE x.row_id = db_rows.id) AS file_count,
        (SELECT x.id FROM attachments x WHERE x.row_id = db_rows.id AND x.has_thumbs = 1
            ORDER BY x.created_at, x.id LIMIT 1) AS cover_id
    FROM db_rows";

async fn one_row(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Row> {
    let rec = sqlx::query_as::<_, RowRecord>(sqlx::AssertSqlSafe(format!("{ROW_SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(build_rows(conn, user_id, vec![rec]).await?.remove(0))
}

/// A database's rows, through a view's filters and sorts and a search.
pub(crate) async fn query_rows(
    conn: &mut SqliteConnection,
    user_id: &str,
    database_id: &str,
    view: Option<&View>,
    q: Option<&str>,
) -> AppResult<Vec<Row>> {
    let columns = columns_of(&mut *conn, database_id).await?;
    let records = sqlx::query_as::<_, RowRecord>(sqlx::AssertSqlSafe(format!(
        "{ROW_SELECT} WHERE database_id = ? ORDER BY position"
    )))
    .bind(database_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut rows = build_rows(conn, user_id, records).await?;
    // Relations reach filters and sorts as their titles, joined.
    let cell = |row: &Row, column: &Column| -> Option<Value> {
        if column.kind == "relation" {
            let links = row.relations.get(&column.id)?;
            if links.is_empty() {
                return None;
            }
            return Some(Value::Array(links.iter().map(|l| Value::String(l.id.clone())).collect()));
        }
        row.values.get(&column.id).cloned()
    };
    let text_of = |row: &Row, column: &Column| -> String {
        if column.kind == "relation" {
            return row
                .relations
                .get(&column.id)
                .map(|l| l.iter().map(|r| r.title.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
        }
        row.values.get(&column.id).map(|v| values::display(column, v)).unwrap_or_default()
    };
    if let Some(view) = view {
        for f in &view.config.filters {
            let Some(column) = columns.iter().find(|c| c.id == f.column_id) else { continue };
            if f.op == "contains" && column.kind == "relation" {
                let needle = f.value.as_ref().and_then(Value::as_str).unwrap_or("").to_lowercase();
                rows.retain(|r| text_of(r, column).to_lowercase().contains(&needle));
            } else {
                rows.retain(|r| values::matches(f, column, cell(r, column).as_ref()));
            }
        }
    }
    if let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) {
        let needle = q.to_lowercase();
        rows.retain(|r| columns.iter().any(|c| text_of(r, c).to_lowercase().contains(&needle)));
    }
    if let Some(view) = view.filter(|v| !v.config.sorts.is_empty()) {
        rows.sort_by(|a, b| {
            for s in &view.config.sorts {
                let Some(column) = columns.iter().find(|c| c.id == s.column_id) else { continue };
                let ord = if column.kind == "relation" {
                    let (x, y) = (text_of(a, column), text_of(b, column));
                    let ord = match (x.is_empty(), y.is_empty()) {
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => x.to_lowercase().cmp(&y.to_lowercase()),
                    };
                    if s.desc && !x.is_empty() && !y.is_empty() { ord.reverse() } else { ord }
                } else {
                    values::compare(s, column, a.values.get(&column.id), b.values.get(&column.id))
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    Ok(rows)
}

/// Databases in projects you can see, by name.
pub(crate) async fn all_refs(conn: &mut SqliteConnection, user_id: &str) -> AppResult<Vec<DatabaseRef>> {
    let mut refs = sqlx::query_as::<_, DatabaseRef>(
        "SELECT d.id, d.name, d.project_id, p.title AS project_title, p.color AS project_color
         FROM databases d JOIN projects p ON p.id = d.project_id
         JOIN project_access a ON a.project_id = d.project_id AND a.user_id = ?
         ORDER BY d.name COLLATE NOCASE, p.title COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    for r in &mut refs {
        r.views = views_of(&mut *conn, &r.id).await?;
    }
    Ok(refs)
}

// ---------------------------------------------------------------------------
// Database handlers

/// Every database you can see, in any project, with its views. For embeds
/// and pickers.
#[utoipa::path(get, path = "/databases", tag = "databases", responses((status = 200, body = Vec<DatabaseRef>)))]
async fn list_all_databases(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<DatabaseRef>>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(all_refs(&mut conn, &user.id).await?))
}

/// A project's databases, in order, each with its views.
#[utoipa::path(get, path = "/projects/{id}/databases", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, body = Vec<DatabaseDetail>), (status = 404, body = crate::error::ErrorBody)))]
async fn list_databases(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<DatabaseDetail>>> {
    let mut conn = state.db.acquire().await?;
    let project = visible_project(&mut conn, &user.id, &id).await?;
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM databases WHERE project_id = ? ORDER BY position")
            .bind(&id)
            .fetch_all(&mut *conn)
            .await?;
    let mut out = vec![];
    for db in ids {
        out.push(detail(&mut conn, &db, &project).await?);
    }
    Ok(Json(out))
}

/// Make a database with a Name column and an All view.
#[utoipa::path(post, path = "/projects/{id}/databases", tag = "databases", params(("id" = String, Path)),
    request_body = CreateDatabaseRequest,
    responses((status = 201, body = DatabaseDetail), (status = 400, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody, description = "Name taken in this project")))]
async fn create_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateDatabaseRequest>,
) -> AppResult<(StatusCode, Json<DatabaseDetail>)> {
    let name = clean_name(&req.name, "database", 60)?;
    let mut tx = state.db.begin().await?;
    let project = editable_project(&mut tx, &user.id, &id).await?;
    let db = match req.template.as_deref() {
        None | Some("") => {
            let db = insert_database(&mut tx, &id, &name).await?;
            insert_column(&mut tx, &db, "Name", "text", &ColumnOptions::default(), true).await?;
            db
        }
        Some(crate::parts::TEMPLATE) => crate::parts::create_parts_database(&mut tx, &id, &name).await?,
        Some(_) => {
            return Err(AppError::BadRequest("invalid_template", "The only template is parts.".into()));
        }
    };
    let out = detail(&mut tx, &db, &project).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// A database with its columns and views.
#[utoipa::path(get, path = "/databases/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, body = DatabaseDetail), (status = 404, body = crate::error::ErrorBody)))]
async fn get_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<DatabaseDetail>> {
    let mut conn = state.db.acquire().await?;
    let (_, project) = visible_database(&mut conn, &user.id, &id).await?;
    Ok(Json(detail(&mut conn, &id, &project).await?))
}

/// Rename or move a database.
#[utoipa::path(patch, path = "/databases/{id}", tag = "databases", params(("id" = String, Path)),
    request_body = UpdateDatabaseRequest,
    responses((status = 200, body = DatabaseDetail), (status = 404, body = crate::error::ErrorBody)))]
async fn update_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateDatabaseRequest>,
) -> AppResult<Json<DatabaseDetail>> {
    let mut tx = state.db.begin().await?;
    let (db, project) = editable_database(&mut tx, &user.id, &id).await?;
    if let Some(name) = &req.name {
        let name = clean_name(name, "database", 60)?;
        let taken: Option<String> = sqlx::query_scalar(
            "SELECT id FROM databases WHERE project_id = ? AND name = ? COLLATE NOCASE AND id <> ?",
        )
        .bind(&db.project_id)
        .bind(&name)
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?;
        if taken.is_some() {
            return Err(AppError::Conflict(
                "name_taken",
                format!("This project already has a database called {name}."),
            ));
        }
        sqlx::query("UPDATE databases SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(time::now())
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(after) = &req.after_id {
        let pos =
            position_after(&mut tx, "databases", "project_id", &db.project_id, after.as_deref(), Some(&id))
                .await?;
        sqlx::query("UPDATE databases SET position = ? WHERE id = ?")
            .bind(&pos)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    let out = detail(&mut tx, &id, &project).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Delete a database and every row in it.
#[utoipa::path(delete, path = "/databases/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    editable_database(&mut tx, &user.id, &id).await?;
    // Relations from other databases to these rows go with them (trigger), and
    // relation columns that pointed at this database turn into nothing to pick.
    sqlx::query("DELETE FROM databases WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Column handlers

/// Add a column at the right.
#[utoipa::path(post, path = "/databases/{id}/columns", tag = "databases", params(("id" = String, Path)),
    request_body = CreateColumnRequest,
    responses((status = 201, body = Column), (status = 400, body = crate::error::ErrorBody)))]
async fn create_column(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateColumnRequest>,
) -> AppResult<(StatusCode, Json<Column>)> {
    let name = clean_name(&req.name, "column", 60)?;
    let mut tx = state.db.begin().await?;
    editable_database(&mut tx, &user.id, &id).await?;
    let options = check_column(&mut tx, &user.id, &req.kind, req.options).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM db_columns WHERE database_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_COLUMNS {
        return Err(AppError::BadRequest("too_many_columns", "A database can have up to 60 columns.".into()));
    }
    let column = insert_column(&mut tx, &id, &name, &req.kind, &options, false).await?;
    touch(&mut tx, &id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(column)))
}

/// Rename, retype, change the choices of, or move a column. Changing the
/// type converts the values that fit and clears the rest.
#[utoipa::path(patch, path = "/db-columns/{id}", tag = "databases", params(("id" = String, Path)),
    request_body = UpdateColumnRequest,
    responses((status = 200, body = Column), (status = 400, body = crate::error::ErrorBody)))]
async fn update_column(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateColumnRequest>,
) -> AppResult<Json<Column>> {
    let mut tx = state.db.begin().await?;
    let current = column(&mut tx, &id).await?;
    editable_database(&mut tx, &user.id, &current.database_id).await?;
    let name = match &req.name {
        Some(n) => clean_name(n, "column", 60)?,
        None => current.name.clone(),
    };
    let kind = req.kind.clone().unwrap_or_else(|| current.kind.clone());
    if current.is_title && kind != "text" {
        return Err(AppError::BadRequest("title_column", "The name column stays text.".into()));
    }
    if current.role.is_some() && kind != current.kind {
        return Err(AppError::BadRequest(
            "template_column",
            format!("The Shopping list reads {}, so it keeps its type.", current.name),
        ));
    }
    let mut options = req.options.clone().unwrap_or_else(|| current.options.clone());
    if kind != current.kind && req.options.is_none() && matches!(kind.as_str(), "select" | "multi_select") {
        // Turning text into a select: its distinct values become the choices.
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT data FROM db_rows WHERE database_id = ? ORDER BY position")
                .bind(&current.database_id)
                .fetch_all(&mut *tx)
                .await?;
        let mut names: Vec<String> = options.choices.iter().map(|c| c.name.clone()).collect();
        for data in rows {
            let map: Map<String, Value> = serde_json::from_str(&data).unwrap_or_default();
            if let Some(v) = map.get(&id) {
                for part in values::display(&current, v).split(", ") {
                    let part = part.trim();
                    if !part.is_empty()
                        && part.chars().count() <= 60
                        && !names.iter().any(|n| n.eq_ignore_ascii_case(part))
                        && names.len() < 100
                    {
                        names.push(part.to_string());
                    }
                }
            }
        }
        options.choices = names
            .into_iter()
            .map(|name| {
                current.options.choices.iter().find(|c| c.name == name).cloned().unwrap_or(Choice {
                    id: String::new(),
                    name,
                    color: None,
                })
            })
            .collect();
    }
    let options = check_column(&mut tx, &user.id, &kind, options).await?;
    let updated =
        Column { name: name.clone(), kind: kind.clone(), options: options.clone(), ..current.clone() };

    // Rewrite values when the type or the choices change.
    let retype = kind != current.kind;
    let choices_changed = matches!(kind.as_str(), "select" | "multi_select")
        && options.choices.iter().map(|c| &c.id).collect::<Vec<_>>()
            != current.options.choices.iter().map(|c| &c.id).collect::<Vec<_>>();
    let retarget = kind == "relation"
        && (current.options.target != options.target
            || current.options.target_database_id != options.target_database_id);
    if retype || choices_changed {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, data FROM db_rows WHERE database_id = ?")
                .bind(&current.database_id)
                .fetch_all(&mut *tx)
                .await?;
        for (row_id, data) in rows {
            let mut map: Map<String, Value> = serde_json::from_str(&data).unwrap_or_default();
            let Some(old) = map.get(&id).cloned() else { continue };
            let new = if retype {
                values::convert(&current, &updated, &old)
            } else {
                values::clean(&updated, &old).ok().flatten().or_else(|| {
                    // A multi-select keeps the choices that are still there.
                    old.as_array()
                        .map(|list| {
                            Value::Array(
                                list.iter()
                                    .filter(|v| {
                                        options.choices.iter().any(|c| Some(c.id.as_str()) == v.as_str())
                                    })
                                    .cloned()
                                    .collect(),
                            )
                        })
                        .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
                })
            };
            match new {
                Some(v) => map.insert(id.clone(), v),
                None => map.remove(&id),
            };
            sqlx::query("UPDATE db_rows SET data = ? WHERE id = ?")
                .bind(Value::Object(map).to_string())
                .bind(&row_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    if (retype && current.kind == "relation") || retarget {
        sqlx::query("DELETE FROM db_relations WHERE column_id = ?").bind(&id).execute(&mut *tx).await?;
    }
    let pos = match &req.after_id {
        Some(after) if after.as_deref() != Some(id.as_str()) => {
            position_after(
                &mut tx,
                "db_columns",
                "database_id",
                &current.database_id,
                after.as_deref(),
                Some(&id),
            )
            .await?
        }
        _ => current.position.clone(),
    };
    sqlx::query("UPDATE db_columns SET name = ?, kind = ?, options = ?, position = ? WHERE id = ?")
        .bind(&name)
        .bind(&kind)
        .bind(serde_json::to_string(&options).unwrap_or_else(|_| "{}".into()))
        .bind(&pos)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    // Views that grouped or dated by this column may not fit it any more.
    if retype {
        for mut v in views_of(&mut tx, &current.database_id).await? {
            let before = serde_json::to_string(&v.config).unwrap_or_default();
            if v.config.group_by.as_deref() == Some(id.as_str()) && kind != "select" {
                v.config.group_by = None;
            }
            if v.config.date_column.as_deref() == Some(id.as_str()) && kind != "date" {
                v.config.date_column = None;
            }
            v.config.filters.retain(|f| f.column_id != id);
            let after = serde_json::to_string(&v.config).unwrap_or_default();
            if before != after {
                sqlx::query("UPDATE db_views SET config = ? WHERE id = ?")
                    .bind(&after)
                    .bind(&v.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    touch(&mut tx, &current.database_id).await?;
    let out = column(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Remove a column and its values. The name column can't be removed.
#[utoipa::path(delete, path = "/db-columns/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 204), (status = 400, body = crate::error::ErrorBody)))]
async fn delete_column(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let current = column(&mut tx, &id).await?;
    editable_database(&mut tx, &user.id, &current.database_id).await?;
    if current.is_title {
        return Err(AppError::BadRequest("title_column", "Every database keeps its name column.".into()));
    }
    sqlx::query("UPDATE db_rows SET data = json_remove(data, '$.\"' || ? || '\"') WHERE database_id = ?")
        .bind(&id)
        .bind(&current.database_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM db_columns WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    for mut v in views_of(&mut tx, &current.database_id).await? {
        v.config.filters.retain(|f| f.column_id != id);
        v.config.sorts.retain(|s| s.column_id != id);
        v.config.hidden.retain(|h| h != &id);
        if v.config.group_by.as_deref() == Some(id.as_str()) {
            v.config.group_by = None;
        }
        if v.config.date_column.as_deref() == Some(id.as_str()) {
            v.config.date_column = None;
        }
        sqlx::query("UPDATE db_views SET config = ? WHERE id = ?")
            .bind(serde_json::to_string(&v.config).unwrap_or_default())
            .bind(&v.id)
            .execute(&mut *tx)
            .await?;
    }
    touch(&mut tx, &current.database_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// View handlers

/// Save a new view.
#[utoipa::path(post, path = "/databases/{id}/views", tag = "databases", params(("id" = String, Path)),
    request_body = CreateViewRequest,
    responses((status = 201, body = View), (status = 400, body = crate::error::ErrorBody)))]
async fn create_view(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateViewRequest>,
) -> AppResult<(StatusCode, Json<View>)> {
    let name = clean_name(&req.name, "view", 40)?;
    let mut tx = state.db.begin().await?;
    editable_database(&mut tx, &user.id, &id).await?;
    let columns = columns_of(&mut tx, &id).await?;
    let mut config = req.config;
    // A new board or calendar picks the first column that fits.
    if req.kind == "board" && config.group_by.is_none() {
        config.group_by = columns.iter().find(|c| c.kind == "select").map(|c| c.id.clone());
    }
    if req.kind == "calendar" && config.date_column.is_none() {
        config.date_column = columns.iter().find(|c| c.kind == "date").map(|c| c.id.clone());
    }
    check_view(&req.kind, &config, &columns)?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM db_views WHERE database_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_VIEWS {
        return Err(AppError::BadRequest("too_many_views", "A database can have up to 20 views.".into()));
    }
    let view = insert_view(&mut tx, &id, &name, &req.kind, &config).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// Change a view's name, type, filters, sorts, columns, pin or place.
#[utoipa::path(patch, path = "/db-views/{id}", tag = "databases", params(("id" = String, Path)),
    request_body = UpdateViewRequest,
    responses((status = 200, body = View), (status = 400, body = crate::error::ErrorBody)))]
async fn update_view(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateViewRequest>,
) -> AppResult<Json<View>> {
    let mut tx = state.db.begin().await?;
    let current = view(&mut tx, &id).await?;
    editable_database(&mut tx, &user.id, &current.database_id).await?;
    let columns = columns_of(&mut tx, &current.database_id).await?;
    let name = match &req.name {
        Some(n) => clean_name(n, "view", 40)?,
        None => current.name.clone(),
    };
    let kind = req.kind.clone().unwrap_or_else(|| current.kind.clone());
    let mut config = req.config.clone().unwrap_or_else(|| current.config.clone());
    if kind == "board" && config.group_by.is_none() {
        config.group_by = columns.iter().find(|c| c.kind == "select").map(|c| c.id.clone());
    }
    if kind == "calendar" && config.date_column.is_none() {
        config.date_column = columns.iter().find(|c| c.kind == "date").map(|c| c.id.clone());
    }
    check_view(&kind, &config, &columns)?;
    let pos = match &req.after_id {
        Some(after) if after.as_deref() != Some(id.as_str()) => {
            position_after(
                &mut tx,
                "db_views",
                "database_id",
                &current.database_id,
                after.as_deref(),
                Some(&id),
            )
            .await?
        }
        _ => current.position.clone(),
    };
    sqlx::query("UPDATE db_views SET name = ?, kind = ?, config = ?, pinned = ?, position = ? WHERE id = ?")
        .bind(&name)
        .bind(&kind)
        .bind(serde_json::to_string(&config).unwrap_or_default())
        .bind(req.pinned.unwrap_or(current.pinned))
        .bind(&pos)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let out = view(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Remove a view. A database keeps at least one.
#[utoipa::path(delete, path = "/db-views/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 204), (status = 409, body = crate::error::ErrorBody, description = "Last view")))]
async fn delete_view(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let current = view(&mut tx, &id).await?;
    editable_database(&mut tx, &user.id, &current.database_id).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM db_views WHERE database_id = ?")
        .bind(&current.database_id)
        .fetch_one(&mut *tx)
        .await?;
    if count <= 1 {
        return Err(AppError::Conflict("last_view", "A database keeps at least one view.".into()));
    }
    sqlx::query("DELETE FROM db_views WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Row handlers

/// A database's rows. With `view`, its filters and sorts apply.
#[utoipa::path(get, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path), RowsQuery),
    responses((status = 200, body = Vec<Row>), (status = 404, body = crate::error::ErrorBody)))]
async fn list_rows(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<RowsQuery>,
) -> AppResult<Json<Vec<Row>>> {
    let mut conn = state.db.acquire().await?;
    visible_database(&mut conn, &user.id, &id).await?;
    let v = match &q.view {
        Some(view_id) => {
            let v = view(&mut conn, view_id).await?;
            if v.database_id != id {
                return Err(AppError::NotFound);
            }
            Some(v)
        }
        None => None,
    };
    Ok(Json(query_rows(&mut conn, &user.id, &id, v.as_ref(), q.q.as_deref()).await?))
}

/// Add a row.
#[utoipa::path(post, path = "/databases/{id}/rows", tag = "databases", params(("id" = String, Path)),
    request_body = CreateRowRequest,
    responses((status = 201, body = Row), (status = 400, body = crate::error::ErrorBody)))]
async fn create_row(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateRowRequest>,
) -> AppResult<(StatusCode, Json<Row>)> {
    let mut tx = state.db.begin().await?;
    let (db, _) = editable_database(&mut tx, &user.id, &id).await?;
    let row_id = insert_row(&mut tx, &user.id, &db.project_id, &db.id, &req).await?;
    let row = one_row(&mut tx, &user.id, &row_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// Change some cells, replace some relations, or move a row.
#[utoipa::path(patch, path = "/db-rows/{id}", tag = "databases", params(("id" = String, Path)),
    request_body = UpdateRowRequest,
    responses((status = 200, body = Row), (status = 400, body = crate::error::ErrorBody)))]
async fn update_row(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRowRequest>,
) -> AppResult<Json<Row>> {
    let mut tx = state.db.begin().await?;
    let (database_id, data, position): (String, String, String) =
        sqlx::query_as("SELECT database_id, data, position FROM db_rows WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound)?;
    let (db, _) = editable_database(&mut tx, &user.id, &database_id).await?;
    let columns = columns_of(&mut tx, &database_id).await?;
    let mut map: Map<String, Value> = serde_json::from_str(&data).unwrap_or_default();
    // A Parts database fills in Ordered on and Arrived on as Status moves.
    let parts = crate::parts::database_roles(&mut tx, &database_id).await?;
    let old_stage = parts.as_ref().map(|r| crate::parts::stage_of(r, &map)).unwrap_or_default();
    if let Some(values) = &req.values {
        clean_values(&mut tx, &db.project_id, &columns, values, &mut map).await?;
    }
    if let Some(roles) = &parts {
        let today = crate::parts::today_for(&mut tx, &user.id).await?;
        crate::parts::fill_dates(roles, &old_stage, &mut map, &today);
    }
    let new_stage = parts.as_ref().map(|r| crate::parts::stage_of(r, &map)).unwrap_or_default();
    if let Some(relations) = &req.relations {
        for (column_id, targets) in relations {
            let column = columns.iter().find(|c| &c.id == column_id).ok_or_else(|| {
                AppError::BadRequest("invalid_column", "A relation names a column that isn't there.".into())
            })?;
            check_targets(&mut tx, &user.id, column, targets).await?;
            set_relations(&mut tx, &id, column_id, targets).await?;
        }
    }
    let pos = match &req.after_id {
        Some(after) if after.as_deref() != Some(id.as_str()) => {
            position_after(&mut tx, "db_rows", "database_id", &database_id, after.as_deref(), Some(&id))
                .await?
        }
        _ => position,
    };
    sqlx::query("UPDATE db_rows SET data = ?, position = ?, updated_at = ? WHERE id = ?")
        .bind(Value::Object(map).to_string())
        .bind(&pos)
        .bind(time::now())
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    touch(&mut tx, &database_id).await?;
    if parts.is_some() && crate::parts::arrived(&old_stage, &new_stage) {
        crate::parts::after_arrival(&mut tx, &user.id, &id).await?;
    }
    let row = one_row(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(row))
}

/// Delete a row.
#[utoipa::path(delete, path = "/db-rows/{id}", tag = "databases", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_row(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let database_id: String = sqlx::query_scalar("SELECT database_id FROM db_rows WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    editable_database(&mut tx, &user.id, &database_id).await?;
    sqlx::query("DELETE FROM db_rows WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    touch(&mut tx, &database_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rows that link to a task, grouped by database. Databases in the task's
/// project with a relation column to tasks come back even with no rows yet,
/// so you can add one.
#[utoipa::path(get, path = "/tasks/{id}/rows", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, body = Vec<LinkedRows>), (status = 404, body = crate::error::ErrorBody)))]
async fn task_rows(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<LinkedRows>>> {
    let mut conn = state.db.acquire().await?;
    let project_id: String = sqlx::query_scalar(
        "SELECT t.project_id FROM tasks t JOIN project_access a ON a.project_id = t.project_id
         WHERE t.id = ? AND a.user_id = ?",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    // Relation columns to tasks: in this project's databases, or with rows
    // that already point here.
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.id, c.database_id FROM db_columns c JOIN databases d ON d.id = c.database_id
         JOIN project_access a ON a.project_id = d.project_id AND a.user_id = ?
         WHERE c.kind = 'relation' AND json_extract(c.options, '$.target') = 'task'
           AND (d.project_id = ? OR EXISTS (SELECT 1 FROM db_relations x WHERE x.column_id = c.id
                AND x.target_kind = 'task' AND x.target_id = ?))
         ORDER BY d.position, c.position",
    )
    .bind(&user.id)
    .bind(&project_id)
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;
    let refs = all_refs(&mut conn, &user.id).await?;
    let mut out: Vec<LinkedRows> = vec![];
    for (column_id, database_id) in columns {
        if out.iter().any(|l| l.database.id == database_id) {
            continue;
        }
        let Some(r) = refs.iter().find(|r| r.id == database_id) else { continue };
        let records = sqlx::query_as::<_, RowRecord>(sqlx::AssertSqlSafe(format!(
            "{ROW_SELECT} WHERE id IN (SELECT row_id FROM db_relations WHERE column_id = ? AND target_kind = 'task'
             AND target_id = ?) ORDER BY position"
        )))
        .bind(&column_id)
        .bind(&id)
        .fetch_all(&mut *conn)
        .await?;
        let rows = build_rows(&mut conn, &user.id, records).await?;
        out.push(LinkedRows {
            database: DatabaseRef {
                id: r.id.clone(),
                name: r.name.clone(),
                project_id: r.project_id.clone(),
                project_title: r.project_title.clone(),
                project_color: r.project_color.clone(),
                views: r.views.clone(),
            },
            columns: columns_of(&mut conn, &database_id).await?,
            rows,
            column_id,
        });
    }
    Ok(Json(out))
}
