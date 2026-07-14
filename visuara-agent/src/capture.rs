//! Single-display screen capture, via `xcap` which handles the underlying
//! platform capture API (Windows/macOS capture APIs, X11 on Linux) behind
//! one cross-platform interface.

use anyhow::{Context, Result};
use image::RgbaImage;
use visuara_common::control::MonitorInfo;
use xcap::Monitor;

pub struct Capturer {
    monitor: Monitor,
}

impl Capturer {
    pub fn primary() -> Result<Self> {
        let monitor = Monitor::all()
            .context("enumerate monitors")?
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .context("no primary monitor found")?;
        Ok(Self { monitor })
    }

    pub fn for_monitor_id(id: u32) -> Result<Self> {
        let monitor = Monitor::all()
            .context("enumerate monitors")?
            .into_iter()
            .find(|m| m.id().unwrap_or(0) == id)
            .with_context(|| format!("no monitor with id {id}"))?;
        Ok(Self { monitor })
    }

    pub fn capture_frame(&self) -> Result<RgbaImage> {
        self.monitor.capture_image().context("capture frame")
    }

    pub fn width(&self) -> u32 {
        self.monitor.width().unwrap_or(0)
    }

    pub fn height(&self) -> u32 {
        self.monitor.height().unwrap_or(0)
    }
}

pub fn list_monitors() -> Result<Vec<MonitorInfo>> {
    Monitor::all()
        .context("enumerate monitors")?
        .into_iter()
        .map(|m| {
            Ok(MonitorInfo {
                id: m.id().context("monitor id")?,
                name: m.name().unwrap_or_else(|_| "unknown".into()),
                width: m.width().context("monitor width")?,
                height: m.height().context("monitor height")?,
                is_primary: m.is_primary().unwrap_or(false),
            })
        })
        .collect()
}
