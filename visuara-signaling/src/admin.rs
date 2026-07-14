//! A small admin web UI (fixed operator password, gated by a session
//! cookie) for setting what gets embedded into downloadable clients, plus a
//! public download endpoint that patches a pre-built per-platform release
//! binary with the current settings and streams it.
//!
//! Template binaries (built and placed by the operator, not by this server)
//! live in `client_templates_dir`, named by platform key — see `PLATFORMS`.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use visuara_common::embedded_config::EmbeddedConfig;

use crate::auth;
use crate::state::AppState;

const ADMIN_COOKIE: &str = "visuara_admin";

struct PlatformInfo {
    key: &'static str,
    template_filename: &'static str,
    download_filename: &'static str,
    content_type: &'static str,
}

const PLATFORMS: &[PlatformInfo] = &[
    PlatformInfo {
        key: "windows-x86_64",
        template_filename: "windows-x86_64.exe",
        download_filename: "visuara.exe",
        content_type: "application/vnd.microsoft.portable-executable",
    },
    PlatformInfo {
        key: "linux-x86_64",
        template_filename: "linux-x86_64",
        download_filename: "visuara",
        content_type: "application/octet-stream",
    },
];

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

fn is_admin(headers: &HeaderMap, state: &AppState) -> bool {
    cookie_value(headers, ADMIN_COOKIE)
        .map(|token| state.admin_sessions.contains(&token))
        .unwrap_or(false)
}

fn html_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:sans-serif;max-width:640px;margin:2rem auto;padding:0 1rem}}\
         input{{display:block;width:100%;padding:.4rem;margin:.3rem 0 1rem}}\
         button{{padding:.5rem 1rem}}li{{margin:.4rem 0}}</style></head>\
         <body><h1>{title}</h1>{body}</body></html>"
    )
}

fn login_html(error: Option<&str>) -> String {
    let error_html = error.map(|e| format!("<p style=\"color:red\">{}</p>", html_escape(e))).unwrap_or_default();
    page(
        "Visuara Admin",
        &format!(
            "{error_html}<form method=\"post\" action=\"/admin/login\">\
             <label>Admin password<input type=\"password\" name=\"password\" autofocus></label>\
             <button type=\"submit\">Log in</button></form>"
        ),
    )
}

fn settings_html(server_url: &str, device_name: &str, notice: Option<&str>) -> String {
    let notice_html = notice.map(|n| format!("<p style=\"color:green\">{}</p>", html_escape(n))).unwrap_or_default();
    page(
        "Visuara Settings",
        &format!(
            "{notice_html}<form method=\"post\" action=\"/admin/settings\">\
             <label>Server URL embedded in downloaded clients\
             <input type=\"text\" name=\"server_url\" value=\"{}\" placeholder=\"wss://visuara.example.com/ws\"></label>\
             <label>Default device name\
             <input type=\"text\" name=\"default_device_name\" value=\"{}\" placeholder=\"this-machine\"></label>\
             <button type=\"submit\">Save</button></form>\
             <p><a href=\"/download\">View public download page</a> &middot; \
             <a href=\"/admin/logout\">Log out</a></p>",
            html_escape(server_url),
            html_escape(device_name),
        ),
    )
}

pub async fn admin_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if is_admin(&headers, &state) {
        Redirect::to("/admin/settings").into_response()
    } else {
        Html(login_html(None)).into_response()
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

pub async fn admin_login(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    if form.password != *state.admin_password {
        return Html(login_html(Some("Incorrect password"))).into_response();
    }
    let token = auth::generate_session_token();
    state.admin_sessions.insert(token.clone());
    let mut resp = Redirect::to("/admin/settings").into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        format!("{ADMIN_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict").parse().unwrap(),
    );
    resp
}

pub async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_value(&headers, ADMIN_COOKIE) {
        state.admin_sessions.remove(&token);
    }
    let mut resp = Redirect::to("/admin").into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, format!("{ADMIN_COOKIE}=; Path=/; Max-Age=0").parse().unwrap());
    resp
}

pub async fn settings_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    let server_url = state.db.get_setting("server_url").await.ok().flatten().unwrap_or_default();
    let device_name = state.db.get_setting("default_device_name").await.ok().flatten().unwrap_or_default();
    Html(settings_html(&server_url, &device_name, None)).into_response()
}

#[derive(Deserialize)]
pub struct SettingsForm {
    server_url: String,
    default_device_name: String,
}

pub async fn save_settings(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SettingsForm>) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    let server_url = form.server_url.trim();
    let device_name = form.default_device_name.trim();
    let _ = state.db.set_setting("server_url", server_url).await;
    let _ = state.db.set_setting("default_device_name", device_name).await;
    Html(settings_html(server_url, device_name, Some("Saved."))).into_response()
}

pub async fn download_page(State(state): State<AppState>) -> Html<String> {
    let server_url = state.db.get_setting("server_url").await.ok().flatten().unwrap_or_default();
    let mut items = String::new();
    for platform in PLATFORMS {
        let path = state.client_templates_dir.join(platform.template_filename);
        if path.exists() {
            items.push_str(&format!(
                "<li><a href=\"/download/{}\">{}</a></li>",
                platform.key, platform.key
            ));
        } else {
            items.push_str(&format!("<li>{} (not available yet)</li>", platform.key));
        }
    }
    Html(page(
        "Download Visuara",
        &format!(
            "<p>Downloads below connect to: <code>{}</code></p><ul>{items}</ul>",
            if server_url.is_empty() { "(not configured yet)".to_string() } else { html_escape(&server_url) }
        ),
    ))
}

pub async fn download_platform(State(state): State<AppState>, Path(platform): Path<String>) -> Response {
    let Some(info) = PLATFORMS.iter().find(|p| p.key == platform) else {
        return (StatusCode::NOT_FOUND, "unknown platform").into_response();
    };

    let template_path = state.client_templates_dir.join(info.template_filename);
    let template = match tokio::fs::read(&template_path).await {
        Ok(bytes) => bytes,
        Err(_) => return (StatusCode::NOT_FOUND, "no build uploaded for this platform yet").into_response(),
    };

    let server_url = state.db.get_setting("server_url").await.ok().flatten();
    let device_name = state.db.get_setting("default_device_name").await.ok().flatten();
    let config = EmbeddedConfig { server_url, device_name };

    let patched = match tokio::task::spawn_blocking(move || config.patch_binary(&template)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to patch client binary: {e:#}")).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("patch task panicked: {e}")).into_response(),
    };

    let disposition = format!("attachment; filename=\"{}\"", info.download_filename);
    (
        [(header::CONTENT_TYPE, info.content_type.to_string()), (header::CONTENT_DISPOSITION, disposition)],
        patched,
    )
        .into_response()
}
