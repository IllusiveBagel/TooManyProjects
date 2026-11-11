//! TooManyProjects server: the JSON API under `/api`, plus the built web app.

pub mod app;
pub mod attachments;
pub mod auth;
pub mod board;
pub mod calendar;
pub mod canvas;
pub mod capture;
pub mod comments;
pub mod config;
pub mod dashboard;
pub mod databases;
pub mod db;
pub mod error;
pub mod invites;
pub mod layout;
pub mod limits;
pub mod links;
pub mod mcp;
pub mod notes;
pub mod oauth;
pub mod openapi;
pub mod parts;
pub mod position;
pub mod projects;
pub mod search;
pub mod sharing;
pub mod tasks;
pub mod templates;
pub mod themes;
pub mod time;
pub mod tls;
pub mod today;
pub mod tokens;
pub mod updates;

pub use app::{AppState, build_router};
pub use config::Config;
