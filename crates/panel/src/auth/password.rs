use crate::error::{AppError, AppResult};

pub async fn hash_password(password: &str, cost: u32) -> AppResult<String> {
    let pw = password.to_string();
    tokio::task::spawn_blocking(move || bcrypt::hash(pw, cost))
        .await
        .map_err(|e| AppError::internal(format!("bcrypt join error: {e}")))?
        .map_err(|e| AppError::internal(format!("bcrypt hash error: {e}")))
}

pub async fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let pw = password.to_string();
    let h = hash.to_string();
    Ok(tokio::task::spawn_blocking(move || bcrypt::verify(pw, &h).unwrap_or(false))
        .await
        .map_err(|e| AppError::internal(format!("bcrypt verify join error: {e}")))?)
}
