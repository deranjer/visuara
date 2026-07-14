//! Mints short-lived TURN credentials using coturn's time-limited REST API
//! convention: username is "<expiry-unix-ts>:<label>", password is
//! base64(HMAC-SHA1(shared_secret, username)). coturn is configured with the
//! same shared secret via `static-auth-secret` so it can verify these
//! credentials itself, with no shared user database needed.

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

pub struct TurnConfig {
    pub urls: Vec<String>,
    pub shared_secret: String,
}

pub fn mint_credentials(cfg: &TurnConfig, label: &str, ttl_secs: u32) -> (String, String) {
    let expiry = now_secs() + ttl_secs as u64;
    let username = format!("{expiry}:{label}");
    let mut mac =
        HmacSha1::new_from_slice(cfg.shared_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(username.as_bytes());
    let password = STANDARD.encode(mac.finalize().into_bytes());
    (username, password)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_secs()
}
