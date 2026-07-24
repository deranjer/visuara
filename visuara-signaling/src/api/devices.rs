//! Device listing and self-service deletion for the current user's own
//! devices (as opposed to `api::admin`'s device deletion, which can target
//! any account's devices).

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use visuara_common::signaling::DeviceSummary;

use crate::session;
use crate::state::AppState;

use super::error_response;

pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let account = match session::require_account(&headers, &state).await {
        Ok(a) => a,
        Err(status) => return status.into_response(),
    };
    let devices = state.db.list_devices_for_account(account.id).await.unwrap_or_default();
    let summaries: Vec<DeviceSummary> = devices
        .into_iter()
        .map(|d| DeviceSummary {
            online: state.device_online.contains_key(&d.id),
            unattended_access_enabled: d.unattended_password_hash.is_some(),
            device_id: d.id,
            name: d.name,
        })
        .collect();
    Json(summaries).into_response()
}

pub async fn delete_own(State(state): State<AppState>, headers: HeaderMap, Path(device_id): Path<String>) -> Response {
    let account = match session::require_account(&headers, &state).await {
        Ok(a) => a,
        Err(status) => return status.into_response(),
    };
    match state.db.find_device(&device_id).await {
        Ok(Some(device)) if device.account_id == account.id => {
            let _ = state.db.delete_device(&device_id).await;
            kick_device(&state, &device_id, "This device was removed.");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Some(_)) => error_response(StatusCode::FORBIDDEN, "not your device"),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "device not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

/// Disconnects a device's live WebSocket connection (if any) and notifies
/// it that it was removed. Shared by self-service and admin device deletion.
pub(crate) fn kick_device(state: &AppState, device_id: &str, message: &str) {
    let live_conn = state.device_online.remove(device_id).map(|(_, conn_id)| conn_id);
    state.otp.remove(device_id);
    if let Some(conn_id) = live_conn {
        if let Some(tx) = state.connections.get(&conn_id) {
            let _ = tx.send(visuara_common::signaling::ServerMessage::Error { message: message.to_string() });
        }
    }
}
