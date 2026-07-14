//! The controller role: pairs with a target device by ID + credential,
//! negotiates a WebRTC session as the offerer, sends local input events over
//! the data channel, and decodes the incoming video track.

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::rtp_transceiver::rtp_receiver::RTCRtpReceiver;
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCRtpTransceiver, RTCRtpTransceiverInit};
use webrtc::track::track_remote::TrackRemote;

use visuara_agent::decode::VideoDecoder;
use visuara_common::control::{ControlMessage, InputEvent};
use visuara_common::signaling::{ClientMessage, ConnectCredential, ServerMessage};

use crate::session::{build_peer_connection, decode_ice_candidate, encode_ice_candidate};
use crate::signaling_client::SignalingClient;

/// A decoded remote frame, handed to whatever is rendering the session
/// (an egui window in the real app, or a test harness).
pub type FrameSender = mpsc::UnboundedSender<image::RgbaImage>;

pub struct ControllerSession {
    pub data_channel: Arc<RTCDataChannel>,
    pub frames: mpsc::UnboundedReceiver<image::RgbaImage>,
}

pub async fn connect(
    server_url: &str,
    email: &str,
    password: &str,
    target_device_id: &str,
    credential: ConnectCredential,
) -> Result<ControllerSession> {
    let mut signaling = SignalingClient::connect(server_url).await?;

    signaling
        .send(&ClientMessage::Register { email: email.to_string(), password: password.to_string() })
        .await?;
    match signaling.recv().await? {
        ServerMessage::AuthOk { .. } => {}
        ServerMessage::AuthError { .. } => {
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

    signaling.send(&ClientMessage::RequestTurnCredentials).await?;
    let ice_servers = match signaling.recv().await? {
        ServerMessage::TurnCredentials { urls, username, password, .. } => {
            vec![RTCIceServer { urls, username, credential: password, ..Default::default() }]
        }
        _ => vec![],
    };

    signaling
        .send(&ClientMessage::RequestConnection { target_device_id: target_device_id.to_string(), credential })
        .await?;
    let session_id = match signaling.recv().await? {
        ServerMessage::ConnectionEstablished { session_id, .. } => session_id,
        other => anyhow::bail!("expected ConnectionEstablished, got {other:?}"),
    };

    let pc = build_peer_connection(ice_servers).await?;
    pc.on_peer_connection_state_change(Box::new(|s| {
        eprintln!("[controller] connection state: {s:?}");
        Box::pin(async {})
    }));
    pc.on_ice_connection_state_change(Box::new(|s| {
        eprintln!("[controller] ice state: {s:?}");
        Box::pin(async {})
    }));

    let (candidate_tx, mut candidate_rx) = mpsc::unbounded_channel::<RTCIceCandidate>();
    pc.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
        if let Some(c) = c {
            let _ = candidate_tx.send(c);
        }
        Box::pin(async {})
    }));

    let (frame_tx, frame_rx) = mpsc::unbounded_channel::<image::RgbaImage>();
    pc.on_track(Box::new(move |track: Arc<TrackRemote>, _receiver: Arc<RTCRtpReceiver>, _transceiver: Arc<RTCRtpTransceiver>| {
        eprintln!("[controller] on_track fired");
        let frame_tx = frame_tx.clone();
        Box::pin(async move {
            tokio::spawn(async move {
                if let Err(e) = read_video_track(track, frame_tx).await {
                    eprintln!("[visuara-controller] video track ended: {e:#}");
                }
            });
        })
    }));

    let data_channel = pc.create_data_channel("control", None).await.context("create data channel")?;

    // We don't send video, only receive it, but the offerer still has to
    // declare a video m-line for the host to answer into.
    pc.add_transceiver_from_kind(
        RTPCodecType::Video,
        Some(RTCRtpTransceiverInit { direction: RTCRtpTransceiverDirection::Recvonly, send_encodings: vec![] }),
    )
    .await
    .context("add recvonly video transceiver")?;

    let offer = pc.create_offer(None).await.context("create offer")?;
    pc.set_local_description(offer.clone()).await.context("set local description")?;
    signaling
        .send(&ClientMessage::SdpOffer { session_id: session_id.clone(), sdp: offer.sdp })
        .await?;

    let sdp = loop {
        match signaling.recv().await? {
            ServerMessage::SdpAnswer { session_id: s, sdp } if s == session_id => break sdp,
            ServerMessage::IceCandidate { session_id: s, candidate } if s == session_id => {
                let init = decode_ice_candidate(&candidate)?;
                pc.add_ice_candidate(init).await.context("add remote ICE candidate")?;
            }
            _ => continue,
        }
    };
    let answer = RTCSessionDescription::answer(sdp).context("parse remote answer")?;
    pc.set_remote_description(answer).await.context("set remote description")?;

    let session_id_owned = session_id.clone();
    let pc_for_ice = pc.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(candidate) = candidate_rx.recv() => {
                    let Ok(encoded) = encode_ice_candidate(candidate) else { continue };
                    if signaling
                        .send(&ClientMessage::IceCandidate { session_id: session_id_owned.clone(), candidate: encoded })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                msg = signaling.recv() => {
                    match msg {
                        Ok(ServerMessage::IceCandidate { session_id: s, candidate }) if s == session_id_owned => {
                            if let Ok(init) = decode_ice_candidate(&candidate) {
                                let _ = pc_for_ice.add_ice_candidate(init).await;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    });

    Ok(ControllerSession { data_channel, frames: frame_rx })
}

async fn read_video_track(track: Arc<TrackRemote>, frame_tx: mpsc::UnboundedSender<image::RgbaImage>) -> Result<()> {
    let mut depacketizer = rtp::codecs::h264::H264Packet::default();
    let mut decoder = VideoDecoder::new().context("create video decoder")?;

    loop {
        let (packet, _attrs) = track.read_rtp().await.context("read RTP packet")?;
        let nal = {
            use rtp::packetizer::Depacketizer;
            depacketizer.depacketize(&packet.payload).context("depacketize H.264 payload")?
        };
        if nal.is_empty() {
            continue;
        }
        // H264Packet already emits Annex-B (start-code-prefixed) output, so
        // `nal` is fed to the decoder as-is.
        if let Some(frame) = decoder.decode(&nal).context("decode H.264 NAL")? {
            if frame_tx.send(frame).is_err() {
                return Ok(());
            }
        }
    }
}

/// Sends a local input event to the remote host over the data channel.
pub async fn send_input(dc: &RTCDataChannel, event: InputEvent) -> Result<()> {
    let bytes = ControlMessage::Input(event).to_bytes().context("encode input event")?;
    dc.send(&bytes.into()).await.context("send input event")?;
    Ok(())
}
