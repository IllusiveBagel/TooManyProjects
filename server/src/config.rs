use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::Context;

/// Runtime settings, read from `TMP_*` environment variables.
#[derive(Clone, Debug)]
pub struct Config {
    /// Where the database, themes and (later) the notes vault live.
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    /// The built web app. When missing, only the API is served.
    pub static_dir: Option<PathBuf>,
    /// Base URL people reach the app at, used to build invite links.
    pub public_url: Option<String>,
    /// Mark the session cookie `Secure`. Defaults to on when `public_url` is https.
    pub cookie_secure: bool,
    pub session_days: i64,
    /// Ask GitHub a few times a day whether a newer release is out (`TMP_UPDATE_CHECK`).
    pub update_check: bool,
    /// Where to ask; only changed for tests or a fork (`TMP_UPDATE_CHECK_URL`).
    pub update_check_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let data_dir = PathBuf::from(env::var("TMP_DATA_DIR").unwrap_or_else(|_| "./data".into()));
        let bind = env::var("TMP_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("TMP_BIND must be host:port")?;
        let static_dir = env::var("TMP_STATIC_DIR")
            .map(PathBuf::from)
            .ok()
            .or_else(|| Some(PathBuf::from("../web/dist")))
            .filter(|p| p.join("index.html").is_file());
        let public_url = env::var("TMP_PUBLIC_URL")
            .ok()
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty());
        let cookie_secure = match env::var("TMP_COOKIE_SECURE").ok().as_deref() {
            Some("1" | "true" | "yes") => true,
            Some("0" | "false" | "no") => false,
            _ => public_url.as_deref().is_some_and(|u| u.starts_with("https://")),
        };
        let session_days = env::var("TMP_SESSION_DAYS").ok().and_then(|d| d.parse().ok()).unwrap_or(30);
        let update_check = !matches!(
            env::var("TMP_UPDATE_CHECK").ok().as_deref().map(str::to_ascii_lowercase).as_deref(),
            Some("0" | "false" | "no" | "off")
        );
        let update_check_url = env::var("TMP_UPDATE_CHECK_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| crate::updates::DEFAULT_RELEASES_URL.into());
        Ok(Self {
            data_dir,
            bind,
            static_dir,
            public_url,
            cookie_secure,
            session_days,
            update_check,
            update_check_url,
        })
    }

    /// Settings for tests: everything under `data_dir`, no web app.
    pub fn for_tests(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            bind: "127.0.0.1:0".parse().unwrap(),
            static_dir: None,
            public_url: Some("http://localhost:8080".into()),
            cookie_secure: false,
            session_days: 30,
            // Tests never reach the network; the update tests point the checker at a local server.
            update_check: false,
            update_check_url: crate::updates::DEFAULT_RELEASES_URL.into(),
        }
    }
}
