use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("Authentication required")]
    Unauthorized,
    #[error("Invalid email or password")]
    InvalidCredentials,
    #[error("This action requires administrator access")]
    Forbidden,
    #[error("Resource not found")]
    NotFound,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::Invalid(s) => (StatusCode::BAD_REQUEST, "invalid_request", s.clone()),
            Self::Conflict(s) => (StatusCode::CONFLICT, "conflict", s.clone()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                self.to_string(),
            ),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            _ => {
                tracing::error!(error = %self, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "The request could not be completed. Retry with the same idempotency key."
                        .into(),
                )
            }
        };
        (
            status,
            Json(json!({"error": {"code": code, "message": message}})),
        )
            .into_response()
    }
}

pub fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
pub fn conflict(message: &str) -> Error {
    Error::Conflict(message.into())
}
