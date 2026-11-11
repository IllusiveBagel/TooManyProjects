//! Copying someone's canvas layout onto yours (T-23).
//!
//! Everyone arranges their own canvas, so a project someone shares with you
//! used to land in a plain grid. Now it lands where its owner keeps it,
//! inside a copy of the owner's group around it, with the owner's whole
//! block set to the right of your own cards. The copies are yours: nothing
//! follows the owner's later changes. `POST /canvases/home/copy-layout` does
//! the same on demand, for everything one person shares with you.

use std::collections::{HashMap, HashSet};

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    canvas::{self, card_in_group, home_canvas_id, place_project},
    error::{AppError, AppResult},
    projects, time,
};

/// Cards are 256 by about 140.
const CARD: (f64, f64) = (256.0, 140.0);
/// Space between your own cards and a copied layout.
const GAP: f64 = 160.0;
const MAX_UNDO: usize = 500;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(copy_layout)).routes(routes!(restore_layout))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Deserialize, ToSchema)]
pub struct CopyLayoutRequest {
    /// The person whose layout to copy. They must share at least one project with you.
    pub owner_id: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CardPosition {
    pub project_id: String,
    pub x: f64,
    pub y: f64,
}

/// A group as it was, to put back.
#[derive(Serialize, Deserialize, FromRow, ToSchema)]
pub struct SavedGroup {
    pub label: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub copied_from: Option<String>,
}

/// What a copy changed, so it can be undone with `POST /canvases/home/restore-layout`.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LayoutSnapshot {
    /// Where the moved cards were before.
    pub cards: Vec<CardPosition>,
    /// Earlier copies of this person's groups that the copy replaced.
    pub removed_groups: Vec<SavedGroup>,
    /// The groups the copy added.
    pub added_groups: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CopyLayoutResult {
    pub owner_name: String,
    /// Cards moved to match the owner's canvas.
    pub moved: usize,
    /// Groups copied around them.
    pub groups: usize,
    pub undo: LayoutSnapshot,
}

// ---------------------------------------------------------------------------
// Working out the layout

#[derive(FromRow)]
struct Group {
    id: String,
    label: Option<String>,
    color: Option<String>,
    x: f64,
    y: f64,
    width: Option<f64>,
    height: Option<f64>,
}

impl Group {
    fn as_node(&self) -> canvas::CanvasNode {
        canvas::CanvasNode {
            id: self.id.clone(),
            kind: "group".into(),
            project_id: None,
            label: self.label.clone(),
            color: self.color.clone(),
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            collapsed: false,
        }
    }
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width.unwrap_or(0.0), self.height.unwrap_or(0.0))
    }
}

/// The owner's cards for some projects, and the owner's groups that hold at
/// least one of them. Groups holding none are left out, so a copy never
/// shows the name of a group of projects you can't see.
struct OwnerLayout {
    cards: HashMap<String, (f64, f64)>,
    groups: Vec<Group>,
    /// Every group on the owner's canvas, to recognise earlier copies.
    all_group_ids: HashSet<String>,
}

async fn owner_layout(
    conn: &mut SqliteConnection,
    owner_id: &str,
    projects: &HashSet<String>,
) -> AppResult<OwnerLayout> {
    let canvas_id = home_canvas_id(conn, owner_id).await?;
    let cards: HashMap<String, (f64, f64)> = sqlx::query_as::<_, (String, f64, f64)>(
        "SELECT project_id, x, y FROM canvas_nodes WHERE canvas_id = ? AND kind = 'project'",
    )
    .bind(&canvas_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .filter(|(id, _, _)| projects.contains(id))
    .map(|(id, x, y)| (id, (x, y)))
    .collect();
    let all: Vec<Group> = sqlx::query_as(
        "SELECT id, label, color, x, y, width, height FROM canvas_nodes WHERE canvas_id = ? AND kind = 'group'",
    )
    .bind(&canvas_id)
    .fetch_all(&mut *conn)
    .await?;
    let all_group_ids = all.iter().map(|g| g.id.clone()).collect();
    let groups = all
        .into_iter()
        .filter(|g| {
            let node = g.as_node();
            cards.values().any(|&(x, y)| card_in_group(&node, x, y))
        })
        .collect();
    Ok(OwnerLayout { cards, groups, all_group_ids })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Rect {
    fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x0: x, y0: y, x1: x + w, y1: y + h }
    }
    fn union(self, o: Rect) -> Rect {
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
}

fn bounds(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    rects.into_iter().reduce(Rect::union)
}

/// How far to shift the owner's block so it sits to the right of what is
/// already on your canvas, tops lined up. Nothing there yet: no shift.
fn offset_beside(mine: Option<Rect>, theirs: Option<Rect>) -> (f64, f64) {
    match (mine, theirs) {
        (Some(m), Some(t)) => ((m.x1 + GAP - t.x0).round(), (m.y0 - t.y0).round()),
        _ => (0.0, 0.0),
    }
}

fn layout_bounds(layout: &OwnerLayout) -> Option<Rect> {
    bounds(
        layout
            .cards
            .values()
            .map(|&(x, y)| Rect::new(x, y, CARD.0, CARD.1))
            .chain(layout.groups.iter().map(Group::rect)),
    )
}

/// Everything on your canvas except some cards and some groups, as rectangles.
async fn my_bounds(
    conn: &mut SqliteConnection,
    canvas_id: &str,
    skip_cards: &HashSet<String>,
    skip_groups: &HashSet<String>,
) -> AppResult<Option<Rect>> {
    #[allow(clippy::type_complexity)]
    let nodes: Vec<(String, String, Option<String>, f64, f64, Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT id, kind, project_id, x, y, width, height FROM canvas_nodes WHERE canvas_id = ?",
    )
    .bind(canvas_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(bounds(nodes.into_iter().filter_map(|(id, kind, project, x, y, w, h)| match kind.as_str() {
        "group" if skip_groups.contains(&id) => None,
        "group" => Some(Rect::new(x, y, w.unwrap_or(0.0), h.unwrap_or(0.0))),
        _ if project.as_ref().is_some_and(|p| skip_cards.contains(p)) => None,
        _ => Some(Rect::new(x, y, w.unwrap_or(CARD.0), h.unwrap_or(CARD.1))),
    })))
}

/// Your groups that are copies of the owner's, as (your id, their id, x, y).
async fn my_copies(
    conn: &mut SqliteConnection,
    canvas_id: &str,
    owner_groups: &HashSet<String>,
) -> AppResult<Vec<(String, String, f64, f64)>> {
    Ok(sqlx::query_as::<_, (String, String, f64, f64)>(
        "SELECT id, copied_from, x, y FROM canvas_nodes
         WHERE canvas_id = ? AND kind = 'group' AND copied_from IS NOT NULL",
    )
    .bind(canvas_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .filter(|(_, from, _, _)| owner_groups.contains(from))
    .collect())
}

async fn insert_group(conn: &mut SqliteConnection, canvas_id: &str, g: &SavedGroup) -> AppResult<String> {
    let id = time::new_id();
    sqlx::query(
        "INSERT INTO canvas_nodes (id, canvas_id, kind, label, color, x, y, width, height, copied_from, updated_at)
         VALUES (?, ?, 'group', ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(canvas_id)
    .bind(&g.label)
    .bind(&g.color)
    .bind(g.x)
    .bind(g.y)
    .bind(g.width)
    .bind(g.height)
    .bind(&g.copied_from)
    .bind(time::now())
    .execute(conn)
    .await?;
    Ok(id)
}

fn copy_of(g: &Group, (dx, dy): (f64, f64)) -> SavedGroup {
    SavedGroup {
        label: g.label.clone().unwrap_or_else(|| "Group".into()),
        color: g.color.clone().unwrap_or_else(|| "slot-1".into()),
        x: g.x + dx,
        y: g.y + dy,
        width: g.width.unwrap_or(0.0),
        height: g.height.unwrap_or(0.0),
        copied_from: Some(g.id.clone()),
    }
}

/// Place projects that `owner_id` shares with you and that have no card on
/// your canvas yet, where the owner has them. Returns the ones it could not
/// place (not on the owner's canvas), for the grid.
pub async fn place_shared(
    conn: &mut SqliteConnection,
    canvas_id: &str,
    owner_id: &str,
    project_ids: &[String],
) -> AppResult<Vec<String>> {
    let wanted: HashSet<String> = project_ids.iter().cloned().collect();
    let layout = owner_layout(conn, owner_id, &wanted).await?;
    if layout.cards.is_empty() {
        return Ok(project_ids.to_vec());
    }
    // Line up with earlier copies of this person's groups if there are any,
    // so a project shared later lands beside its old neighbours.
    let copies = my_copies(conn, canvas_id, &layout.all_group_ids).await?;
    let owner_canvas = home_canvas_id(conn, owner_id).await?;
    let mut offset = None;
    for (_, from, x, y) in &copies {
        let orig: Option<(f64, f64)> =
            sqlx::query_as("SELECT x, y FROM canvas_nodes WHERE id = ? AND canvas_id = ?")
                .bind(from)
                .bind(&owner_canvas)
                .fetch_optional(&mut *conn)
                .await?;
        if let Some((ox, oy)) = orig {
            offset = Some((x - ox, y - oy));
            break;
        }
    }
    let offset = match offset {
        Some(o) => o,
        None => offset_beside(
            my_bounds(conn, canvas_id, &HashSet::new(), &HashSet::new()).await?,
            layout_bounds(&layout),
        ),
    };

    let copied: HashSet<String> = copies.iter().map(|c| c.1.clone()).collect();
    for g in layout.groups.iter().filter(|g| !copied.contains(&g.id)) {
        insert_group(conn, canvas_id, &copy_of(g, offset)).await?;
    }
    let mut left = Vec::new();
    for id in project_ids {
        match layout.cards.get(id) {
            Some(&(x, y)) => place_project(conn, canvas_id, id, x + offset.0, y + offset.1).await?,
            None => left.push(id.clone()),
        }
    }
    Ok(left)
}

// ---------------------------------------------------------------------------
// Handlers

/// Lay out everything one person shares with you the way they have it: their
/// projects move to where they keep them, and their groups are copied around
/// them, set to the right of your own cards. Earlier copies of their groups
/// are replaced. Your own projects and groups stay put, and nothing changes
/// on their canvas. The result says how to undo it.
#[utoipa::path(post, path = "/canvases/home/copy-layout", tag = "canvas", request_body = CopyLayoutRequest,
    responses((status = 200, body = CopyLayoutResult), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn copy_layout(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CopyLayoutRequest>,
) -> AppResult<Json<CopyLayoutResult>> {
    if req.owner_id == user.id {
        return Err(AppError::BadRequest("own_layout", "That's already your layout.".into()));
    }
    let mut tx = state.db.begin().await?;
    let shared: HashSet<String> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN project_access a ON a.project_id = p.id AND a.user_id = ?
         WHERE p.owner_id = ?",
    )
    .bind(&user.id)
    .bind(&req.owner_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    if shared.is_empty() {
        return Err(AppError::NotFound);
    }
    let owner_name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = ?")
        .bind(&req.owner_id)
        .fetch_one(&mut *tx)
        .await?;
    let canvas_id = home_canvas_id(&mut tx, &user.id).await?;
    let layout = owner_layout(&mut tx, &req.owner_id, &shared).await?;

    // Before: where the cards are now, and the earlier copies we replace.
    let mut before = Vec::new();
    for id in layout.cards.keys() {
        let at: Option<(f64, f64)> =
            sqlx::query_as("SELECT x, y FROM canvas_nodes WHERE canvas_id = ? AND project_id = ?")
                .bind(&canvas_id)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((x, y)) = at {
            before.push(CardPosition { project_id: id.clone(), x, y });
        }
    }
    let old: Vec<String> =
        my_copies(&mut tx, &canvas_id, &layout.all_group_ids).await?.into_iter().map(|c| c.0).collect();
    let mut removed_groups = Vec::with_capacity(old.len());
    for id in &old {
        removed_groups.push(
            sqlx::query_as::<_, SavedGroup>(
                "SELECT COALESCE(label, 'Group') AS label, COALESCE(color, 'slot-1') AS color, x, y,
                    COALESCE(width, 0) AS width, COALESCE(height, 0) AS height, copied_from
                 FROM canvas_nodes WHERE id = ?",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?,
        );
    }

    let skip_cards: HashSet<String> = layout.cards.keys().cloned().collect();
    let skip_groups: HashSet<String> = old.iter().cloned().collect();
    let offset = offset_beside(
        my_bounds(&mut tx, &canvas_id, &skip_cards, &skip_groups).await?,
        layout_bounds(&layout),
    );
    for id in &old {
        sqlx::query("DELETE FROM canvas_nodes WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    let mut added_groups = Vec::with_capacity(layout.groups.len());
    for g in &layout.groups {
        added_groups.push(insert_group(&mut tx, &canvas_id, &copy_of(g, offset)).await?);
    }
    for (id, &(x, y)) in &layout.cards {
        place_project(&mut tx, &canvas_id, id, x + offset.0, y + offset.1).await?;
    }
    tx.commit().await?;
    Ok(Json(CopyLayoutResult {
        owner_name,
        moved: layout.cards.len(),
        groups: added_groups.len(),
        undo: LayoutSnapshot { cards: before, removed_groups, added_groups },
    }))
}

/// Undo a layout copy: put the cards back, take the copied groups away and
/// bring back the ones it replaced.
#[utoipa::path(post, path = "/canvases/home/restore-layout", tag = "canvas", request_body = LayoutSnapshot,
    responses((status = 204), (status = 400, body = crate::error::ErrorBody)))]
async fn restore_layout(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<LayoutSnapshot>,
) -> AppResult<StatusCode> {
    if req.cards.len() + req.removed_groups.len() + req.added_groups.len() > MAX_UNDO {
        return Err(AppError::BadRequest("too_many", format!("At most {MAX_UNDO} changes at once.")));
    }
    let finite = |v: f64| v.is_finite() && v.abs() < 1e7;
    if req.cards.iter().any(|c| !finite(c.x) || !finite(c.y))
        || req
            .removed_groups
            .iter()
            .any(|g| !finite(g.x) || !finite(g.y) || !finite(g.width) || !finite(g.height))
    {
        return Err(AppError::BadRequest("invalid_position", "Positions must be finite numbers.".into()));
    }
    for g in &req.removed_groups {
        let label = g.label.trim();
        if label.is_empty() || label.chars().count() > 80 {
            return Err(AppError::BadRequest(
                "invalid_label",
                "Give the group a name of 1 to 80 characters.".into(),
            ));
        }
        projects::check_color(&g.color)?;
        if g.width < 120.0 || g.height < 120.0 {
            return Err(AppError::BadRequest("invalid_size", "Groups are at least 120 by 120.".into()));
        }
    }
    let mut tx = state.db.begin().await?;
    let canvas_id = home_canvas_id(&mut tx, &user.id).await?;
    for id in &req.added_groups {
        sqlx::query("DELETE FROM canvas_nodes WHERE id = ? AND canvas_id = ? AND kind = 'group'")
            .bind(id)
            .bind(&canvas_id)
            .execute(&mut *tx)
            .await?;
    }
    for g in &req.removed_groups {
        insert_group(&mut tx, &canvas_id, g).await?;
    }
    for c in &req.cards {
        // Only cards already on your canvas.
        sqlx::query(
            "UPDATE canvas_nodes SET x = ?, y = ?, updated_at = ? WHERE canvas_id = ? AND project_id = ?",
        )
        .bind(c.x)
        .bind(c.y)
        .bind(time::now())
        .bind(&canvas_id)
        .bind(&c.project_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_their_block_to_the_right_of_yours() {
        let mine = Some(Rect::new(0.0, 100.0, 256.0, 140.0));
        let theirs = Some(Rect::new(40.0, 60.0, 1500.0, 600.0));
        assert_eq!(offset_beside(mine, theirs), (256.0 + GAP - 40.0, 40.0));
    }

    #[test]
    fn keeps_their_positions_on_an_empty_canvas() {
        assert_eq!(offset_beside(None, Some(Rect::new(40.0, 60.0, 10.0, 10.0))), (0.0, 0.0));
    }
}
