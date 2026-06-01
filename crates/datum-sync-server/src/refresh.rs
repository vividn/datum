//! Refresh-token issuance and rotation.
//!
//! A refresh token is a 32-byte random value (URL-safe base64, no pad)
//! delivered to the client alongside every fresh JWT. The server stores
//! only the SHA-256 hash so a database leak does NOT yield reusable
//! credentials. Single-use rotation: every successful `/v0/auth/refresh`
//! deletes the consumed row before inserting the replacement.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{db, error::AppError};

/// Refresh tokens live ~60 days — comfortably longer than the 30-day JWT
/// TTL so a quiet device that comes back online can still rotate without
/// the user noticing. Numbers are not security-sensitive: the token is
/// invalidated on first use.
const REFRESH_TOKEN_TTL_SECS: i64 = 60 * 24 * 60 * 60;

/// Number of random bytes per refresh token. 32 bytes ≈ 256 bits of
/// entropy; URL-safe base64 yields 43 chars on the wire.
const REFRESH_TOKEN_BYTES: usize = 32;

/// SHA-256 the URL-safe-base64 token. Hex output keeps the column
/// human-greppable in dolt/sqlite without leaking entropy.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    hex
}

/// Mint a fresh refresh token, persist its hash for `user_id`, and
/// return the plaintext value to hand back to the client. Callers must
/// only ever transmit the plaintext over the wire (never the hash).
pub fn mint_and_persist(conn: &Connection, user_id: &Uuid) -> Result<String, AppError> {
    let mut bytes = [0u8; REFRESH_TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let plaintext = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&plaintext);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::Internal("system clock before unix epoch".to_string()))?
        .as_secs() as i64;
    let expires_at = now.saturating_add(REFRESH_TOKEN_TTL_SECS);
    db::insert_refresh_token(conn, &hash, user_id, expires_at)?;
    Ok(plaintext)
}
