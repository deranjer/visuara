//! JSON REST API for the React SPA, mounted at `/api/v1`. Replaces the
//! former server-rendered HTML — auth is still cookie/session based (see
//! `crate::session`), just JSON in, JSON out. Versioned in the path so a
//! future breaking change can ship as `/api/v2` alongside it.

pub mod admin;
pub mod auth;
pub mod devices;
pub mod download;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/registration-status", get(auth::registration_status))
        .route("/devices", get(devices::list))
        .route("/devices/{device_id}", axum::routing::delete(devices::delete_own))
        .route("/admin/settings", get(admin::get_settings).put(admin::save_settings))
        .route("/admin/client-builds", get(admin::client_builds))
        .route("/admin/client-builds/fetch", post(admin::fetch_release))
        .route("/admin/accounts", get(admin::list_accounts).post(admin::create_account))
        .route(
            "/admin/accounts/{id}",
            get(admin::account_detail).delete(admin::delete_account),
        )
        .route("/admin/accounts/{id}/role", put(admin::set_account_role))
        .route(
            "/admin/accounts/{id}/devices/{device_id}",
            axum::routing::delete(admin::delete_device),
        )
        .route("/admin/registration", get(admin::get_registration).put(admin::set_registration))
        .route("/download", get(download::list))
        .route("/download/{platform}", get(download::download_platform))
}

pub(crate) fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}
