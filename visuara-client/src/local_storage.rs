//! Shared helper for writing small local JSON files with owner-only
//! permissions on Unix (`saved_credentials.rs`'s unattended-host
//! credentials and `local_settings.rs`'s remembered login both use this).
//! On Windows this relies on the per-user AppData directory's default NTFS
//! ACLs, same caveat as documented on `saved_credentials.rs`.

use anyhow::{Context, Result};
use std::path::Path;

pub fn write_restricted(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
    restrict_permissions(path)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restrict permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
