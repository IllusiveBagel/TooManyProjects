//! Parts and purchases (phase 4d): the Parts database template, one Shopping
//! list across every project, spend totals, and tasks that wait on parts.
//!
//! A Parts database is an ordinary database with `template = 'parts'`. Its
//! template columns carry a `role`, so people can rename them, and its Status
//! choices keep fixed ids (`need`, `ordered`, `shipped`, `arrived`, `fitted`),
//! so renaming a choice doesn't change what it means.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqliteConnection;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    calendar,
    databases::{
        self, Choice, ColumnOptions, Filter, Sort, ViewConfig, insert_column, insert_database, insert_view,
        placeholders,
    },
    error::{AppError, AppResult},
    projects::{via, visible_project},
    tasks::log_task,
    time,
};

pub const TEMPLATE: &str = "parts";

/// The Status stages, in order. Each is also its choice's id.
pub const STAGES: &[(&str, &str)] = &[
    ("need", "Need"),
    ("ordered", "Ordered"),
    ("shipped", "Shipped"),
    ("arrived", "Arrived"),
    ("fitted", "Fitted"),
];

/// Stages that count as money spent.
fn is_spent(stage: &str) -> bool {
    matches!(stage, "ordered" | "shipped" | "arrived" | "fitted")
}

/// Stages where the part is in hand.
fn is_in(stage: &str) -> bool {
    matches!(stage, "arrived" | "fitted")
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(shopping_list))
        .routes(routes!(mark_parts))
        .routes(routes!(project_spend))
        .routes(routes!(get_instance_settings, update_instance_settings))
}

// ---------------------------------------------------------------------------
// Instance settings

const DEFAULT_CURRENCY: &str = "GBP";

/// Settings for the whole install.
#[derive(Serialize, ToSchema)]
pub struct InstanceSettings {
    /// ISO code for every price in Parts databases and the Shopping list.
    pub currency: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateInstanceSettings {
    pub currency: Option<String>,
}

pub async fn currency(conn: &mut SqliteConnection) -> AppResult<String> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = 'currency'")
        .fetch_optional(conn)
        .await?;
    Ok(v.unwrap_or_else(|| DEFAULT_CURRENCY.into()))
}

fn clean_currency(code: &str) -> AppResult<String> {
    let code = code.trim().to_uppercase();
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::BadRequest(
            "invalid_currency",
            "A currency is a 3-letter code such as GBP.".into(),
        ));
    }
    Ok(code)
}

/// Settings for the whole install.
#[utoipa::path(get, path = "/instance-settings", tag = "parts",
    responses((status = 200, body = InstanceSettings)))]
async fn get_instance_settings(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
) -> AppResult<Json<InstanceSettings>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(InstanceSettings { currency: currency(&mut conn).await? }))
}

/// Change settings for the whole install. Owner only. Changing the currency
/// also changes the Unit cost column of every Parts database.
#[utoipa::path(patch, path = "/instance-settings", tag = "parts", request_body = UpdateInstanceSettings,
    responses((status = 200, body = InstanceSettings), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody)))]
async fn update_instance_settings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<UpdateInstanceSettings>,
) -> AppResult<Json<InstanceSettings>> {
    if !user.is_owner() {
        return Err(AppError::Forbidden("Only the owner can change this.".into()));
    }
    let mut tx = state.db.begin().await?;
    if let Some(code) = &req.currency {
        let code = clean_currency(code)?;
        sqlx::query(
            "INSERT INTO instance_settings (key, value) VALUES ('currency', ?)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(&code)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE db_columns SET options = json_set(options, '$.currency', ?)
             WHERE role = 'unit_cost' AND kind = 'currency'",
        )
        .bind(&code)
        .execute(&mut *tx)
        .await?;
    }
    let out = InstanceSettings { currency: currency(&mut tx).await? };
    tx.commit().await?;
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// The template

/// Make a Parts database: Item, Qty, Unit cost, Supplier, Link, Status, For
/// task, Ordered on, Arrived on and Notes, with All, By status and Gallery
/// views.
pub async fn create_parts_database(
    conn: &mut SqliteConnection,
    project_id: &str,
    name: &str,
) -> AppResult<String> {
    let id = insert_database(&mut *conn, project_id, name).await?;
    sqlx::query("UPDATE databases SET template = ? WHERE id = ?")
        .bind(TEMPLATE)
        .bind(&id)
        .execute(&mut *conn)
        .await?;
    let money = currency(&mut *conn).await?;
    let status_options = ColumnOptions {
        choices: STAGES
            .iter()
            .zip(["slot-1", "slot-6", "slot-3", "slot-5", "slot-2"])
            .map(|((id, name), color)| Choice {
                id: id.to_string(),
                name: name.to_string(),
                color: Some(color.into()),
            })
            .collect(),
        ..Default::default()
    };
    let plain = ColumnOptions::default();
    let columns: Vec<(&str, &str, ColumnOptions, Option<&str>)> = vec![
        ("Item", "text", plain.clone(), None),
        ("Qty", "number", plain.clone(), Some("qty")),
        (
            "Unit cost",
            "currency",
            ColumnOptions { currency: Some(money), ..Default::default() },
            Some("unit_cost"),
        ),
        ("Supplier", "text", plain.clone(), Some("supplier")),
        ("Link", "url", plain.clone(), Some("link")),
        ("Status", "select", status_options, Some("status")),
        (
            "For task",
            "relation",
            ColumnOptions { target: Some("task".into()), ..Default::default() },
            Some("for_task"),
        ),
        ("Ordered on", "date", plain.clone(), Some("ordered_on")),
        ("Arrived on", "date", plain.clone(), Some("arrived_on")),
        ("Notes", "text", plain, None),
    ];
    let mut status_id = String::new();
    let mut hidden = vec![];
    for (i, (col_name, kind, options, role)) in columns.into_iter().enumerate() {
        let col = insert_column(&mut *conn, &id, col_name, kind, &options, i == 0).await?;
        if let Some(role) = role {
            sqlx::query("UPDATE db_columns SET role = ? WHERE id = ?")
                .bind(role)
                .bind(&col.id)
                .execute(&mut *conn)
                .await?;
        }
        if role == Some("status") {
            status_id = col.id.clone();
        }
        if matches!(role, Some("ordered_on" | "arrived_on")) || col_name == "Notes" {
            hidden.push(col.id.clone());
        }
    }
    // The All view leaves out the dates and notes, which the row dialog shows.
    let all: String = sqlx::query_scalar("SELECT id FROM db_views WHERE database_id = ?")
        .bind(&id)
        .fetch_one(&mut *conn)
        .await?;
    let config = ViewConfig { hidden: hidden.clone(), ..Default::default() };
    sqlx::query("UPDATE db_views SET config = ? WHERE id = ?")
        .bind(serde_json::to_string(&config).unwrap_or_else(|_| "{}".into()))
        .bind(&all)
        .execute(&mut *conn)
        .await?;
    let board =
        ViewConfig { group_by: Some(status_id.clone()), hidden: hidden.clone(), ..Default::default() };
    insert_view(&mut *conn, &id, "By status", "board", &board).await?;
    let to_buy = ViewConfig {
        filters: vec![Filter {
            column_id: status_id.clone(),
            op: "is".into(),
            value: Some(Value::from("need")),
        }],
        sorts: vec![Sort { column_id: status_id, desc: false }],
        hidden,
        ..Default::default()
    };
    insert_view(&mut *conn, &id, "To buy", "table", &to_buy).await?;
    insert_view(&mut *conn, &id, "Gallery", "gallery", &ViewConfig::default()).await?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Reading parts

/// A task a part is for.
#[derive(Clone, Serialize, ToSchema)]
pub struct PartTask {
    pub id: String,
    pub number: i64,
    pub title: String,
}

/// One row of a Parts database, read through its template columns.
#[derive(Clone, Serialize, ToSchema)]
pub struct Part {
    pub row_id: String,
    pub database_id: String,
    pub database_name: String,
    pub project_id: String,
    pub project_title: String,
    pub project_color: String,
    pub item: String,
    /// 1 when the Qty cell is empty.
    pub qty: f64,
    pub unit_cost: Option<f64>,
    /// Qty times unit cost.
    pub total: f64,
    pub supplier: Option<String>,
    pub link: Option<String>,
    /// `need`, `ordered`, `shipped`, `arrived` or `fitted`; empty when Status
    /// is empty or set to a choice someone added.
    pub stage: String,
    pub ordered_on: Option<String>,
    pub arrived_on: Option<String>,
    pub tasks: Vec<PartTask>,
    /// False when the project is shared with you to view.
    pub can_edit: bool,
}

/// Which projects' parts to read.
enum Scope<'a> {
    /// Every project you can change that isn't archived: the Shopping list.
    Shopping,
    /// These projects, whatever your role: spend on an overview.
    Projects(&'a [String]),
}

type Roles = HashMap<String, HashMap<String, String>>;

/// Template columns of some databases: database id to role to column id.
async fn roles_of(
    conn: &mut SqliteConnection,
    database_ids: &[String],
) -> AppResult<(Roles, HashMap<String, String>)> {
    let mut roles: Roles = HashMap::new();
    let mut titles = HashMap::new();
    for chunk in database_ids.chunks(500) {
        let sql = format!(
            "SELECT database_id, id, role, is_title FROM db_columns WHERE database_id IN ({})
               AND (role IS NOT NULL OR is_title = 1)",
            placeholders(chunk.len())
        );
        let mut q = sqlx::query_as::<_, (String, String, Option<String>, bool)>(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            q = q.bind(id);
        }
        for (db, col, role, is_title) in q.fetch_all(&mut *conn).await? {
            if is_title {
                titles.insert(db.clone(), col.clone());
            }
            if let Some(role) = role {
                roles.entry(db).or_default().insert(role, col);
            }
        }
    }
    Ok((roles, titles))
}

async fn load_parts(conn: &mut SqliteConnection, user_id: &str, scope: Scope<'_>) -> AppResult<Vec<Part>> {
    let mut sql = String::from(
        "SELECT r.id, r.data, d.id, d.name, p.id, p.title, p.color, a.role FROM db_rows r
         JOIN databases d ON d.id = r.database_id
         JOIN projects p ON p.id = d.project_id
         JOIN project_access a ON a.project_id = p.id AND a.user_id = ?
         WHERE d.template = 'parts'",
    );
    let projects: &[String] = match &scope {
        Scope::Shopping => {
            sql.push_str(" AND a.role <> 'viewer' AND p.status <> 'archived'");
            &[]
        }
        Scope::Projects(ids) => {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            sql.push_str(&format!(" AND p.id IN ({})", placeholders(ids.len())));
            ids
        }
    };
    sql.push_str(" ORDER BY p.title COLLATE NOCASE, d.position, r.position");
    let mut q = sqlx::query_as::<_, (String, String, String, String, String, String, String, String)>(
        sqlx::AssertSqlSafe(sql),
    )
    .bind(user_id);
    for p in projects {
        q = q.bind(p);
    }
    let records = q.fetch_all(&mut *conn).await?;
    let mut db_ids: Vec<String> = records.iter().map(|r| r.2.clone()).collect();
    db_ids.sort();
    db_ids.dedup();
    let (roles, titles) = roles_of(&mut *conn, &db_ids).await?;

    // The tasks each part is for, among those you can see.
    let row_ids: Vec<String> = records.iter().map(|r| r.0.clone()).collect();
    let mut tasks: HashMap<String, Vec<PartTask>> = HashMap::new();
    for chunk in row_ids.chunks(500) {
        let sql = format!(
            "SELECT x.row_id, t.id, t.number, t.title FROM db_relations x
             JOIN db_columns c ON c.id = x.column_id AND c.role = 'for_task'
             JOIN tasks t ON t.id = x.target_id
             JOIN project_access a ON a.project_id = t.project_id AND a.user_id = ?
             WHERE x.target_kind = 'task' AND x.row_id IN ({}) ORDER BY x.position",
            placeholders(chunk.len())
        );
        let mut q =
            sqlx::query_as::<_, (String, String, i64, String)>(sqlx::AssertSqlSafe(sql)).bind(user_id);
        for id in chunk {
            q = q.bind(id);
        }
        for (row, id, number, title) in q.fetch_all(&mut *conn).await? {
            tasks.entry(row).or_default().push(PartTask { id, number, title });
        }
    }

    let empty = HashMap::new();
    Ok(records
        .into_iter()
        .map(|(row_id, data, database_id, database_name, project_id, project_title, project_color, role)| {
            let map: Map<String, Value> = serde_json::from_str(&data).unwrap_or_default();
            let cols = roles.get(&database_id).unwrap_or(&empty);
            let get = |role: &str| cols.get(role).and_then(|c| map.get(c));
            let text = |role: &str| {
                get(role).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
            };
            let num = |role: &str| get(role).and_then(Value::as_f64);
            let item = titles
                .get(&database_id)
                .and_then(|c| map.get(c))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Untitled")
                .to_string();
            let qty = num("qty").unwrap_or(1.0);
            let unit_cost = num("unit_cost");
            let stage = text("status").filter(|s| STAGES.iter().any(|(id, _)| id == s)).unwrap_or_default();
            Part {
                total: round2(qty * unit_cost.unwrap_or(0.0)),
                tasks: tasks.remove(&row_id).unwrap_or_default(),
                row_id,
                database_id,
                database_name,
                project_id,
                project_title,
                project_color,
                item,
                qty,
                unit_cost,
                supplier: text("supplier"),
                link: text("link"),
                stage,
                ordered_on: text("ordered_on"),
                arrived_on: text("arrived_on"),
                can_edit: role != "viewer",
            }
        })
        .collect())
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// The Shopping list

/// Parts from one supplier.
#[derive(Serialize, ToSchema)]
pub struct SupplierGroup {
    /// As first written; empty for parts with no supplier.
    pub supplier: String,
    pub total: f64,
    pub parts: Vec<Part>,
}

#[derive(Serialize, ToSchema)]
pub struct ShoppingList {
    /// Today in the viewer's time zone, `YYYY-MM-DD`, for "Mark ordered".
    pub date: String,
    pub currency: String,
    /// Parts in Need, by supplier, biggest spend first and no supplier last.
    pub need: Vec<SupplierGroup>,
    /// Ordered and Shipped parts, by supplier.
    pub on_the_way: Vec<SupplierGroup>,
    /// Parts that arrived this month, newest first.
    pub arrived: Vec<Part>,
    pub to_buy: f64,
    pub on_the_way_total: f64,
    /// Projects with parts in Need.
    pub project_count: i64,
}

/// Group parts by supplier, ignoring case and spaces at the ends.
fn by_supplier(parts: Vec<Part>) -> Vec<SupplierGroup> {
    let mut groups: Vec<SupplierGroup> = vec![];
    for part in parts {
        let key = part.supplier.clone().unwrap_or_default();
        match groups.iter_mut().find(|g| g.supplier.to_lowercase() == key.to_lowercase()) {
            Some(g) => {
                g.total = round2(g.total + part.total);
                g.parts.push(part);
            }
            None => groups.push(SupplierGroup { supplier: key, total: part.total, parts: vec![part] }),
        }
    }
    groups.sort_by(|a, b| {
        a.supplier
            .is_empty()
            .cmp(&b.supplier.is_empty())
            .then(b.total.partial_cmp(&a.total).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.supplier.to_lowercase().cmp(&b.supplier.to_lowercase()))
    });
    groups
}

#[derive(Deserialize, IntoParams)]
pub struct ShoppingQuery {
    /// The viewer's IANA time zone, which says what today and this month are.
    pub tz: Option<String>,
}

async fn local_today(conn: &mut SqliteConnection, user_id: &str, tz: Option<&str>) -> AppResult<NaiveDate> {
    let tz = match tz.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => calendar::parse_tz(t)?,
        None => calendar::parse_tz(&calendar::usual_tz(conn, user_id).await?).unwrap_or(chrono_tz::UTC),
    };
    Ok(Utc::now().with_timezone(&tz).date_naive())
}

pub async fn build_shopping_list(
    conn: &mut SqliteConnection,
    user_id: &str,
    tz: Option<&str>,
) -> AppResult<ShoppingList> {
    let today = local_today(&mut *conn, user_id, tz).await?;
    let month = today.format("%Y-%m").to_string();
    let parts = load_parts(&mut *conn, user_id, Scope::Shopping).await?;
    let mut need = vec![];
    let mut on_the_way = vec![];
    let mut arrived = vec![];
    for p in parts {
        match p.stage.as_str() {
            "need" => need.push(p),
            "ordered" | "shipped" => on_the_way.push(p),
            "arrived" | "fitted" if p.arrived_on.as_deref().is_some_and(|d| d.starts_with(&month)) => {
                arrived.push(p)
            }
            _ => {}
        }
    }
    arrived.sort_by(|a, b| b.arrived_on.cmp(&a.arrived_on));
    let mut projects: Vec<&str> = need.iter().map(|p| p.project_id.as_str()).collect();
    projects.sort();
    projects.dedup();
    let project_count = projects.len() as i64;
    let to_buy = round2(need.iter().map(|p| p.total).sum());
    let on_the_way_total = round2(on_the_way.iter().map(|p| p.total).sum());
    Ok(ShoppingList {
        date: today.format("%Y-%m-%d").to_string(),
        currency: currency(&mut *conn).await?,
        need: by_supplier(need),
        on_the_way: by_supplier(on_the_way),
        arrived,
        to_buy,
        on_the_way_total,
        project_count,
    })
}

/// Every part to buy across your projects, grouped by supplier, plus what's on
/// the way and what arrived this month.
#[utoipa::path(get, path = "/shopping", tag = "parts", params(ShoppingQuery),
    responses((status = 200, body = ShoppingList), (status = 400, body = crate::error::ErrorBody)))]
async fn shopping_list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ShoppingQuery>,
) -> AppResult<Json<ShoppingList>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(build_shopping_list(&mut conn, &user.id, q.tz.as_deref()).await?))
}

/// How much there is to buy, for the Today screen.
#[derive(Serialize, ToSchema)]
pub struct ShoppingSummary {
    pub need_count: i64,
    pub to_buy: f64,
    pub currency: String,
}

pub async fn summary(conn: &mut SqliteConnection, user_id: &str) -> AppResult<ShoppingSummary> {
    let need: Vec<Part> = load_parts(&mut *conn, user_id, Scope::Shopping)
        .await?
        .into_iter()
        .filter(|p| p.stage == "need")
        .collect();
    Ok(ShoppingSummary {
        need_count: need.len() as i64,
        to_buy: round2(need.iter().map(|p| p.total).sum()),
        currency: currency(conn).await?,
    })
}

// ---------------------------------------------------------------------------
// Moving parts along

#[derive(Deserialize, ToSchema)]
pub struct MarkPartsRequest {
    /// Rows of Parts databases.
    pub row_ids: Vec<String>,
    /// `need`, `ordered`, `shipped`, `arrived` or `fitted`.
    pub stage: String,
    /// The day it happened, `YYYY-MM-DD`; today in your time zone when left out.
    pub date: Option<String>,
    pub tz: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MarkPartsResult {
    pub moved: i64,
    /// Tasks whose last part just arrived, which got a "Parts arrived" comment.
    pub tasks_ready: Vec<PartTask>,
}

/// Fill Ordered on or Arrived on when a part reaches that stage, unless set.
/// `map` is the row's values after the change.
pub fn fill_dates(
    roles: &HashMap<String, String>,
    old_stage: &str,
    map: &mut Map<String, Value>,
    date: &str,
) {
    let stage =
        roles.get("status").and_then(|c| map.get(c)).and_then(Value::as_str).unwrap_or("").to_string();
    if stage == old_stage {
        return;
    }
    let mut set = |role: &str| {
        if let Some(col) = roles.get(role)
            && map.get(col).and_then(Value::as_str).is_none_or(str::is_empty)
        {
            map.insert(col.clone(), Value::String(date.to_string()));
        }
    };
    if is_spent(&stage) {
        set("ordered_on");
    }
    if is_in(&stage) {
        set("arrived_on");
    }
}

/// The template columns of one database (role to column id), and whether it
/// is a Parts database at all.
pub async fn database_roles(
    conn: &mut SqliteConnection,
    database_id: &str,
) -> AppResult<Option<HashMap<String, String>>> {
    let template: Option<String> = sqlx::query_scalar("SELECT template FROM databases WHERE id = ?")
        .bind(database_id)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    if template.as_deref() != Some(TEMPLATE) {
        return Ok(None);
    }
    let (mut roles, _) = roles_of(conn, &[database_id.to_string()]).await?;
    Ok(Some(roles.remove(database_id).unwrap_or_default()))
}

/// The stage a row's values say, or empty.
pub fn stage_of(roles: &HashMap<String, String>, map: &Map<String, Value>) -> String {
    roles.get("status").and_then(|c| map.get(c)).and_then(Value::as_str).unwrap_or("").to_string()
}

/// After a part arrives: each task it is for whose parts are now all in gets
/// a "Parts arrived" comment. Returns those tasks.
pub async fn after_arrival(
    conn: &mut SqliteConnection,
    user_id: &str,
    row_id: &str,
) -> AppResult<Vec<PartTask>> {
    let tasks: Vec<(String, String, i64, String)> = sqlx::query_as(
        "SELECT t.id, t.project_id, t.number, t.title FROM db_relations x
         JOIN db_columns c ON c.id = x.column_id AND c.role = 'for_task'
         JOIN tasks t ON t.id = x.target_id
         WHERE x.row_id = ? AND x.target_kind = 'task'",
    )
    .bind(row_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut ready = vec![];
    for (task_id, project_id, number, title) in tasks {
        let parts: Vec<(String, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {TITLE_SQL}, {STAGE_SQL} FROM db_relations x
             JOIN db_columns fc ON fc.id = x.column_id AND fc.role = 'for_task'
             JOIN db_rows r ON r.id = x.row_id
             WHERE x.target_kind = 'task' AND x.target_id = ? ORDER BY r.position"
        )))
        .bind(&task_id)
        .fetch_all(&mut *conn)
        .await?;
        if parts.is_empty() || parts.iter().any(|(_, s)| !is_in(s.as_deref().unwrap_or(""))) {
            continue;
        }
        let names: Vec<String> = parts.into_iter().map(|(n, _)| n).collect();
        let body = if names.len() == 1 {
            format!("Parts arrived: {}.", names[0])
        } else {
            format!("Parts arrived: all {} ({}).", names.len(), names.join(", "))
        };
        let now = time::now();
        sqlx::query(
            "INSERT INTO task_comments (id, task_id, user_id, body, via, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(time::new_id())
        .bind(&task_id)
        .bind(user_id)
        .bind(&body)
        .bind(via())
        .bind(&now)
        .execute(&mut *conn)
        .await?;
        log_task(
            &mut *conn,
            &project_id,
            &task_id,
            user_id,
            "task.commented",
            serde_json::json!({ "title": title }),
        )
        .await?;
        ready.push(PartTask { id: task_id, number, title });
    }
    Ok(ready)
}

/// True when a part has just come into hand.
pub fn arrived(old: &str, new: &str) -> bool {
    is_in(new) && !is_in(old)
}

/// Today for someone, in the time zone they last planned time in.
pub async fn today_for(conn: &mut SqliteConnection, user_id: &str) -> AppResult<String> {
    Ok(local_today(conn, user_id, None).await?.format("%Y-%m-%d").to_string())
}

/// A part's name (its row's title) as SQL over `r`.
const TITLE_SQL: &str = "COALESCE((SELECT json_extract(r.data, '$.\"' || tc.id || '\"') FROM db_columns tc
    WHERE tc.database_id = r.database_id AND tc.is_title = 1), 'Untitled')";
/// A part's stage as SQL over `r`, NULL when its database has no Status.
const STAGE_SQL: &str = "(SELECT json_extract(r.data, '$.\"' || sc.id || '\"') FROM db_columns sc
    WHERE sc.database_id = r.database_id AND sc.role = 'status')";

/// Move parts to a stage, filling in the date. Used by "Mark ordered" and the
/// On the way section's buttons.
pub async fn mark(
    conn: &mut SqliteConnection,
    user_id: &str,
    row_ids: &[String],
    stage: &str,
    date: &str,
) -> AppResult<MarkPartsResult> {
    if !STAGES.iter().any(|(id, _)| *id == stage) {
        return Err(AppError::BadRequest(
            "invalid_stage",
            "A stage is need, ordered, shipped, arrived or fitted.".into(),
        ));
    }
    if row_ids.is_empty() || row_ids.len() > 500 {
        return Err(AppError::BadRequest("invalid_rows", "Pick 1 to 500 parts.".into()));
    }
    let mut ready: Vec<PartTask> = vec![];
    let mut moved = 0;
    for row_id in row_ids {
        let (database_id, data): (String, String) =
            sqlx::query_as("SELECT database_id, data FROM db_rows WHERE id = ?")
                .bind(row_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(AppError::NotFound)?;
        databases::editable_database(&mut *conn, user_id, &database_id).await?;
        let roles = database_roles(&mut *conn, &database_id)
            .await?
            .ok_or_else(|| AppError::BadRequest("not_parts", "That row isn't in a Parts database.".into()))?;
        let status = roles.get("status").ok_or_else(|| {
            AppError::BadRequest("no_status", "That Parts database has lost its Status column.".into())
        })?;
        let columns = databases::columns_of(&mut *conn, &database_id).await?;
        let has_choice = columns
            .iter()
            .find(|c| &c.id == status)
            .is_some_and(|c| c.options.choices.iter().any(|ch| ch.id == stage));
        if !has_choice {
            return Err(AppError::BadRequest(
                "no_status",
                "That Parts database's Status column has lost that choice.".into(),
            ));
        }
        let mut map: Map<String, Value> = serde_json::from_str(&data).unwrap_or_default();
        let old = stage_of(&roles, &map);
        if old == stage {
            continue;
        }
        map.insert(status.clone(), Value::String(stage.to_string()));
        fill_dates(&roles, &old, &mut map, date);
        let now = time::now();
        sqlx::query("UPDATE db_rows SET data = ?, updated_at = ? WHERE id = ?")
            .bind(Value::Object(map).to_string())
            .bind(&now)
            .bind(row_id)
            .execute(&mut *conn)
            .await?;
        sqlx::query("UPDATE databases SET updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&database_id)
            .execute(&mut *conn)
            .await?;
        moved += 1;
        if is_in(stage) && !is_in(&old) {
            for t in after_arrival(&mut *conn, user_id, row_id).await? {
                if !ready.iter().any(|r| r.id == t.id) {
                    ready.push(t);
                }
            }
        }
    }
    Ok(MarkPartsResult { moved, tasks_ready: ready })
}

/// Move parts to a stage together, such as ticking parts on the Shopping
/// list and pressing Mark ordered.
#[utoipa::path(post, path = "/shopping/mark", tag = "parts", request_body = MarkPartsRequest,
    responses((status = 200, body = MarkPartsResult), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
async fn mark_parts(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<MarkPartsRequest>,
) -> AppResult<Json<MarkPartsResult>> {
    let mut tx = state.db.begin().await?;
    let date = match &req.date {
        Some(d) => {
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map_err(|_| AppError::BadRequest("invalid_date", "A date is YYYY-MM-DD.".into()))?;
            d.clone()
        }
        None => local_today(&mut tx, &user.id, req.tz.as_deref()).await?.format("%Y-%m-%d").to_string(),
    };
    let out = mark(&mut tx, &user.id, &req.row_ids, &req.stage, &date).await?;
    tx.commit().await?;
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Spend

/// A task still waiting on parts.
#[derive(Serialize, ToSchema)]
pub struct WaitingTask {
    pub id: String,
    pub number: i64,
    pub title: String,
    /// Every part linked to it.
    pub parts: i64,
    /// Of those, how many sit in each stage that isn't in hand yet.
    pub need: i64,
    pub ordered: i64,
    pub shipped: i64,
}

#[derive(Serialize, ToSchema)]
pub struct FamilySpend {
    pub spent: f64,
    pub to_buy: f64,
    /// Projects spawned from this one (and from those) that have parts.
    pub projects: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectSpend {
    pub currency: String,
    /// Parts in any Parts database of the project.
    pub part_count: i64,
    /// Ordered, shipped, arrived and fitted parts.
    pub spent: f64,
    /// Parts in Need.
    pub to_buy: f64,
    pub need_count: i64,
    /// With its spawned projects, when any of them have parts.
    pub family: Option<FamilySpend>,
    /// The project's tasks waiting on parts.
    pub waiting: Vec<WaitingTask>,
}

fn totals(parts: &[Part]) -> (f64, f64) {
    let spent = parts.iter().filter(|p| is_spent(&p.stage)).map(|p| p.total).sum();
    let to_buy = parts.iter().filter(|p| p.stage == "need").map(|p| p.total).sum();
    (round2(spent), round2(to_buy))
}

pub async fn spend(conn: &mut SqliteConnection, user_id: &str, project_id: &str) -> AppResult<ProjectSpend> {
    // The project and everything spawned from it that you can see.
    let family: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE fam(id) AS (SELECT ? UNION SELECT l.target_id FROM project_links l JOIN fam f
             ON l.source_id = f.id WHERE l.kind = 'spawned')
         SELECT f.id FROM fam f JOIN project_access a ON a.project_id = f.id AND a.user_id = ?",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let all = load_parts(&mut *conn, user_id, Scope::Projects(&family)).await?;
    let (own, others): (Vec<Part>, Vec<Part>) = all.into_iter().partition(|p| p.project_id == project_id);
    let (spent, to_buy) = totals(&own);
    let family = (!others.is_empty()).then(|| {
        let (s, b) = totals(&others);
        let mut projects: Vec<String> = others.iter().map(|p| p.project_title.clone()).collect();
        projects.dedup();
        FamilySpend { spent: round2(spent + s), to_buy: round2(to_buy + b), projects }
    });

    let mut waiting: Vec<WaitingTask> = vec![];
    for p in &own {
        for t in &p.tasks {
            let w = match waiting.iter_mut().position(|w| w.id == t.id) {
                Some(i) => &mut waiting[i],
                None => {
                    waiting.push(WaitingTask {
                        id: t.id.clone(),
                        number: t.number,
                        title: t.title.clone(),
                        parts: 0,
                        need: 0,
                        ordered: 0,
                        shipped: 0,
                    });
                    waiting.last_mut().expect("just pushed")
                }
            };
            w.parts += 1;
            match p.stage.as_str() {
                "need" | "" => w.need += 1,
                "ordered" => w.ordered += 1,
                "shipped" => w.shipped += 1,
                _ => {}
            }
        }
    }
    // Only tasks still waiting, and only open ones.
    let mut open = vec![];
    for w in waiting {
        if w.need + w.ordered + w.shipped == 0 {
            continue;
        }
        let done: Option<String> = sqlx::query_scalar(
            "SELECT s.category FROM tasks t JOIN statuses s ON s.id = t.status_id WHERE t.id = ?",
        )
        .bind(&w.id)
        .fetch_optional(&mut *conn)
        .await?;
        if done.as_deref() != Some("done") {
            open.push(w);
        }
    }
    open.sort_by_key(|w| w.number);
    Ok(ProjectSpend {
        currency: currency(&mut *conn).await?,
        part_count: own.len() as i64,
        need_count: own.iter().filter(|p| p.stage == "need").count() as i64,
        spent,
        to_buy,
        family,
        waiting: open,
    })
}

/// What a project has spent on parts and has left to buy, with its family's
/// total, and its tasks waiting on parts.
#[utoipa::path(get, path = "/projects/{id}/spend", tag = "parts", params(("id" = String, Path)),
    responses((status = 200, body = ProjectSpend), (status = 404, body = crate::error::ErrorBody)))]
async fn project_spend(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<ProjectSpend>> {
    let mut conn = state.db.acquire().await?;
    visible_project(&mut conn, &user.id, &id).await?;
    Ok(Json(spend(&mut conn, &user.id, &id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(supplier: Option<&str>, total: f64) -> Part {
        Part {
            row_id: "r".into(),
            database_id: "d".into(),
            database_name: "Parts".into(),
            project_id: "p".into(),
            project_title: "P".into(),
            project_color: "slot-1".into(),
            item: "x".into(),
            qty: 1.0,
            unit_cost: Some(total),
            total,
            supplier: supplier.map(String::from),
            link: None,
            stage: "need".into(),
            ordered_on: None,
            arrived_on: None,
            tasks: vec![],
            can_edit: true,
        }
    }

    #[test]
    fn groups_suppliers_ignoring_case() {
        let groups = by_supplier(vec![
            part(Some("LCSC"), 6.4),
            part(None, 3.0),
            part(Some("Amazon"), 89.0),
            part(Some("lcsc"), 6.2),
        ]);
        let names: Vec<&str> = groups.iter().map(|g| g.supplier.as_str()).collect();
        assert_eq!(names, ["Amazon", "LCSC", ""]);
        assert_eq!(groups[1].parts.len(), 2);
        assert!((groups[1].total - 12.6).abs() < 1e-9);
    }

    #[test]
    fn dates_fill_once() {
        let roles: HashMap<String, String> = [("status", "s"), ("ordered_on", "o"), ("arrived_on", "a")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut map = Map::new();
        map.insert("s".into(), "arrived".into());
        fill_dates(&roles, "need", &mut map, "2026-11-10");
        assert_eq!(map["o"], "2026-11-10");
        assert_eq!(map["a"], "2026-11-10");
        map.insert("s".into(), "fitted".into());
        fill_dates(&roles, "arrived", &mut map, "2026-11-12");
        assert_eq!(map["a"], "2026-11-10");
    }
}
