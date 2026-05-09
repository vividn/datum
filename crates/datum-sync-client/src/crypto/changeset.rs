//! Changeset envelope + AES-256-GCM seal.
//!
//! A [`Changeset`] is an opaque byte buffer whose internal layout is the
//! wire-protocol responsibility of the caller — this module only cares
//! about sealing and unsealing it. Keeping the envelope type dumb lets
//! WASM builds reuse exactly the same seal/unseal without pulling in the
//! native sqlite layer that `ops.rs` (feature `db`) needs for pack/apply.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;

use crate::crypto::CryptoError;

/// 96-bit nonce — wire-locked by `datum-sync-protocol`. Re-exported here
/// so existing `crate::crypto::changeset::NONCE_LEN` references keep
/// working.
pub use datum_sync_protocol::crypto::constants::NONCE_LEN;

/// Opaque container holding the on-device changeset bytes. The wire format
/// (see `docs/sync-protocol.md`) is a 4-byte magic prefix + JSON payload;
/// writing that prefix is the responsibility of `ops::pack_changeset` (or a
/// future native implementation on a non-Rust client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    bytes: Vec<u8>,
}

impl Changeset {
    /// Wrap a raw byte buffer. Callers should only construct via this path
    /// when round-tripping through [`EncryptedBlob`] / wire; normal packing
    /// goes through `ops::pack_changeset`.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Opaque AES-256-GCM ciphertext: `nonce (12)` || `ciphertext+tag`.
///
/// The aes-gcm crate appends its 16-byte authentication tag to the
/// ciphertext, so the total length is `NONCE_LEN + plaintext.len() + 16`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBlob {
    bytes: Vec<u8>,
}

impl EncryptedBlob {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// AES-256-GCM seal. A fresh 12-byte nonce is generated each call and
/// prepended to the ciphertext. AAD binds the blob to its `(device_id, seq)`
/// coordinates so replay or misattribution surfaces as a decrypt failure.
pub fn encrypt_changeset(
    cs: &Changeset,
    key: &[u8; 32],
    device_id: &str,
    seq: u64,
) -> Result<EncryptedBlob, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let aad = build_aad(device_id, seq);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: cs.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|e| CryptoError(format!("encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(EncryptedBlob { bytes: out })
}

/// AES-256-GCM open. Returns [`CryptoError`] on any failure — wrong key,
/// tampered ciphertext, mismatched device_id/seq. Peers MUST treat every
/// failure mode identically (no oracle).
pub fn decrypt_changeset(
    blob: &EncryptedBlob,
    key: &[u8; 32],
    device_id: &str,
    seq: u64,
) -> Result<Changeset, CryptoError> {
    if blob.bytes.len() < NONCE_LEN {
        return Err(CryptoError("ciphertext shorter than nonce".into()));
    }
    let (nonce_bytes, ct) = blob.bytes.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let aad = build_aad(device_id, seq);
    let pt = cipher
        .decrypt(nonce, Payload { msg: ct, aad: &aad })
        .map_err(|e| CryptoError(format!("decrypt: {e}")))?;
    Ok(Changeset { bytes: pt })
}

fn build_aad(device_id: &str, seq: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(device_id.len() + 8);
    aad.extend_from_slice(device_id.as_bytes());
    aad.extend_from_slice(&seq.to_be_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cs() -> Changeset {
        Changeset::from_bytes(b"hello world".to_vec())
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = [0x11u8; 32];
        let cs = sample_cs();
        let blob = encrypt_changeset(&cs, &key, "dev-alpha", 42).unwrap();
        assert!(blob.as_bytes().len() > NONCE_LEN + 16);
        let got = decrypt_changeset(&blob, &key, "dev-alpha", 42).unwrap();
        assert_eq!(got, cs);
    }

    #[test]
    fn wrong_key_fails() {
        let cs = sample_cs();
        let blob = encrypt_changeset(&cs, &[0x11u8; 32], "dev-alpha", 1).unwrap();
        let err = decrypt_changeset(&blob, &[0x22u8; 32], "dev-alpha", 1).unwrap_err();
        assert!(err.0.starts_with("decrypt"));
    }

    #[test]
    fn wrong_aad_fails() {
        let cs = sample_cs();
        let key = [0x11u8; 32];
        let blob = encrypt_changeset(&cs, &key, "dev-alpha", 1).unwrap();
        assert!(decrypt_changeset(&blob, &key, "dev-beta", 1).is_err());
        assert!(decrypt_changeset(&blob, &key, "dev-alpha", 2).is_err());
        // Flipping a byte in the ciphertext also fails.
        let mut tampered = blob.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(
            decrypt_changeset(&EncryptedBlob::from_bytes(tampered), &key, "dev-alpha", 1,).is_err()
        );
    }
}
