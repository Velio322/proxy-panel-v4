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
    let tg_cfg = crate::services::telegram::load_telegram_config(&state).await;
    Json(json!({
        "apiUrl": state.cfg.api_url,
        "subUrl": state.cfg.sub_url,
        "version": env!("CARGO_PKG_VERSION"),
        "telegramBotUsername": tg_cfg.bot_username,
        "branding": branding,
        "features": {
            "telegramOauth": tg_cfg.bot_token.is_some(),
            "passkeys": true,
            "totp": true,
            "mtls": true
        }
    }))
}
