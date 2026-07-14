//! Fetches pre-built client binaries from the project's GitHub Releases,
//! replacing the fully-manual "operator builds locally and copies the file
//! in" step for `fetched_templates_dir`. Triggered by an admin button
//! (`POST /admin/client-builds/fetch`), never automatically — see
//! `admin::fetch_release`.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::platforms::PLATFORMS;
use crate::state::AppState;

/// The public repo this server's own release binaries are published from.
/// Hardcoded rather than an env var/setting: this is the upstream Visuara
/// project, not something a self-hosted operator would normally repoint —
/// a fork that wants different binaries can change this constant.
pub const RELEASE_REPO: &str = "deranjer/visuara";

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

pub struct FetchResult {
    pub platform_key: &'static str,
    pub outcome: Result<String>,
}

async fn fetch_latest_release(client: &reqwest::Client, repo: &str) -> Result<GithubRelease> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", "visuara-signaling")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("request GitHub latest release")?
        .error_for_status()
        .context("GitHub API returned an error status")?;
    resp.json().await.context("parse GitHub release JSON")
}

/// Fetches the latest release's assets and, for each known platform whose
/// filename matches an asset, downloads it into `fetched_templates_dir`
/// (atomically, via a `.tmp` file + rename) and records its version/fetch
/// time in the `settings` table. Returns a per-platform outcome so the
/// admin UI can report exactly what happened even if some platforms are
/// missing from the release.
pub async fn fetch_and_store(state: &AppState, repo: &str) -> Result<Vec<FetchResult>> {
    let client = reqwest::Client::new();
    let release = fetch_latest_release(&client, repo).await?;

    let mut results = Vec::with_capacity(PLATFORMS.len());
    for platform in PLATFORMS {
        let outcome = fetch_one_platform(&client, state, &release, platform).await;
        results.push(FetchResult { platform_key: platform.key, outcome });
    }
    Ok(results)
}

async fn fetch_one_platform(
    client: &reqwest::Client,
    state: &AppState,
    release: &GithubRelease,
    platform: &crate::platforms::PlatformInfo,
) -> Result<String> {
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == platform.template_filename)
        .with_context(|| format!("no asset named {} in release {}", platform.template_filename, release.tag_name))?;

    let bytes = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "visuara-signaling")
        .send()
        .await
        .with_context(|| format!("download {}", asset.browser_download_url))?
        .error_for_status()
        .with_context(|| format!("download {} returned an error status", asset.browser_download_url))?
        .bytes()
        .await
        .context("read downloaded asset body")?;

    let final_path = state.fetched_templates_dir.join(platform.template_filename);
    let tmp_path = state.fetched_templates_dir.join(format!("{}.tmp", platform.template_filename));
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .with_context(|| format!("write {}", tmp_path.display()))?;
    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .with_context(|| format!("rename {} to {}", tmp_path.display(), final_path.display()))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state.db.set_setting(&format!("client_version_{}", platform.key), &release.tag_name).await?;
    state.db.set_setting(&format!("client_fetched_at_{}", platform.key), &now.to_string()).await?;

    Ok(release.tag_name.clone())
}
