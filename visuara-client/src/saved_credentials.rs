//! Persists account credentials locally so an auto-started, unattended host
//! can log into the signaling server without a human present to type a
//! password. Protected with owner-only file permissions on Unix; on
//! Windows this relies on the per-user AppData directory's default NTFS
//! ACLs (not encrypted at rest — a synced/backed-up copy of this file, e.g.
//! via OneDrive folder sync, would be readable as plaintext JSON. Good
//! enough for v1; DPAPI encryption would be the natural next step).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredentials {
    pub server_url: String,
    pub email: String,
    pub password: String,
    pub device_name: String,
}

fn credentials_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("resolve config directory")?.join("visuara");
    Ok(dir.join("unattended-credentials.json"))
}

impl SavedCredentials {
    pub fn save(&self) -> Result<()> {
        let path = credentials_path()?;
        let json = serde_json::to_vec_pretty(self).context("serialize credentials")?;
        crate::local_storage::write_restricted(&path, &json)
    }

    pub fn load() -> Result<Option<Self>> {
        let path = credentials_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(Some(serde_json::from_slice(&data).context("parse saved credentials")?))
    }

    pub fn clear() -> Result<()> {
        let path = credentials_path()?;
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercises the actual serialize/write/read/remove round trip against a
    // real file, just redirecting HOME/APPDATA so it doesn't touch the
    // developer's real config directory. Tests run single-threaded within
    // this process for env-var safety (see #[serial] note below) — there's
    // only one test here so that's moot, but worth keeping in mind if more
    // are added.
    #[test]
    fn save_load_and_clear_round_trip() {
        let temp_home = std::env::temp_dir().join(format!("visuara-cred-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();
        #[cfg(unix)]
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }
        #[cfg(windows)]
        unsafe {
            std::env::set_var("APPDATA", &temp_home);
        }

        assert!(SavedCredentials::load().unwrap().is_none());

        let creds = SavedCredentials {
            server_url: "wss://example.com/ws".to_string(),
            email: "test@example.com".to_string(),
            password: "hunter2".to_string(),
            device_name: "test-device".to_string(),
        };
        creds.save().unwrap();

        let loaded = SavedCredentials::load().unwrap().expect("expected saved credentials");
        assert_eq!(loaded.server_url, creds.server_url);
        assert_eq!(loaded.email, creds.email);
        assert_eq!(loaded.password, creds.password);
        assert_eq!(loaded.device_name, creds.device_name);

        SavedCredentials::clear().unwrap();
        assert!(SavedCredentials::load().unwrap().is_none());

        let _ = std::fs::remove_dir_all(&temp_home);
    }
}
