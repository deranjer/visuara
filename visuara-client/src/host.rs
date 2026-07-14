//! The host role: registers a device with the signaling server, waits for an
//! incoming connection, negotiates a WebRTC session, then streams captured
//! screen frames out over a video track and applies incoming input events,
//! clipboard updates, monitor switches, and incoming file transfers.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc_media::Sample;

use visuara_agent::capture::{list_monitors, Capturer};
use visuara_agent::clipboard::ClipboardHandle;
use visuara_agent::encode::VideoEncoder;
use visuara_common::control::{ClipboardMessage, ControlMessage, InputEvent, MonitorMessage};
use visuara_common::signaling::{ClientMessage, ServerMessage};

use crate::clipboard_sync::ClipboardSync;
use crate::file_transfer::FileReceiver;
use crate::session::{build_peer_connection, decode_ice_candidate, encode_ice_candidate, make_video_track};
use crate::signaling_client::SignalingClient;

/// Abstracts "do something with an incoming input event" so tests can swap
/// in a recording sink instead of driving the real OS input injector.
pub trait InputSink: Send {
    fn handle_input(&mut self, event: InputEvent) -> Result<()>;
}

impl InputSink for visuara_agent::input::InputInjector {
    fn handle_input(&mut self, event: InputEvent) -> Result<()> {
        self.apply(event)
    }
}

pub struct HostHandle {
    pub device_id: String,
    pub one_time_password: String,
}

/// Registers this machine as a host device and returns its ID/OTP immediately;
/// the actual incoming-connection loop runs in the background.
pub async fn register_and_serve(
    server_url: &str,
    email: &str,
    password: &str,
    device_name: &str,
    input_sink: Box<dyn InputSink>,
    file_receive_dir: std::path::PathBuf,
) -> Result<HostHandle> {
    let mut signaling = SignalingClient::connect(server_url).await?;

    signaling
        .send(&ClientMessage::Register { email: email.to_string(), password: password.to_string() })
        .await?;
    match signaling.recv().await? {
        ServerMessage::AuthOk { .. } => {}
        ServerMessage::AuthError { .. } => {
            // Account likely already exists from a previous run; try logging in.
            signaling
                .send(&ClientMessage::Login { email: email.to_string(), password: password.to_string() })
                .await?;
            match signaling.recv().await? {
                ServerMessage::AuthOk { .. } => {}
                other => anyhow::bail!("login failed: {other:?}"),
            }
        }
        other => anyhow::bail!("unexpected auth response: {other:?}"),
    }

    signaling
        .send(&ClientMessage::RegisterDevice { name: device_name.to_string() })
        .await?;
    let (device_id, one_time_password) = match signaling.recv().await? {
        ServerMessage::DeviceRegistered { device_id, one_time_password } => (device_id, one_time_password),
        other => anyhow::bail!("expected DeviceRegistered, got {other:?}"),
    };

    let handle = HostHandle { device_id: device_id.clone(), one_time_password: one_time_password.clone() };
    let input_sink = Arc::new(AsyncMutex::new(input_sink));

    tokio::spawn(async move {
        if let Err(e) = serve_connections(signaling, input_sink, file_receive_dir).await {
            tracing_or_eprintln(&format!("host loop ended: {e:#}"));
        }
    });

    Ok(handle)
}

fn tracing_or_eprintln(msg: &str) {
    eprintln!("[visuara-host] {msg}");
}

type SharedInputSink = Arc<AsyncMutex<Box<dyn InputSink>>>;

async fn serve_connections(
    mut signaling: SignalingClient,
    input_sink: SharedInputSink,
    file_receive_dir: std::path::PathBuf,
) -> Result<()> {
    loop {
        let session_id = match signaling.recv().await? {
            ServerMessage::IncomingConnection { session_id, .. } => session_id,
            ServerMessage::Error { message } => {
                tracing_or_eprintln(&format!("server error: {message}"));
                continue;
            }
            _ => continue,
        };
        handle_one_connection(&mut signaling, &session_id, input_sink.clone(), file_receive_dir.clone()).await?;
    }
}

async fn handle_one_connection(
    signaling: &mut SignalingClient,
    session_id: &str,
    input_sink: SharedInputSink,
    file_receive_dir: std::path::PathBuf,
) -> Result<()> {
    signaling.send(&ClientMessage::RequestTurnCredentials).await?;
    let ice_servers = match signaling.recv().await? {
        ServerMessage::TurnCredentials { urls, username, password, .. } => {
            vec![RTCIceServer { urls, username, credential: password, ..Default::default() }]
        }
        _ => vec![],
    };

    let pc = build_peer_connection(ice_servers).await?;
    pc.on_peer_connection_state_change(Box::new(|s| {
        tracing_or_eprintln(&format!("[host] connection state: {s:?}"));
        Box::pin(async {})
    }));
    pc.on_ice_connection_state_change(Box::new(|s| {
        tracing_or_eprintln(&format!("[host] ice state: {s:?}"));
        Box::pin(async {})
    }));

    let video_track = make_video_track();
    pc.add_track(video_track.clone() as Arc<dyn webrtc::track::track_local::TrackLocal + Send + Sync>)
        .await
        .context("add video track")?;

    let (candidate_tx, mut candidate_rx) = tokio::sync::mpsc::unbounded_channel::<RTCIceCandidate>();
    pc.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
        if let Some(c) = c {
            let _ = candidate_tx.send(c);
        }
        Box::pin(async {})
    }));

    // Monitor switches from the controller reach the blocking capture thread
    // through this plain channel (it's a std thread, not a tokio task).
    let (monitor_switch_tx, monitor_switch_rx) = std::sync::mpsc::channel::<u32>();
    let file_receiver = Arc::new(FileReceiver::new(file_receive_dir)?);

    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        tracing_or_eprintln("on_data_channel fired");
        let input_sink = input_sink.clone();
        let file_receiver = file_receiver.clone();
        let monitor_switch_tx = monitor_switch_tx.clone();
        Box::pin(async move {
            let dc_for_open = dc.clone();
            dc.on_open(Box::new(move || {
                tracing_or_eprintln("data channel open (host side)");
                let dc = dc_for_open.clone();
                Box::pin(async move {
                    if let Ok(monitors) = list_monitors() {
                        let msg = ControlMessage::Monitor(MonitorMessage::List { monitors });
                        if let Ok(bytes) = msg.to_bytes() {
                            let _ = dc.send(&bytes.into()).await;
                        }
                    }
                })
            }));

            let clipboard_sync = Arc::new({
                let dc = dc.clone();
                // ClipboardSync's polling thread is a plain std::thread with
                // no ambient tokio runtime, so on_local_change must spawn
                // via an explicit Handle rather than the bare tokio::spawn.
                let rt_handle = tokio::runtime::Handle::current();
                ClipboardSync::start(
                    || Ok(Box::new(ClipboardHandle::new()?)),
                    move |text| {
                        let dc = dc.clone();
                        rt_handle.spawn(async move {
                            let msg = ControlMessage::Clipboard(ClipboardMessage::TextUpdated { text });
                            if let Ok(bytes) = msg.to_bytes() {
                                let _ = dc.send(&bytes.into()).await;
                            }
                        });
                    },
                )
            });

            let dc_for_messages = dc.clone();
            dc.on_message(Box::new(move |msg: DataChannelMessage| {
                let input_sink = input_sink.clone();
                let file_receiver = file_receiver.clone();
                let monitor_switch_tx = monitor_switch_tx.clone();
                let dc = dc_for_messages.clone();
                let clipboard_sync = clipboard_sync.clone();
                Box::pin(async move {
                    match ControlMessage::from_bytes(&msg.data) {
                        Ok(ControlMessage::Input(event)) => {
                            let mut sink = input_sink.lock().await;
                            if let Err(e) = sink.handle_input(event) {
                                tracing_or_eprintln(&format!("failed to apply input event: {e:#}"));
                            }
                        }
                        Ok(ControlMessage::Clipboard(ClipboardMessage::TextUpdated { text })) => {
                            clipboard_sync.apply_remote_update(text);
                        }
                        Ok(ControlMessage::File(file_msg)) => {
                            if let Err(e) = file_receiver.handle(file_msg) {
                                tracing_or_eprintln(&format!("file transfer error: {e:#}"));
                            }
                        }
                        Ok(ControlMessage::Monitor(MonitorMessage::ListRequest)) => {
                            if let Ok(monitors) = list_monitors() {
                                let reply = ControlMessage::Monitor(MonitorMessage::List { monitors });
                                if let Ok(bytes) = reply.to_bytes() {
                                    let _ = dc.send(&bytes.into()).await;
                                }
                            }
                        }
                        Ok(ControlMessage::Monitor(MonitorMessage::SwitchRequest { monitor_id })) => {
                            let _ = monitor_switch_tx.send(monitor_id);
                            let reply = ControlMessage::Monitor(MonitorMessage::Switched { monitor_id });
                            if let Ok(bytes) = reply.to_bytes() {
                                let _ = dc.send(&bytes.into()).await;
                            }
                        }
                        Ok(other) => tracing_or_eprintln(&format!("ignoring control message: {other:?}")),
                        Err(e) => tracing_or_eprintln(&format!("failed to parse control message: {e:#}")),
                    }
                })
            }));
        })
    }));

    // Wait for the controller's SDP offer for this session, answer it.
    let sdp = loop {
        match signaling.recv().await? {
            ServerMessage::SdpOffer { session_id: s, sdp } if s == session_id => break sdp,
            ServerMessage::IceCandidate { session_id: s, candidate } if s == session_id => {
                let init = decode_ice_candidate(&candidate)?;
                pc.add_ice_candidate(init).await.context("add remote ICE candidate")?;
            }
            _ => continue,
        }
    };
    let offer = RTCSessionDescription::offer(sdp).context("parse remote offer")?;
    pc.set_remote_description(offer).await.context("set remote description")?;
    let answer = pc.create_answer(None).await.context("create answer")?;
    pc.set_local_description(answer.clone()).await.context("set local description")?;
    signaling
        .send(&ClientMessage::SdpAnswer { session_id: session_id.to_string(), sdp: answer.sdp })
        .await?;

    // Capture + encode happen on a dedicated blocking thread: xcap's Monitor
    // wraps a raw platform handle that isn't Send/Sync, so it can never be
    // held across an .await inside this async task. Only the encoded bytes
    // (a plain Vec<u8>) cross back over a channel.
    let (encoded_tx, mut encoded_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    tokio::task::spawn_blocking(move || {
        let mut capturer = match Capturer::primary() {
            Ok(c) => c,
            Err(e) => {
                tracing_or_eprintln(&format!("capture init failed: {e:#}"));
                return;
            }
        };
        let mut encoder = match VideoEncoder::new() {
            Ok(e) => e,
            Err(e) => {
                tracing_or_eprintln(&format!("encoder init failed: {e:#}"));
                return;
            }
        };
        loop {
            if let Ok(monitor_id) = monitor_switch_rx.try_recv() {
                match Capturer::for_monitor_id(monitor_id) {
                    Ok(c) => capturer = c,
                    Err(e) => tracing_or_eprintln(&format!("switch to monitor {monitor_id} failed: {e:#}")),
                }
            }
            let result = capturer
                .capture_frame()
                .and_then(|frame| encoder.encode_frame(&frame));
            match result {
                Ok(bytes) => {
                    if encoded_tx.blocking_send(bytes).is_err() {
                        break;
                    }
                }
                Err(e) => tracing_or_eprintln(&format!("capture/encode error: {e:#}")),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let session_id_owned = session_id.to_string();
    loop {
        tokio::select! {
            Some(candidate) = candidate_rx.recv() => {
                let encoded = encode_ice_candidate(candidate)?;
                signaling.send(&ClientMessage::IceCandidate { session_id: session_id_owned.clone(), candidate: encoded }).await?;
            }
            Some(encoded) = encoded_rx.recv() => {
                video_track
                    .write_sample(&Sample { data: encoded.into(), duration: Duration::from_millis(100), ..Default::default() })
                    .await
                    .context("write video sample")?;
            }
            msg = signaling.recv() => {
                if let ServerMessage::IceCandidate { session_id: s, candidate } = msg? {
                    if s == session_id_owned {
                        let init = decode_ice_candidate(&candidate)?;
                        pc.add_ice_candidate(init).await.context("add remote ICE candidate")?;
                    }
                }
            }
        }
    }
}

/// Sets (or changes) the fixed unattended-access password for a device the
/// caller's account owns. Opens a short-lived signaling connection just for
/// this request — separate from the device's own persistent connection.
pub async fn set_unattended_password(
    server_url: &str,
    email: &str,
    password: &str,
    target_device_id: &str,
    unattended_password: &str,
) -> Result<()> {
    let mut signaling = SignalingClient::connect(server_url).await?;
    signaling
        .send(&ClientMessage::Login { email: email.to_string(), password: password.to_string() })
        .await?;
    match signaling.recv().await? {
        ServerMessage::AuthOk { .. } => {}
        other => anyhow::bail!("login failed: {other:?}"),
    }
    signaling
        .send(&ClientMessage::SetUnattendedPassword {
            target_device_id: target_device_id.to_string(),
            password: unattended_password.to_string(),
        })
        .await?;
    Ok(())
}
