//! Bidirectional text clipboard access. v1 scope is text-only per the plan.

use anyhow::{Context, Result};
use arboard::Clipboard;

pub struct ClipboardHandle {
    clipboard: Clipboard,
}

impl ClipboardHandle {
    pub fn new() -> Result<Self> {
        Ok(Self {
            clipboard: Clipboard::new().context("open system clipboard")?,
        })
    }

    pub fn get_text(&mut self) -> Result<String> {
        self.clipboard.get_text().context("read clipboard text")
    }

    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.clipboard
            .set_text(text.to_string())
            .context("write clipboard text")
    }
}

/// Abstracts clipboard access so sync logic (echo avoidance, polling) can be
/// unit tested without touching the real OS clipboard.
pub trait ClipboardBackend: Send {
    fn get_text(&mut self) -> Result<String>;
    fn set_text(&mut self, text: &str) -> Result<()>;
}

impl ClipboardBackend for ClipboardHandle {
    fn get_text(&mut self) -> Result<String> {
        ClipboardHandle::get_text(self)
    }

    fn set_text(&mut self, text: &str) -> Result<()> {
        ClipboardHandle::set_text(self, text)
    }
}
