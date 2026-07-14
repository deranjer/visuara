pub mod admin;
pub mod auth;
pub mod db;
pub mod state;
pub mod turn;
pub mod ws;

use axum::routing::{get, post};
use axum::Router;

use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws::ws_handler))
        .route("/admin", get(admin::admin_page))
        .route("/admin/login", post(admin::admin_login))
        .route("/admin/logout", get(admin::admin_logout))
        .route("/admin/settings", get(admin::settings_page).post(admin::save_settings))
        .route("/download", get(admin::download_page))
        .route("/download/{platform}", get(admin::download_platform))
        .with_state(state)
}
