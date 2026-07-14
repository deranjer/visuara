//! Password hashing and ID/OTP generation. Session tokens are intentionally
//! opaque random strings, not JWTs — this server has no need for stateless
//! session verification at self-hosted single-instance scale.

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::distributions::Alphanumeric;
use rand::Rng;

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn generate_session_token() -> String {
    random_alnum(32)
}

/// An 8-digit, TeamViewer-style one-time password for attended access.
pub fn generate_one_time_password() -> String {
    let mut rng = rand::thread_rng();
    format!("{:04} {:04}", rng.gen_range(0..10000), rng.gen_range(0..10000))
}

/// A 9-digit device ID, grouped for readability.
pub fn generate_device_id() -> String {
    let mut rng = rand::thread_rng();
    format!(
        "{:03} {:03} {:03}",
        rng.gen_range(0..1000),
        rng.gen_range(0..1000),
        rng.gen_range(0..1000)
    )
}

fn random_alnum(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
