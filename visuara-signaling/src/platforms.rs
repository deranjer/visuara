//! The set of client platforms the server can offer for download, and where
//! to find a template binary for a given platform: either a manually placed
//! file in `client_templates_dir` (operator's explicit override, always
//! wins) or one fetched from GitHub Releases into `fetched_templates_dir`
//! (see `release_fetch`).

use std::path::{Path, PathBuf};

pub struct PlatformInfo {
    pub key: &'static str,
    pub template_filename: &'static str,
    pub download_filename: &'static str,
    pub content_type: &'static str,
}

pub const PLATFORMS: &[PlatformInfo] = &[
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateSource {
    Manual,
    Fetched,
}

/// Finds the template binary for a platform, preferring a manually-placed
/// override over one auto-fetched from GitHub Releases.
pub fn resolve_template_path(
    info: &PlatformInfo,
    client_templates_dir: &Path,
    fetched_templates_dir: &Path,
) -> Option<(PathBuf, TemplateSource)> {
    let manual = client_templates_dir.join(info.template_filename);
    if manual.exists() {
        return Some((manual, TemplateSource::Manual));
    }
    let fetched = fetched_templates_dir.join(info.template_filename);
    if fetched.exists() {
        return Some((fetched, TemplateSource::Fetched));
    }
    None
}
