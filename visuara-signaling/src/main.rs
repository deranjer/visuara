use dashmap::{DashMap, DashSet};
use std::path::PathBuf;
use std::sync::Arc;

use visuara_signaling::db::Db;
use visuara_signaling::state::AppState;
use visuara_signaling::turn::TurnConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "visuara.db".to_string());
    let db = Db::open(&db_path)?;

    let turn_secret = std::env::var("TURN_SHARED_SECRET")
        .expect("TURN_SHARED_SECRET must be set (shared with coturn's static-auth-secret)");
    let turn_urls = std::env::var("TURN_URLS")
        .unwrap_or_else(|_| "turn:localhost:3478".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let admin_password = std::env::var("ADMIN_PASSWORD")
        .expect("ADMIN_PASSWORD must be set to protect the /admin settings UI");
    let client_templates_dir = std::env::var("CLIENT_TEMPLATES_DIR")
        .unwrap_or_else(|_| "client-templates".to_string());
    let fetched_templates_dir = std::env::var("FETCHED_TEMPLATES_DIR")
        .unwrap_or_else(|_| "fetched-client-templates".to_string());
    tokio::fs::create_dir_all(&fetched_templates_dir).await?;

    let state = AppState {
        db,
        turn: Arc::new(TurnConfig {
            urls: turn_urls,
            shared_secret: turn_secret,
        }),
        connections: Arc::new(DashMap::new()),
        device_online: Arc::new(DashMap::new()),
        otp: Arc::new(DashMap::new()),
        sessions: Arc::new(DashMap::new()),
        admin_password: Arc::new(admin_password),
        admin_sessions: Arc::new(DashSet::new()),
        user_sessions: Arc::new(DashMap::new()),
        client_templates_dir: Arc::new(PathBuf::from(client_templates_dir)),
        fetched_templates_dir: Arc::new(PathBuf::from(fetched_templates_dir)),
    };

    let app = visuara_signaling::build_router(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("visuara-signaling listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}
