//! Tells the owner when a newer release is out.
//!
//! A background task asks GitHub for the latest release a few times a day and
//! keeps the answer in memory. Nothing about the install is sent: it is one
//! anonymous GET. Set `TMP_UPDATE_CHECK=off` to never ask.

use std::{sync::Arc, time::Duration};

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
};

/// Where releases are published.
pub const DEFAULT_RELEASES_URL: &str =
    "https://api.github.com/repos/IllusiveBagel/TooManyProjects/releases/latest";
/// How often the background task asks.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(version)).routes(routes!(check_now))
}

/// The version this binary was built as. Release images get it from the git
/// tag (`TMP_BUILD_VERSION` at build time); anything else is a development build.
pub fn build_version() -> Option<&'static str> {
    option_env!("TMP_BUILD_VERSION").map(|v| v.trim_start_matches('v')).filter(|v| !v.is_empty())
}

/// The version to show people: the release version, or the crate version for a development build.
pub fn display_version() -> &'static str {
    build_version().unwrap_or(env!("CARGO_PKG_VERSION"))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Release {
    /// Without the leading `v`, such as `1.3.0`.
    pub version: String,
    pub name: String,
    /// The release notes page.
    pub url: String,
    pub published_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct VersionInfo {
    /// The running version, such as `1.2.0`.
    pub current: String,
    /// True when this server wasn't built from a release tag, so there is nothing to compare.
    pub development_build: bool,
    /// False when `TMP_UPDATE_CHECK` turns the check off.
    pub check_enabled: bool,
    /// The newest release GitHub knows of, once a check has worked.
    pub latest: Option<Release>,
    /// True when `latest` is newer than `current`.
    pub update_available: bool,
    pub checked_at: Option<String>,
    /// Why the last check failed, if it did.
    pub error: Option<String>,
}

#[derive(Default)]
struct Snapshot {
    latest: Option<Release>,
    checked_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

/// The release check and what it last found.
pub struct UpdateChecker {
    enabled: bool,
    url: String,
    current: Option<String>,
    client: reqwest::Client,
    state: RwLock<Snapshot>,
    /// Keeps two checks (the timer and "Check now") from running at once.
    running: Mutex<()>,
}

/// The part of a GitHub release we read.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

impl UpdateChecker {
    /// `current` is the running release version, or `None` for a development build.
    pub fn new(enabled: bool, url: String, current: Option<&str>) -> Arc<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("toomanyprojects/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client builds");
        Arc::new(Self {
            enabled,
            url,
            current: current.map(str::to_string),
            client,
            state: RwLock::new(Snapshot::default()),
            running: Mutex::new(()),
        })
    }

    pub fn from_config(config: &crate::Config) -> Arc<Self> {
        Self::new(config.update_check, config.update_check_url.clone(), build_version())
    }

    /// Checks now and every few hours after. Does nothing when the check is off.
    pub fn spawn(self: &Arc<Self>) {
        if !self.enabled {
            tracing::info!("update check is off");
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(CHECK_EVERY);
            loop {
                tick.tick().await;
                me.check().await;
            }
        });
    }

    /// Asks GitHub for the latest release and remembers the answer (or the error).
    pub async fn check(&self) {
        if !self.enabled {
            return;
        }
        let _one_at_a_time = self.running.lock().await;
        let result = self.fetch().await;
        let mut state = self.state.write().await;
        state.checked_at = Some(Utc::now());
        match result {
            Ok(latest) => {
                state.error = None;
                // Keep the last good answer if GitHub has nothing usable this time.
                if latest.is_some() {
                    state.latest = latest;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "checking for a new release");
                state.error = Some(e);
            }
        }
    }

    async fn fetch(&self) -> Result<Option<Release>, String> {
        let res = self
            .client
            .get(&self.url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| format!("Couldn't reach GitHub: {e}"))?;
        // No release published yet.
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(format!("GitHub answered {}.", res.status()));
        }
        let release: GithubRelease =
            res.json().await.map_err(|e| format!("Unexpected answer from GitHub: {e}"))?;
        if release.draft || release.prerelease || Semver::parse(&release.tag_name).is_none() {
            return Ok(None);
        }
        let version = release.tag_name.trim_start_matches('v').to_string();
        Ok(Some(Release {
            name: release.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| release.tag_name.clone()),
            version,
            url: release.html_url,
            published_at: release.published_at,
        }))
    }

    pub async fn info(&self) -> VersionInfo {
        let state = self.state.read().await;
        let update_available = match (&self.current, &state.latest) {
            (Some(current), Some(latest)) => is_newer(&latest.version, current),
            _ => false,
        };
        VersionInfo {
            current: self.current.clone().unwrap_or_else(|| display_version().to_string()),
            development_build: self.current.is_none(),
            check_enabled: self.enabled,
            latest: state.latest.clone(),
            update_available,
            checked_at: state.checked_at.map(|t| t.to_rfc3339()),
            error: state.error.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Versions

/// `major.minor.patch` with an optional `-pre` part; build metadata is ignored.
#[derive(Debug, PartialEq, Eq)]
struct Semver {
    core: (u64, u64, u64),
    pre: Option<String>,
}

impl Semver {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string())),
            None => (s, None),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let v = (parts.next()??, parts.next()??, parts.next()??);
        if parts.next().is_some() {
            return None;
        }
        Some(Self { core: v, pre })
    }
}

/// True when release `latest` is newer than the running `current`. A
/// pre-release counts as older than its release (1.3.0-rc.1 < 1.3.0).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(l), Some(c)) = (Semver::parse(latest), Semver::parse(current)) else {
        return false;
    };
    match l.core.cmp(&c.core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => l.pre.is_none() && c.pre.is_some(),
    }
}

// ---------------------------------------------------------------------------
// Handlers

fn owner_only(user: &CurrentUser) -> AppResult<()> {
    if user.0.is_owner() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Only the owner sees update checks.".into()))
    }
}

/// The running version and whether a newer release is out. Owner only.
#[utoipa::path(get, path = "/system/version", tag = "system", responses((status = 200, body = VersionInfo)))]
async fn version(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<VersionInfo>> {
    owner_only(&user)?;
    Ok(Json(state.updates.info().await))
}

/// Checks GitHub for a new release right away. Owner only.
#[utoipa::path(post, path = "/system/version/check", tag = "system", responses((status = 200, body = VersionInfo)))]
async fn check_now(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<VersionInfo>> {
    owner_only(&user)?;
    state.updates.check().await;
    Ok(Json(state.updates.info().await))
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.3.0", "1.2.9"));
        assert!(is_newer("v2.0.0", "1.99.99"));
        assert!(is_newer("1.2.10", "1.2.9"));
        assert!(is_newer("1.3.0", "1.3.0-rc.1"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("1.2.0", "1.3.0-rc.1"));
        assert!(!is_newer("1.2.0", "1.10.0"));
        assert!(!is_newer("nonsense", "1.0.0"));
        assert!(!is_newer("1.0", "0.9.0"));
    }
}
