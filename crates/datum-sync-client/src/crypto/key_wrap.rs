//! Passphrase-based master-key wrap.
//!
//! The relay stores an Argon2id-wrapped copy of the 32-byte master key; a
//! user who knows their passphrase can unwrap it on any new device. The
//! on-the-wire encoding (`WrappedKey`) is locked by `docs/sync-protocol.md`
//! — field names, base64 variant, and defaults cannot change without a
//! version bump.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use datum_sync_protocol::crypto::constants::{
    ARGON2_DEFAULT_M_COST_KIB, ARGON2_DEFAULT_P_COST, ARGON2_DEFAULT_SALT_LEN,
    ARGON2_DEFAULT_T_COST,
};
use rand::RngCore;

use crate::crypto::CryptoError;

// `KdfParams` and `WrappedKey` wire types live in `datum-sync-protocol`
// so client and server can never drift; this module owns the actual
// crypto (Argon2id + AES-256-GCM) that operates on those types.
pub use datum_sync_protocol::crypto::constants::NONCE_LEN;
pub use datum_sync_protocol::messages::sync::{KdfParams, WrappedKey};

/// Recommended defaults: 64 MiB × 3 passes × p=1 with a fresh 16-byte salt.
/// Free function (rather than an inherent impl on `KdfParams`) because the
/// type lives in a no-`rand` crate; we can't add an `impl` block in client
/// crate that depends on `rand` without trait-extension noise.
pub fn default_kdf_params_with_fresh_salt() -> KdfParams {
    let mut salt = vec![0u8; ARGON2_DEFAULT_SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    KdfParams {
        m_cost: ARGON2_DEFAULT_M_COST_KIB,
        t_cost: ARGON2_DEFAULT_T_COST,
        p_cost: ARGON2_DEFAULT_P_COST,
        salt_b64: b64_encode(&salt),
    }
}

/// Wrap a 32-byte master key with a passphrase. The returned blob is safe to
/// hand to the relay — decryption requires the passphrase.
pub fn wrap_master_key(passphrase: &str, master_key: &[u8; 32]) -> Result<WrappedKey, CryptoError> {
    let params = default_kdf_params_with_fresh_salt();
    wrap_master_key_with_params(passphrase, master_key, params)
}

/// Wrap a 32-byte master key using caller-supplied KDF params. Used by
/// `rotate-passphrase` to re-wrap the same key with a fresh salt and by
/// tests that pin the salt for determinism.
pub fn wrap_master_key_with_params(
    passphrase: &str,
    master_key: &[u8; 32],
    params: KdfParams,
) -> Result<WrappedKey, CryptoError> {
    let kek = derive_kek(passphrase, &params)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&kek));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, master_key.as_slice())
        .map_err(|e| CryptoError(format!("wrap master key: {e}")))?;

    Ok(WrappedKey {
        wrapped_key_b64: b64_encode(&ct),
        kdf: "argon2id".to_string(),
        kdf_params: params,
        wrap_alg: "aes-256-gcm".to_string(),
        nonce_b64: b64_encode(&nonce_bytes),
    })
}

/// Unwrap a passphrase-wrapped master key. Returns a single [`CryptoError`]
/// on a wrong passphrase, tampered ciphertext, or malformed base64 — all
/// failure modes surface identically (no oracle).
pub fn unwrap_master_key(passphrase: &str, wrapped: &WrappedKey) -> Result<[u8; 32], CryptoError> {
    if wrapped.kdf != "argon2id" {
        return Err(CryptoError(format!("unsupported kdf: {}", wrapped.kdf)));
    }
    if wrapped.wrap_alg != "aes-256-gcm" {
        return Err(CryptoError(format!(
            "unsupported wrap_alg: {}",
            wrapped.wrap_alg
        )));
    }
    let kek = derive_kek(passphrase, &wrapped.kdf_params)?;
    let ct = b64_decode(&wrapped.wrapped_key_b64)
        .map_err(|e| CryptoError(format!("decode wrapped_key_b64: {e}")))?;
    let nonce_bytes = b64_decode(&wrapped.nonce_b64)
        .map_err(|e| CryptoError(format!("decode nonce_b64: {e}")))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(CryptoError(format!(
            "nonce must be {NONCE_LEN} bytes, got {}",
            nonce_bytes.len()
        )));
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&kek));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|e| CryptoError(format!("unwrap master key: {e}")))?;
    if pt.len() != 32 {
        return Err(CryptoError(format!(
            "unwrapped key must be 32 bytes, got {}",
            pt.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&pt);
    Ok(out)
}

/// SHA-256 fingerprint of a master key (lowercase hex). Useful for
/// `datum sync key show-fingerprint` so two devices can eyeball-verify they
/// hold the same key without revealing it.
pub fn key_fingerprint(master_key: &[u8; 32]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(master_key);
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn derive_kek(passphrase: &str, params: &KdfParams) -> Result<[u8; 32], CryptoError> {
    let salt =
        b64_decode(&params.salt_b64).map_err(|e| CryptoError(format!("decode salt_b64: {e}")))?;
    let argon_params = argon2::Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
        .map_err(|e| CryptoError(format!("argon2 params: {e}")))?;
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon_params,
    );
    let mut out = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), &salt, &mut out)
        .map_err(|e| CryptoError(format!("argon2 derive: {e}")))?;
    Ok(out)
}

fn b64_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}

fn b64_decode(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_unwrap_round_trip() {
        let master = [0x42u8; 32];
        let wrapped = wrap_master_key("correct horse battery staple", &master).unwrap();
        assert_eq!(wrapped.kdf, "argon2id");
        assert_eq!(wrapped.wrap_alg, "aes-256-gcm");
        let got = unwrap_master_key("correct horse battery staple", &wrapped).unwrap();
        assert_eq!(got, master);
    }

    #[test]
    fn wrap_uses_fresh_salt_and_nonce_each_call() {
        let master = [0x11u8; 32];
        let w1 = wrap_master_key("hunter2", &master).unwrap();
        let w2 = wrap_master_key("hunter2", &master).unwrap();
        assert_ne!(w1.kdf_params.salt_b64, w2.kdf_params.salt_b64);
        assert_ne!(w1.nonce_b64, w2.nonce_b64);
        assert_ne!(w1.wrapped_key_b64, w2.wrapped_key_b64);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let master = [0x77u8; 32];
        let wrapped = wrap_master_key("right", &master).unwrap();
        assert!(unwrap_master_key("wrong", &wrapped).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let master = [0x33u8; 32];
        let mut wrapped = wrap_master_key("pass", &master).unwrap();
        let mut ct = b64_decode(&wrapped.wrapped_key_b64).unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        wrapped.wrapped_key_b64 = b64_encode(&ct);
        assert!(unwrap_master_key("pass", &wrapped).is_err());
    }

    #[test]
    fn fingerprint_is_deterministic_and_distinguishes_keys() {
        let k = [0u8; 32];
        let fp1 = key_fingerprint(&k);
        let fp2 = key_fingerprint(&k);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64);
        let fp3 = key_fingerprint(&[1u8; 32]);
        assert_ne!(fp1, fp3);
    }
}
