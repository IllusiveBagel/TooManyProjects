//! TooManyProjects server: the JSON API under `/api`, plus the built web app.

pub mod app;
pub mod auth;
pub mod board;
pub mod canvas;
pub mod comments;
pub mod config;
pub mod dashboard;
pub mod databases;
pub mod db;
pub mod error;
pub mod invites;
pub mod links;
pub mod mcp;
pub mod notes;
pub mod openapi;
pub mod position;
pub mod projects;
pub mod sharing;
pub mod tasks;
pub mod themes;
pub mod time;
pub mod tokens;
pub mod updates;

pub use app::{AppState, build_router};
pub use config::Config;
