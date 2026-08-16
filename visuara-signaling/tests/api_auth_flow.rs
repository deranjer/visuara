//! Regression coverage for the parts of the auth/admin API not already
//! exercised by `admin_download_flow.rs`: sessions persisting in SQLite
//! across a simulated server restart, and the last-remaining-admin guard on
//! role changes/account deletion.

use dashmap::DashMap;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

fn new_state(db: Db) -> AppState {
    AppState {
        db,
        turn: Arc::new(TurnConfig { urls: vec![], shared_secret: "irrelevant".to_string() }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        client_templates_dir: Arc::new(PathBuf::from("client-templates")),
        fetched_templates_dir: Arc::new(std::env::temp_dir().join(format!("visuara-test-fetched-{}", uuid::Uuid::new_v4()))),
    }
}

async fn spawn(db_path: &str) -> String {
    let db = Db::open(db_path).expect("open db");
    let app = visuara_signaling::build_router(new_state(db));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn session_survives_simulated_restart() {
    let db_path = std::env::temp_dir().join(format!("visuara-test-sessions-{}.db", uuid::Uuid::new_v4()));
    let db_path_str = db_path.to_str().unwrap().to_string();

    let base_url = spawn(&db_path_str).await;
    let client = reqwest::Client::builder().cookie_store(true).build().expect("build client");

    let resp = client
        .post(format!("{base_url}/api/v1/auth/register"))
        .json(&json!({ "email": "admin@example.com", "password": "hunter2" }))
        .send()
        .await
        .expect("register");
    assert_eq!(resp.status(), 200);

    // Confirm the session works against the first server instance.
    let resp = client.get(format!("{base_url}/api/v1/auth/me")).send().await.expect("me before restart");
    assert_eq!(resp.status(), 200);

    // Simulate a server restart: spin up a brand new AppState/router against
    // the same on-disk database file — nothing about the running process is
    // reused except the SQLite file, since sessions are no longer held in an
    // in-memory map.
    let base_url_2 = spawn(&db_path_str).await;
    let resp = client.get(format!("{base_url_2}/api/v1/auth/me")).send().await.expect("me after restart");
    assert_eq!(resp.status(), 200, "session cookie should still authenticate after a restart");
    let account: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(account["email"], "admin@example.com");

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn cannot_demote_or_delete_the_last_admin() {
    let db_path = std::env::temp_dir().join(format!("visuara-test-lastadmin-{}.db", uuid::Uuid::new_v4()));
    let db_path_str = db_path.to_str().unwrap().to_string();

    let base_url = spawn(&db_path_str).await;
    let client = reqwest::Client::builder().cookie_store(true).build().expect("build client");

    let resp = client
        .post(format!("{base_url}/api/v1/auth/register"))
        .json(&json!({ "email": "admin@example.com", "password": "hunter2" }))
        .send()
        .await
        .expect("register");
    let account: serde_json::Value = resp.json().await.unwrap();
    let account_id = account["id"].as_i64().unwrap();

    // Demoting the sole admin is refused...
    let resp = client
        .put(format!("{base_url}/api/v1/admin/accounts/{account_id}/role"))
        .json(&json!({ "role": "user" }))
        .send()
        .await
        .expect("attempt role change");
    assert_eq!(resp.status(), 409);

    // ...and so is deleting them.
    let resp = client
        .delete(format!("{base_url}/api/v1/admin/accounts/{account_id}"))
        .send()
        .await
        .expect("attempt delete");
    assert_eq!(resp.status(), 409);

    // With a second admin present, demoting the first is now fine.
    let resp = client
        .post(format!("{base_url}/api/v1/admin/accounts"))
        .json(&json!({ "email": "second-admin@example.com", "password": "swordfish", "role": "admin" }))
        .send()
        .await
        .expect("create second admin");
    assert_eq!(resp.status(), 201);

    let resp = client
        .put(format!("{base_url}/api/v1/admin/accounts/{account_id}/role"))
        .json(&json!({ "role": "user" }))
        .send()
        .await
        .expect("role change with a second admin present");
    assert_eq!(resp.status(), 204);

    let _ = std::fs::remove_file(&db_path);
}
