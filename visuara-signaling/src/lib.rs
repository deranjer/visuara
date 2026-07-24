pub mod api;
pub mod assets;
pub mod auth;
pub mod db;
pub mod platforms;
pub mod release_fetch;
pub mod session;
pub mod state;
pub mod turn;
pub mod ws;

use axum::routing::get;
use axum::Router;

use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", api::router())
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws::ws_handler))
        .fallback(assets::static_handler)
        .with_state(state)
}
