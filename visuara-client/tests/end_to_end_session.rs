//! The critical-path proof from the plan: a host and a controller, talking
//! through a real in-process signaling server, negotiate a real WebRTC
//! session, stream real captured-and-H.264-encoded screen frames from host
//! to controller, and deliver a real input event from controller to host
//! over the data channel.
//!
//! The host uses a recording input sink instead of the real OS input
//! injector so this test doesn't move the machine's actual mouse cursor.

use anyhow::Result;
use dashmap::{DashMap, DashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use visuara_client::host::{register_and_serve, InputSink};
use visuara_common::control::InputEvent;
use visuara_common::signaling::ConnectCredential;
use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

struct RecordingSink {
    tx: mpsc::UnboundedSender<InputEvent>,
}

impl InputSink for RecordingSink {
    fn handle_input(&mut self, event: InputEvent) -> Result<()> {
        let _ = self.tx.send(event);
        Ok(())
    }
}

async fn spawn_signaling_server() -> String {
    let db = Db::open(":memory:").expect("open in-memory db");
    let state = AppState {
        db,
        turn: Arc::new(TurnConfig {
            urls: vec!["turn:localhost:3478".to_string()],
            shared_secret: "test-secret".to_string(),
        }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        admin_password: Arc::new("test-admin-password".to_string()),
        admin_sessions: Arc::new(DashSet::new()),
        client_templates_dir: Arc::new(PathBuf::from("client-templates")),
    };
    let app = visuara_signaling::build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://127.0.0.1:{port}/ws")
}

#[tokio::test]
async fn host_streams_video_and_receives_input() {
    let server_url = spawn_signaling_server().await;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel();
    let sink = Box::new(RecordingSink { tx: input_tx });

    let host_handle = register_and_serve(&server_url, "host@test.local", "hunter2", "test-host", sink)
        .await
        .expect("host registration");

    let mut session = tokio::time::timeout(
        Duration::from_secs(30),
        visuara_client::controller::connect(
            &server_url,
            "controller@test.local",
            "swordfish",
            &host_handle.device_id,
            ConnectCredential::OneTimePassword(host_handle.one_time_password),
        ),
    )
    .await
    .expect("controller connect timed out")
    .expect("controller connect failed");

    // Video: wait for at least one real decoded frame from the host's screen.
    let frame = tokio::time::timeout(Duration::from_secs(30), session.frames.recv())
        .await
        .expect("timed out waiting for video frame")
        .expect("frame channel closed");
    assert!(frame.width() > 0 && frame.height() > 0, "decoded frame should have real dimensions");
    println!("received decoded frame {}x{}", frame.width(), frame.height());

    // Data channel: send an input event, retrying until the channel is open,
    // then confirm the host actually received it.
    let event = InputEvent::MouseMove { x: 123, y: 456 };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = visuara_client::controller::send_input(&session.data_channel, event.clone()).await;
        println!("send_input attempt {attempt}: {result:?}");
        if result.is_ok() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("data channel never became ready to send");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let received = tokio::time::timeout(Duration::from_secs(20), input_rx.recv())
        .await
        .expect("timed out waiting for host to receive input event")
        .expect("input channel closed");
    assert_eq!(received, event);
}
