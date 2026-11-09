//! Signing in to the MCP server with OAuth 2.1, so an app such as Claude
//! connects by the person pressing Allow instead of pasting a token.
//!
//! The app is its own authorization server. An MCP client finds it through
//! the two `/.well-known` documents, registers itself (`/oauth/register`,
//! public clients only), sends the person to `/oauth/authorize` (which hands
//! over to the web app's Allow page at `/connect`), and swaps the code it gets
//! back for tokens at `/oauth/token` using PKCE. Access tokens last an hour;
//! refresh tokens 30 days and are replaced on every use. A connection acts as
//! the person who allowed it, exactly like an API token.

use axum::{
    Json, Router,
    extract::{Form, Path, Query, RawQuery, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, hash_token, new_token},
    error::{AppError, AppResult},
    time,
    tokens::TokenCaller,
};

/// Access tokens start with this, refresh tokens with [`REFRESH_PREFIX`].
pub const ACCESS_PREFIX: &str = "tmp_at_";
pub const REFRESH_PREFIX: &str = "tmp_rt_";
const ACCESS_SECS: i64 = 3600;
const REFRESH_DAYS: i64 = 30;
const CODE_MINUTES: i64 = 10;
/// Registered apps that never connected are cleared after a day; past this
/// many, registration is refused until some are.
const MAX_CLIENTS: i64 = 500;

// ---------------------------------------------------------------------------
// Where the app is

/// The address people and apps reach this install at, such as
/// `https://projects.example.com`. `TMP_PUBLIC_URL` wins when the request came
/// to that host, or to a local address a proxy put there; otherwise the request's own host is used, with the scheme a
/// reverse proxy passed on in `X-Forwarded-Proto`.
pub fn base_url(state: &AppState, headers: &HeaderMap) -> String {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|p| p.split(',').next())
        .map(str::trim)
        .filter(|p| *p == "https" || *p == "http");
    // A request that came in over HTTPS (the app's own, or a proxy's) never
    // gets sent back to a plain http:// public address.
    let public =
        state.config.public_url.as_ref().filter(|p| proto != Some("https") || p.starts_with("https://"));
    if let Some(public) = public {
        let public_host = public.split_once("://").map(|(_, h)| h).unwrap_or(public);
        // A proxy that rewrote Host to a local address (Tailscale, some tunnels)
        // still means the public address.
        let local = host.is_some_and(|h| {
            let name = h
                .rsplit_once(':')
                .filter(|(n, _)| !n.ends_with(']') || n.starts_with('['))
                .map_or(h, |(n, _)| n);
            is_loopback(name)
        });
        if local || host.is_none_or(|h| h.eq_ignore_ascii_case(public_host)) {
            return public.clone();
        }
    }
    let Some(host) = host else { return "http://localhost:8080".into() };
    format!("{}://{host}", proto.unwrap_or("http"))
}

/// Where an MCP client reads how to sign in, for the 401 from `/mcp`.
pub fn resource_metadata_url(base: &str) -> String {
    format!("{base}/.well-known/oauth-protected-resource/mcp")
}

// ---------------------------------------------------------------------------
// Routes

/// The discovery documents and the endpoints apps call directly. Apps call
/// these from anywhere, without cookies, so they allow any origin.
pub fn routes() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            "mcp-protocol-version".parse().unwrap(),
        ]);
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(protected_resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(protected_resource))
        .route("/.well-known/oauth-authorization-server", get(authorization_server))
        .route("/.well-known/openid-configuration", get(authorization_server))
        .route("/oauth/register", post(register))
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(cors)
        // A browser page, not an API call: no CORS.
        .route("/oauth/authorize", get(authorize))
}

/// The signed-in routes the Allow page and Settings use, under `/api/v1`.
pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(describe_request))
        .routes(routes!(approve))
        .routes(routes!(list_connections))
        .routes(routes!(disconnect))
}

async fn protected_resource(State(state): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let base = base_url(&state, &headers);
    Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "scopes_supported": ["read", "write"],
        "bearer_methods_supported": ["header"],
        "resource_name": "TooManyProjects",
    }))
}

async fn authorization_server(State(state): State<AppState>, headers: HeaderMap) -> Json<serde_json::Value> {
    let base = base_url(&state, &headers);
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["read", "write"],
        "authorization_response_iss_parameter_supported": true,
    }))
}

/// Hands the person to the web app's Allow page, which signs them in first
/// if needed. The page checks the request before showing anything.
async fn authorize(RawQuery(query): RawQuery) -> Redirect {
    match query {
        Some(q) => Redirect::to(&format!("/connect?{q}")),
        None => Redirect::to("/connect"),
    }
}

// ---------------------------------------------------------------------------
// Errors in OAuth's own shape

struct OAuthError(StatusCode, &'static str, String);

impl OAuthError {
    fn bad(code: &'static str, description: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, code, description.into())
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1, "error_description": self.2 }))).into_response()
    }
}

impl From<AppError> for OAuthError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::BadRequest(_, m) => OAuthError::bad("invalid_request", m),
            other => {
                tracing::error!(error = ?other, "oauth error");
                OAuthError(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Something went wrong.".into())
            }
        }
    }
}

impl From<sqlx::Error> for OAuthError {
    fn from(e: sqlx::Error) -> Self {
        AppError::from(e).into()
    }
}

// ---------------------------------------------------------------------------
// Return addresses

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// HTTPS anywhere, or plain HTTP to this computer (Claude Code and other
/// desktop apps listen on a local port). Nothing else, and no fragments.
fn redirect_allowed(uri: &str) -> bool {
    let Ok(url) = Url::parse(uri) else { return false };
    if url.fragment().is_some() || url.username() != "" || url.password().is_some() {
        return false;
    }
    match (url.scheme(), url.host_str()) {
        ("https", Some(_)) => true,
        ("http", Some(host)) => is_loopback(host),
        _ => false,
    }
}

/// Exact match, except that a local address may use any port (RFC 8252 7.3).
fn redirect_matches(registered: &str, given: &str) -> bool {
    if registered == given {
        return true;
    }
    let (Ok(mut a), Ok(mut b)) = (Url::parse(registered), Url::parse(given)) else { return false };
    if a.scheme() != "http" || !a.host_str().is_some_and(is_loopback) {
        return false;
    }
    let _ = a.set_port(None);
    let _ = b.set_port(None);
    a == b
}

fn redirect_host(uri: &str) -> String {
    Url::parse(uri).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let ok_len = (43..=128).contains(&verifier.len());
    let ok_chars = verifier.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b));
    ok_len && ok_chars && URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}

// ---------------------------------------------------------------------------
// Registration

#[derive(Deserialize)]
struct RegisterRequest {
    client_name: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
}

#[derive(FromRow)]
struct Client {
    id: String,
    name: String,
    redirect_uris: String,
}

impl Client {
    fn uris(&self) -> Vec<String> {
        serde_json::from_str(&self.redirect_uris).unwrap_or_default()
    }
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), OAuthError> {
    if req.redirect_uris.is_empty() || req.redirect_uris.len() > 10 {
        return Err(OAuthError::bad("invalid_redirect_uri", "Give between one and ten redirect_uris."));
    }
    if let Some(bad) = req.redirect_uris.iter().find(|u| u.len() > 2000 || !redirect_allowed(u)) {
        return Err(OAuthError::bad(
            "invalid_redirect_uri",
            format!("{bad} is not allowed: use https://, or http:// to localhost."),
        ));
    }
    let name: String = req.client_name.as_deref().map(str::trim).unwrap_or("").chars().take(80).collect();
    let name = if name.is_empty() { "An app".to_string() } else { name };

    let day_ago = time::format(Utc::now() - Duration::days(1));
    sqlx::query(
        "DELETE FROM oauth_clients WHERE created_at < ?
           AND id NOT IN (SELECT client_id FROM oauth_connections)
           AND id NOT IN (SELECT client_id FROM oauth_codes)",
    )
    .bind(&day_ago)
    .execute(&state.db)
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_clients").fetch_one(&state.db).await?;
    if count >= MAX_CLIENTS {
        return Err(OAuthError::bad(
            "invalid_client_metadata",
            "Too many apps registered today; try again tomorrow.",
        ));
    }

    let id = time::new_id();
    let now = Utc::now();
    sqlx::query("INSERT INTO oauth_clients (id, name, redirect_uris, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(serde_json::to_string(&req.redirect_uris).unwrap())
        .bind(time::format(now))
        .execute(&state.db)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "client_id": id,
            "client_name": name,
            "client_id_issued_at": now.timestamp(),
            "redirect_uris": req.redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })),
    ))
}

async fn client_by_id(state: &AppState, id: &str) -> AppResult<Option<Client>> {
    Ok(sqlx::query_as::<_, Client>("SELECT id, name, redirect_uris FROM oauth_clients WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?)
}

// ---------------------------------------------------------------------------
// The Allow page

#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct AuthorizeParams {
    pub client_id: String,
    pub redirect_uri: String,
    /// Must be `code`.
    pub response_type: Option<String>,
    /// The PKCE challenge: base64url SHA-256 of the app's verifier.
    pub code_challenge: Option<String>,
    /// Must be `S256`.
    pub code_challenge_method: Option<String>,
    /// What the app asked for: `read`, `write` or both, space separated.
    pub scope: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectRequest {
    /// The name the app gave itself, such as "Claude".
    pub client_name: String,
    /// Where the person goes back to, such as claude.ai.
    pub redirect_host: String,
    /// Set when the return address looks wrong for the app's name.
    pub warning: Option<String>,
    /// The access to preselect: `read` when the app asked for read only.
    pub suggested_scope: String,
}

/// Checks an authorize request and returns its app. Errors are for the
/// person: an app that sent a bad request is never redirected back to.
async fn check(state: &AppState, p: &AuthorizeParams) -> AppResult<Client> {
    let bad = |m: &str| AppError::BadRequest("invalid_connect_request", m.to_string());
    let client = client_by_id(state, &p.client_id).await?.ok_or_else(|| {
        bad("This app isn't registered here. Remove the connector in the app and add it again.")
    })?;
    if !client.uris().iter().any(|r| redirect_matches(r, &p.redirect_uri)) {
        return Err(bad("The app asked to return somewhere it didn't register. Nothing was shared."));
    }
    if p.response_type.as_deref() != Some("code") {
        return Err(bad("The app asked for an unsupported kind of sign-in (response_type must be code)."));
    }
    let challenge = p.code_challenge.as_deref().unwrap_or("");
    if challenge.len() < 43 || p.code_challenge_method.as_deref() != Some("S256") {
        return Err(bad("The app didn't send a PKCE S256 challenge, which this server needs."));
    }
    Ok(client)
}

fn suggested_scope(scope: Option<&str>) -> &'static str {
    match scope {
        Some(s)
            if s.split_whitespace().any(|w| w == "read") && !s.split_whitespace().any(|w| w == "write") =>
        {
            "read"
        }
        _ => "write",
    }
}

/// What an app is asking for, for the Allow page.
#[utoipa::path(get, path = "/oauth/request", tag = "oauth", params(AuthorizeParams),
    responses((status = 200, body = ConnectRequest), (status = 400, body = crate::error::ErrorBody)))]
async fn describe_request(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Query(p): Query<AuthorizeParams>,
) -> AppResult<Json<ConnectRequest>> {
    let client = check(&state, &p).await?;
    let host = redirect_host(&p.redirect_uri);
    let known = is_loopback(&host)
        || ["claude.ai", "claude.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")));
    let warning = (!known && client.name.to_lowercase().contains("claude")).then(|| {
        format!("This app calls itself “{}” but will send you to {host}. Only allow it if you started this there.", client.name)
    });
    Ok(Json(ConnectRequest {
        client_name: client.name,
        redirect_host: host,
        warning,
        suggested_scope: suggested_scope(p.scope.as_deref()).into(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ApproveRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub response_type: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    /// Passed back to the app untouched.
    pub state: Option<String>,
    /// `read` or `write`.
    pub scope: String,
    /// False for Cancel.
    pub allow: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ApproveResponse {
    /// Where to send the browser next: back to the app.
    pub redirect_to: String,
}

/// Allow (or cancel) a connection. Returns the address to send the person back to.
#[utoipa::path(post, path = "/oauth/approve", tag = "oauth", request_body = ApproveRequest,
    responses((status = 200, body = ApproveResponse), (status = 400, body = crate::error::ErrorBody)))]
async fn approve(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
    Json(req): Json<ApproveRequest>,
) -> AppResult<Json<ApproveResponse>> {
    if req.scope != "read" && req.scope != "write" {
        return Err(AppError::BadRequest("invalid_scope", "Choose read only or read and write.".into()));
    }
    let params = AuthorizeParams {
        client_id: req.client_id,
        redirect_uri: req.redirect_uri,
        response_type: req.response_type,
        code_challenge: req.code_challenge,
        code_challenge_method: req.code_challenge_method,
        scope: None,
    };
    let client = check(&state, &params).await?;
    let code = if req.allow {
        let (raw, hash) = new_token();
        sqlx::query(
            "INSERT INTO oauth_codes (code_hash, user_id, client_id, redirect_uri, code_challenge, scope, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&hash)
        .bind(&user.id)
        .bind(&client.id)
        .bind(&params.redirect_uri)
        .bind(params.code_challenge.as_deref().unwrap_or(""))
        .bind(&req.scope)
        .bind(time::format(Utc::now() + Duration::minutes(CODE_MINUTES)))
        .execute(&state.db)
        .await?;
        Some(raw)
    } else {
        None
    };
    let mut url = Url::parse(&params.redirect_uri)
        .map_err(|_| AppError::BadRequest("invalid_connect_request", "Bad return address.".into()))?;
    {
        let mut q = url.query_pairs_mut();
        match &code {
            Some(code) => q.append_pair("code", code),
            None => q.append_pair("error", "access_denied"),
        };
        if let Some(s) = &req.state {
            q.append_pair("state", s);
        }
        q.append_pair("iss", &base_url(&state, &headers));
    }
    Ok(Json(ApproveResponse { redirect_to: url.to_string() }))
}

// ---------------------------------------------------------------------------
// Tokens

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

#[derive(FromRow)]
struct CodeRow {
    user_id: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    scope: String,
    expires_at: String,
}

async fn token(
    State(state): State<AppState>,
    Form(f): Form<TokenForm>,
) -> Result<Json<serde_json::Value>, OAuthError> {
    let client_id = f.client_id.as_deref().unwrap_or("");
    match f.grant_type.as_str() {
        "authorization_code" => {
            let code = f.code.as_deref().unwrap_or("");
            let mut tx = state.db.begin().await?;
            let row: Option<CodeRow> = sqlx::query_as(
                "DELETE FROM oauth_codes WHERE code_hash = ?
                 RETURNING user_id, client_id, redirect_uri, code_challenge, scope, expires_at",
            )
            .bind(hash_token(code))
            .fetch_optional(&mut *tx)
            .await?;
            let invalid = || OAuthError::bad("invalid_grant", "The code is unknown, used or expired.");
            let row = row.ok_or_else(invalid)?;
            if row.expires_at < time::now() || row.client_id != client_id {
                tx.commit().await?;
                return Err(invalid());
            }
            if f.redirect_uri.as_deref() != Some(row.redirect_uri.as_str()) {
                tx.commit().await?;
                return Err(OAuthError::bad(
                    "invalid_grant",
                    "redirect_uri doesn't match the one used to sign in.",
                ));
            }
            if !pkce_matches(f.code_verifier.as_deref().unwrap_or(""), &row.code_challenge) {
                tx.commit().await?;
                return Err(OAuthError::bad("invalid_grant", "The PKCE code_verifier doesn't match."));
            }
            let name: String = sqlx::query_scalar("SELECT name FROM oauth_clients WHERE id = ?")
                .bind(&row.client_id)
                .fetch_one(&mut *tx)
                .await?;
            let now = time::now();
            let connection_id: String = sqlx::query_scalar(
                "INSERT INTO oauth_connections (id, user_id, client_id, client_name, redirect_host, scope, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (user_id, client_id) DO UPDATE SET
                   scope = excluded.scope,
                   client_name = excluded.client_name,
                   redirect_host = excluded.redirect_host,
                   created_at = CASE WHEN revoked_at IS NULL THEN created_at ELSE excluded.created_at END,
                   last_used_at = CASE WHEN revoked_at IS NULL THEN last_used_at ELSE NULL END,
                   revoked_at = NULL
                 RETURNING id",
            )
            .bind(time::new_id())
            .bind(&row.user_id)
            .bind(&row.client_id)
            .bind(&name)
            .bind(redirect_host(&row.redirect_uri))
            .bind(&row.scope)
            .bind(&now)
            .fetch_one(&mut *tx)
            .await?;
            let body = issue(&mut tx, &connection_id, &row.scope).await?;
            tx.commit().await?;
            Ok(Json(body))
        }
        "refresh_token" => {
            let raw = f.refresh_token.as_deref().unwrap_or("");
            let raw = raw.strip_prefix(REFRESH_PREFIX).unwrap_or("");
            let mut tx = state.db.begin().await?;
            let hash = hash_token(raw);
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT t.connection_id, c.client_id, c.scope FROM oauth_tokens t
                 JOIN oauth_connections c ON c.id = t.connection_id
                 WHERE t.token_hash = ? AND t.kind = 'refresh' AND t.expires_at > ? AND c.revoked_at IS NULL",
            )
            .bind(&hash)
            .bind(time::now())
            .fetch_optional(&mut *tx)
            .await?;
            let invalid =
                || OAuthError::bad("invalid_grant", "The refresh token is unknown, used or expired.");
            let (connection_id, owner, scope) = row.ok_or_else(invalid)?;
            if !client_id.is_empty() && owner != client_id {
                return Err(OAuthError::bad("invalid_grant", "The refresh token belongs to another app."));
            }
            // Each refresh token works once; a second use loses the race here.
            let used = sqlx::query("DELETE FROM oauth_tokens WHERE token_hash = ?")
                .bind(&hash)
                .execute(&mut *tx)
                .await?;
            if used.rows_affected() != 1 {
                return Err(invalid());
            }
            sqlx::query("DELETE FROM oauth_tokens WHERE connection_id = ? AND expires_at <= ?")
                .bind(&connection_id)
                .bind(time::now())
                .execute(&mut *tx)
                .await?;
            let body = issue(&mut tx, &connection_id, &scope).await?;
            tx.commit().await?;
            Ok(Json(body))
        }
        _ => Err(OAuthError::bad("unsupported_grant_type", "Use authorization_code or refresh_token.")),
    }
}

/// A fresh access and refresh token for a connection, as the token reply.
async fn issue(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    connection_id: &str,
    scope: &str,
) -> AppResult<serde_json::Value> {
    let now = Utc::now();
    let (access, access_hash) = new_token();
    let (refresh, refresh_hash) = new_token();
    for (hash, kind, expires) in [
        (&access_hash, "access", now + Duration::seconds(ACCESS_SECS)),
        (&refresh_hash, "refresh", now + Duration::days(REFRESH_DAYS)),
    ] {
        sqlx::query(
            "INSERT INTO oauth_tokens (token_hash, connection_id, kind, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(hash)
        .bind(connection_id)
        .bind(kind)
        .bind(time::format(expires))
        .execute(&mut **tx)
        .await?;
    }
    Ok(json!({
        "access_token": format!("{ACCESS_PREFIX}{access}"),
        "token_type": "Bearer",
        "expires_in": ACCESS_SECS,
        "refresh_token": format!("{REFRESH_PREFIX}{refresh}"),
        "scope": scope,
    }))
}

#[derive(Deserialize)]
struct RevokeForm {
    token: String,
}

/// RFC 7009. A refresh token ends its whole connection; an access token
/// just itself. Unknown tokens are fine.
async fn revoke(State(state): State<AppState>, Form(f): Form<RevokeForm>) -> Result<StatusCode, OAuthError> {
    if let Some(raw) = f.token.strip_prefix(REFRESH_PREFIX) {
        let connection: Option<String> = sqlx::query_scalar(
            "SELECT connection_id FROM oauth_tokens WHERE token_hash = ? AND kind = 'refresh'",
        )
        .bind(hash_token(raw))
        .fetch_optional(&state.db)
        .await?;
        if let Some(id) = connection {
            end_connection(&state, &id).await?;
        }
    } else if let Some(raw) = f.token.strip_prefix(ACCESS_PREFIX) {
        sqlx::query("DELETE FROM oauth_tokens WHERE token_hash = ?")
            .bind(hash_token(raw))
            .execute(&state.db)
            .await?;
    }
    Ok(StatusCode::OK)
}

async fn end_connection(state: &AppState, id: &str) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE oauth_connections SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?")
        .bind(time::now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM oauth_tokens WHERE connection_id = ?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Who an `Authorization: Bearer tmp_at_…` acts for, if it is live.
pub async fn caller_for(state: &AppState, secret: &str) -> AppResult<Option<TokenCaller>> {
    let Some(raw) = secret.strip_prefix(ACCESS_PREFIX) else { return Ok(None) };
    let now = time::now();
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT c.id, c.user_id, c.client_name, c.scope FROM oauth_tokens t
         JOIN oauth_connections c ON c.id = t.connection_id
         WHERE t.token_hash = ? AND t.kind = 'access' AND t.expires_at > ? AND c.revoked_at IS NULL",
    )
    .bind(hash_token(raw))
    .bind(&now)
    .fetch_optional(&state.db)
    .await?;
    let Some((connection_id, user_id, name, scope)) = row else { return Ok(None) };
    let user = crate::auth::user_by_id(&state.db, &user_id).await?;
    sqlx::query("UPDATE oauth_connections SET last_used_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&connection_id)
        .execute(&state.db)
        .await?;
    Ok(Some(TokenCaller { user, token_name: name, can_write: scope == "write" }))
}

// ---------------------------------------------------------------------------
// Connected apps in Settings

#[derive(Serialize, FromRow, ToSchema)]
pub struct Connection {
    pub id: String,
    pub client_name: String,
    /// Where it signed in from, such as claude.ai or localhost.
    pub redirect_host: String,
    /// `read` or `write`.
    pub scope: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// Apps you've allowed, newest first.
#[utoipa::path(get, path = "/oauth/connections", tag = "oauth", responses((status = 200, body = Vec<Connection>)))]
async fn list_connections(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<Connection>>> {
    let rows = sqlx::query_as::<_, Connection>(
        "SELECT id, client_name, redirect_host, scope, created_at, last_used_at FROM oauth_connections
         WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at DESC",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Stop an app at once: its tokens stop working and it leaves the list.
#[utoipa::path(delete, path = "/oauth/connections/{id}", tag = "oauth", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn disconnect(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT id FROM oauth_connections WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;
    let id = found.ok_or(AppError::NotFound)?;
    end_connection(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects() {
        assert!(redirect_allowed("https://claude.ai/api/mcp/auth_callback"));
        assert!(redirect_allowed("http://localhost:33418/callback"));
        assert!(redirect_allowed("http://127.0.0.1:5000/cb"));
        assert!(!redirect_allowed("http://claude.ai/cb"));
        assert!(!redirect_allowed("https://claude.ai/cb#x"));
        assert!(!redirect_allowed("javascript:alert(1)"));
        assert!(redirect_matches("http://localhost:1/cb", "http://localhost:9999/cb"));
        assert!(!redirect_matches("http://localhost:1/cb", "http://localhost:9999/other"));
        assert!(!redirect_matches("https://a.com/cb", "https://a.com:444/cb"));
    }

    #[test]
    fn pkce() {
        // The verifier from RFC 7636 appendix B.
        assert!(pkce_matches(
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        ));
        assert!(!pkce_matches("short", "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    }
}
