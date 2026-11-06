use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

/// Body of every error response.
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    /// Stable, machine-readable code such as `invalid_credentials`.
    pub code: &'static str,
    /// A sentence fit to show a person.
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{1}")]
    BadRequest(&'static str, String),
    #[error("not signed in")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("{1}")]
    Conflict(&'static str, String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(code, m) => (StatusCode::BAD_REQUEST, code, m),
            AppError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "unauthorized", "Please sign in.".to_string())
            }
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", "Not found.".to_string()),
            AppError::Conflict(code, m) => (StatusCode::CONFLICT, code, m),
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Something went wrong on the server.".to_string(),
                )
            }
        };
        (status, Json(ErrorBody { code, message })).into_response()
    }
}
