//! Shared WebRTC plumbing used by both the host and controller roles: peer
//! connection setup, the H.264 video track, and ICE candidate JSON
//! (de)serialization for relaying over the signaling WebSocket.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

pub async fn build_peer_connection(ice_servers: Vec<RTCIceServer>) -> Result<Arc<RTCPeerConnection>> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .context("register default codecs")?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .context("register interceptors")?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers,
        ..Default::default()
    };

    let pc = api
        .new_peer_connection(config)
        .await
        .context("create peer connection")?;
    Ok(Arc::new(pc))
}

pub fn make_video_track() -> Arc<TrackLocalStaticSample> {
    Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "visuara".to_owned(),
    ))
}

/// Our signaling protocol carries ICE candidates as an opaque `String`; we
/// JSON-encode the candidate init struct into that string.
pub fn encode_ice_candidate(candidate: RTCIceCandidate) -> Result<String> {
    let init = candidate.to_json().context("serialize ICE candidate")?;
    serde_json::to_string(&init).context("encode ICE candidate as JSON")
}

pub fn decode_ice_candidate(candidate: &str) -> Result<RTCIceCandidateInit> {
    serde_json::from_str(candidate).context("decode ICE candidate JSON")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCredentials {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

pub fn ice_servers_from_turn(creds: &TurnCredentials) -> Vec<RTCIceServer> {
    vec![RTCIceServer {
        urls: creds.urls.clone(),
        username: creds.username.clone(),
        credential: creds.password.clone(),
        ..Default::default()
    }]
}
