//! Proves file transfer and monitor listing work over a real WebRTC session
//! between a host and controller talking through a real in-process
//! signaling server — the same critical-path setup as
//! end_to_end_session.rs, extended to cover the features added after the
//! initial P2P demo.
//!
//! Deliberately does not touch the real system clipboard (see
//! clipboard_sync.rs's own unit tests for that logic) or move the real
//! mouse — the host's InputSink is a no-op recorder as elsewhere.

use anyhow::Result;
use dashmap::{DashMap, DashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use visuara_client::host::{register_and_serve, InputSink};
use visuara_common::control::InputEvent;
use visuara_common::signaling::ConnectCredential;
use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

struct NoopSink;
impl InputSink for NoopSink {
    fn handle_input(&mut self, _event: InputEvent) -> Result<()> {
        Ok(())
    }
}

async fn spawn_signaling_server() -> String {
    let db = Db::open(":memory:").expect("open in-memory db");
    let state = AppState {
        db,
        turn: Arc::new(TurnConfig { urls: vec!["turn:localhost:3478".to_string()], shared_secret: "test-secret".to_string() }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        admin_password: Arc::new("test-admin-password".to_string()),
        admin_sessions: Arc::new(DashSet::new()),
        user_sessions: Arc::new(DashMap::new()),
        client_templates_dir: Arc::new(PathBuf::from("client-templates")),
        fetched_templates_dir: Arc::new(PathBuf::from("fetched-client-templates")),
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
async fn file_transfer_and_monitor_list_over_a_real_session() {
    let server_url = spawn_signaling_server().await;
    let received_dir = std::env::temp_dir().join(format!("visuara-file-test-{}", uuid::Uuid::new_v4()));

    let host_handle = register_and_serve(
        &server_url,
        "host@test.local",
        "hunter2",
        "test-host",
        Box::new(NoopSink),
        received_dir.clone(),
    )
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

    // The host proactively announces its real monitor list once the data
    // channel opens — this is genuine `xcap` enumeration, not a fixture.
    let monitors = tokio::time::timeout(Duration::from_secs(15), session.monitor_updates.recv())
        .await
        .expect("timed out waiting for monitor list")
        .expect("monitor channel closed");
    assert!(!monitors.is_empty(), "expected at least one real monitor");
    println!("host reported monitors: {monitors:?}");

    // Wait for the data channel to actually be open before sending a file
    // (mirrors the retry pattern used for input events in the other test).
    let source_dir = std::env::temp_dir().join(format!("visuara-file-source-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&source_dir).expect("create source dir");
    let source_path = source_dir.join("transfer-me.txt");
    let payload = "this file was transferred over a real WebRTC data channel\n".repeat(500);
    std::fs::write(&source_path, &payload).expect("write source file");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if visuara_client::file_transfer::send_file(&session.data_channel, &source_path).await.is_ok() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("data channel never became ready to send the file");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Poll for the file to show up on the "host" side (written to
    // received_dir by the real FileReceiver, over the real chunked
    // transfer), rather than assuming a fixed delay.
    let received_path = received_dir.join("transfer-me.txt");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if received_path.exists() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("host never received the transferred file");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let received_contents = std::fs::read_to_string(&received_path).expect("read received file");
    assert_eq!(received_contents, payload, "received file must match exactly");

    let _ = std::fs::remove_dir_all(&source_dir);
    let _ = std::fs::remove_dir_all(&received_dir);
}
