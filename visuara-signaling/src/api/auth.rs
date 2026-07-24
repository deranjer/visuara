//! Auth endpoints: register, login, logout, current-account, and public
//! registration-status. Registration applies the first-user-becomes-admin
//! rule and the registration-enabled gate (see `crate::session`).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::AccountRole;
use crate::session::{self, create_account_and_maybe_bootstrap_admin};
use crate::state::AppState;

use super::error_response;

#[derive(Serialize)]
pub struct AccountView {
    pub id: i64,
    pub email: String,
    pub role: AccountRole,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    email: String,
    password: String,
}

pub async fn register(State(state): State<AppState>, Json(body): Json<RegisterRequest>) -> Response {
    let email = body.email.trim().to_string();
    if email.is_empty() || body.password.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "email and password are required");
    }
    match state.db.registration_enabled().await {
        Ok(true) => {}
        Ok(false) => return error_response(StatusCode::FORBIDDEN, "registration is currently disabled"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
    let hash = match crate::auth::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to hash password: {e:#}"))
        }
    };
    match create_account_and_maybe_bootstrap_admin(&state.db, &email, &hash).await {
        Ok((account_id, role)) => start_session(&state, account_id, email, role).await,
        Err(e) => {
            let message = if e.to_string().contains("UNIQUE") {
                "That email is already registered — try logging in instead.".to_string()
            } else {
                format!("registration failed: {e:#}")
            };
            error_response(StatusCode::CONFLICT, message)
        }
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

pub async fn login(State(state): State<AppState>, Json(body): Json<LoginRequest>) -> Response {
    let email = body.email.trim().to_string();
    match state.db.find_account_by_email(&email).await {
        Ok(Some(account)) if crate::auth::verify_password(&body.password, &account.password_hash) => {
            start_session(&state, account.id, account.email, account.role).await
        }
        Ok(_) => error_response(StatusCode::UNAUTHORIZED, "incorrect email or password"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

async fn start_session(state: &AppState, account_id: i64, email: String, role: AccountRole) -> Response {
    let token = crate::auth::generate_session_token();
    if let Err(e) = state.db.create_session(&token, account_id).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to create session: {e:#}"));
    }
    let mut resp = Json(AccountView { id: account_id, email, role }).into_response();
    session::set_session_cookie(&mut resp, &token);
    resp
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session::cookie_value(&headers, session::SESSION_COOKIE) {
        let _ = state.db.delete_session(&token).await;
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    session::clear_session_cookie(&mut resp);
    resp
}

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match session::current_account(&headers, &state).await {
        Some(account) => Json(AccountView { id: account.id, email: account.email, role: account.role }).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

#[derive(Serialize)]
pub struct RegistrationStatus {
    enabled: bool,
}

pub async fn registration_status(State(state): State<AppState>) -> Response {
    let enabled = state.db.registration_enabled().await.unwrap_or(true);
    Json(RegistrationStatus { enabled }).into_response()
}
