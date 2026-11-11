//! Accounts, sessions and login.
//!
//! The first person to open a new install creates the owner account through
//! `/auth/setup`. After that, accounts are made only by accepting an invite
//! (see [`crate::invites`]). Sessions are random tokens in an HttpOnly cookie;
//! only their SHA-256 is stored.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqliteConnection};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    error::{AppError, AppResult},
    time,
};

pub const SESSION_COOKIE: &str = "tmp_session";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(setup))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(update_preferences))
        .routes(routes!(change_password))
        .routes(routes!(list_sessions))
        .routes(routes!(end_other_sessions))
        .routes(routes!(end_session))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, Serialize, FromRow, ToSchema)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar: Option<String>,
    /// `owner` (the account that set up the install) or `member`.
    pub role: String,
    pub theme_id: String,
    /// `system`, `dark` or `light`.
    pub theme_mode: String,
    pub created_at: String,
}

impl User {
    pub fn is_owner(&self) -> bool {
        self.role == "owner"
    }
}

const USER_COLUMNS: &str = "id, email, name, avatar, role, theme_id, theme_mode, created_at";

#[derive(Serialize, ToSchema)]
pub struct AuthStatus {
    /// True until the owner account exists. The web app shows setup instead of login.
    pub needs_setup: bool,
    /// The signed-in person, if any.
    pub user: Option<User>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct PreferencesRequest {
    pub theme_id: Option<String>,
    /// `system`, `dark` or `light`.
    pub theme_mode: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// A browser or device signed in to this account.
#[derive(Serialize, FromRow, ToSchema)]
pub struct SessionInfo {
    /// A short handle for signing it out; not the session token.
    pub id: String,
    pub created_at: String,
    pub last_seen_at: String,
    /// The browser it signed in from, as it described itself.
    pub user_agent: Option<String>,
    /// The browser making this request.
    #[sqlx(default)]
    pub current: bool,
}

#[derive(Serialize, ToSchema)]
pub struct SessionsEnded {
    pub ended: u64,
}

// ---------------------------------------------------------------------------
// Passwords and tokens

pub const MIN_PASSWORD_LEN: usize = 10;

pub async fn hash_password(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(AppError::from)
}

async fn verify_password(password: String, hash: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|e| anyhow::anyhow!("bad hash: {e}"))?;
        Ok::<_, anyhow::Error>(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(AppError::from)
}

/// Verified against when the email is unknown, so a wrong email takes as
/// long as a wrong password.
static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(b"not a real password", &salt).unwrap().to_string()
});

/// A new random token and the SHA-256 hex of it, which is what gets stored.
pub fn new_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&raw);
    (raw, hash)
}

pub fn hash_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

// ---------------------------------------------------------------------------
// Validation

pub struct NewAccount {
    pub name: String,
    pub email: String,
    pub password: String,
}

pub fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN || password.len() > 1024 {
        return Err(AppError::BadRequest(
            "weak_password",
            format!("Use a password of at least {MIN_PASSWORD_LEN} characters."),
        ));
    }
    Ok(())
}

pub fn validate_account(name: &str, email: &str, password: &str) -> AppResult<NewAccount> {
    let name = name.trim();
    let email = email.trim().to_lowercase();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::BadRequest("invalid_name", "Enter a name up to 100 characters.".into()));
    }
    let valid_email = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        })
        && !email.contains(char::is_whitespace);
    if !valid_email {
        return Err(AppError::BadRequest("invalid_email", "Enter a valid email address.".into()));
    }
    validate_password(password)?;
    Ok(NewAccount { name: name.to_string(), email, password: password.to_string() })
}

// ---------------------------------------------------------------------------
// Sessions

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
}

fn session_cookie(state: &AppState, raw: &str, max_age_secs: i64) -> HeaderValue {
    let secure = if state.config.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={raw}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}"
    ))
    .expect("cookie is ascii")
}

/// Starts a session for `user_id` and returns the `Set-Cookie` value.
pub async fn start_session(
    conn: &mut SqliteConnection,
    state: &AppState,
    user_id: &str,
    headers: &HeaderMap,
) -> AppResult<HeaderValue> {
    let (raw, hash) = new_token();
    let now = Utc::now();
    let expires = now + Duration::days(state.config.session_days);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(255).collect::<String>());
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at, user_agent)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&hash)
    .bind(user_id)
    .bind(time::format(now))
    .bind(time::format(now))
    .bind(time::format(expires))
    .bind(user_agent)
    .execute(&mut *conn)
    .await?;
    Ok(session_cookie(state, &raw, state.config.session_days * 24 * 3600))
}

/// The signed-in person. Use as an extractor on any handler that needs login.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let raw = cookie_token(&parts.headers).ok_or(AppError::Unauthorized)?;
        let user = user_for_token(state, &raw).await?.ok_or(AppError::Unauthorized)?;
        Ok(CurrentUser(user))
    }
}

async fn user_for_token(state: &AppState, raw: &str) -> AppResult<Option<User>> {
    let hash = hash_token(raw);
    let now = time::now();
    let user = sqlx::query_as::<_, User>(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
        USER_COLUMNS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ")
    )))
    .bind(&hash)
    .bind(&now)
    .fetch_optional(&state.db)
    .await?;
    if user.is_some() {
        sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE token_hash = ?")
            .bind(&now)
            .bind(&hash)
            .execute(&state.db)
            .await?;
    }
    Ok(user)
}

pub async fn user_by_id<'e, E>(executor: E, id: &str) -> AppResult<User>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    sqlx::query_as::<_, User>(sqlx::AssertSqlSafe(format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?")))
        .bind(id)
        .fetch_optional(executor)
        .await?
        .ok_or(AppError::NotFound)
}

/// Every account gets a "Home" canvas when it is made.
pub async fn create_home_canvas(conn: &mut SqliteConnection, user_id: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO canvases (id, user_id, name, created_at) VALUES (?, ?, 'Home', ?)")
        .bind(time::new_id())
        .bind(user_id)
        .bind(time::now())
        .execute(conn)
        .await?;
    Ok(())
}

fn with_cookie(status: StatusCode, cookie: HeaderValue, body: impl Serialize) -> Response {
    let mut res = (status, Json(body)).into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie);
    res
}

pub(crate) fn signed_in(cookie: HeaderValue, user: User) -> Response {
    with_cookie(StatusCode::OK, cookie, user)
}

// ---------------------------------------------------------------------------
// Handlers

/// Whether the install needs setting up, and who is signed in.
#[utoipa::path(get, path = "/auth/status", tag = "auth",
    responses((status = 200, body = AuthStatus)))]
async fn status(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<AuthStatus>> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&state.db).await?;
    let user = match cookie_token(&headers) {
        Some(raw) => user_for_token(&state, &raw).await?,
        None => None,
    };
    Ok(Json(AuthStatus { needs_setup: users == 0, user }))
}

/// Create the owner account. Works only once, on an install with no accounts.
#[utoipa::path(post, path = "/auth/setup", tag = "auth", request_body = SetupRequest,
    responses(
        (status = 200, body = User, description = "Owner created and signed in"),
        (status = 400, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody, description = "Already set up"),
    ))]
async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> AppResult<Response> {
    let account = validate_account(&req.name, &req.email, &req.password)?;
    let hash = hash_password(account.password).await?;
    let id = time::new_id();
    let now = time::now();

    let mut tx = state.db.begin().await?;
    // One statement, so two people racing to set up cannot both win.
    let inserted = sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, role, created_at, updated_at)
         SELECT ?, ?, ?, ?, 'owner', ?, ? WHERE NOT EXISTS (SELECT 1 FROM users)",
    )
    .bind(&id)
    .bind(&account.email)
    .bind(&account.name)
    .bind(&hash)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Err(AppError::Conflict("already_set_up", "This install already has an owner.".into()));
    }
    create_home_canvas(&mut tx, &id).await?;
    let cookie = start_session(&mut tx, &state, &id, &headers).await?;
    let user = user_by_id(&mut *tx, &id).await?;
    tx.commit().await?;
    tracing::info!(user = %user.id, "owner account created");
    Ok(signed_in(cookie, user))
}

/// Sign in with email and password.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest,
    responses(
        (status = 200, body = User),
        (status = 400, body = crate::error::ErrorBody, description = "Wrong email or password"),
        (status = 429, body = crate::error::ErrorBody, description = "Too many wrong passwords; wait"),
    ))]
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<Response> {
    let email = req.email.trim().to_lowercase();
    if let Err(secs) = state.logins.check(&email) {
        let wait =
            if secs > 90 { format!("{} minutes", secs.div_ceil(60)) } else { format!("{secs} seconds") };
        return Err(AppError::TooManyRequests(format!(
            "Too many wrong passwords for that email. Try again in {wait}."
        )));
    }
    let row: Option<(String, String)> = sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?;
    let (id, hash) = match row {
        Some(r) => (Some(r.0), r.1),
        None => (None, DUMMY_HASH.clone()),
    };
    let ok = verify_password(req.password, hash).await?;
    let Some(id) = id.filter(|_| ok) else {
        state.logins.failed(&email);
        return Err(AppError::BadRequest(
            "invalid_credentials",
            "That email and password don't match an account.".into(),
        ));
    };
    state.logins.succeeded(&email);
    let mut conn = state.db.acquire().await?;
    let cookie = start_session(&mut conn, &state, &id, &headers).await?;
    let user = user_by_id(&mut *conn, &id).await?;
    Ok(signed_in(cookie, user))
}

/// Sign out of this browser.
#[utoipa::path(post, path = "/auth/logout", tag = "auth",
    responses((status = 204, description = "Signed out")))]
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(raw) = cookie_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(&raw))
            .execute(&state.db)
            .await?;
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&state, "", 0));
    Ok(res)
}

/// The signed-in person.
#[utoipa::path(get, path = "/me", tag = "auth",
    responses((status = 200, body = User), (status = 401, body = crate::error::ErrorBody)))]
async fn me(CurrentUser(user): CurrentUser) -> Json<User> {
    Json(user)
}

/// Save the signed-in person's theme choice.
#[utoipa::path(patch, path = "/me/preferences", tag = "auth", request_body = PreferencesRequest,
    responses((status = 200, body = User), (status = 400, body = crate::error::ErrorBody)))]
async fn update_preferences(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<PreferencesRequest>,
) -> AppResult<Json<User>> {
    let theme_id = match req.theme_id {
        Some(id) if state.themes.get(&id).is_none() => {
            return Err(AppError::BadRequest("unknown_theme", format!("No theme called {id}.")));
        }
        Some(id) => id,
        None => user.theme_id.clone(),
    };
    let theme_mode = match req.theme_mode.as_deref() {
        Some(m @ ("system" | "dark" | "light")) => m.to_string(),
        Some(_) => {
            return Err(AppError::BadRequest(
                "invalid_theme_mode",
                "Theme mode is system, dark or light.".into(),
            ));
        }
        None => user.theme_mode.clone(),
    };
    sqlx::query("UPDATE users SET theme_id = ?, theme_mode = ?, updated_at = ? WHERE id = ?")
        .bind(&theme_id)
        .bind(&theme_mode)
        .bind(time::now())
        .bind(&user.id)
        .execute(&state.db)
        .await?;
    Ok(Json(user_by_id(&state.db, &user.id).await?))
}

/// The session id the API shows: the start of the token's hash. Knowing it
/// doesn't let anyone sign in.
const SESSION_ID_LEN: usize = 16;

/// The hash of the session cookie on this request.
fn current_session_hash(headers: &HeaderMap) -> AppResult<String> {
    cookie_token(headers).map(|raw| hash_token(&raw)).ok_or(AppError::Unauthorized)
}

/// Change your password. Checks the current one first, then signs out every
/// other browser and device; this one stays signed in.
#[utoipa::path(post, path = "/me/password", tag = "auth", request_body = ChangePasswordRequest,
    responses(
        (status = 200, body = SessionsEnded, description = "Changed; how many other sessions were signed out"),
        (status = 400, body = crate::error::ErrorBody, description = "Wrong current password, or the new one is too short"),
        (status = 429, body = crate::error::ErrorBody, description = "Too many wrong passwords; wait"),
    ))]
async fn change_password(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<Json<SessionsEnded>> {
    let current = current_session_hash(&headers)?;
    // Same throttle as login, so a borrowed browser can't guess the password here.
    if let Err(secs) = state.logins.check(&user.email) {
        let wait =
            if secs > 90 { format!("{} minutes", secs.div_ceil(60)) } else { format!("{secs} seconds") };
        return Err(AppError::TooManyRequests(format!("Too many wrong passwords. Try again in {wait}.")));
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&state.db)
        .await?;
    if !verify_password(req.current_password, hash).await? {
        state.logins.failed(&user.email);
        return Err(AppError::BadRequest("wrong_password", "Your current password isn't right.".into()));
    }
    state.logins.succeeded(&user.email);
    validate_password(&req.new_password)?;
    let new_hash = hash_password(req.new_password).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(&new_hash)
        .bind(time::now())
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    let ended = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?")
        .bind(&user.id)
        .bind(&current)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    tracing::info!(user = %user.id, ended, "password changed");
    Ok(Json(SessionsEnded { ended }))
}

/// The browsers and devices signed in to your account, most recent first.
#[utoipa::path(get, path = "/me/sessions", tag = "auth",
    responses((status = 200, body = Vec<SessionInfo>), (status = 401, body = crate::error::ErrorBody)))]
async fn list_sessions(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
) -> AppResult<Json<Vec<SessionInfo>>> {
    let current = current_session_hash(&headers)?;
    let sessions = sqlx::query_as::<_, SessionInfo>(sqlx::AssertSqlSafe(format!(
        "SELECT substr(token_hash, 1, {SESSION_ID_LEN}) AS id, created_at, last_seen_at, user_agent,
                token_hash = ? AS current
         FROM sessions WHERE user_id = ? AND expires_at > ?
         ORDER BY token_hash = ? DESC, last_seen_at DESC"
    )))
    .bind(&current)
    .bind(&user.id)
    .bind(time::now())
    .bind(&current)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(sessions))
}

/// Sign out every browser and device except this one.
#[utoipa::path(post, path = "/me/sessions/sign-out-others", tag = "auth",
    responses((status = 200, body = SessionsEnded), (status = 401, body = crate::error::ErrorBody)))]
async fn end_other_sessions(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
) -> AppResult<Json<SessionsEnded>> {
    let current = current_session_hash(&headers)?;
    let ended = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?")
        .bind(&user.id)
        .bind(&current)
        .execute(&state.db)
        .await?
        .rows_affected();
    Ok(Json(SessionsEnded { ended }))
}

/// Sign out one browser or device. Use `/auth/logout` for this one.
#[utoipa::path(delete, path = "/me/sessions/{id}", tag = "auth", params(("id" = String, Path)),
    responses((status = 204, description = "Signed out"), (status = 404, body = crate::error::ErrorBody)))]
async fn end_session(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> AppResult<StatusCode> {
    let current = current_session_hash(&headers)?;
    if id.len() != SESSION_ID_LEN || !id.bytes().all(|b| b.is_ascii_hexdigit()) || current.starts_with(&id) {
        return Err(AppError::NotFound);
    }
    let ended = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND substr(token_hash, 1, ?) = ?")
        .bind(&user.id)
        .bind(SESSION_ID_LEN as i64)
        .bind(&id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if ended == 0 { Err(AppError::NotFound) } else { Ok(StatusCode::NO_CONTENT) }
}
