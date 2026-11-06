//! API tokens: how an AI assistant signs in to the MCP server. Each token
//! acts as the person who made it, either read-only or read-write.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, User, hash_token, new_token},
    error::{AppError, AppResult},
    time,
};

/// Every secret starts with this, so a leaked one is easy to spot.
pub const PREFIX: &str = "tmp_";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_tokens, create_token)).routes(routes!(revoke_token))
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    /// `read` or `write`.
    pub scope: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    /// What it's for, such as "Claude on my laptop".
    pub name: String,
    /// `read` (the default) or `write`.
    pub scope: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreatedToken {
    pub token: ApiToken,
    /// Shown once. Only a hash is stored.
    pub secret: String,
}

/// Who an MCP request acts for, found from its bearer token.
#[derive(Clone, Debug)]
pub struct TokenCaller {
    pub user: User,
    pub token_name: String,
    pub can_write: bool,
}

const COLUMNS: &str = "id, name, scope, created_at, last_used_at, revoked_at";

/// The person and scope behind a bearer token, if it is live.
pub async fn caller_for(state: &AppState, secret: &str) -> AppResult<Option<TokenCaller>> {
    let Some(raw) = secret.strip_prefix(PREFIX) else { return Ok(None) };
    let hash = hash_token(raw);
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT user_id, name, scope FROM api_tokens WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;
    let Some((user_id, token_name, scope)) = row else { return Ok(None) };
    let user = crate::auth::user_by_id(&state.db, &user_id).await?;
    sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE token_hash = ?")
        .bind(time::now())
        .bind(&hash)
        .execute(&state.db)
        .await?;
    Ok(Some(TokenCaller { user, token_name, can_write: scope == "write" }))
}

/// Your tokens, newest first, revoked ones included.
#[utoipa::path(get, path = "/tokens", tag = "tokens", responses((status = 200, body = Vec<ApiToken>)))]
async fn list_tokens(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<ApiToken>>> {
    let rows = sqlx::query_as::<_, ApiToken>(&format!(
        "SELECT {COLUMNS} FROM api_tokens WHERE user_id = ? ORDER BY revoked_at IS NOT NULL, created_at DESC"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Make a token for an AI assistant. The secret is in the reply and nowhere else.
#[utoipa::path(post, path = "/tokens", tag = "tokens", request_body = CreateTokenRequest,
    responses((status = 201, body = CreatedToken), (status = 400, body = crate::error::ErrorBody)))]
async fn create_token(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateTokenRequest>,
) -> AppResult<(StatusCode, Json<CreatedToken>)> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::BadRequest("invalid_name", "Name the token, in up to 80 characters.".into()));
    }
    let scope = req.scope.as_deref().unwrap_or("read");
    if scope != "read" && scope != "write" {
        return Err(AppError::BadRequest("invalid_scope", "A token is either read or write.".into()));
    }
    let live: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM api_tokens WHERE user_id = ? AND revoked_at IS NULL")
            .bind(&user.id)
            .fetch_one(&state.db)
            .await?;
    if live >= 50 {
        return Err(AppError::BadRequest(
            "too_many_tokens",
            "Revoke an old token first; 50 is the most.".into(),
        ));
    }
    let (raw, hash) = new_token();
    let id = time::new_id();
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scope, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(name)
    .bind(&hash)
    .bind(scope)
    .bind(time::now())
    .execute(&state.db)
    .await?;
    let token = sqlx::query_as::<_, ApiToken>(&format!("SELECT {COLUMNS} FROM api_tokens WHERE id = ?"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(CreatedToken { token, secret: format!("{PREFIX}{raw}") })))
}

/// Stop a token working. It stays in the list, marked revoked.
#[utoipa::path(delete, path = "/tokens/{id}", tag = "tokens", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn revoke_token(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let done = sqlx::query(
        "UPDATE api_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ? AND user_id = ?",
    )
    .bind(time::now())
    .bind(&id)
    .bind(&user.id)
    .execute(&state.db)
    .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
