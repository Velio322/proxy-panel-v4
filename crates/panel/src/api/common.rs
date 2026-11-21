use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// JSON extractor that maps deserialization/syntax errors into `AppError::BadRequest`.
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(val)) => Ok(Self(val)),
            Err(JsonRejection::JsonDataError(err)) => Err(AppError::validation("", err.body_text())),
            Err(JsonRejection::JsonSyntaxError(_)) => Err(AppError::bad_request("Invalid JSON")),
            Err(JsonRejection::BytesRejection(_)) => {
                Err(AppError::payload_too_large("Request body too large"))
            }
            Err(_) => Err(AppError::bad_request("Bad Request")),
        }
    }
}

/// Optional JSON extractor that yields `Value::Object(Default)` on empty bodies.
pub struct OptionalJson<T>(pub T);

impl<S, T> FromRequest<S> for OptionalJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Default,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|_| AppError::payload_too_large("Request body too large"))?;
        if bytes.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(Self(T::default()));
        }
        let val = serde_json::from_slice::<T>(&bytes)
            .map_err(|e| AppError::bad_request(format!("Invalid JSON: {e}")))?;
        Ok(Self(val))
    }
}

pub fn ok() -> Json<Value> {
    Json(json!({ "success": true }))
}

pub fn valid_id(id: &str) -> AppResult<()> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed.chars().any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(AppError::bad_request("Invalid identifier"));
    }
    Ok(())
}

pub fn positive_int(raw: Option<&str>, default: i64, max: i64, field: &str) -> AppResult<i64> {
    match raw {
        None => Ok(default),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty()
                || !trimmed.chars().all(|c| c.is_ascii_digit())
                || trimmed.starts_with('0')
            {
                return Err(AppError::validation(
                    field,
                    format!("{field} must be a positive integer"),
                ));
            }
            let n: i64 = trimmed.parse().map_err(|_| {
                AppError::validation(field, format!("{field} must be a positive integer"))
            })?;
            if n < 1 || n > max {
                return Err(AppError::validation(
                    field,
                    format!("{field} must be between 1 and {max}"),
                ));
            }
            Ok(n)
        }
    }
}

pub fn like_pattern(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

pub fn req_str<'a>(obj: &'a Value, key: &str) -> AppResult<&'a str> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::validation(key, format!("{key} is required")))
}

pub fn check_manage(actor: UserRole, target: UserRole, action: &str) -> AppResult<()> {
    if actor.can_manage(target) {
        Ok(())
    } else {
        Err(AppError::forbidden(format!(
            "Forbidden: insufficient permissions to {action} user with role {target}"
        )))
    }
}

pub fn sanitize_file_name(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
