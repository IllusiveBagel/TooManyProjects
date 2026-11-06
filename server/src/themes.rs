//! Themes are JSON files in `<data_dir>/themes`. The bundled Workshop theme is
//! written there on first start, so it can be copied and edited. A file that
//! fails to parse is skipped with a warning; Workshop is always available.

use std::{collections::BTreeMap, path::Path};

use axum::{
    Json,
    extract::{Path as UrlPath, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    error::{AppError, AppResult},
};

pub const DEFAULT_THEME: &str = "workshop";
const BUNDLED: &[(&str, &str)] = &[("workshop", include_str!("../../themes/workshop.json"))];

const SEMANTIC_KEYS: &[&str] = &[
    "surface",
    "surface-card",
    "surface-raised",
    "line",
    "text",
    "text-quiet",
    "accent",
    "on-accent",
    "late",
    "done",
    "focus",
    "shadow",
];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_themes)).routes(routes!(get_theme))
}

/// The fields the server checks. The full JSON is passed through untouched.
#[derive(Deserialize)]
struct ThemeShape {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    primitives: BTreeMap<String, String>,
    modes: BTreeMap<String, ModeShape>,
}

#[derive(Deserialize)]
struct ModeShape {
    label: String,
    semantic: BTreeMap<String, String>,
    slots: Vec<String>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ThemeSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub modes: Vec<ThemeModeSummary>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ThemeModeSummary {
    /// `dark` or `light`.
    pub mode: String,
    pub label: String,
}

#[derive(Clone)]
struct LoadedTheme {
    summary: ThemeSummary,
    json: Value,
}

#[derive(Default)]
pub struct ThemeStore {
    themes: BTreeMap<String, LoadedTheme>,
}

impl ThemeStore {
    /// Seeds the bundled themes into `dir` if missing, then loads every `*.json`.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut store = Self::default();
        for (id, text) in BUNDLED {
            let path = dir.join(format!("{id}.json"));
            if !path.exists() {
                std::fs::write(&path, text)?;
            }
            store.insert(parse(text).expect("bundled theme is valid"));
        }
        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        entries.sort();
        for path in entries {
            if path.file_name().is_some_and(|n| n == "theme.schema.json") {
                continue;
            }
            match std::fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|t| parse(&t)) {
                Ok(theme) => store.insert(theme),
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "skipping theme"),
            }
        }
        tracing::info!(themes = ?store.themes.keys().collect::<Vec<_>>(), "themes loaded");
        Ok(store)
    }

    /// Only the bundled themes, for tests.
    pub fn bundled() -> Self {
        let mut store = Self::default();
        for (_, text) in BUNDLED {
            store.insert(parse(text).expect("bundled theme is valid"));
        }
        store
    }

    fn insert(&mut self, theme: LoadedTheme) {
        self.themes.insert(theme.summary.id.clone(), theme);
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.themes.get(id).map(|t| &t.json)
    }

    pub fn summaries(&self) -> Vec<ThemeSummary> {
        self.themes.values().map(|t| t.summary.clone()).collect()
    }
}

fn parse(text: &str) -> anyhow::Result<LoadedTheme> {
    let json: Value = serde_json::from_str(text)?;
    let shape: ThemeShape = serde_json::from_value(json.clone())?;
    anyhow::ensure!(
        !shape.id.is_empty()
            && shape.id.len() <= 64
            && shape.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "theme id must be lower-case letters, digits and dashes"
    );
    anyhow::ensure!(!shape.modes.is_empty(), "theme needs a dark or light mode");
    for (mode, m) in &shape.modes {
        anyhow::ensure!(mode == "dark" || mode == "light", "unknown mode {mode}");
        for key in SEMANTIC_KEYS {
            anyhow::ensure!(m.semantic.contains_key(*key), "{mode} mode is missing {key}");
        }
        anyhow::ensure!(m.slots.len() == 8, "{mode} mode needs 8 project colour slots");
        for value in m.semantic.values().chain(m.slots.iter()) {
            if let Some(name) = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
                anyhow::ensure!(shape.primitives.contains_key(name), "unknown primitive {{{name}}}");
            }
        }
    }
    let modes = shape
        .modes
        .iter()
        .map(|(mode, m)| ThemeModeSummary { mode: mode.clone(), label: m.label.clone() })
        .collect();
    Ok(LoadedTheme {
        summary: ThemeSummary { id: shape.id, name: shape.name, description: shape.description, modes },
        json,
    })
}

/// Every installed theme.
#[utoipa::path(get, path = "/themes", tag = "themes",
    responses((status = 200, body = Vec<ThemeSummary>)))]
async fn list_themes(State(state): State<AppState>) -> Json<Vec<ThemeSummary>> {
    Json(state.themes.summaries())
}

/// One theme's full token file. See `themes/theme.schema.json` for its shape.
#[utoipa::path(get, path = "/themes/{id}", tag = "themes",
    params(("id" = String, Path)),
    responses((status = 200, body = Object), (status = 404, body = crate::error::ErrorBody)))]
async fn get_theme(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> AppResult<Json<Value>> {
    state.themes.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_theme_is_valid() {
        let store = ThemeStore::bundled();
        assert!(store.get(DEFAULT_THEME).is_some());
        let modes: Vec<_> = store.summaries()[0].modes.iter().map(|m| m.mode.clone()).collect();
        assert_eq!(modes, ["dark", "light"]);
    }

    #[test]
    fn rejects_unknown_primitive() {
        let mut json: Value = serde_json::from_str(BUNDLED[0].1).unwrap();
        json["modes"]["dark"]["semantic"]["surface"] = "{nope}".into();
        assert!(parse(&json.to_string()).is_err());
    }

    #[test]
    fn loads_user_themes_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut json: Value = serde_json::from_str(BUNDLED[0].1).unwrap();
        json["id"] = "night-shift".into();
        json["name"] = "Night shift".into();
        std::fs::write(dir.path().join("night-shift.json"), json.to_string()).unwrap();
        std::fs::write(dir.path().join("broken.json"), "{").unwrap();
        let store = ThemeStore::load(dir.path()).unwrap();
        assert!(store.get("night-shift").is_some());
        assert!(dir.path().join("workshop.json").exists());
        assert_eq!(store.summaries().len(), 2);
    }
}
