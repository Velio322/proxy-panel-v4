use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub path: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{message}")]
    Validation {
        message: String,
        details: Vec<ValidationIssue>,
    },
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    ServiceUnavailable(String),
    #[error("{0}")]
    Internal(String),
    #[error("{body}")]
    PlainText {
        status: StatusCode,
        content_type: &'static str,
        body: String,
    },
    #[error("{body:?}")]
    CustomJson {
        status: StatusCode,
        body: Value,
    },
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn validation(field: impl Into<String>, msg: impl Into<String>) -> Self {
        let field_str = field.into();
        let path = if field_str.is_empty() {
            Vec::new()
        } else {
            field_str.split('.').map(ToString::to_string).collect()
        };
        Self::Validation {
            message: "Validation error".to_string(),
            details: vec![ValidationIssue {
                code: "custom".to_string(),
                message: msg.into(),
                path,
            }],
        }
    }

    pub fn validation_custom(
        top_msg: impl Into<String>,
        field: impl Into<String>,
        issue_msg: impl Into<String>,
    ) -> Self {
        let field_str = field.into();
        let path = if field_str.is_empty() {
            Vec::new()
        } else {
            field_str.split('.').map(ToString::to_string).collect()
        };
        Self::Validation {
            message: top_msg.into(),
            details: vec![ValidationIssue {
                code: "custom".to_string(),
                message: issue_msg.into(),
                path,
            }],
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::PayloadTooLarge(msg.into())
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self::UnprocessableEntity(msg.into())
    }

    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self::TooManyRequests(msg.into())
    }

    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn plain_html(status: StatusCode, body: impl Into<String>) -> Self {
        Self::PlainText {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.into(),
        }
    }

    pub fn plain_text(status: StatusCode, body: impl Into<String>) -> Self {
        Self::PlainText {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.into(),
        }
    }

    pub fn custom_json(status: StatusCode, body: Value) -> Self {
        Self::CustomJson { status, body }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Self::NotFound("Resource not found".to_string()),
            sqlx::Error::Database(db_err) => {
                let code = db_err.code().unwrap_or_default();
                match code.as_ref() {
                    "23505" => Self::Conflict("Duplicate record already exists".to_string()),
                    "23503" => Self::BadRequest("Referenced record does not exist".to_string()),
                    "22P02" => Self::BadRequest("Invalid input syntax".to_string()),
                    _ => {
                        tracing::error!(error = %err, "database error");
                        Self::Internal("Internal server error".to_string())
                    }
                }
            }
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
                tracing::error!(error = %err, "database unavailable");
                Self::ServiceUnavailable("Database unavailable".to_string())
            }
            _ => {
                tracing::error!(error = %err, "unexpected sqlx error");
                Self::Internal("Internal server error".to_string())
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
            }
            Self::Validation { message, details } => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": message, "details": details })),
            )
                .into_response(),
            Self::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg }))).into_response()
            }
            Self::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, Json(json!({ "error": msg }))).into_response()
            }
            Self::NotFound(msg) => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": msg }))).into_response()
            }
            Self::Conflict(msg) => {
                (StatusCode::CONFLICT, Json(json!({ "error": msg }))).into_response()
            }
            Self::PayloadTooLarge(msg) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({ "error": msg })),
            )
                .into_response(),
            Self::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": msg })),
            )
                .into_response(),
            Self::TooManyRequests(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": msg })),
            )
                .into_response(),
            Self::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": msg })),
            )
                .into_response(),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
            Self::PlainText {
                status,
                content_type,
                body,
            } => (
                status,
                [(axum::http::header::CONTENT_TYPE, content_type)],
                body,
            )
                .into_response(),
            Self::CustomJson { status, body } => (status, Json(body)).into_response(),
        }
    }
}
