//! Public download endpoints: a JSON listing of available per-platform
//! client builds, and the actual patched-binary download. Ported near
//! verbatim from the former server-rendered admin panel (byte-patching
//! logic unchanged).

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use visuara_common::embedded_config::EmbeddedConfig;

use crate::platforms::{resolve_template_path, PLATFORMS};
use crate::state::AppState;

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

#[derive(Serialize)]
pub struct DownloadOption {
    platform: &'static str,
    available: bool,
    recommended: bool,
    custom_build: bool,
    version: Option<String>,
    fetched_at: Option<i64>,
}

#[derive(Serialize)]
pub struct DownloadListView {
    server_url: Option<String>,
    options: Vec<DownloadOption>,
}

pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let server_url = state.db.get_setting("server_url").await.ok().flatten();
    let detected = detect_platform(&headers);

    let mut options = Vec::with_capacity(PLATFORMS.len());
    for platform in PLATFORMS {
        let resolved = resolve_template_path(platform, &state.client_templates_dir, &state.fetched_templates_dir);
        let recommended = Some(platform.key) == detected;
        match resolved {
            Some((_, source)) => {
                let (custom_build, version, fetched_at) = if source == crate::platforms::TemplateSource::Fetched {
                    let version = state.db.get_setting(&format!("client_version_{}", platform.key)).await.ok().flatten();
                    let fetched_at = state
                        .db
                        .get_setting(&format!("client_fetched_at_{}", platform.key))
                        .await
                        .ok()
                        .flatten()
                        .and_then(|s| s.parse::<i64>().ok());
                    (false, version, fetched_at)
                } else {
                    (true, None, None)
                };
                options.push(DownloadOption {
                    platform: platform.key,
                    available: true,
                    recommended,
                    custom_build,
                    version,
                    fetched_at,
                });
            }
            None => options.push(DownloadOption {
                platform: platform.key,
                available: false,
                recommended,
                custom_build: false,
                version: None,
                fetched_at: None,
            }),
        }
    }
    options.sort_by_key(|o| !o.recommended);

    Json(DownloadListView { server_url, options }).into_response()
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
        Ok(Err(e)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to patch client binary: {e:#}")).into_response()
        }
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("patch task panicked: {e}")).into_response(),
    };

    let disposition = format!("attachment; filename=\"{}\"", info.download_filename);
    (
        [(header::CONTENT_TYPE, info.content_type.to_string()), (header::CONTENT_DISPOSITION, disposition)],
        patched,
    )
        .into_response()
}
