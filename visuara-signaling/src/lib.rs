pub mod admin;
pub mod auth;
pub mod db;
pub mod html;
pub mod platforms;
pub mod release_fetch;
pub mod state;
pub mod turn;
pub mod web;
pub mod ws;

use axum::routing::{get, post};
use axum::Router;

use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(web::home_page))
        .route("/login", post(web::login))
        .route("/register", get(web::register_page).post(web::register))
        .route("/logout", post(web::logout))
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws::ws_handler))
        .route("/admin", get(admin::admin_page))
        .route("/admin/login", post(admin::admin_login))
        .route("/admin/logout", get(admin::admin_logout))
        .route("/admin/settings", get(admin::settings_page).post(admin::save_settings))
        .route("/admin/client-builds", get(admin::client_builds_page))
        .route("/admin/client-builds/fetch", post(admin::fetch_release))
        .route("/admin/accounts", get(admin::accounts_page))
        .route("/admin/accounts/{id}", get(admin::account_detail_page))
        .route("/admin/accounts/{id}/devices/{device_id}/delete", post(admin::delete_device))
        .route("/download", get(admin::download_page))
        .route("/download/{platform}", get(admin::download_platform))
        .with_state(state)
}
