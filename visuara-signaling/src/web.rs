//! The public-facing user web app: a real home page at `/` instead of a
//! 404, with its own login/sign-up (against the same `accounts` table the
//! desktop client's Register/Login messages use) and, once logged in, a
//! read-only view of the account's devices and their live online status.
//! Entirely separate from the operator-only `/admin` system, which is
//! gated by a single fixed password unrelated to user accounts.

use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use crate::auth;
use crate::html::{cookie_value, html_escape, page};
use crate::state::AppState;

const USER_SESSION_COOKIE: &str = "visuara_session";

/// Shared nav strip for the public site — distinct from `admin::admin_nav`.
pub(crate) fn web_nav(logged_in: bool) -> String {
    if logged_in {
        "<nav><a href=\"/\">Home</a><a href=\"/download\">Download</a>\
         <form method=\"post\" action=\"/logout\" style=\"display:inline\">\
         <button type=\"submit\">Log out</button></form></nav><hr>"
            .to_string()
    } else {
        "<nav><a href=\"/\">Home</a><a href=\"/download\">Download</a>\
         <a href=\"/register\">Sign up</a></nav><hr>"
            .to_string()
    }
}

/// Resolves the logged-in account (id, email) for a request, if any, from
/// its session cookie.
pub(crate) fn current_account(headers: &HeaderMap, state: &AppState) -> Option<(i64, String)> {
    let token = cookie_value(headers, USER_SESSION_COOKIE)?;
    state.user_sessions.get(&token).map(|entry| entry.value().clone())
}

fn set_session_cookie(resp: &mut Response, token: &str) {
    resp.headers_mut().insert(
        header::SET_COOKIE,
        format!("{USER_SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict").parse().unwrap(),
    );
}

fn login_landing_html(error: Option<&str>) -> String {
    let error_html = error.map(|e| format!("<p style=\"color:red\">{}</p>", html_escape(e))).unwrap_or_default();
    page(
        "Visuara",
        &format!(
            "{}{error_html}<h2>Log in</h2>\
             <form method=\"post\" action=\"/login\">\
             <label>Email<input type=\"email\" name=\"email\" autofocus></label>\
             <label>Password<input type=\"password\" name=\"password\"></label>\
             <button type=\"submit\">Log in</button></form>\
             <p>Don't have an account? <a href=\"/register\">Sign up</a></p>\
             <p><a href=\"/download\">Download the Visuara client</a></p>",
            web_nav(false),
        ),
    )
}

async fn home_dashboard_html(state: &AppState, account_id: i64, email: &str) -> String {
    let devices = state.db.list_devices_for_account(account_id).await.unwrap_or_default();
    let mut rows = String::new();
    for device in &devices {
        let online = state.device_online.contains_key(&device.id);
        let (dot_class, status_label) = if online { ("online", "online") } else { ("offline", "offline") };
        rows.push_str(&format!(
            "<tr><td>{}</td><td><span class=\"dot {dot_class}\"></span>{status_label}</td></tr>",
            html_escape(&device.name),
        ));
    }
    let devices_html = if devices.is_empty() {
        "<p>No devices yet — share a machine from the desktop client's Host tab, then it'll show up here.</p>"
            .to_string()
    } else {
        format!("<table><tr><th>Device</th><th>Status</th></tr>{rows}</table>")
    };
    page(
        "Visuara",
        &format!(
            "{}<h2>Welcome, {}</h2>{devices_html}<p><a href=\"/download\">Download the Visuara client</a></p>",
            web_nav(true),
            html_escape(email),
        ),
    )
}

pub async fn home_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match current_account(&headers, &state) {
        Some((account_id, email)) => Html(home_dashboard_html(&state, account_id, &email).await).into_response(),
        None => Html(login_landing_html(None)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

pub async fn login(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let email = form.email.trim().to_string();
    match state.db.find_account_by_email(&email).await {
        Ok(Some(account)) if auth::verify_password(&form.password, &account.password_hash) => {
            let token = auth::generate_session_token();
            state.user_sessions.insert(token.clone(), (account.id, email));
            let mut resp = Redirect::to("/").into_response();
            set_session_cookie(&mut resp, &token);
            resp
        }
        _ => Html(login_landing_html(Some("Incorrect email or password"))).into_response(),
    }
}

fn register_html(error: Option<&str>) -> String {
    let error_html = error.map(|e| format!("<p style=\"color:red\">{}</p>", html_escape(e))).unwrap_or_default();
    page(
        "Sign up — Visuara",
        &format!(
            "{}{error_html}<h2>Create an account</h2>\
             <form method=\"post\" action=\"/register\">\
             <label>Email<input type=\"email\" name=\"email\" autofocus></label>\
             <label>Password<input type=\"password\" name=\"password\"></label>\
             <button type=\"submit\">Sign up</button></form>\
             <p>Already have an account? <a href=\"/\">Log in</a></p>",
            web_nav(false),
        ),
    )
}

pub async fn register_page() -> Html<String> {
    Html(register_html(None))
}

#[derive(Deserialize)]
pub struct RegisterForm {
    email: String,
    password: String,
}

pub async fn register(State(state): State<AppState>, Form(form): Form<RegisterForm>) -> Response {
    let email = form.email.trim().to_string();
    if email.is_empty() || form.password.is_empty() {
        return Html(register_html(Some("Email and password are required"))).into_response();
    }
    let hash = match auth::hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => return Html(register_html(Some(&format!("failed to hash password: {e:#}")))).into_response(),
    };
    match state.db.create_account(&email, &hash).await {
        Ok(account_id) => {
            let token = auth::generate_session_token();
            state.user_sessions.insert(token.clone(), (account_id, email));
            let mut resp = Redirect::to("/").into_response();
            set_session_cookie(&mut resp, &token);
            resp
        }
        Err(e) => {
            let message = if e.to_string().contains("UNIQUE") {
                "That email is already registered — try logging in instead.".to_string()
            } else {
                format!("registration failed: {e:#}")
            };
            Html(register_html(Some(&message))).into_response()
        }
    }
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_value(&headers, USER_SESSION_COOKIE) {
        state.user_sessions.remove(&token);
    }
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, format!("{USER_SESSION_COOKIE}=; Path=/; Max-Age=0").parse().unwrap());
    resp
}
