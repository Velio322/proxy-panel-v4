use crate::api::common::{ok, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::AppResult;
use crate::services::node_client;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mypanel_proto::DropSessionCmd;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_sessions))
        .route("/drop", post(drop_session))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionsQuery {
    node_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DropSessionReq {
    node_id: String,
    session_id: Option<String>,
    client_id: Option<String>,
    #[serde(alias = "ip")]
    source_ip: Option<String>,
}

async fn list_sessions(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<SessionsQuery>,
) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let now = crate::db::now();
    let items = state.hub.get_sessions(q.node_id.as_deref());
    let out = items
        .into_iter()
        .map(|(nid, s)| {
            let secs = i64::try_from(s.uptime_secs).unwrap_or(0);
            let connected = now - chrono::Duration::seconds(secs);
            json!({
                "nodeId": nid,
                "id": s.id,
                "clientId": s.client_id,
                "username": s.username,
                "sourceIp": s.source_ip,
                "destination": s.destination,
                "target": s.destination,
                "protocol": s.protocol,
                "inboundTag": s.inbound_tag,
                "uptimeSecs": s.uptime_secs,
                "upload": s.upload,
                "download": s.download,
                "uploadBytes": s.upload,
                "downloadBytes": s.download,
                "connectedAt": crate::db::ts::format_iso(&connected),
            })
        })
        .collect();
    Ok(Json(out))
}

async fn drop_session(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<DropSessionReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    state.hub.drop_local_session(
        &body.node_id,
        body.session_id.as_deref(),
        body.client_id.as_deref(),
        body.source_ip.as_deref(),
    );
    let _ = node_client::drop_session(
        &state,
        &body.node_id,
        DropSessionCmd {
            command_id: crate::db::new_id(),
            session_id: body.session_id,
            client_id: body.client_id,
            source_ip: body.source_ip,
        },
    )
    .await;
    Ok(ok())
}
