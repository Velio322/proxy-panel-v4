use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rust_embed::RustEmbed;
use serde_json::json;

#[derive(RustEmbed)]
#[folder = "../../client/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))).into_response();
    }

    if !path.is_empty() {
        if let Some(content) = Assets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache_ctrl = if path.starts_with("assets/") {
                HeaderValue::from_static("public, max-age=31536000, immutable")
            } else {
                HeaderValue::from_static("no-cache")
            };
            let mut res = (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                Body::from(content.data),
            )
                .into_response();
            res.headers_mut().insert(header::CACHE_CONTROL, cache_ctrl);
            return res;
        }
    }

    if let Some(index) = Assets::get("index.html") {
        let mut res = (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            Body::from(index.data),
        )
            .into_response();
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return res;
    }

    (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))).into_response()
}
