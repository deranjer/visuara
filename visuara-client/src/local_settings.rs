//! Remembers this client's own settings and (optionally) a login across
//! launches. Distinct from `saved_credentials.rs`, which is scoped only to
//! the unattended-host autostart flow — this file exists purely so the
//! Host/Connect/Dashboard panels don't reset to embedded-config defaults or
//! an empty login every time the app starts. Same not-encrypted-at-rest
//! caveat as `saved_credentials.rs` applies to `remembered_login`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalSettings {
    pub server_url: Option<String>,
    pub device_name: Option<String>,
    pub remembered_login: Option<RememberedLogin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberedLogin {
    pub email: String,
    pub password: String,
}

fn settings_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("resolve config directory")?.join("visuara");
    Ok(dir.join("local-settings.json"))
}

impl LocalSettings {
    pub fn save(&self) -> Result<()> {
        let path = settings_path()?;
        let json = serde_json::to_vec_pretty(self).context("serialize local settings")?;
        crate::local_storage::write_restricted(&path, &json)
    }

    pub fn load() -> Result<Option<Self>> {
        let path = settings_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(Some(serde_json::from_slice(&data).context("parse local settings")?))
    }

    pub fn clear() -> Result<()> {
        let path = settings_path()?;
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
        Ok(())
    }

    /// Drops just the remembered login (a "log out and forget me" action),
    /// keeping any remembered server_url/device_name.
    pub fn clear_remembered_login() -> Result<()> {
        let Some(mut settings) = Self::load()? else { return Ok(()) };
        if settings.remembered_login.take().is_some() {
            settings.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_and_clear_round_trip() {
        let temp_home = std::env::temp_dir().join(format!("visuara-local-settings-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();
        #[cfg(unix)]
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }
        #[cfg(windows)]
        unsafe {
            std::env::set_var("APPDATA", &temp_home);
        }

        assert!(LocalSettings::load().unwrap().is_none());

        let settings = LocalSettings {
            server_url: Some("wss://example.com/ws".to_string()),
            device_name: Some("my-laptop".to_string()),
            remembered_login: Some(RememberedLogin {
                email: "test@example.com".to_string(),
                password: "hunter2".to_string(),
            }),
        };
        settings.save().unwrap();

        let loaded = LocalSettings::load().unwrap().expect("expected saved local settings");
        assert_eq!(loaded.server_url, settings.server_url);
        assert_eq!(loaded.device_name, settings.device_name);
        assert_eq!(loaded.remembered_login.as_ref().unwrap().email, "test@example.com");

        LocalSettings::clear_remembered_login().unwrap();
        let after_forget = LocalSettings::load().unwrap().expect("settings should survive forgetting login");
        assert!(after_forget.remembered_login.is_none());
        assert_eq!(after_forget.server_url, settings.server_url);

        LocalSettings::clear().unwrap();
        assert!(LocalSettings::load().unwrap().is_none());

        let _ = std::fs::remove_dir_all(&temp_home);
    }
}
