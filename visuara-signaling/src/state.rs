use dashmap::{DashMap, DashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;
use visuara_common::signaling::{DeviceId, ServerMessage, SessionId};

use crate::db::Db;
use crate::turn::TurnConfig;

pub type ConnectionId = Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub turn: Arc<TurnConfig>,
    /// Every open WebSocket connection, keyed by its ephemeral connection ID.
    pub connections: Arc<DashMap<ConnectionId, mpsc::UnboundedSender<ServerMessage>>>,
    /// Which connection a given (persistent) device ID is currently online as.
    pub device_online: Arc<DashMap<DeviceId, ConnectionId>>,
    /// Current rotating one-time password for each online device.
    pub otp: Arc<DashMap<DeviceId, String>>,
    /// Active pairing sessions, mapping session_id to the two connections
    /// that should have SDP/ICE messages relayed between them.
    pub sessions: Arc<DashMap<SessionId, SessionPeers>>,
    /// Fixed operator password gating the /admin settings UI.
    pub admin_password: Arc<String>,
    /// Session tokens (cookie values) for browsers that have logged into /admin.
    pub admin_sessions: Arc<DashSet<String>>,
    /// Directory containing pre-built per-platform release binaries, which
    /// /download/:platform patches with the current settings and serves.
    pub client_templates_dir: Arc<PathBuf>,
}

#[derive(Clone, Copy)]
pub struct SessionPeers {
    pub a: ConnectionId,
    pub b: ConnectionId,
}

impl SessionPeers {
    pub fn other(&self, me: ConnectionId) -> Option<ConnectionId> {
        if self.a == me {
            Some(self.b)
        } else if self.b == me {
            Some(self.a)
        } else {
            None
        }
    }
}
