//! Sharing: give someone on this install your whole account or one project,
//! as a viewer or an editor. What a share grants is worked out by the
//! `project_access` and `note_access` views; this module only keeps the rows.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    projects::{owned_project, visible_project},
    time,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_people))
        .routes(routes!(list_shares, create_share))
        .routes(routes!(update_share, delete_share))
        .routes(routes!(project_people))
}

// ---------------------------------------------------------------------------
// Types

/// Someone with an account on this install.
#[derive(Serialize, FromRow, ToSchema)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct Share {
    pub id: String,
    /// `account` (everything the owner has) or `project` (one project).
    pub kind: String,
    pub owner_id: String,
    pub owner_name: String,
    /// Set for a project share.
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub grantee_id: String,
    pub grantee_name: String,
    pub grantee_email: String,
    /// `viewer` or `editor`.
    pub role: String,
    /// Project shares: projects spawned from it come too.
    pub include_children: bool,
    /// Account shares: knowledge notes come too, not only project notes.
    pub include_notes: bool,
    pub created_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct MyShares {
    /// Shares you have made.
    pub given: Vec<Share>,
    /// Shares others have made with you.
    pub received: Vec<Share>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateShareRequest {
    pub grantee_id: String,
    /// Share this project. Leave out to share your whole account.
    pub project_id: Option<String>,
    /// `viewer` (the default) or `editor`.
    pub role: Option<String>,
    #[serde(default)]
    pub include_children: bool,
    #[serde(default)]
    pub include_notes: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateShareRequest {
    pub role: Option<String>,
    pub include_children: Option<bool>,
    pub include_notes: Option<bool>,
}

/// Someone who can see a project, and why.
#[derive(Serialize, ToSchema)]
pub struct Member {
    pub user_id: String,
    pub name: String,
    pub email: String,
    /// `owner`, `editor` or `viewer`.
    pub role: String,
    /// `owner`, `project` (shared directly), `account` (the owner shared
    /// everything) or `family` (shared through a parent project).
    pub via: String,
    /// The share to change or remove, for `project` and `account`.
    pub share_id: Option<String>,
}

const SHARE_SELECT: &str =
    "SELECT s.id, CASE WHEN s.project_id IS NULL THEN 'account' ELSE 'project' END AS kind,
        COALESCE(s.owner_id, p.owner_id) AS owner_id, o.name AS owner_name,
        s.project_id, p.title AS project_title,
        s.grantee_id, g.name AS grantee_name, g.email AS grantee_email,
        s.role, s.include_children, s.include_notes, s.created_at
    FROM shares s
    LEFT JOIN projects p ON p.id = s.project_id
    JOIN users o ON o.id = COALESCE(s.owner_id, p.owner_id)
    JOIN users g ON g.id = s.grantee_id";

pub fn check_role(role: &str) -> AppResult<()> {
    if role == "viewer" || role == "editor" {
        Ok(())
    } else {
        Err(AppError::BadRequest("invalid_role", "A share is either viewer or editor.".into()))
    }
}

/// Shares a project, or updates the share if there is one.
pub async fn insert_project_share(
    conn: &mut SqliteConnection,
    project_id: &str,
    grantee_id: &str,
    role: &str,
    include_children: bool,
) -> AppResult<String> {
    sqlx::query(
        "INSERT INTO shares (id, project_id, grantee_id, role, include_children, include_notes, created_at)
         VALUES (?, ?, ?, ?, ?, 0, ?)
         ON CONFLICT (project_id, grantee_id) WHERE project_id IS NOT NULL
         DO UPDATE SET role = excluded.role, include_children = excluded.include_children",
    )
    .bind(time::new_id())
    .bind(project_id)
    .bind(grantee_id)
    .bind(role)
    .bind(include_children)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    Ok(sqlx::query_scalar("SELECT id FROM shares WHERE project_id = ? AND grantee_id = ?")
        .bind(project_id)
        .bind(grantee_id)
        .fetch_one(conn)
        .await?)
}

/// Shares everything `owner_id` has, or updates the share if there is one.
pub async fn insert_account_share(
    conn: &mut SqliteConnection,
    owner_id: &str,
    grantee_id: &str,
    role: &str,
    include_notes: bool,
) -> AppResult<String> {
    sqlx::query(
        "INSERT INTO shares (id, owner_id, grantee_id, role, include_children, include_notes, created_at)
         VALUES (?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT (owner_id, grantee_id) WHERE owner_id IS NOT NULL
         DO UPDATE SET role = excluded.role, include_notes = excluded.include_notes",
    )
    .bind(time::new_id())
    .bind(owner_id)
    .bind(grantee_id)
    .bind(role)
    .bind(include_notes)
    .bind(time::now())
    .execute(&mut *conn)
    .await?;
    Ok(sqlx::query_scalar("SELECT id FROM shares WHERE owner_id = ? AND grantee_id = ?")
        .bind(owner_id)
        .bind(grantee_id)
        .fetch_one(conn)
        .await?)
}

async fn share_by_id(conn: &mut SqliteConnection, id: &str) -> AppResult<Share> {
    sqlx::query_as::<_, Share>(&format!("{SHARE_SELECT} WHERE s.id = ?"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

// ---------------------------------------------------------------------------
// Handlers

/// Everyone else with an account here, to share with.
#[utoipa::path(get, path = "/people", tag = "sharing", responses((status = 200, body = Vec<Person>)))]
async fn list_people(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<Person>>> {
    let people = sqlx::query_as::<_, Person>("SELECT id, name, email FROM users WHERE id <> ? ORDER BY name")
        .bind(&user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(people))
}

/// Shares you have made and shares made with you.
#[utoipa::path(get, path = "/shares", tag = "sharing", responses((status = 200, body = MyShares)))]
async fn list_shares(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<MyShares>> {
    let given = sqlx::query_as::<_, Share>(&format!(
        "{SHARE_SELECT} WHERE COALESCE(s.owner_id, p.owner_id) = ? ORDER BY s.project_id IS NOT NULL, p.title, g.name"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    let received = sqlx::query_as::<_, Share>(&format!(
        "{SHARE_SELECT} WHERE s.grantee_id = ? ORDER BY s.project_id IS NOT NULL, o.name, p.title"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(MyShares { given, received }))
}

/// Share your whole account, or one of your projects, with someone here.
#[utoipa::path(post, path = "/shares", tag = "sharing", request_body = CreateShareRequest,
    responses((status = 201, body = Share), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
async fn create_share(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateShareRequest>,
) -> AppResult<(StatusCode, Json<Share>)> {
    let role = req.role.unwrap_or_else(|| "viewer".into());
    check_role(&role)?;
    let mut tx = state.db.begin().await?;
    let grantee: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE id = ?")
        .bind(&req.grantee_id)
        .fetch_optional(&mut *tx)
        .await?;
    if grantee.is_none() {
        return Err(AppError::BadRequest("unknown_person", "That person has no account here.".into()));
    }
    if req.grantee_id == user.id {
        return Err(AppError::BadRequest("self_share", "You already have it.".into()));
    }
    let id = match &req.project_id {
        Some(project_id) => {
            owned_project(&mut tx, &user.id, project_id).await?;
            insert_project_share(&mut tx, project_id, &req.grantee_id, &role, req.include_children).await?
        }
        None => insert_account_share(&mut tx, &user.id, &req.grantee_id, &role, req.include_notes).await?,
    };
    let share = share_by_id(&mut tx, &id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(share)))
}

/// Change a share's role or what it takes in. Only the person who shared.
#[utoipa::path(patch, path = "/shares/{id}", tag = "sharing", params(("id" = String, Path)),
    request_body = UpdateShareRequest,
    responses((status = 200, body = Share), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
async fn update_share(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateShareRequest>,
) -> AppResult<Json<Share>> {
    let mut tx = state.db.begin().await?;
    let share = share_by_id(&mut tx, &id).await?;
    if share.owner_id != user.id {
        return Err(AppError::NotFound);
    }
    let role = req.role.unwrap_or(share.role);
    check_role(&role)?;
    sqlx::query("UPDATE shares SET role = ?, include_children = ?, include_notes = ? WHERE id = ?")
        .bind(&role)
        .bind(req.include_children.unwrap_or(share.include_children) && share.kind == "project")
        .bind(req.include_notes.unwrap_or(share.include_notes) && share.kind == "account")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let share = share_by_id(&mut tx, &id).await?;
    tx.commit().await?;
    Ok(Json(share))
}

/// Stop sharing. The person who shared can remove it, and so can the person
/// it was shared with, to leave.
#[utoipa::path(delete, path = "/shares/{id}", tag = "sharing", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn delete_share(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let share = share_by_id(&mut tx, &id).await?;
    if share.owner_id != user.id && share.grantee_id != user.id {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM shares WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Everyone who can see a project, owner first.
#[utoipa::path(get, path = "/projects/{id}/people", tag = "sharing", params(("id" = String, Path)),
    responses((status = 200, body = Vec<Member>), (status = 404, body = crate::error::ErrorBody)))]
async fn project_people(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Member>>> {
    let mut conn = state.db.acquire().await?;
    let project = visible_project(&mut conn, &user.id, &id).await?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, a.role,
            (SELECT s.id FROM shares s WHERE s.project_id = ?1 AND s.grantee_id = u.id),
            (SELECT s.id FROM shares s WHERE s.owner_id = ?2 AND s.grantee_id = u.id)
         FROM project_access a JOIN users u ON u.id = a.user_id
         WHERE a.project_id = ?1
         ORDER BY a.role = 'owner' DESC, a.role = 'editor' DESC, u.name",
    )
    .bind(&id)
    .bind(&project.owner_id)
    .fetch_all(&mut *conn)
    .await?;
    let members = rows
        .into_iter()
        .map(|(user_id, name, email, role, project_share, account_share)| {
            let (via, share_id) = if user_id == project.owner_id {
                ("owner", None)
            } else if let Some(s) = project_share {
                ("project", Some(s))
            } else if let Some(s) = account_share {
                ("account", Some(s))
            } else {
                ("family", None)
            };
            Member { user_id, name, email, role, via: via.into(), share_id }
        })
        .collect();
    Ok(Json(members))
}
