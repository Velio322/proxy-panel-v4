use crate::state::AppState;
use axum::Router;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Keeper API",
        version = "4.0.0",
        description = "Control plane HTTP API for Keeper"
    ),
    paths()
)]
pub struct ApiDoc;

pub fn router() -> Router<AppState> {
    Router::new().merge(Scalar::with_url("/docs", ApiDoc::openapi()))
}
