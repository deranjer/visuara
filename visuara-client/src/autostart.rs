//! Per-user autostart so an unattended host comes back up automatically
//! after a login, without a real OS service (no admin rights needed to
//! install/uninstall) — deliberately lighter than a true Windows
//! Service/systemd system unit, since v1 already doesn't support
//! controlling the login/lock screen, so starting before login buys
//! nothing.

use anyhow::Result;

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::enums::*;
    use winreg::RegKey;

    const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "Visuara";

    pub fn enable() -> Result<()> {
        let exe = std::env::current_exe()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu.create_subkey(RUN_KEY_PATH)?;
        run_key.set_value(VALUE_NAME, &format!("\"{}\" host-autostart", exe.display()))?;
        Ok(())
    }

    pub fn disable() -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(RUN_KEY_PATH, KEY_SET_VALUE) {
            let _ = run_key.delete_value(VALUE_NAME);
        }
        Ok(())
    }

    pub fn is_enabled() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.open_subkey(RUN_KEY_PATH)
            .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
            .is_ok()
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::io::Write;

    fn unit_path() -> Result<std::path::PathBuf> {
        let dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("resolve config directory"))?.join("systemd/user");
        Ok(dir.join("visuara-host.service"))
    }

    pub fn enable() -> Result<()> {
        let exe = std::env::current_exe()?;
        let path = unit_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&path)?;
        write!(
            file,
            "[Unit]\nDescription=Visuara unattended host\n\n[Service]\nExecStart=\"{}\" host-autostart\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
            exe.display()
        )?;
        drop(file);

        let status = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "visuara-host.service"])
            .status();
        if let Ok(status) = status {
            if !status.success() {
                anyhow::bail!("systemctl --user enable failed with {status}");
            }
        }
        Ok(())
    }

    pub fn disable() -> Result<()> {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "visuara-host.service"])
            .status();
        let path = unit_path()?;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn is_enabled() -> bool {
        unit_path().map(|p| p.exists()).unwrap_or(false)
    }
}

pub fn enable() -> Result<()> {
    platform::enable()
}

pub fn disable() -> Result<()> {
    platform::disable()
}

pub fn is_enabled() -> bool {
    platform::is_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercises the real registry/systemd-unit round trip — there's no
    // sensible way to fake `current_exe`/HKCU here, and no other test
    // touches this same autostart entry.
    #[test]
    fn enable_disable_round_trip() {
        disable().unwrap();
        assert!(!is_enabled());

        enable().unwrap();
        assert!(is_enabled());

        disable().unwrap();
        assert!(!is_enabled());
    }
}
