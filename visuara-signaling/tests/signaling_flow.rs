//! End-to-end smoke test for the signaling server: two WebSocket clients
//! (playing host and controller) walk through account creation, device
//! registration, pairing by one-time password, SDP/ICE relay, TURN
//! credential issuance, and unattended-password push.

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use visuara_common::signaling::{ClientMessage, ConnectCredential, ServerMessage};
use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

const TEST_TURN_SECRET: &str = "test-shared-secret";

async fn spawn_test_server() -> u16 {
    let db = Db::open(":memory:").expect("open in-memory db");
    let state = AppState {
        db,
        turn: Arc::new(TurnConfig {
            urls: vec!["turn:localhost:3478".to_string()],
            shared_secret: TEST_TURN_SECRET.to_string(),
        }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        client_templates_dir: Arc::new(PathBuf::from("client-templates")),
        fetched_templates_dir: Arc::new(PathBuf::from("fetched-client-templates")),
    };
    let app = visuara_signaling::build_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn connect(port: u16) -> WsStream {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let (stream, _) = tokio_tungstenite::connect_async(url).await.expect("connect");
    stream
}

async fn send(ws: &mut WsStream, msg: ClientMessage) {
    let text = serde_json::to_string(&msg).unwrap();
    ws.send(Message::Text(text.into())).await.unwrap();
}

async fn recv(ws: &mut WsStream) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ws.next().await.expect("stream ended").expect("ws error") {
                Message::Text(text) => return serde_json::from_str(&text).expect("parse"),
                _ => continue,
            }
        }
    })
    .await
    .expect("timed out waiting for message")
}

#[tokio::test]
async fn full_pairing_and_relay_flow() {
    let port = spawn_test_server().await;

    let mut host = connect(port).await;
    let mut controller = connect(port).await;

    // Host account + device registration. As the first account ever
    // created, the host becomes admin and registration auto-disables — so
    // it re-enables registration (as admin, over the HTTP API) before the
    // controller's own signup below.
    send(&mut host, ClientMessage::Register { email: "host@example.com".into(), password: "hunter2".into() }).await;
    assert!(matches!(recv(&mut host).await, ServerMessage::AuthOk { .. }));

    let http = reqwest::Client::builder().cookie_store(true).build().expect("build http client");
    let base_url = format!("http://127.0.0.1:{port}");
    let resp = http
        .post(format!("{base_url}/api/v1/auth/login"))
        .json(&serde_json::json!({ "email": "host@example.com", "password": "hunter2" }))
        .send()
        .await
        .expect("admin login");
    assert_eq!(resp.status(), 200);
    let resp = http
        .put(format!("{base_url}/api/v1/admin/registration"))
        .json(&serde_json::json!({ "enabled": true }))
        .send()
        .await
        .expect("re-enable registration");
    assert_eq!(resp.status(), 200);

    send(&mut host, ClientMessage::RegisterDevice { name: "office-pc".into() }).await;
    let (device_id, otp) = match recv(&mut host).await {
        ServerMessage::DeviceRegistered { device_id, one_time_password } => (device_id, one_time_password),
        other => panic!("expected DeviceRegistered, got {other:?}"),
    };

    // Controller account, then pair using the host's one-time password.
    send(&mut controller, ClientMessage::Register { email: "controller@example.com".into(), password: "swordfish".into() }).await;
    assert!(matches!(recv(&mut controller).await, ServerMessage::AuthOk { .. }));

    send(
        &mut controller,
        ClientMessage::RequestConnection {
            target_device_id: device_id.clone(),
            credential: ConnectCredential::OneTimePassword(otp),
        },
    )
    .await;

    let session_id = match recv(&mut controller).await {
        ServerMessage::ConnectionEstablished { session_id, target_device_id: t } => {
            assert_eq!(t, device_id);
            session_id
        }
        other => panic!("expected ConnectionEstablished, got {other:?}"),
    };
    match recv(&mut host).await {
        ServerMessage::IncomingConnection { session_id: s, .. } => assert_eq!(s, session_id),
        other => panic!("expected IncomingConnection, got {other:?}"),
    }

    // SDP offer/answer relay.
    send(&mut controller, ClientMessage::SdpOffer { session_id: session_id.clone(), sdp: "v=0 offer".into() }).await;
    match recv(&mut host).await {
        ServerMessage::SdpOffer { sdp, .. } => assert_eq!(sdp, "v=0 offer"),
        other => panic!("expected SdpOffer, got {other:?}"),
    }

    send(&mut host, ClientMessage::SdpAnswer { session_id: session_id.clone(), sdp: "v=0 answer".into() }).await;
    match recv(&mut controller).await {
        ServerMessage::SdpAnswer { sdp, .. } => assert_eq!(sdp, "v=0 answer"),
        other => panic!("expected SdpAnswer, got {other:?}"),
    }

    // ICE candidate relay, both directions.
    send(&mut controller, ClientMessage::IceCandidate { session_id: session_id.clone(), candidate: "candidate-a".into() }).await;
    match recv(&mut host).await {
        ServerMessage::IceCandidate { candidate, .. } => assert_eq!(candidate, "candidate-a"),
        other => panic!("expected IceCandidate, got {other:?}"),
    }
    send(&mut host, ClientMessage::IceCandidate { session_id: session_id.clone(), candidate: "candidate-b".into() }).await;
    match recv(&mut controller).await {
        ServerMessage::IceCandidate { candidate, .. } => assert_eq!(candidate, "candidate-b"),
        other => panic!("expected IceCandidate, got {other:?}"),
    }

    // TURN credential issuance: verify the HMAC actually matches the shared secret.
    send(&mut controller, ClientMessage::RequestTurnCredentials).await;
    match recv(&mut controller).await {
        ServerMessage::TurnCredentials { username, password, .. } => {
            use hmac::{Hmac, Mac};
            use sha1::Sha1;
            let mut mac = Hmac::<Sha1>::new_from_slice(TEST_TURN_SECRET.as_bytes()).unwrap();
            mac.update(username.as_bytes());
            let expected = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, mac.finalize().into_bytes());
            assert_eq!(password, expected);
        }
        other => panic!("expected TurnCredentials, got {other:?}"),
    }

    // Unattended password: set it, then confirm it round-trips as a valid credential.
    send(
        &mut host,
        ClientMessage::SetUnattendedPassword { target_device_id: device_id.clone(), password: "unattended-pw".into() },
    )
    .await;
    match recv(&mut host).await {
        ServerMessage::UnattendedPasswordUpdated { password } => assert_eq!(password, "unattended-pw"),
        other => panic!("expected UnattendedPasswordUpdated, got {other:?}"),
    }

    send(
        &mut controller,
        ClientMessage::RequestConnection {
            target_device_id: device_id.clone(),
            credential: ConnectCredential::UnattendedPassword("unattended-pw".into()),
        },
    )
    .await;
    assert!(matches!(recv(&mut controller).await, ServerMessage::ConnectionEstablished { .. }));
}
