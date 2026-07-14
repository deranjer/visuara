//! Messages exchanged between a client (controller or host) and the signaling
//! server over a WebSocket connection: auth, device registry, pairing, and
//! SDP/ICE relay for establishing the underlying WebRTC peer connection.

use serde::{Deserialize, Serialize};

pub type DeviceId = String;
pub type SessionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    Register { email: String, password: String },
    Login { email: String, password: String },

    /// Sent once authenticated, to announce this device and obtain a device ID.
    RegisterDevice { name: String },
    ListDevices,

    /// Controller side: initiate a connection to a host device.
    RequestConnection {
        target_device_id: DeviceId,
        credential: ConnectCredential,
    },

    SdpOffer { session_id: SessionId, sdp: String },
    SdpAnswer { session_id: SessionId, sdp: String },
    IceCandidate { session_id: SessionId, candidate: String },

    RequestTurnCredentials,

    /// Account holder pushing a new unattended-access password to a device they own.
    SetUnattendedPassword { target_device_id: DeviceId, password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectCredential {
    OneTimePassword(String),
    UnattendedPassword(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    AuthOk { session_token: String },
    AuthError { message: String },

    /// Returned after RegisterDevice; one_time_password is the rotating
    /// attended-access password shown to the user on the host machine.
    DeviceRegistered { device_id: DeviceId, one_time_password: String },
    DeviceList { devices: Vec<DeviceSummary> },

    /// Delivered to a host's persistent connection when a controller wants in.
    IncomingConnection { session_id: SessionId, from_device_id: Option<DeviceId> },

    /// Sent back to the requester once RequestConnection is accepted, so it
    /// can start sending SdpOffer for this session_id.
    ConnectionEstablished { session_id: SessionId, target_device_id: DeviceId },

    SdpOffer { session_id: SessionId, sdp: String },
    SdpAnswer { session_id: SessionId, sdp: String },
    IceCandidate { session_id: SessionId, candidate: String },

    /// Short-lived TURN credentials minted via coturn's REST API HMAC scheme.
    TurnCredentials {
        urls: Vec<String>,
        username: String,
        password: String,
        ttl_secs: u32,
    },

    /// Pushed to a host's persistent control connection when the account
    /// holder sets/changes the unattended-access password remotely.
    UnattendedPasswordUpdated { password: String },

    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub device_id: DeviceId,
    pub name: String,
    pub online: bool,
    pub unattended_access_enabled: bool,
}
