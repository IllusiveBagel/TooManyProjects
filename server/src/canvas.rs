//! The canvas: where each person's cards sit. Layout is kept apart from the
//! projects themselves, so moving a card never touches a project, and each
//! person arranges their own canvas.

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
    projects::{PROJECT_SELECT, Project},
    time,
};

/// Horizontal step between auto-placed cards (card width plus a gap).
const STEP_X: f64 = 320.0;
const STEP_Y: f64 = 220.0;
const AUTO_COLUMNS: usize = 4;
const MAX_BATCH: usize = 500;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(home_canvas)).routes(routes!(save_nodes))
}

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

/// Everything the canvas draws, in one call.
#[derive(Serialize, ToSchema)]
pub struct CanvasView {
    pub canvas: Canvas,
    pub nodes: Vec<CanvasNode>,
    pub projects: Vec<Project>,
    /// Links whose two ends are both in `projects`.
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
pub struct SaveNodesRequest {
    /// Up to 500 cards at once, sent after a drag.
    pub nodes: Vec<NodePosition>,
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
        "{PROJECT_SELECT} WHERE p.owner_id = ?{archived} ORDER BY p.created_at"
    ))
    .bind(&user.id)
    .fetch_all(&mut *tx)
    .await?;

    let unplaced: Vec<String> = sqlx::query_scalar(
        "SELECT p.id FROM projects p WHERE p.owner_id = ?
           AND NOT EXISTS (SELECT 1 FROM canvas_nodes n WHERE n.canvas_id = ? AND n.project_id = p.id)
         ORDER BY p.created_at",
    )
    .bind(&user.id)
    .bind(&canvas_id)
    .fetch_all(&mut *tx)
    .await?;
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
    let shown: std::collections::HashSet<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    nodes.retain(|n| n.project_id.as_deref().is_none_or(|id| shown.contains(id)));

    let links = sqlx::query_as::<_, Link>(&format!(
        "SELECT {LINK_COLUMNS} FROM project_links l
         WHERE EXISTS (SELECT 1 FROM projects p WHERE p.id = l.source_id AND p.owner_id = ?)
           AND EXISTS (SELECT 1 FROM projects p WHERE p.id = l.target_id AND p.owner_id = ?)
         ORDER BY created_at"
    ))
    .bind(&user.id)
    .bind(&user.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .filter(|l| shown.contains(l.source_id.as_str()) && shown.contains(l.target_id.as_str()))
    .collect();
    tx.commit().await?;
    Ok(Json(CanvasView { canvas, nodes, projects, links }))
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
    if req.nodes.len() > MAX_BATCH {
        return Err(AppError::BadRequest(
            "too_many_nodes",
            format!("Save at most {MAX_BATCH} cards at once."),
        ));
    }
    let valid = |v: f64| v.is_finite() && v.abs() < 1e7;
    if req.nodes.iter().any(|n| {
        !valid(n.x)
            || !valid(n.y)
            || n.width.is_some_and(|w| !valid(w))
            || n.height.is_some_and(|h| !valid(h))
    }) {
        return Err(AppError::BadRequest("invalid_position", "Positions must be finite numbers.".into()));
    }
    let mut tx = state.db.begin().await?;
    let owned: Option<String> = sqlx::query_scalar("SELECT id FROM canvases WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&mut *tx)
        .await?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }
    let now = time::now();
    for n in &req.nodes {
        // Only cards for projects this person can see.
        let changed = sqlx::query(
            "UPDATE canvas_nodes SET x = ?, y = ?, width = COALESCE(?, width), height = COALESCE(?, height),
                updated_at = ?
             WHERE canvas_id = ? AND project_id = ?
               AND EXISTS (SELECT 1 FROM projects p WHERE p.id = ? AND p.owner_id = ?)",
        )
        .bind(n.x)
        .bind(n.y)
        .bind(n.width)
        .bind(n.height)
        .bind(&now)
        .bind(&id)
        .bind(&n.project_id)
        .bind(&n.project_id)
        .bind(&user.id)
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

#[cfg(test)]
mod tests {
    use super::free_spot;

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
