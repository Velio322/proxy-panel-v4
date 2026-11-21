use crate::auth::extract::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_logs))
        .route("/stream", get(stream_logs))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsQuery {
    node_id: Option<String>,
    core: Option<String>,
    stream: Option<String>,
    limit: Option<usize>,
}

async fn get_logs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<LogsQuery>,
) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let entries = state.hub.get_logs(
        q.node_id.as_deref(),
        q.core.as_deref(),
        q.stream.as_deref(),
        q.limit.unwrap_or(200),
    );
    let out = entries
        .into_iter()
        .map(|e| serde_json::to_value(e).unwrap_or_else(|_| json!({})))
        .collect();
    Ok(Json(out))
}

async fn stream_logs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<LogsQuery>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    user.require_admin()?;
    let rx = state.hub.subscribe_logs();
    let node_filter = q.node_id;
    let core_filter = q.core;

    let stream = stream::unfold(rx, move |mut rx| {
        let nf = node_filter.clone();
        let cf = core_filter.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(entry) => {
                        if nf.as_deref().is_some_and(|n| !n.is_empty() && n != entry.node_id) {
                            continue;
                        }
                        if cf
                            .as_deref()
                            .is_some_and(|c| !c.is_empty() && !c.eq_ignore_ascii_case(&entry.core))
                        {
                            continue;
                        }
                        if let Ok(json_str) = serde_json::to_string(&entry) {
                            return Some((Ok(Event::default().data(json_str)), rx));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
