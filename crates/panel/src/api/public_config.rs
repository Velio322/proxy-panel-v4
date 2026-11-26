use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(public_config))
}

async fn public_config(State(state): State<AppState>) -> Json<Value> {
    let branding = crate::api::branding::load_branding(&state).await;
    Json(json!({
        "apiUrl": state.cfg.api_url,
        "subUrl": state.cfg.sub_url,
        "version": env!("CARGO_PKG_VERSION"),
        "telegramBotUsername": state.cfg.telegram_bot_username,
        "branding": branding,
        "features": {
            "telegramOauth": state.cfg.telegram_bot_token.is_some(),
            "passkeys": true,
            "totp": true,
            "mtls": true
        }
    }))
}
