//! Messages exchanged directly between peers over WebRTC data channels once a
//! session is established: input events, clipboard sync, file transfer, and
//! monitor selection. The video stream itself travels over a separate WebRTC
//! video track, not through these messages.

use serde::{Deserialize, Serialize};

// Note: no #[serde(tag = ...)] here, unlike the signaling messages — this
// enum is encoded with bincode (see to_bytes/from_bytes below), which only
// supports serde's default index-based enum representation, not internally-
// or adjacently-tagged ones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    Input(InputEvent),
    Clipboard(ClipboardMessage),
    File(FileTransferMessage),
    Monitor(MonitorMessage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove { x: i32, y: i32 },
    MouseButton { button: MouseButton, pressed: bool },
    MouseScroll { delta_x: i32, delta_y: i32 },
    /// Discrete press/release for keys that don't carry a printable
    /// character (arrows, modifiers, enter, etc). Regular typed text goes
    /// through `TypeText` instead, since GUI toolkits hand us composed text
    /// (respecting shift/layout/IME) rather than raw per-key events.
    KeyEvent { key: KeyCode, pressed: bool },
    TypeText { text: String },
}

/// A portable (not raw-platform-keycode) set of non-printable keys, shared
/// meaning on both ends of the connection regardless of the sender's or
/// receiver's OS or keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyCode {
    Backspace,
    Enter,
    Tab,
    Escape,
    Space,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Shift,
    Control,
    Alt,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardMessage {
    TextUpdated { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferMessage {
    Offer { transfer_id: String, file_name: String, size_bytes: u64 },
    Accept { transfer_id: String },
    Reject { transfer_id: String },
    Chunk { transfer_id: String, sequence: u32, data: Vec<u8> },
    Complete { transfer_id: String },
    Cancel { transfer_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorMessage {
    ListRequest,
    List { monitors: Vec<MonitorInfo> },
    SwitchRequest { monitor_id: u32 },
    Switched { monitor_id: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

impl ControlMessage {
    /// Data channel messages use a compact binary encoding (not JSON) since
    /// they're peer-to-peer only and file chunks make encoding size matter.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}
