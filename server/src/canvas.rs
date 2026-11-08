//! The canvas: where each person's cards sit. Layout is kept apart from the
//! projects themselves, so moving a card never touches a project, and each
//! person arranges their own canvas.

use std::collections::HashSet;

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
    auth::{self, CurrentUser},
    error::{AppError, AppResult},
    links::{LINK_COLUMNS, Link},
    projects::{self, PROJECT_SELECT, Project},
    time,
};

/// Horizontal step between auto-placed cards (card width plus a gap).
const STEP_X: f64 = 320.0;
const STEP_Y: f64 = 220.0;
const AUTO_COLUMNS: usize = 4;
const MAX_BATCH: usize = 500;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(home_canvas))
        .routes(routes!(save_nodes))
        .routes(routes!(create_group))
        .routes(routes!(update_group, delete_group))
}

/// Groups are labelled areas drawn behind cards. A card belongs to a group
/// while its centre is inside it; nothing else is stored, so dragging a card
/// out of the area takes it out of the group.
const MAX_LABEL: usize = 80;
/// Where the server takes a card's centre to be, for working out which
/// group it sits in. Cards are 256 wide and roughly 140 tall.
const CARD_CENTRE: (f64, f64) = (128.0, 70.0);
const MIN_GROUP: f64 = 120.0;

#[derive(Serialize, FromRow, ToSchema)]
pub struct Canvas {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct CanvasNode {
    pub id: String,
    /// `project`, `note` or `group`.
    pub kind: String,
    pub project_id: Option<String>,
    pub label: Option<String>,
    pub color: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub collapsed: bool,
}

/// A project you can't see that is linked to one you can. The canvas shows it
/// as a locked card, so the arrow has somewhere to go, without its title.
#[derive(Serialize, FromRow, ToSchema)]
pub struct LockedProject {
    pub id: String,
    pub owner_name: String,
}

/// Everything the canvas draws, in one call.
#[derive(Serialize, ToSchema)]
pub struct CanvasView {
    pub canvas: Canvas,
    pub nodes: Vec<CanvasNode>,
    /// Your projects and the ones shared with you.
    pub projects: Vec<Project>,
    /// Linked projects that aren't shared with you.
    pub locked: Vec<LockedProject>,
    /// Links whose ends are both in `projects` or `locked`, at least one in `projects`.
    pub links: Vec<Link>,
}

#[derive(Deserialize, IntoParams)]
pub struct CanvasQuery {
    /// Include archived projects. Default false.
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct NodePosition {
    pub project_id: String,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Deserialize, ToSchema)]
pub struct GroupPosition {
    /// The group's node id.
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Deserialize, ToSchema)]
pub struct SaveNodesRequest {
    /// Up to 500 cards at once, sent after a drag.
    #[serde(default)]
    pub nodes: Vec<NodePosition>,
    /// Groups moved or resized in the same drag.
    #[serde(default)]
    pub groups: Vec<GroupPosition>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateGroupRequest {
    /// What the area holds, such as "Electronics". Up to 80 characters.
    pub label: String,
    /// A theme slot (slot-1 to slot-8) or #rrggbb. Default slot-1.
    pub color: Option<String>,
    pub x: f64,
    pub y: f64,
    /// At least 120 each.
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateGroupRequest {
    pub label: Option<String>,
    pub color: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

/// A group and the projects whose cards sit inside it.
pub struct GroupMembers {
    pub group: CanvasNode,
    pub project_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers

pub async fn home_canvas_id(conn: &mut SqliteConnection, user_id: &str) -> AppResult<String> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT id FROM canvases WHERE user_id = ? ORDER BY created_at LIMIT 1")
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;
    match id {
        Some(id) => Ok(id),
        None => {
            auth::create_home_canvas(conn, user_id).await?;
            Box::pin(home_canvas_id(conn, user_id)).await
        }
    }
}

/// The next empty grid cell, filling rows of four left to right below the
/// lowest card. Good enough until someone drags the card where they want it.
pub async fn next_free_spot(conn: &mut SqliteConnection, canvas_id: &str) -> AppResult<(f64, f64)> {
    let taken: Vec<(f64, f64)> = sqlx::query_as("SELECT x, y FROM canvas_nodes WHERE canvas_id = ?")
        .bind(canvas_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(free_spot(&taken))
}

fn free_spot(taken: &[(f64, f64)]) -> (f64, f64) {
    let near = |x: f64, y: f64| {
        taken.iter().any(|&(tx, ty)| (tx - x).abs() < STEP_X - 40.0 && (ty - y).abs() < STEP_Y - 40.0)
    };
    for i in 0.. {
        let (x, y) = ((i % AUTO_COLUMNS) as f64 * STEP_X, (i / AUTO_COLUMNS) as f64 * STEP_Y);
        if !near(x, y) {
            return (x, y);
        }
    }
    unreachable!()
}

pub async fn place_project(
    conn: &mut SqliteConnection,
    canvas_id: &str,
    project_id: &str,
    x: f64,
    y: f64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO canvas_nodes (id, canvas_id, kind, project_id, x, y, updated_at)
         VALUES (?, ?, 'project', ?, ?, ?, ?)
         ON CONFLICT (canvas_id, project_id) DO UPDATE SET x = excluded.x, y = excluded.y,
             updated_at = excluded.updated_at",
    )
    .bind(time::new_id())
    .bind(canvas_id)
    .bind(project_id)
    .bind(x)
    .bind(y)
    .bind(time::now())
    .execute(conn)
    .await?;
    Ok(())
}

const NODE_COLUMNS: &str = "id, kind, project_id, label, color, x, y, width, height, collapsed";

fn valid_number(v: f64) -> bool {
    v.is_finite() && v.abs() < 1e7
}

fn check_label(label: &str) -> AppResult<String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > MAX_LABEL {
        return Err(AppError::BadRequest(
            "invalid_label",
            format!("Give the group a name of 1 to {MAX_LABEL} characters."),
        ));
    }
    Ok(label.to_string())
}

fn check_size(width: f64, height: f64) -> AppResult<()> {
    if !valid_number(width) || !valid_number(height) || width < MIN_GROUP || height < MIN_GROUP {
        return Err(AppError::BadRequest(
            "invalid_size",
            format!("Groups are at least {MIN_GROUP} by {MIN_GROUP}."),
        ));
    }
    Ok(())
}

async fn own_canvas(conn: &mut SqliteConnection, canvas_id: &str, user_id: &str) -> AppResult<()> {
    let owned: Option<String> = sqlx::query_scalar("SELECT id FROM canvases WHERE id = ? AND user_id = ?")
        .bind(canvas_id)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;
    owned.map(|_| ()).ok_or(AppError::NotFound)
}

async fn fetch_group(conn: &mut SqliteConnection, canvas_id: &str, group_id: &str) -> AppResult<CanvasNode> {
    sqlx::query_as::<_, CanvasNode>(&format!(
        "SELECT {NODE_COLUMNS} FROM canvas_nodes WHERE id = ? AND canvas_id = ? AND kind = 'group'"
    ))
    .bind(group_id)
    .bind(canvas_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Whether a card at (x, y) has its centre inside the group.
pub fn card_in_group(group: &CanvasNode, x: f64, y: f64) -> bool {
    let (cx, cy) = (x + CARD_CENTRE.0, y + CARD_CENTRE.1);
    let (w, h) = (group.width.unwrap_or(0.0), group.height.unwrap_or(0.0));
    cx >= group.x && cx <= group.x + w && cy >= group.y && cy <= group.y + h
}

/// Every group on someone's home canvas, biggest first, with the projects
/// (that they can see) whose cards sit inside it.
pub async fn groups_with_members(conn: &mut SqliteConnection, user_id: &str) -> AppResult<Vec<GroupMembers>> {
    let canvas_id = home_canvas_id(conn, user_id).await?;
    let mut groups = sqlx::query_as::<_, CanvasNode>(&format!(
        "SELECT {NODE_COLUMNS} FROM canvas_nodes WHERE canvas_id = ? AND kind = 'group'"
    ))
    .bind(&canvas_id)
    .fetch_all(&mut *conn)
    .await?;
    groups.sort_by(|a, b| {
        let area = |n: &CanvasNode| n.width.unwrap_or(0.0) * n.height.unwrap_or(0.0);
        area(b).total_cmp(&area(a))
    });
    let cards: Vec<(String, f64, f64)> = sqlx::query_as(
        "SELECT n.project_id, n.x, n.y FROM canvas_nodes n
         JOIN project_access a ON a.project_id = n.project_id AND a.user_id = ?
         WHERE n.canvas_id = ?",
    )
    .bind(user_id)
    .bind(&canvas_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(groups
        .into_iter()
        .map(|group| {
            let project_ids = cards
                .iter()
                .filter(|(_, x, y)| card_in_group(&group, *x, *y))
                .map(|(id, _, _)| id.clone())
                .collect();
            GroupMembers { group, project_ids }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Handlers

/// Your home canvas: cards, their positions and the links between them.
/// Projects without a card yet are placed automatically.
#[utoipa::path(get, path = "/canvases/home", tag = "canvas", params(CanvasQuery),
    responses((status = 200, body = CanvasView)))]
async fn home_canvas(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<CanvasQuery>,
) -> AppResult<Json<CanvasView>> {
    let mut tx = state.db.begin().await?;
    let canvas_id = home_canvas_id(&mut tx, &user.id).await?;
    let canvas = sqlx::query_as::<_, Canvas>("SELECT id, name FROM canvases WHERE id = ?")
        .bind(&canvas_id)
        .fetch_one(&mut *tx)
        .await?;

    let archived = if q.include_archived { "" } else { " AND p.status <> 'archived'" };
    let projects = sqlx::query_as::<_, Project>(&format!(
        "{PROJECT_SELECT} WHERE a.user_id = ?{archived} ORDER BY p.created_at"
    ))
    .bind(&user.id)
    .fetch_all(&mut *tx)
    .await?;
    let shown: HashSet<String> = projects.iter().map(|p| p.id.clone()).collect();
    let access: HashSet<String> =
        sqlx::query_scalar("SELECT project_id FROM project_access WHERE user_id = ?")
            .bind(&user.id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();

    // Every link touching a project shown here. An end you have no access to
    // at all becomes a locked card; an end you can see but that is hidden
    // (archived) drops the link, as before.
    let links: Vec<Link> = sqlx::query_as::<_, Link>(&format!(
        "SELECT {LINK_COLUMNS} FROM project_links l
         WHERE l.source_id IN (SELECT project_id FROM project_access WHERE user_id = ?1)
            OR l.target_id IN (SELECT project_id FROM project_access WHERE user_id = ?1)
         ORDER BY created_at"
    ))
    .bind(&user.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .filter(|l| {
        let ok = |id: &str| shown.contains(id) || !access.contains(id);
        ok(&l.source_id) && ok(&l.target_id) && (shown.contains(&l.source_id) || shown.contains(&l.target_id))
    })
    .collect();
    let mut locked_ids: Vec<&str> = links
        .iter()
        .flat_map(|l| [l.source_id.as_str(), l.target_id.as_str()])
        .filter(|id| !access.contains(*id))
        .collect();
    locked_ids.sort_unstable();
    locked_ids.dedup();
    let mut locked = Vec::with_capacity(locked_ids.len());
    for id in &locked_ids {
        locked.push(
            sqlx::query_as::<_, LockedProject>(
                "SELECT p.id, u.name AS owner_name FROM projects p JOIN users u ON u.id = p.owner_id WHERE p.id = ?",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?,
        );
    }

    let placed: HashSet<String> = sqlx::query_scalar(
        "SELECT project_id FROM canvas_nodes WHERE canvas_id = ? AND project_id IS NOT NULL",
    )
    .bind(&canvas_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let unplaced: Vec<String> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN project_access a ON a.project_id = p.id AND a.user_id = ?
         ORDER BY p.created_at",
    )
    .bind(&user.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .chain(locked_ids.iter().map(|s| s.to_string()))
    .filter(|id| !placed.contains(id))
    .collect();
    for project_id in unplaced {
        let (x, y) = next_free_spot(&mut tx, &canvas_id).await?;
        place_project(&mut tx, &canvas_id, &project_id, x, y).await?;
    }

    let mut nodes = sqlx::query_as::<_, CanvasNode>(
        "SELECT id, kind, project_id, label, color, x, y, width, height, collapsed
         FROM canvas_nodes WHERE canvas_id = ? ORDER BY updated_at",
    )
    .bind(&canvas_id)
    .fetch_all(&mut *tx)
    .await?;
    nodes.retain(|n| {
        n.project_id.as_deref().is_none_or(|id| shown.contains(id) || locked_ids.binary_search(&id).is_ok())
    });
    tx.commit().await?;
    Ok(Json(CanvasView { canvas, nodes, projects, locked, links }))
}

/// Save card positions after a drag.
#[utoipa::path(patch, path = "/canvases/{id}/nodes", tag = "canvas", params(("id" = String, Path)),
    request_body = SaveNodesRequest,
    responses((status = 204), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn save_nodes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<SaveNodesRequest>,
) -> AppResult<StatusCode> {
    if req.nodes.len() + req.groups.len() > MAX_BATCH {
        return Err(AppError::BadRequest(
            "too_many_nodes",
            format!("Save at most {MAX_BATCH} cards at once."),
        ));
    }
    let valid = valid_number;
    let bad = |x: f64, y: f64, w: Option<f64>, h: Option<f64>| {
        !valid(x) || !valid(y) || w.is_some_and(|w| !valid(w)) || h.is_some_and(|h| !valid(h))
    };
    if req.nodes.iter().any(|n| bad(n.x, n.y, n.width, n.height))
        || req.groups.iter().any(|g| bad(g.x, g.y, g.width, g.height))
    {
        return Err(AppError::BadRequest("invalid_position", "Positions must be finite numbers.".into()));
    }
    if req
        .groups
        .iter()
        .any(|g| g.width.is_some_and(|w| w < MIN_GROUP) || g.height.is_some_and(|h| h < MIN_GROUP))
    {
        return Err(AppError::BadRequest(
            "invalid_size",
            format!("Groups are at least {MIN_GROUP} by {MIN_GROUP}."),
        ));
    }
    let mut tx = state.db.begin().await?;
    own_canvas(&mut tx, &id, &user.id).await?;
    let now = time::now();
    for g in &req.groups {
        let changed = sqlx::query(
            "UPDATE canvas_nodes SET x = ?, y = ?, width = COALESCE(?, width), height = COALESCE(?, height),
                updated_at = ?
             WHERE canvas_id = ? AND id = ? AND kind = 'group'",
        )
        .bind(g.x)
        .bind(g.y)
        .bind(g.width)
        .bind(g.height)
        .bind(&now)
        .bind(&id)
        .bind(&g.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(AppError::NotFound);
        }
    }
    for n in &req.nodes {
        // Only cards already on this canvas, which the server placed for
        // projects this person can see or for locked cards beside them.
        let changed = sqlx::query(
            "UPDATE canvas_nodes SET x = ?, y = ?, width = COALESCE(?, width), height = COALESCE(?, height),
                updated_at = ?
             WHERE canvas_id = ? AND project_id = ?",
        )
        .bind(n.x)
        .bind(n.y)
        .bind(n.width)
        .bind(n.height)
        .bind(&now)
        .bind(&id)
        .bind(&n.project_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(AppError::NotFound);
        }
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Draw a group: a labelled, coloured area behind cards. Dragging it moves
/// the cards inside. Groups live on your own canvas, so nobody else sees them.
#[utoipa::path(post, path = "/canvases/{id}/groups", tag = "canvas", params(("id" = String, Path)),
    request_body = CreateGroupRequest,
    responses((status = 201, body = CanvasNode), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn create_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CreateGroupRequest>,
) -> AppResult<(StatusCode, Json<CanvasNode>)> {
    let label = check_label(&req.label)?;
    let color = req.color.unwrap_or_else(|| "slot-1".into());
    projects::check_color(&color)?;
    if !valid_number(req.x) || !valid_number(req.y) {
        return Err(AppError::BadRequest("invalid_position", "Positions must be finite numbers.".into()));
    }
    check_size(req.width, req.height)?;
    let mut tx = state.db.begin().await?;
    own_canvas(&mut tx, &id, &user.id).await?;
    let group_id = time::new_id();
    sqlx::query(
        "INSERT INTO canvas_nodes (id, canvas_id, kind, label, color, x, y, width, height, updated_at)
         VALUES (?, ?, 'group', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&group_id)
    .bind(&id)
    .bind(&label)
    .bind(&color)
    .bind(req.x)
    .bind(req.y)
    .bind(req.width)
    .bind(req.height)
    .bind(time::now())
    .execute(&mut *tx)
    .await?;
    let group = fetch_group(&mut tx, &id, &group_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(group)))
}

/// Rename, recolour, move or resize a group. Moving it here does not move
/// the cards inside; the canvas sends those in the same drag.
#[utoipa::path(patch, path = "/canvases/{id}/groups/{group_id}", tag = "canvas",
    params(("id" = String, Path), ("group_id" = String, Path)),
    request_body = UpdateGroupRequest,
    responses((status = 200, body = CanvasNode), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn update_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
    Json(req): Json<UpdateGroupRequest>,
) -> AppResult<Json<CanvasNode>> {
    let mut tx = state.db.begin().await?;
    own_canvas(&mut tx, &id, &user.id).await?;
    let cur = fetch_group(&mut tx, &id, &group_id).await?;
    let label = match &req.label {
        Some(l) => check_label(l)?,
        None => cur.label.clone().unwrap_or_default(),
    };
    let color = req.color.or(cur.color).unwrap_or_else(|| "slot-1".into());
    projects::check_color(&color)?;
    let (x, y) = (req.x.unwrap_or(cur.x), req.y.unwrap_or(cur.y));
    if !valid_number(x) || !valid_number(y) {
        return Err(AppError::BadRequest("invalid_position", "Positions must be finite numbers.".into()));
    }
    let (w, h) =
        (req.width.or(cur.width).unwrap_or(MIN_GROUP), req.height.or(cur.height).unwrap_or(MIN_GROUP));
    check_size(w, h)?;
    sqlx::query(
        "UPDATE canvas_nodes SET label = ?, color = ?, x = ?, y = ?, width = ?, height = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&label)
    .bind(&color)
    .bind(x)
    .bind(y)
    .bind(w)
    .bind(h)
    .bind(time::now())
    .bind(&group_id)
    .execute(&mut *tx)
    .await?;
    let group = fetch_group(&mut tx, &id, &group_id).await?;
    tx.commit().await?;
    Ok(Json(group))
}

/// Remove a group. The cards inside stay where they are.
#[utoipa::path(delete, path = "/canvases/{id}/groups/{group_id}", tag = "canvas",
    params(("id" = String, Path), ("group_id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_group(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, group_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let mut conn = state.db.acquire().await?;
    own_canvas(&mut conn, &id, &user.id).await?;
    let gone = sqlx::query("DELETE FROM canvas_nodes WHERE id = ? AND canvas_id = ? AND kind = 'group'")
        .bind(&group_id)
        .bind(&id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::{CanvasNode, card_in_group, free_spot};

    #[test]
    fn a_card_is_in_a_group_when_its_centre_is() {
        let g = CanvasNode {
            id: "g".into(),
            kind: "group".into(),
            project_id: None,
            label: Some("Cars".into()),
            color: None,
            x: 0.0,
            y: 0.0,
            width: Some(600.0),
            height: Some(400.0),
            collapsed: false,
        };
        assert!(card_in_group(&g, 20.0, 20.0));
        // Mostly hanging off the right edge: centre at 728.
        assert!(!card_in_group(&g, 600.0, 20.0));
        // Half over the edge still counts while the centre is inside.
        assert!(card_in_group(&g, 400.0, 300.0));
        assert!(!card_in_group(&g, 20.0, 340.0));
    }

    #[test]
    fn fills_the_grid_in_rows() {
        assert_eq!(free_spot(&[]), (0.0, 0.0));
        assert_eq!(free_spot(&[(0.0, 0.0)]), (320.0, 0.0));
        let row: Vec<_> = (0..4).map(|i| (i as f64 * 320.0, 0.0)).collect();
        assert_eq!(free_spot(&row), (0.0, 220.0));
        // A card dragged far away leaves the first cell free.
        assert_eq!(free_spot(&[(5000.0, 5000.0)]), (0.0, 0.0));
    }
}
