//! End-to-end HTTP test for the admin settings API and the public download
//! endpoint: the first registered account becomes admin, admin-only
//! settings persistence works, and a downloaded "client" actually comes
//! back with the configured server URL/device name patched into its
//! embedded-config slot.

use dashmap::DashMap;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

use visuara_common::embedded_config::{EmbeddedConfig, CONFIG_MAGIC, CONFIG_SLOT_SIZE};
use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

/// A stand-in for a real release binary: just enough bytes around the
/// marker to prove the patch/serve logic works. The real byte-for-byte
/// patching mechanism itself is proven against an actual compiled
/// executable in visuara-client's embedded_config_patch test.
fn fake_template_bytes() -> Vec<u8> {
    let mut data = b"fake-pe-header-padding-".to_vec();
    data.extend_from_slice(CONFIG_MAGIC);
    data.extend_from_slice(&[0u8; CONFIG_SLOT_SIZE]);
    data.extend_from_slice(b"-trailing-section-bytes");
    data
}

async fn spawn_test_server(templates_dir: PathBuf) -> String {
    let db = Db::open(":memory:").expect("open in-memory db");
    let state = AppState {
        db,
        turn: Arc::new(TurnConfig { urls: vec![], shared_secret: "irrelevant".to_string() }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        client_templates_dir: Arc::new(templates_dir),
        fetched_templates_dir: Arc::new(std::env::temp_dir().join(format!("visuara-test-fetched-{}", uuid::Uuid::new_v4()))),
    };
    let app = visuara_signaling::build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn admin_bootstrap_settings_and_download_round_trip() {
    let templates_dir = std::env::temp_dir().join(format!("visuara-test-templates-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&templates_dir).expect("create temp templates dir");
    std::fs::write(templates_dir.join("windows-x86_64.exe"), fake_template_bytes()).expect("write fake template");

    let base_url = spawn_test_server(templates_dir.clone()).await;
    let client = reqwest::Client::builder().cookie_store(true).build().expect("build client");

    // Admin-only settings reject an unauthenticated request.
    let resp = client.get(format!("{base_url}/api/v1/admin/settings")).send().await.expect("get settings unauth");
    assert_eq!(resp.status(), 401);

    // The first account ever registered becomes admin automatically.
    let resp = client
        .post(format!("{base_url}/api/v1/auth/register"))
        .json(&json!({ "email": "admin@example.com", "password": "hunter2" }))
        .send()
        .await
        .expect("register first account");
    assert_eq!(resp.status(), 200);
    let account: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(account["role"], "admin");

    // Registration auto-disabled the moment the first account was created.
    let resp = client.get(format!("{base_url}/api/v1/registration-status")).send().await.expect("registration status");
    let status: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status["enabled"], false);

    // A second self-service registration is now rejected...
    let resp = client
        .post(format!("{base_url}/api/v1/auth/register"))
        .json(&json!({ "email": "second@example.com", "password": "swordfish" }))
        .send()
        .await
        .expect("second registration attempt");
    assert_eq!(resp.status(), 403);

    // ...but the admin can still create accounts directly, bypassing the gate.
    let resp = client
        .post(format!("{base_url}/api/v1/admin/accounts"))
        .json(&json!({ "email": "second@example.com", "password": "swordfish" }))
        .send()
        .await
        .expect("admin-created account");
    assert_eq!(resp.status(), 201);

    // Save settings as the now-authenticated admin.
    let resp = client
        .put(format!("{base_url}/api/v1/admin/settings"))
        .json(&json!({ "server_url": "wss://visuara.example.com/ws", "default_device_name": "warehouse-01" }))
        .send()
        .await
        .expect("save settings");
    assert_eq!(resp.status(), 200);

    // Public download listing reflects the configured server URL and lists
    // the platform we dropped a template for.
    let resp = client.get(format!("{base_url}/api/v1/download")).send().await.expect("get download listing");
    let listing: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(listing["server_url"], "wss://visuara.example.com/ws");
    assert!(listing["options"].as_array().unwrap().iter().any(|o| o["platform"] == "windows-x86_64" && o["available"] == true));

    // Downloading actually patches the template with the current settings.
    let resp = client.get(format!("{base_url}/api/v1/download/windows-x86_64")).send().await.expect("download");
    assert_eq!(resp.status(), 200);
    let content_disposition = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .expect("content-disposition header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_disposition.contains("visuara.exe"));
    let bytes = resp.bytes().await.expect("download bytes");
    let config = EmbeddedConfig::find_in_binary(&bytes)
        .expect("search downloaded binary")
        .expect("downloaded binary should carry embedded config");
    assert_eq!(config.server_url.as_deref(), Some("wss://visuara.example.com/ws"));
    assert_eq!(config.device_name.as_deref(), Some("warehouse-01"));

    // A platform with no uploaded template 404s with a helpful message.
    let resp = client.get(format!("{base_url}/api/v1/download/linux-x86_64")).send().await.expect("download missing");
    assert_eq!(resp.status(), 404);

    let _ = std::fs::remove_dir_all(&templates_dir);
}
