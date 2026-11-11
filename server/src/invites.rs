//! Invite links: the only way to make an account once the owner exists.
//!
//! An invite's token is shown once, inside its link, when it is created. Only
//! its SHA-256 is stored. Scope `server` only lets someone join; `account`
//! and `project` also share the inviter's whole account or one project with
//! them as they join.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{self, CurrentUser, User},
    error::{AppError, AppResult},
    projects::owned_project,
    sharing, time,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_invites, create_invite))
        .routes(routes!(revoke_invite))
        .routes(routes!(lookup_invite))
        .routes(routes!(accept_invite))
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct Invite {
    pub id: String,
    /// `server` (join only), `account` or `project`.
    pub scope: String,
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    /// For `account` and `project` invites: `viewer` or `editor`.
    pub role: String,
    pub include_children: bool,
    pub include_notes: bool,
    pub note: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub accepted_at: Option<String>,
    /// Name of the person who accepted it.
    pub accepted_by_name: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    /// A reminder of who it is for, shown in the list.
    pub note: Option<String>,
    /// 1 to 30. Defaults to 7.
    pub expires_in_days: Option<i64>,
    /// `server` (the default), `account` or `project`.
    pub scope: Option<String>,
    /// Required for a `project` invite. You must own it.
    pub project_id: Option<String>,
    /// `viewer` (the default) or `editor`.
    pub role: Option<String>,
    #[serde(default)]
    pub include_children: bool,
    #[serde(default)]
    pub include_notes: bool,
}

#[derive(Serialize, ToSchema)]
pub struct CreatedInvite {
    pub invite: Invite,
    /// The raw token. It is never shown again.
    pub token: String,
    /// The link to send, when the server knows its public URL.
    pub url: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct InviteLookup {
    pub invited_by: String,
    pub expires_at: String,
    /// `server`, `account` or `project`.
    pub scope: String,
    pub project_title: Option<String>,
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct AcceptInviteRequest {
    pub token: String,
    pub name: String,
    pub email: String,
    pub password: String,
}

const INVITE_SELECT: &str = "SELECT i.id, i.scope, i.project_id, p.title AS project_title, i.role,
        i.include_children, i.include_notes, i.note, i.created_at, i.expires_at,
        i.accepted_at, u.name AS accepted_by_name, i.revoked_at
    FROM invites i LEFT JOIN users u ON u.id = i.accepted_by LEFT JOIN projects p ON p.id = i.project_id";

fn require_owner(user: &User) -> AppResult<()> {
    if user.is_owner() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Only the owner can manage invites.".into()))
    }
}

/// Invites made on this install, newest first. Owner only.
#[utoipa::path(get, path = "/invites", tag = "invites",
    responses((status = 200, body = Vec<Invite>), (status = 403, body = crate::error::ErrorBody)))]
async fn list_invites(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<Invite>>> {
    require_owner(&user)?;
    let invites = sqlx::query_as::<_, Invite>(sqlx::AssertSqlSafe(format!(
        "{INVITE_SELECT} ORDER BY i.created_at DESC"
    )))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(invites))
}

/// Make an invite link. Owner only.
#[utoipa::path(post, path = "/invites", tag = "invites", request_body = CreateInviteRequest,
    responses((status = 201, body = CreatedInvite), (status = 403, body = crate::error::ErrorBody)))]
async fn create_invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<CreatedInvite>)> {
    require_owner(&user)?;
    let days = req.expires_in_days.unwrap_or(7);
    if !(1..=30).contains(&days) {
        return Err(AppError::BadRequest("invalid_expiry", "Invites last 1 to 30 days.".into()));
    }
    let note = req.note.map(|n| n.trim().chars().take(200).collect::<String>()).filter(|n| !n.is_empty());
    let scope = req.scope.unwrap_or_else(|| "server".into());
    let role = req.role.unwrap_or_else(|| "viewer".into());
    sharing::check_role(&role)?;
    let project_id = match scope.as_str() {
        "server" | "account" => None,
        "project" => {
            let Some(pid) = req.project_id else {
                return Err(AppError::BadRequest("missing_project", "Say which project to share.".into()));
            };
            owned_project(&mut *state.db.acquire().await?, &user.id, &pid).await?;
            Some(pid)
        }
        _ => {
            return Err(AppError::BadRequest(
                "invalid_scope",
                "An invite's scope is server, account or project.".into(),
            ));
        }
    };
    let (token, hash) = auth::new_token();
    let id = time::new_id();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO invites (id, token_hash, created_by, scope, project_id, role, include_children,
            include_notes, note, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&hash)
    .bind(&user.id)
    .bind(&scope)
    .bind(&project_id)
    .bind(&role)
    .bind(req.include_children && scope == "project")
    .bind(req.include_notes && scope == "account")
    .bind(&note)
    .bind(time::format(now + Duration::days(days)))
    .bind(time::format(now))
    .execute(&state.db)
    .await?;
    let invite = sqlx::query_as::<_, Invite>(sqlx::AssertSqlSafe(format!("{INVITE_SELECT} WHERE i.id = ?")))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    let url = state.config.public_url.as_ref().map(|base| format!("{base}/invite/{token}"));
    Ok((StatusCode::CREATED, Json(CreatedInvite { invite, token, url })))
}

/// Cancel an unused invite. Owner only.
#[utoipa::path(delete, path = "/invites/{id}", tag = "invites",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn revoke_invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    require_owner(&user)?;
    let changed = sqlx::query(
        "UPDATE invites SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL AND accepted_at IS NULL",
    )
    .bind(time::now())
    .bind(&id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Check an invite token before showing the sign-up form. Public.
#[utoipa::path(get, path = "/invites/lookup/{token}", tag = "invites",
    params(("token" = String, Path)),
    responses((status = 200, body = InviteLookup),
        (status = 404, body = crate::error::ErrorBody, description = "Unknown, used, revoked or expired")))]
async fn lookup_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Json<InviteLookup>> {
    let row: Option<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT u.name, i.expires_at, i.scope, p.title, i.role FROM invites i JOIN users u ON u.id = i.created_by
         LEFT JOIN projects p ON p.id = i.project_id
         WHERE i.token_hash = ? AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > ?",
    )
    .bind(auth::hash_token(&token))
    .bind(time::now())
    .fetch_optional(&state.db)
    .await?;
    let (invited_by, expires_at, scope, project_title, role) = row.ok_or(AppError::NotFound)?;
    Ok(Json(InviteLookup { invited_by, expires_at, scope, project_title, role }))
}

/// Make an account from an invite, and sign in. Public.
#[utoipa::path(post, path = "/auth/accept-invite", tag = "invites", request_body = AcceptInviteRequest,
    responses(
        (status = 200, body = User),
        (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody, description = "Invite unusable"),
        (status = 409, body = crate::error::ErrorBody, description = "Email already has an account"),
    ))]
async fn accept_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AcceptInviteRequest>,
) -> AppResult<Response> {
    let account = auth::validate_account(&req.name, &req.email, &req.password)?;
    // Check the invite before the slow password hash, and before saying
    // whether an email has an account, so neither is open to anyone without one.
    let live: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM invites WHERE token_hash = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(auth::hash_token(&req.token))
    .bind(time::now())
    .fetch_optional(&state.db)
    .await?;
    if live.is_none() {
        return Err(AppError::NotFound);
    }
    let password_hash = auth::hash_password(account.password).await?;
    let user_id = time::new_id();
    let now = time::now();

    let mut tx = state.db.begin().await?;
    let taken: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(&account.email)
        .fetch_optional(&mut *tx)
        .await?;
    if taken.is_some() {
        return Err(AppError::Conflict(
            "email_taken",
            "That email already has an account. Sign in instead.".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'member', ?, ?)",
    )
    .bind(&user_id)
    .bind(&account.email)
    .bind(&account.name)
    .bind(&password_hash)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    // Claim the invite in the same statement that checks it, so it can be used once.
    let claimed = sqlx::query(
        "UPDATE invites SET accepted_by = ?, accepted_at = ?
         WHERE token_hash = ? AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(&user_id)
    .bind(&now)
    .bind(auth::hash_token(&req.token))
    .bind(&now)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if claimed == 0 {
        return Err(AppError::NotFound);
    }
    let (scope, created_by, project_id, role, include_children, include_notes): (
        String,
        String,
        Option<String>,
        String,
        bool,
        bool,
    ) = sqlx::query_as(
        "SELECT scope, created_by, project_id, role, include_children, include_notes FROM invites
         WHERE token_hash = ?",
    )
    .bind(auth::hash_token(&req.token))
    .fetch_one(&mut *tx)
    .await?;
    match (scope.as_str(), project_id) {
        ("account", _) => {
            sharing::insert_account_share(&mut tx, &created_by, &user_id, &role, include_notes).await?;
        }
        ("project", Some(pid)) => {
            sharing::insert_project_share(&mut tx, &pid, &user_id, &role, include_children).await?;
        }
        _ => {}
    }
    auth::create_home_canvas(&mut tx, &user_id).await?;
    let cookie = auth::start_session(&mut tx, &state, &user_id, &headers).await?;
    let user = auth::user_by_id(&mut *tx, &user_id).await?;
    tx.commit().await?;
    tracing::info!(user = %user.id, "account created from invite");
    Ok(auth::signed_in(cookie, user))
}
