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
use crate::html::{cookie_value, html_escape, page};
use crate::platforms::{resolve_template_path, PLATFORMS};
use crate::state::AppState;
use crate::web;

const ADMIN_COOKIE: &str = "visuara_admin";

fn is_admin(headers: &HeaderMap, state: &AppState) -> bool {
    cookie_value(headers, ADMIN_COOKIE)
        .map(|token| state.admin_sessions.contains(&token))
        .unwrap_or(false)
}

/// Shared nav strip for every logged-in admin page.
fn admin_nav() -> &'static str {
    "<nav><a href=\"/admin/settings\">Settings</a>\
     <a href=\"/admin/client-builds\">Client Builds</a>\
     <a href=\"/admin/accounts\">Accounts</a>\
     <a href=\"/admin/logout\">Log out</a></nav><hr>"
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
            "{}{notice_html}<form method=\"post\" action=\"/admin/settings\">\
             <label>Server URL embedded in downloaded clients\
             <input type=\"text\" name=\"server_url\" value=\"{}\" placeholder=\"wss://visuara.example.com/ws\"></label>\
             <label>Default device name\
             <input type=\"text\" name=\"default_device_name\" value=\"{}\" placeholder=\"this-machine\"></label>\
             <button type=\"submit\">Save</button></form>\
             <p><a href=\"/download\">View public download page</a></p>",
            admin_nav(),
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

/// Very rough User-Agent sniffing, just enough to suggest the right platform
/// first on the download page — never used to decide what's servable.
fn detect_platform(headers: &HeaderMap) -> Option<&'static str> {
    let ua = headers.get(header::USER_AGENT)?.to_str().ok()?;
    if ua.contains("Windows") {
        Some("windows-x86_64")
    } else if ua.contains("Linux") && !ua.contains("Android") {
        Some("linux-x86_64")
    } else {
        None
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

pub async fn download_page(State(state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let server_url = state.db.get_setting("server_url").await.ok().flatten().unwrap_or_default();
    let detected = detect_platform(&headers);

    let mut ordered: Vec<_> = PLATFORMS.iter().collect();
    ordered.sort_by_key(|p| Some(p.key) != detected);

    let mut items = String::new();
    for platform in ordered {
        let resolved = resolve_template_path(platform, &state.client_templates_dir, &state.fetched_templates_dir);
        let recommended = if Some(platform.key) == detected {
            " &mdash; <strong>Recommended for your system</strong>"
        } else {
            ""
        };
        match resolved {
            Some((_, source)) => {
                let mut meta = String::new();
                if source == crate::platforms::TemplateSource::Fetched {
                    let version = state.db.get_setting(&format!("client_version_{}", platform.key)).await.ok().flatten();
                    let fetched_at = state
                        .db
                        .get_setting(&format!("client_fetched_at_{}", platform.key))
                        .await
                        .ok()
                        .flatten()
                        .and_then(|s| s.parse::<i64>().ok());
                    if let Some(version) = version {
                        meta.push_str(&format!(" ({}", html_escape(&version)));
                        if let Some(ts) = fetched_at {
                            meta.push_str(&format!(", fetched {}", relative_time(ts)));
                        }
                        meta.push(')');
                    }
                } else {
                    meta.push_str(" (custom build)");
                }
                items.push_str(&format!(
                    "<li><a href=\"/download/{}\">{}</a>{}{}</li>",
                    platform.key, platform.key, meta, recommended
                ));
            }
            None => {
                items.push_str(&format!("<li>{} (not available yet)</li>", platform.key));
            }
        }
    }
    let logged_in = web::current_account(&headers, &state).is_some();
    Html(page(
        "Download Visuara",
        &format!(
            "{}<p>Downloads below connect to: <code>{}</code></p><ul>{items}</ul>",
            web::web_nav(logged_in),
            if server_url.is_empty() { "(not configured yet)".to_string() } else { html_escape(&server_url) }
        ),
    ))
}

pub async fn download_platform(State(state): State<AppState>, Path(platform): Path<String>) -> Response {
    let Some(info) = PLATFORMS.iter().find(|p| p.key == platform) else {
        return (StatusCode::NOT_FOUND, "unknown platform").into_response();
    };

    let Some((template_path, _source)) =
        resolve_template_path(info, &state.client_templates_dir, &state.fetched_templates_dir)
    else {
        return (StatusCode::NOT_FOUND, "no build uploaded for this platform yet").into_response();
    };
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

async fn client_builds_html(state: &AppState, notice: Option<&str>) -> String {
    let notice_html = notice.map(|n| format!("<p>{}</p>", html_escape(n))).unwrap_or_default();
    let mut rows = String::new();
    for platform in PLATFORMS {
        let resolved = resolve_template_path(platform, &state.client_templates_dir, &state.fetched_templates_dir);
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
                    Some(ts) => format!("fetched: {} ({})", html_escape(&version), relative_time(ts)),
                    None => format!("fetched: {}", html_escape(&version)),
                }
            }
            None => "not available".to_string(),
        };
        rows.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", html_escape(platform.key), status));
    }
    page(
        "Client Builds",
        &format!(
            "{}{notice_html}<table><tr><th>Platform</th><th>Status</th></tr>{rows}</table>\
             <form method=\"post\" action=\"/admin/client-builds/fetch\">\
             <button type=\"submit\">Check GitHub for latest release</button></form>\
             <p>Fetches from <code>{}</code> into a server-managed directory. A manually-placed \
             file in the operator's <code>client_templates_dir</code> always takes priority over \
             a fetched one.</p>",
            admin_nav(),
            html_escape(crate::release_fetch::RELEASE_REPO),
        ),
    )
}

pub async fn client_builds_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    Html(client_builds_html(&state, None).await).into_response()
}

pub async fn fetch_release(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    let notice = match crate::release_fetch::fetch_and_store(&state, crate::release_fetch::RELEASE_REPO).await {
        Ok(results) => {
            let parts: Vec<String> = results
                .into_iter()
                .map(|r| match r.outcome {
                    Ok(version) => format!("{}: fetched {}", r.platform_key, version),
                    Err(e) => format!("{}: {e:#}", r.platform_key),
                })
                .collect();
            parts.join(" | ")
        }
        Err(e) => format!("failed to check GitHub releases: {e:#}"),
    };
    Html(client_builds_html(&state, Some(&notice)).await).into_response()
}

async fn accounts_html(state: &AppState) -> String {
    let accounts = state.db.list_all_accounts().await.unwrap_or_default();
    let mut rows = String::new();
    for account in accounts {
        rows.push_str(&format!(
            "<tr><td><a href=\"/admin/accounts/{}\">{}</a></td><td>{}</td></tr>",
            account.id,
            html_escape(&account.email),
            account.created_at,
        ));
    }
    page(
        "Accounts",
        &format!(
            "{}<table><tr><th>Email</th><th>Created (unix)</th></tr>{rows}</table>",
            admin_nav()
        ),
    )
}

pub async fn accounts_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    Html(accounts_html(&state).await).into_response()
}

async fn account_detail_html(state: &AppState, account_id: i64) -> String {
    let devices = state.db.list_devices_for_account(account_id).await.unwrap_or_default();
    let mut rows = String::new();
    for device in devices {
        let online = state.device_online.contains_key(&device.id);
        let dot_class = if online { "online" } else { "offline" };
        let status_label = if online { "online" } else { "offline" };
        let unattended = if device.unattended_password_hash.is_some() { "yes" } else { "no" };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td>\
             <td><span class=\"dot {dot_class}\"></span>{status_label}</td><td>{}</td>\
             <td><form method=\"post\" action=\"/admin/accounts/{}/devices/{}/delete\" \
             onsubmit=\"return confirm('Delete this device?')\"><button type=\"submit\">Delete</button></form></td></tr>",
            html_escape(&device.name),
            html_escape(&device.id),
            unattended,
            account_id,
            device.id,
        ));
    }
    page(
        "Account Detail",
        &format!(
            "{}<table><tr><th>Name</th><th>Device ID</th><th>Status</th><th>Unattended</th><th></th></tr>{rows}</table>\
             <p><a href=\"/admin/accounts\">Back to accounts</a></p>",
            admin_nav()
        ),
    )
}

pub async fn account_detail_page(State(state): State<AppState>, headers: HeaderMap, Path(account_id): Path<i64>) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    Html(account_detail_html(&state, account_id).await).into_response()
}

pub async fn delete_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((account_id, device_id)): Path<(i64, String)>,
) -> Response {
    if !is_admin(&headers, &state) {
        return Redirect::to("/admin").into_response();
    }
    if let Ok(Some(device)) = state.db.find_device(&device_id).await {
        if device.account_id == account_id {
            let _ = state.db.delete_device(&device_id).await;
            let live_conn = state.device_online.remove(&device_id).map(|(_, conn_id)| conn_id);
            state.otp.remove(&device_id);
            if let Some(conn_id) = live_conn {
                if let Some(tx) = state.connections.get(&conn_id) {
                    let _ = tx.send(visuara_common::signaling::ServerMessage::Error {
                        message: "This device was removed by an administrator.".to_string(),
                    });
                }
            }
        }
    }
    Redirect::to(&format!("/admin/accounts/{account_id}")).into_response()
}
