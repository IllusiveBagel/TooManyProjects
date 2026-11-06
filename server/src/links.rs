//! Links between projects, drawn as arrows on the canvas.
//!
//! - `spawned`: the target grew out of the source (parent to child).
//! - `related`: loosely connected, no direction that matters.
//! - `blocks`: the source must finish before the target can.
//!
//! Projects form a graph, not a tree, so a project can have several parents.
//! Loops are refused for `spawned` and `blocks`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    projects::{log_activity, visible_project},
    time,
};

pub const KINDS: &[&str] = &["spawned", "related", "blocks"];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(create_link)).routes(routes!(update_link, delete_link))
}

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct Link {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    /// `spawned`, `related` or `blocks`.
    pub kind: String,
    pub label: Option<String>,
    pub created_at: String,
}

pub(crate) const LINK_COLUMNS: &str = "id, source_id, target_id, kind, label, created_at";

#[derive(Deserialize, ToSchema)]
pub struct CreateLinkRequest {
    pub source_id: String,
    pub target_id: String,
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateLinkRequest {
    pub kind: Option<String>,
    /// Send `null` to clear.
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub label: Option<Option<String>>,
    /// Swap source and target.
    #[serde(default)]
    pub reverse: bool,
}

fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

fn clean_label(label: Option<String>) -> AppResult<Option<String>> {
    match label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()) {
        Some(l) if l.chars().count() > 60 => {
            Err(AppError::BadRequest("invalid_label", "Keep link labels under 60 characters.".into()))
        }
        other => Ok(other),
    }
}

/// Checks a link could exist, ignoring `except` (the link being changed).
async fn check_link(
    conn: &mut SqliteConnection,
    source: &str,
    target: &str,
    kind: &str,
    except: Option<&str>,
) -> AppResult<()> {
    if !KINDS.contains(&kind) {
        return Err(AppError::BadRequest("invalid_kind", "Link kind is spawned, related or blocks.".into()));
    }
    if source == target {
        return Err(AppError::BadRequest("self_link", "A project can't link to itself.".into()));
    }
    let except = except.unwrap_or("");
    // Same pair and kind, either way round for `related`.
    let duplicate: Option<String> = sqlx::query_scalar(
        "SELECT id FROM project_links WHERE kind = ? AND id <> ?
           AND ((source_id = ? AND target_id = ?) OR (? = 'related' AND source_id = ? AND target_id = ?))",
    )
    .bind(kind)
    .bind(except)
    .bind(source)
    .bind(target)
    .bind(kind)
    .bind(target)
    .bind(source)
    .fetch_optional(&mut *conn)
    .await?;
    if duplicate.is_some() {
        return Err(AppError::Conflict(
            "duplicate_link",
            "Those projects are already linked that way.".into(),
        ));
    }
    if kind == "spawned" || kind == "blocks" {
        // Would `target` already lead back to `source` through links of this kind?
        let loops: Option<i64> = sqlx::query_scalar(
            "WITH RECURSIVE reach(id) AS (
                 SELECT ?
                 UNION
                 SELECT l.target_id FROM project_links l JOIN reach r ON l.source_id = r.id
                 WHERE l.kind = ? AND l.id <> ?
             )
             SELECT 1 FROM reach WHERE id = ? LIMIT 1",
        )
        .bind(target)
        .bind(kind)
        .bind(except)
        .bind(source)
        .fetch_optional(&mut *conn)
        .await?;
        if loops.is_some() {
            let msg = if kind == "spawned" {
                "That would make a loop: a project can't be spawned from its own descendant."
            } else {
                "That would make a loop of projects blocking each other."
            };
            return Err(AppError::Conflict("link_loop", msg.into()));
        }
    }
    Ok(())
}

/// Inserts a link after checking it. Both projects must already be visible.
pub async fn insert_link(
    conn: &mut SqliteConnection,
    source: &str,
    target: &str,
    kind: &str,
    label: Option<String>,
) -> AppResult<Link> {
    check_link(conn, source, target, kind, None).await?;
    let id = time::new_id();
    sqlx::query(
        "INSERT INTO project_links (id, source_id, target_id, kind, label, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(source)
    .bind(target)
    .bind(kind)
    .bind(&label)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    link_by_id(conn, &id).await
}

async fn link_by_id(conn: &mut SqliteConnection, id: &str) -> AppResult<Link> {
    sqlx::query_as::<_, Link>(&format!("SELECT {LINK_COLUMNS} FROM project_links WHERE id = ?"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

/// A link the person may change: they must see both ends.
async fn visible_link(conn: &mut SqliteConnection, user_id: &str, id: &str) -> AppResult<Link> {
    let link = link_by_id(conn, id).await?;
    visible_project(conn, user_id, &link.source_id).await?;
    visible_project(conn, user_id, &link.target_id).await?;
    Ok(link)
}

/// Draw a link between two projects.
#[utoipa::path(post, path = "/links", tag = "links", request_body = CreateLinkRequest,
    responses((status = 201, body = Link), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody, description = "Duplicate, or would make a loop")))]
async fn create_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateLinkRequest>,
) -> AppResult<(StatusCode, Json<Link>)> {
    let label = clean_label(req.label)?;
    let mut tx = state.db.begin().await?;
    visible_project(&mut tx, &user.id, &req.source_id).await?;
    visible_project(&mut tx, &user.id, &req.target_id).await?;
    let link = insert_link(&mut tx, &req.source_id, &req.target_id, &req.kind, label).await?;
    log_activity(
        &mut tx,
        &req.source_id,
        &user.id,
        "link.created",
        serde_json::json!({ "link_id": link.id, "kind": link.kind, "target_id": link.target_id }),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(link)))
}

/// Change a link's kind or label, or swap its direction.
#[utoipa::path(patch, path = "/links/{id}", tag = "links", params(("id" = String, Path)),
    request_body = UpdateLinkRequest,
    responses((status = 200, body = Link), (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
async fn update_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateLinkRequest>,
) -> AppResult<Json<Link>> {
    let mut tx = state.db.begin().await?;
    let link = visible_link(&mut tx, &user.id, &id).await?;
    let kind = req.kind.unwrap_or_else(|| link.kind.clone());
    let label = match req.label {
        Some(l) => clean_label(l)?,
        None => link.label.clone(),
    };
    let (source, target) =
        if req.reverse { (link.target_id, link.source_id) } else { (link.source_id, link.target_id) };
    check_link(&mut tx, &source, &target, &kind, Some(&id)).await?;
    sqlx::query("UPDATE project_links SET source_id = ?, target_id = ?, kind = ?, label = ? WHERE id = ?")
        .bind(&source)
        .bind(&target)
        .bind(&kind)
        .bind(&label)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let link = link_by_id(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(link))
}

/// Remove a link. The projects stay.
#[utoipa::path(delete, path = "/links/{id}", tag = "links", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let link = visible_link(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM project_links WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    log_activity(
        &mut tx,
        &link.source_id,
        &user.id,
        "link.deleted",
        serde_json::json!({ "kind": link.kind, "target_id": link.target_id }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
