//! Backs the GUI's "known computers" dashboard panel: logs in, then answers
//! refresh pings by re-fetching the device list via the already-existing
//! `ListDevices`/`DeviceList` signaling messages, so the user can see every
//! device on their account (and its live online status) without
//! hand-typing a device ID to connect.

use anyhow::Result;
use tokio::sync::mpsc;

use visuara_common::signaling::{ClientMessage, DeviceSummary, ServerMessage};

use crate::signaling_client::SignalingClient;

pub struct DashboardHandle {
    pub refresh_tx: mpsc::UnboundedSender<()>,
    pub devices_rx: mpsc::UnboundedReceiver<Vec<DeviceSummary>>,
}

/// Logs in and starts a background task that answers `refresh_tx` pings by
/// re-fetching the device list and pushing it to `devices_rx`. The caller
/// drives the polling cadence (e.g. once on login, then periodically while
/// the dashboard panel is visible, plus a manual refresh button) — there's
/// no server-side push for presence changes, so this is deliberately
/// pull-based rather than adding a new protocol message.
pub async fn start(server_url: &str, email: &str, password: &str) -> Result<DashboardHandle> {
    let mut signaling = SignalingClient::connect(server_url).await?;
    signaling.authenticate(email, password).await?;

    let (refresh_tx, mut refresh_rx) = mpsc::unbounded_channel::<()>();
    let (devices_tx, devices_rx) = mpsc::unbounded_channel::<Vec<DeviceSummary>>();

    // Populate the list immediately without the caller having to remember
    // to send the first refresh itself.
    let _ = refresh_tx.send(());

    tokio::spawn(async move {
        loop {
            tokio::select! {
                pinged = refresh_rx.recv() => {
                    if pinged.is_none() {
                        break;
                    }
                    if signaling.send(&ClientMessage::ListDevices).await.is_err() {
                        break;
                    }
                }
                msg = signaling.recv() => {
                    match msg {
                        Ok(ServerMessage::DeviceList { devices }) => {
                            if devices_tx.send(devices).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    });

    Ok(DashboardHandle { refresh_tx, devices_rx })
}
