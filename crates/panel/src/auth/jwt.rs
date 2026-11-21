use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use chrono::{NaiveDateTime, TimeZone, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub id: String,
    pub username: String,
    pub role: UserRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reseller_id: Option<String>,
    /// Optional purpose marker (e.g. `"2fa_challenge"` for temporary TOTP step-2 tokens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Clone)]
pub struct JwtService {
    secret: String,
    ttl: Duration,
}

impl std::fmt::Debug for JwtService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtService")
            .field("ttl", &self.ttl)
            .finish()
    }
}

impl JwtService {
    pub fn new(secret: impl Into<String>, ttl: Duration) -> Self {
        Self {
            secret: secret.into(),
            ttl,
        }
    }

    pub fn ttl_secs(&self) -> i64 {
        i64::try_from(self.ttl.as_secs()).unwrap_or(7 * 86_400)
    }

    pub fn sign(
        &self,
        id: &str,
        username: &str,
        role: UserRole,
        reseller_id: Option<String>,
    ) -> AppResult<(String, NaiveDateTime)> {
        self.sign_with_ttl(id, username, role, reseller_id, None, self.ttl_secs())
    }

    pub fn sign_2fa_challenge(
        &self,
        id: &str,
        username: &str,
        role: UserRole,
        reseller_id: Option<String>,
    ) -> AppResult<String> {
        let (tok, _) = self.sign_with_ttl(
            id,
            username,
            role,
            reseller_id,
            Some("2fa_challenge".to_string()),
            300,
        )?;
        Ok(tok)
    }

    pub fn sign_with_ttl(
        &self,
        id: &str,
        username: &str,
        role: UserRole,
        reseller_id: Option<String>,
        purpose: Option<String>,
        ttl_secs: i64,
    ) -> AppResult<(String, NaiveDateTime)> {
        let now_ts = Utc::now().timestamp();
        let exp_ts = now_ts.saturating_add(ttl_secs.max(1));
        let claims = Claims {
            id: id.to_string(),
            username: username.to_string(),
            role,
            reseller_id: reseller_id.filter(|s| !s.is_empty()),
            purpose,
            iat: now_ts,
            exp: exp_ts,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| AppError::internal(format!("jwt sign failed: {e}")))?;

        let expires_at = Utc
            .timestamp_opt(exp_ts, 0)
            .single()
            .map(|dt| dt.naive_utc())
            .unwrap_or_else(crate::db::now);
        Ok((token, expires_at))
    }

    pub fn verify(&self, token: &str) -> AppResult<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::unauthorized("Invalid or expired token"))?;

        if data.claims.id.is_empty() {
            return Err(AppError::unauthorized("Invalid token"));
        }
        if data.claims.purpose.as_deref() == Some("2fa_challenge") {
            return Err(AppError::unauthorized("2FA verification required"));
        }
        Ok(data.claims)
    }

    pub fn verify_2fa_challenge(&self, token: &str) -> AppResult<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 5;
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AppError::unauthorized("Invalid or expired 2FA challenge token"))?;

        if data.claims.purpose.as_deref() != Some("2fa_challenge") || data.claims.id.is_empty() {
            return Err(AppError::unauthorized("Invalid 2FA challenge token"));
        }
        Ok(data.claims)
    }
}
