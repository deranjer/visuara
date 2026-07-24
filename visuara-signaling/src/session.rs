//! Shared session resolution for the JSON API — backed by the `sessions`
//! table (see `db.rs`) rather than an in-memory map, so a server restart
//! doesn't force everyone to log back in. Also home to the
//! first-user-becomes-admin registration rule, shared by both the HTTP API
//! and the WebSocket protocol's own registration path.

use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;

use crate::db::{AccountRole, Db};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "visuara_session";

pub struct AuthedAccount {
    pub id: i64,
    pub email: String,
    pub role: AccountRole,
}

/// Reads a single cookie's value by name out of the request headers.
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

pub fn set_session_cookie(resp: &mut Response, token: &str) {
    resp.headers_mut().insert(
        header::SET_COOKIE,
        format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict").parse().unwrap(),
    );
}

pub fn clear_session_cookie(resp: &mut Response) {
    resp.headers_mut()
        .insert(header::SET_COOKIE, format!("{SESSION_COOKIE}=; Path=/; Max-Age=0").parse().unwrap());
}

/// Resolves the logged-in account for a request, if any, from its session
/// cookie.
pub async fn current_account(headers: &HeaderMap, state: &AppState) -> Option<AuthedAccount> {
    let token = cookie_value(headers, SESSION_COOKIE)?;
    let account = state.db.find_session(&token).await.ok()??;
    Some(AuthedAccount { id: account.id, email: account.email, role: account.role })
}

pub async fn require_account(headers: &HeaderMap, state: &AppState) -> Result<AuthedAccount, StatusCode> {
    current_account(headers, state).await.ok_or(StatusCode::UNAUTHORIZED)
}

pub async fn require_admin(headers: &HeaderMap, state: &AppState) -> Result<AuthedAccount, StatusCode> {
    let account = require_account(headers, state).await?;
    if account.role == AccountRole::Admin {
        Ok(account)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Registers a new account, applying the first-user-becomes-admin rule and
/// auto-disabling public registration immediately afterward. Used by both
/// the JSON API and the WebSocket protocol's own registration path so a
/// desktop-client signup also bootstraps admin/registration state
/// consistently — otherwise a user who registers via the GUI client before
/// ever visiting the web UI would create an account with no admin bootstrap.
pub async fn create_account_and_maybe_bootstrap_admin(
    db: &Db,
    email: &str,
    password_hash: &str,
) -> anyhow::Result<(i64, AccountRole)> {
    // Small TOCTOU race if two people register in the same instant on a
    // brand-new instance (both could see count == 0) — acceptable at this
    // project's single-instance scale, same reasoning as the
    // Mutex<Connection> used throughout db.rs.
    let is_first = db.count_accounts().await? == 0;
    let role = if is_first { AccountRole::Admin } else { AccountRole::User };
    let account_id = db.create_account(email, password_hash, role).await?;
    if is_first {
        db.set_registration_enabled(false).await?;
    }
    Ok((account_id, role))
}
