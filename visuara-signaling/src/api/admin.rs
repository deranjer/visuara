//! Admin-only JSON endpoints: settings, client-build fetching, account
//! management, and the registration-enabled toggle. Ported from the former
//! server-rendered admin panel — the business logic (settings storage,
//! release fetching, platform template resolution) is unchanged, only the
//! response format and auth check (role-based via `session::require_admin`,
//! replacing the old fixed-password `ADMIN_PASSWORD` gate) differ.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::AccountRole;
use crate::platforms::PLATFORMS;
use crate::session;
use crate::state::AppState;

use super::devices::kick_device;
use super::error_response;

#[derive(Serialize)]
pub struct SettingsView {
    server_url: String,
    default_device_name: String,
}

pub async fn get_settings(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let server_url = state.db.get_setting("server_url").await.ok().flatten().unwrap_or_default();
    let default_device_name =
        state.db.get_setting("default_device_name").await.ok().flatten().unwrap_or_default();
    Json(SettingsView { server_url, default_device_name }).into_response()
}

#[derive(Deserialize)]
pub struct SaveSettingsRequest {
    server_url: String,
    default_device_name: String,
}

pub async fn save_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SaveSettingsRequest>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let server_url = body.server_url.trim();
    let default_device_name = body.default_device_name.trim();
    let _ = state.db.set_setting("server_url", server_url).await;
    let _ = state.db.set_setting("default_device_name", default_device_name).await;
    Json(SettingsView {
        server_url: server_url.to_string(),
        default_device_name: default_device_name.to_string(),
    })
    .into_response()
}

#[derive(Serialize)]
pub struct ClientBuildStatus {
    platform: &'static str,
    status: String,
}

async fn client_build_statuses(state: &AppState) -> Vec<ClientBuildStatus> {
    let mut out = Vec::with_capacity(PLATFORMS.len());
    for platform in PLATFORMS {
        let resolved = crate::platforms::resolve_template_path(
            platform,
            &state.client_templates_dir,
            &state.fetched_templates_dir,
        );
        let status = match resolved {
            Some((_, crate::platforms::TemplateSource::Manual)) => "manual override".to_string(),
            Some((_, crate::platforms::TemplateSource::Fetched)) => {
                let version = state
                    .db
                    .get_setting(&format!("client_version_{}", platform.key))
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "unknown".to_string());
                let fetched_at = state
                    .db
                    .get_setting(&format!("client_fetched_at_{}", platform.key))
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<i64>().ok());
                match fetched_at {
                    Some(ts) => format!("fetched: {version} ({})", relative_time(ts)),
                    None => format!("fetched: {version}"),
                }
            }
            None => "not available".to_string(),
        };
        out.push(ClientBuildStatus { platform: platform.key, status });
    }
    out
}

pub async fn client_builds(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    Json(client_build_statuses(&state).await).into_response()
}

#[derive(Serialize)]
pub struct FetchReleaseResult {
    platform: &'static str,
    version: Option<String>,
    error: Option<String>,
}

pub async fn fetch_release(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    match crate::release_fetch::fetch_and_store(&state, crate::release_fetch::RELEASE_REPO).await {
        Ok(results) => {
            let mapped: Vec<FetchReleaseResult> = results
                .into_iter()
                .map(|r| match r.outcome {
                    Ok(version) => FetchReleaseResult { platform: r.platform_key, version: Some(version), error: None },
                    Err(e) => {
                        FetchReleaseResult { platform: r.platform_key, version: None, error: Some(format!("{e:#}")) }
                    }
                })
                .collect();
            Json(mapped).into_response()
        }
        Err(e) => error_response(StatusCode::BAD_GATEWAY, format!("failed to check GitHub releases: {e:#}")),
    }
}

#[derive(Serialize)]
pub struct AccountSummaryView {
    id: i64,
    email: String,
    created_at: i64,
    role: AccountRole,
}

pub async fn list_accounts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let accounts = state.db.list_all_accounts().await.unwrap_or_default();
    let views: Vec<AccountSummaryView> = accounts
        .into_iter()
        .map(|a| AccountSummaryView { id: a.id, email: a.email, created_at: a.created_at, role: a.role })
        .collect();
    Json(views).into_response()
}

#[derive(Serialize)]
pub struct DeviceView {
    id: String,
    name: String,
    online: bool,
    unattended_access_enabled: bool,
}

#[derive(Serialize)]
pub struct AccountDetailView {
    id: i64,
    email: String,
    role: AccountRole,
    devices: Vec<DeviceView>,
}

pub async fn account_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<i64>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let account = match state.db.find_account_by_id(account_id).await {
        Ok(Some(a)) => a,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "account not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    };
    let devices = state.db.list_devices_for_account(account_id).await.unwrap_or_default();
    let views: Vec<DeviceView> = devices
        .into_iter()
        .map(|d| DeviceView {
            online: state.device_online.contains_key(&d.id),
            unattended_access_enabled: d.unattended_password_hash.is_some(),
            id: d.id,
            name: d.name,
        })
        .collect();
    Json(AccountDetailView { id: account.id, email: account.email, role: account.role, devices: views }).into_response()
}

#[derive(Deserialize)]
pub struct CreateAccountRequest {
    email: String,
    password: String,
    #[serde(default)]
    role: Option<AccountRole>,
}

#[derive(Serialize)]
pub struct CreatedAccountView {
    id: i64,
    email: String,
    role: AccountRole,
}

/// Admin-initiated account creation bypasses the `registration_enabled`
/// gate entirely — that flag only governs public self-service signup.
pub async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateAccountRequest>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let email = body.email.trim().to_string();
    if email.is_empty() || body.password.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "email and password are required");
    }
    let hash = match crate::auth::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to hash password: {e:#}"))
        }
    };
    let role = body.role.unwrap_or(AccountRole::User);
    match state.db.create_account(&email, &hash, role).await {
        Ok(id) => (StatusCode::CREATED, Json(CreatedAccountView { id, email, role })).into_response(),
        Err(e) => {
            let message = if e.to_string().contains("UNIQUE") {
                "That email is already registered.".to_string()
            } else {
                format!("failed to create account: {e:#}")
            };
            error_response(StatusCode::CONFLICT, message)
        }
    }
}

#[derive(Deserialize)]
pub struct SetRoleRequest {
    role: AccountRole,
}

pub async fn set_account_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<i64>,
    Json(body): Json<SetRoleRequest>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    if body.role != AccountRole::Admin {
        if let Err(resp) = guard_last_admin(&state, account_id).await {
            return resp;
        }
    }
    match state.db.set_account_role(account_id, body.role).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

pub async fn delete_account(State(state): State<AppState>, headers: HeaderMap, Path(account_id): Path<i64>) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    if let Err(resp) = guard_last_admin(&state, account_id).await {
        return resp;
    }
    match state.db.delete_account(account_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

/// Refuses to demote/delete `account_id` if doing so would leave the
/// instance with zero admins — with registration auto-disabled and only
/// admins able to create accounts, that would be an unrecoverable lockout
/// short of manual SQL.
async fn guard_last_admin(state: &AppState, account_id: i64) -> Result<(), Response> {
    let Ok(Some(account)) = state.db.find_account_by_id(account_id).await else {
        return Ok(());
    };
    if account.role != AccountRole::Admin {
        return Ok(());
    }
    match state.db.count_admins().await {
        Ok(n) if n <= 1 => Err(error_response(
            StatusCode::CONFLICT,
            "cannot remove the last remaining admin account",
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))),
    }
}

pub async fn delete_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((account_id, device_id)): Path<(i64, String)>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    match state.db.find_device(&device_id).await {
        Ok(Some(device)) if device.account_id == account_id => {
            let _ = state.db.delete_device(&device_id).await;
            kick_device(&state, &device_id, "This device was removed by an administrator.");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => error_response(StatusCode::NOT_FOUND, "device not found for this account"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

#[derive(Serialize)]
pub struct RegistrationSetting {
    enabled: bool,
}

pub async fn get_registration(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    let enabled = state.db.registration_enabled().await.unwrap_or(true);
    Json(RegistrationSetting { enabled }).into_response()
}

#[derive(Deserialize)]
pub struct SetRegistrationRequest {
    enabled: bool,
}

pub async fn set_registration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetRegistrationRequest>,
) -> Response {
    if let Err(status) = session::require_admin(&headers, &state).await {
        return status.into_response();
    }
    match state.db.set_registration_enabled(body.enabled).await {
        Ok(()) => Json(RegistrationSetting { enabled: body.enabled }).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

/// Renders "3h ago" / "5d ago" / "just now" from a stored unix-seconds
/// timestamp, without pulling in a date/time crate for one label.
fn relative_time(unix_secs: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(unix_secs);
    let delta = (now - unix_secs).max(0);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}
