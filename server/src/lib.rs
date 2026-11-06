//! TooManyProjects server: the JSON API under `/api`, plus the built web app.

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod invites;
pub mod openapi;
pub mod themes;
pub mod time;

pub use app::{AppState, build_router};
pub use config::Config;
