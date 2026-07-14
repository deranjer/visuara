//! Support for baking a small config blob into a compiled client binary, so
//! the server can offer "download a pre-configured client" without
//! recompiling anything: it byte-patches a placeholder slot in a pre-built
//! release binary with the desired JSON config and streams the result.
//!
//! The client, at startup, reads its own executable off disk and searches
//! for the same marker to recover the config it was patched with. An
//! un-patched (dev) build simply won't find the marker and falls back to
//! defaults.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Unique enough that it won't collide with incidental byte sequences
/// elsewhere in the binary. The trailing null lets both sides treat this as
/// an ordinary byte string.
pub const CONFIG_MAGIC: &[u8] = b"VISUARA_EMBEDDED_CONFIG_V1\0";

/// Fixed-size slot for the JSON payload following the magic marker, null
/// padded. Must be generous enough for any config we embed; enforced at
/// patch time (`to_slot_bytes` errors if the JSON doesn't fit).
pub const CONFIG_SLOT_SIZE: usize = 4096;

const TOTAL_LEN: usize = CONFIG_MAGIC.len() + CONFIG_SLOT_SIZE;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbeddedConfig {
    pub server_url: Option<String>,
    pub device_name: Option<String>,
}

impl EmbeddedConfig {
    /// Encodes this config as the fixed-size, null-padded slot content a
    /// patched binary should have immediately after `CONFIG_MAGIC`.
    pub fn to_slot_bytes(&self) -> Result<[u8; CONFIG_SLOT_SIZE]> {
        let json = serde_json::to_vec(self).context("serialize embedded config")?;
        if json.len() > CONFIG_SLOT_SIZE {
            anyhow::bail!(
                "embedded config is {} bytes, exceeds the {CONFIG_SLOT_SIZE}-byte slot",
                json.len()
            );
        }
        let mut buf = [0u8; CONFIG_SLOT_SIZE];
        buf[..json.len()].copy_from_slice(&json);
        Ok(buf)
    }

    fn from_slot_bytes(bytes: &[u8]) -> Result<Self> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        serde_json::from_slice(&bytes[..end]).context("parse embedded config JSON")
    }

    /// Locates `CONFIG_MAGIC` in an arbitrary byte buffer (a whole
    /// executable file) and, if found, decodes the config that follows it.
    /// Returns `Ok(None)` — not an error — when the marker isn't present,
    /// since that just means an un-patched (dev) build.
    pub fn find_in_binary(data: &[u8]) -> Result<Option<Self>> {
        let Some(pos) = find_subslice(data, CONFIG_MAGIC) else {
            return Ok(None);
        };
        let start = pos + CONFIG_MAGIC.len();
        let end = (start + CONFIG_SLOT_SIZE).min(data.len());
        if end <= start {
            return Ok(None);
        }
        Ok(Some(Self::from_slot_bytes(&data[start..end])?))
    }

    /// Reads this process's own executable off disk and extracts the
    /// embedded config, if any.
    pub fn read_from_current_exe() -> Result<Option<Self>> {
        let path = std::env::current_exe().context("locate current executable")?;
        let data = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        Self::find_in_binary(&data)
    }

    /// Returns a copy of `template` with the embedded slot overwritten by
    /// this config, ready to be served as a downloadable client. Errors if
    /// the template doesn't contain the marker (it wasn't built with the
    /// embedding support compiled in) or the config doesn't fit the slot.
    pub fn patch_binary(&self, template: &[u8]) -> Result<Vec<u8>> {
        let pos = find_subslice(template, CONFIG_MAGIC)
            .context("template binary has no embedded-config marker — was it built from a version with embedded_config support?")?;
        let start = pos + CONFIG_MAGIC.len();
        let end = start + CONFIG_SLOT_SIZE;
        anyhow::ensure!(end <= template.len(), "template binary's config slot is truncated");

        let slot = self.to_slot_bytes()?;
        let mut patched = template.to_vec();
        patched[start..end].copy_from_slice(&slot);
        Ok(patched)
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The static slot embedded in the compiled binary for the server to locate
/// and patch. `#[used]` keeps it in the final binary even though nothing
/// reads it by symbol reference; client startup code should additionally
/// touch it via `std::hint::black_box` to be doubly sure an aggressive
/// optimizer/LTO pass can't reason it away entirely.
#[used]
pub static VISUARA_EMBEDDED_CONFIG_SLOT: [u8; TOTAL_LEN] = build_marker_slot();

const fn build_marker_slot() -> [u8; TOTAL_LEN] {
    let mut buf = [0u8; TOTAL_LEN];
    let mut i = 0;
    while i < CONFIG_MAGIC.len() {
        buf[i] = CONFIG_MAGIC[i];
        i += 1;
    }
    buf
}
