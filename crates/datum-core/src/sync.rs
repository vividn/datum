//! Sync primitives — changeset pack/apply, AES-256-GCM encryption, and
//! per-DB push/pull watermarks. Pure library: no HTTP, no CLI.
//!
//! The wire protocol (see `docs/sync-protocol.md`) treats changesets as
//! opaque encrypted blobs: the relay never sees plaintext. This module is
//! the only place where changesets exist in the clear on-device.
//!
//! # Watermark model
//!
//! - `since_seq` in [`pack_changeset`] is a local watermark: the largest
//!   `modified_at` epoch-ms already included in a prior pack. Passing `0`
//!   produces a full dump of every live and tombstoned entry.
//! - [`advance_push_seq`] / [`advance_pull_seq`] record the latest seq the
//!   relay accepted on a push and the latest server-assigned global seq
//!   consumed on a pull.
//!
//! # Encryption
//!
//! AES-256-GCM with a 12-byte random nonce prepended to the ciphertext;
//! AAD = `device_id` bytes || `seq` as 8 big-endian bytes. Tampering with
//! any of those inputs fails [`decrypt_changeset`].

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use chrono::{DateTime, TimeZone, Utc};
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::entry::Entry;
use crate::error::{DatumError, Result};

/// Magic prefix identifying a v1 changeset payload. Bumping the trailing
/// digit is the versioning mechanism for the on-device changeset format.
const CHANGESET_MAGIC_V1: &[u8; 4] = b"DSC1";

/// 96-bit nonce — the aes-gcm crate's required size for `Aes256Gcm`.
const NONCE_LEN: usize = 12;

/// Default `db_id` when the caller doesn't distinguish identities. One row
/// per DB is the common case; callers can override by writing directly to
/// `sync_state` if they need multi-identity.
pub const DEFAULT_DB_ID: &str = "local";

/// Opaque container holding the on-device changeset bytes.
///
/// Layout: `CHANGESET_MAGIC_V1` (4 bytes) || JSON payload. Keeping the
/// format self-describing lets a later revision swap in cr-sqlite's native
/// binary without breaking old DBs — the magic tells the decoder which
/// branch to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    bytes: Vec<u8>,
}

impl Changeset {
    /// Wrap a raw byte buffer. Callers should only construct via this path
    /// when round-tripping through `EncryptedBlob` / wire; normal code uses
    /// [`pack_changeset`].
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

/// Internal v1 payload: a flat list of entries (live + tombstoned).
/// Tombstones carry `deleted_at = Some(..)`; peers apply them as soft
/// deletes.
#[derive(Serialize, Deserialize)]
struct ChangesetPayloadV1 {
    entries: Vec<Entry>,
}

// -----------------------------------------------------------------------------
// Pack / apply
// -----------------------------------------------------------------------------

/// Pack every entry (including tombstones) whose `modified_at` epoch-ms is
/// strictly greater than `since_seq`. Passing `0` produces a full dump.
///
/// The returned changeset is self-contained plaintext bytes — the caller
/// is expected to pipe it through [`encrypt_changeset`] before it leaves
/// the device.
pub fn pack_changeset(db: &Db, since_seq: u64) -> Result<Changeset> {
    let entries = select_entries_since(db, since_seq)?;
    let payload = ChangesetPayloadV1 { entries };
    let json = serde_json::to_vec(&payload)?;
    let mut bytes = Vec::with_capacity(CHANGESET_MAGIC_V1.len() + json.len());
    bytes.extend_from_slice(CHANGESET_MAGIC_V1);
    bytes.extend_from_slice(&json);
    Ok(Changeset { bytes })
}

/// Apply a received changeset to the local DB using last-writer-wins on
/// `modified_at`. Existing rows whose stored `modified_at` is >= the
/// incoming row's `modified_at` are left untouched.
pub fn apply_changeset(db: &Db, cs: &Changeset) -> Result<()> {
    let payload = parse_payload(cs.as_bytes())?;
    let tx = db.conn().unchecked_transaction()?;
    for entry in payload.entries.iter() {
        upsert_lww(&tx, entry)?;
    }
    tx.commit()?;
    Ok(())
}

fn parse_payload(bytes: &[u8]) -> Result<ChangesetPayloadV1> {
    if bytes.len() < CHANGESET_MAGIC_V1.len() {
        return Err(DatumError::Other("changeset too short".to_string()));
    }
    let (magic, rest) = bytes.split_at(CHANGESET_MAGIC_V1.len());
    if magic != CHANGESET_MAGIC_V1 {
        return Err(DatumError::Other(format!(
            "unknown changeset magic: {:02x?}",
            magic
        )));
    }
    let payload: ChangesetPayloadV1 = serde_json::from_slice(rest)?;
    Ok(payload)
}

fn select_entries_since(db: &Db, since_ms: u64) -> Result<Vec<Entry>> {
    // Fetch candidate rows and filter by epoch-ms in Rust — the stored
    // `modify_time` is RFC3339, so a pure-SQL comparison would require a
    // cast-and-parse dance that's more fragile than a one-pass filter.
    // Use `SELECT *` so the generated `verb` / `dur_seconds` columns are
    // included without listing them explicitly.
    let mut stmt = db.conn().prepare(
        "SELECT * FROM entries ORDER BY modify_time",
    )?;
    let rows = stmt.query_map([], crate::db::row_to_entry_public)?;
    let mut out = Vec::new();
    for row in rows {
        let entry = row?;
        if entry.modify_time.timestamp_millis() as u64 > since_ms {
            out.push(entry);
        }
    }
    Ok(out)
}

fn upsert_lww(tx: &rusqlite::Transaction<'_>, entry: &Entry) -> Result<()> {
    // Skip rows we already have as-new-or-newer.
    let existing: Option<String> = tx
        .query_row(
            "SELECT modify_time FROM entries WHERE slug = ?1",
            params![entry.slug],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing_str) = existing {
        let existing_at: DateTime<Utc> = existing_str
            .parse()
            .map_err(|e: chrono::ParseError| DatumError::Other(e.to_string()))?;
        if existing_at >= entry.modify_time {
            return Ok(());
        }
    }

    tx.execute(
        r#"INSERT INTO entries (
            hid, slug, content_hash, field, entry_shape, intent,
            value, data, unit,
            start_time, start_time_offset, start_time_tz,
            start_time_precision, start_time_precision_unit,
            end_time, end_time_offset, end_time_tz,
            end_time_precision, end_time_precision_unit,
            lat, lon, location,
            create_time, modify_time, device_id,
            deleted_at, id_structure, field_structure,
            raw_input, alias_chain, parser_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9,
            ?10, ?11, ?12,
            ?13, ?14,
            ?15, ?16, ?17,
            ?18, ?19,
            ?20, ?21, ?22,
            ?23, ?24, ?25,
            ?26, ?27, ?28,
            ?29, ?30, ?31
        )
        ON CONFLICT(slug) DO UPDATE SET
            hid             = excluded.hid,
            content_hash    = excluded.content_hash,
            field           = excluded.field,
            entry_shape     = excluded.entry_shape,
            intent          = excluded.intent,
            value           = excluded.value,
            data            = excluded.data,
            unit            = excluded.unit,
            start_time      = excluded.start_time,
            start_time_offset = excluded.start_time_offset,
            start_time_tz   = excluded.start_time_tz,
            start_time_precision = excluded.start_time_precision,
            start_time_precision_unit = excluded.start_time_precision_unit,
            end_time        = excluded.end_time,
            end_time_offset = excluded.end_time_offset,
            end_time_tz     = excluded.end_time_tz,
            end_time_precision = excluded.end_time_precision,
            end_time_precision_unit = excluded.end_time_precision_unit,
            lat             = excluded.lat,
            lon             = excluded.lon,
            location        = excluded.location,
            create_time     = excluded.create_time,
            modify_time     = excluded.modify_time,
            device_id       = excluded.device_id,
            deleted_at      = excluded.deleted_at,
            id_structure    = excluded.id_structure,
            field_structure = excluded.field_structure,
            raw_input       = excluded.raw_input,
            alias_chain     = excluded.alias_chain,
            parser_version  = excluded.parser_version"#,
        params![
            entry.hid,
            entry.slug,
            entry.content_hash,
            entry.field,
            entry.entry_shape.to_string(),
            entry.intent,
            entry.value.as_ref().map(|v| v.to_string()),
            entry.data.as_ref().map(|v| v.to_string()),
            entry.unit,
            entry.start_time.as_ref().map(|t| t.utc.to_rfc3339()),
            entry.start_time.as_ref().map(|t| t.offset_minutes),
            entry.start_time.as_ref().map(|t| t.timezone.as_str()),
            entry.start_time_precision,
            entry.start_time_precision_unit.to_string(),
            entry.end_time.as_ref().map(|t| t.utc.to_rfc3339()),
            entry.end_time.as_ref().map(|t| t.offset_minutes),
            entry.end_time.as_ref().map(|t| t.timezone.as_str()),
            entry.end_time_precision,
            entry.end_time_precision_unit.to_string(),
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            entry.create_time.to_rfc3339(),
            entry.modify_time.to_rfc3339(),
            entry.device_id,
            entry.deleted_at.map(|t| t.to_rfc3339()),
            entry.id_structure.as_deref(),
            entry.field_structure.as_deref(),
            entry.raw_input,
            entry.alias_chain.as_ref().map(|v| v.to_string()),
            entry.parser_version,
        ],
    )?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Encryption
// -----------------------------------------------------------------------------

/// AES-256-GCM seal. A fresh 12-byte nonce is generated each call and
/// prepended to the ciphertext. AAD binds the blob to its `(device_id, seq)`
/// coordinates so replay or misattribution surfaces as a decrypt failure.
pub fn encrypt_changeset(
    cs: &Changeset,
    key: &[u8; 32],
    device_id: &str,
    seq: u64,
) -> Result<EncryptedBlob> {
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
        .map_err(|e| DatumError::Crypto(format!("encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(EncryptedBlob { bytes: out })
}

/// AES-256-GCM open. Returns `Crypto` on any failure — wrong key, tampered
/// ciphertext, mismatched device_id/seq. Peers should treat every failure
/// mode identically (no oracle).
pub fn decrypt_changeset(
    blob: &EncryptedBlob,
    key: &[u8; 32],
    device_id: &str,
    seq: u64,
) -> Result<Changeset> {
    if blob.bytes.len() < NONCE_LEN {
        return Err(DatumError::Crypto("ciphertext shorter than nonce".into()));
    }
    let (nonce_bytes, ct) = blob.bytes.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let aad = build_aad(device_id, seq);
    let pt = cipher
        .decrypt(
            nonce,
            Payload {
                msg: ct,
                aad: &aad,
            },
        )
        .map_err(|e| DatumError::Crypto(format!("decrypt: {e}")))?;
    Ok(Changeset { bytes: pt })
}

fn build_aad(device_id: &str, seq: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(device_id.len() + 8);
    aad.extend_from_slice(device_id.as_bytes());
    aad.extend_from_slice(&seq.to_be_bytes());
    aad
}

// -----------------------------------------------------------------------------
// Key wrapping (passphrase → master key)
// -----------------------------------------------------------------------------

/// Parameters for the Argon2id KDF used to derive a key-encryption key from a
/// user passphrase. The wire protocol (`docs/sync-protocol.md`) locks the
/// field names and encoding — do not rename without bumping the version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB (typical: 65536 = 64 MiB).
    pub m_cost: u32,
    /// Time cost (passes). Typical: 3.
    pub t_cost: u32,
    /// Parallelism. Typical: 1.
    pub p_cost: u32,
    /// Base64 (url-safe, no-pad) encoded salt bytes (16 bytes recommended).
    pub salt_b64: String,
}

impl KdfParams {
    /// Recommended defaults: 64 MiB × 3 passes × p=1 with a fresh 16-byte salt.
    pub fn default_with_fresh_salt() -> Self {
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        Self {
            m_cost: 65536,
            t_cost: 3,
            p_cost: 1,
            salt_b64: b64_encode(&salt),
        }
    }
}

/// The passphrase-wrapped master key, ready to send on the wire.
///
/// Round-trips through `/v1/keys/wrapped` (see `docs/sync-protocol.md`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrappedKey {
    /// Ciphertext of the master key, base64 url-safe no-pad. Includes the
    /// 16-byte GCM authenticator.
    pub wrapped_key_b64: String,
    /// Always `"argon2id"` in v1.
    pub kdf: String,
    pub kdf_params: KdfParams,
    /// Always `"aes-256-gcm"` in v1.
    pub wrap_alg: String,
    /// Base64 url-safe no-pad 12-byte nonce.
    pub nonce_b64: String,
}

/// Wrap a 32-byte master key with a passphrase. The returned blob is safe to
/// hand to the relay — decryption requires the passphrase.
pub fn wrap_master_key(passphrase: &str, master_key: &[u8; 32]) -> Result<WrappedKey> {
    let params = KdfParams::default_with_fresh_salt();
    wrap_master_key_with_params(passphrase, master_key, params)
}

/// Wrap a 32-byte master key using caller-supplied KDF params. Used by
/// `rotate-passphrase` to re-wrap the same key with a fresh salt and by
/// tests that pin the salt for determinism.
pub fn wrap_master_key_with_params(
    passphrase: &str,
    master_key: &[u8; 32],
    params: KdfParams,
) -> Result<WrappedKey> {
    if params.kdf_name() != "argon2id" {
        // params has no `kdf` field; the enum is implicit in KdfParams. This
        // branch is a sentinel for future parameter shapes.
    }
    let kek = derive_kek(passphrase, &params)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&kek));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, master_key.as_slice())
        .map_err(|e| DatumError::Crypto(format!("wrap master key: {e}")))?;

    Ok(WrappedKey {
        wrapped_key_b64: b64_encode(&ct),
        kdf: "argon2id".to_string(),
        kdf_params: params,
        wrap_alg: "aes-256-gcm".to_string(),
        nonce_b64: b64_encode(&nonce_bytes),
    })
}

/// Unwrap a passphrase-wrapped master key. Returns `DatumError::Crypto` on
/// a wrong passphrase, tampered ciphertext, or malformed base64 — all failure
/// modes surface identically (no oracle).
pub fn unwrap_master_key(passphrase: &str, wrapped: &WrappedKey) -> Result<[u8; 32]> {
    if wrapped.kdf != "argon2id" {
        return Err(DatumError::Crypto(format!(
            "unsupported kdf: {}",
            wrapped.kdf
        )));
    }
    if wrapped.wrap_alg != "aes-256-gcm" {
        return Err(DatumError::Crypto(format!(
            "unsupported wrap_alg: {}",
            wrapped.wrap_alg
        )));
    }
    let kek = derive_kek(passphrase, &wrapped.kdf_params)?;
    let ct = b64_decode(&wrapped.wrapped_key_b64)
        .map_err(|e| DatumError::Crypto(format!("decode wrapped_key_b64: {e}")))?;
    let nonce_bytes = b64_decode(&wrapped.nonce_b64)
        .map_err(|e| DatumError::Crypto(format!("decode nonce_b64: {e}")))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(DatumError::Crypto(format!(
            "nonce must be {NONCE_LEN} bytes, got {}",
            nonce_bytes.len()
        )));
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&kek));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|e| DatumError::Crypto(format!("unwrap master key: {e}")))?;
    if pt.len() != 32 {
        return Err(DatumError::Crypto(format!(
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

fn derive_kek(passphrase: &str, params: &KdfParams) -> Result<[u8; 32]> {
    let salt = b64_decode(&params.salt_b64)
        .map_err(|e| DatumError::Crypto(format!("decode salt_b64: {e}")))?;
    let argon_params = argon2::Params::new(
        params.m_cost,
        params.t_cost,
        params.p_cost,
        Some(32),
    )
    .map_err(|e| DatumError::Crypto(format!("argon2 params: {e}")))?;
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon_params,
    );
    let mut out = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), &salt, &mut out)
        .map_err(|e| DatumError::Crypto(format!("argon2 derive: {e}")))?;
    Ok(out)
}

impl KdfParams {
    fn kdf_name(&self) -> &'static str {
        "argon2id"
    }
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

// -----------------------------------------------------------------------------
// Sync-state watermarks
// -----------------------------------------------------------------------------

/// Read the local `push_seq` watermark. Returns 0 if no row exists yet
/// (fresh DB, never synced).
pub fn get_push_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "push_seq")
}

/// Read the local `pull_seq` watermark.
pub fn get_pull_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "pull_seq")
}

/// Advance `push_seq` and stamp `last_push_at` (epoch-ms). A rewind to a
/// lower value is rejected — `push_seq` is monotonic.
pub fn advance_push_seq(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "push_seq", "last_push_at", new_seq)
}

/// Advance `pull_seq` and stamp `last_pull_at` (epoch-ms).
pub fn advance_pull_seq(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "pull_seq", "last_pull_at", new_seq)
}

fn read_seq_column(db: &Db, col: &str) -> Result<u64> {
    let sql = format!(
        "SELECT {col} FROM sync_state WHERE db_id = ?1",
    );
    let got: Option<i64> = db
        .conn()
        .query_row(&sql, params![DEFAULT_DB_ID], |row| row.get(0))
        .optional()?;
    Ok(got.unwrap_or(0) as u64)
}

fn advance_seq(db: &Db, seq_col: &str, at_col: &str, new_seq: u64) -> Result<()> {
    let current = read_seq_column(db, seq_col)?;
    if new_seq < current {
        return Err(DatumError::Other(format!(
            "sync_state.{seq_col} rewind rejected: stored {current}, asked for {new_seq}"
        )));
    }
    let now_ms = Utc::now().timestamp_millis();
    let sql = format!(
        "INSERT INTO sync_state (db_id, {seq_col}, {at_col})
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            {seq_col} = excluded.{seq_col},
            {at_col}  = excluded.{at_col}",
    );
    db.conn()
        .execute(&sql, params![DEFAULT_DB_ID, new_seq as i64, now_ms])?;
    Ok(())
}

/// Read the `last_push_at` stamp (epoch-ms), if any push has been recorded.
pub fn get_last_push_at(db: &Db) -> Result<Option<DateTime<Utc>>> {
    read_at_column(db, "last_push_at")
}

/// Read the `last_pull_at` stamp (epoch-ms), if any pull has been recorded.
pub fn get_last_pull_at(db: &Db) -> Result<Option<DateTime<Utc>>> {
    read_at_column(db, "last_pull_at")
}

fn read_at_column(db: &Db, col: &str) -> Result<Option<DateTime<Utc>>> {
    let sql = format!(
        "SELECT {col} FROM sync_state WHERE db_id = ?1",
    );
    let got: Option<Option<i64>> = db
        .conn()
        .query_row(&sql, params![DEFAULT_DB_ID], |row| row.get(0))
        .optional()?;
    match got.flatten() {
        Some(ms) => Ok(Some(Utc.timestamp_millis_opt(ms).single().ok_or_else(
            || DatumError::Other(format!("sync_state.{col} out of range: {ms}")),
        )?)),
        None => Ok(None),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, Verb};
    use crate::time::DatumTime;
    use chrono::{Duration, TimeZone, Utc};

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn make_entry(field: &str, offset_secs: i64) -> Entry {
        let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
        let at = base + Duration::seconds(offset_secs);
        let t = DatumTime {
            utc: at,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let mut e = Entry::new(
            field,
            Verb::Occur,
            Some(serde_json::json!(1)),
            None,
            None,
            Some(t),
            None,
            "dev-1",
        );
        // Pin created_at/modified_at so the since_seq filter is deterministic.
        e.create_time = at;
        e.modify_time = at;
        e
    }

    #[test]
    fn changeset_pack_unpack_round_trip() {
        let db1 = fresh_db();
        let db2 = fresh_db();
        db1.insert_entry(&make_entry("coffee", 0)).unwrap();
        db1.insert_entry(&make_entry("mood", 60)).unwrap();

        let cs = pack_changeset(&db1, 0).unwrap();
        apply_changeset(&db2, &cs).unwrap();

        let count: i64 = db2
            .conn()
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // Packing from db2 produces an equivalent set.
        let cs2 = pack_changeset(&db2, 0).unwrap();
        let p1 = parse_payload(cs.as_bytes()).unwrap();
        let p2 = parse_payload(cs2.as_bytes()).unwrap();
        assert_eq!(p1.entries.len(), p2.entries.len());
    }

    #[test]
    fn pack_changeset_since_seq_returns_only_new_rows() {
        let db = fresh_db();
        let e1 = make_entry("coffee", 0);
        let e2 = make_entry("mood", 120);

        db.insert_entry(&e1).unwrap();
        let watermark_ms = e1.modify_time.timestamp_millis() as u64;

        db.insert_entry(&e2).unwrap();
        let cs = pack_changeset(&db, watermark_ms).unwrap();
        let payload = parse_payload(cs.as_bytes()).unwrap();
        assert_eq!(payload.entries.len(), 1);
        assert_eq!(payload.entries[0].field, "mood");
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let db = fresh_db();
        db.insert_entry(&make_entry("coffee", 0)).unwrap();
        let cs = pack_changeset(&db, 0).unwrap();

        let key = [0x11u8; 32];
        let blob = encrypt_changeset(&cs, &key, "dev-alpha", 42).unwrap();
        assert!(blob.as_bytes().len() > NONCE_LEN + 16);

        let got = decrypt_changeset(&blob, &key, "dev-alpha", 42).unwrap();
        assert_eq!(got, cs);
    }

    #[test]
    fn encrypt_with_wrong_key_fails() {
        let db = fresh_db();
        db.insert_entry(&make_entry("coffee", 0)).unwrap();
        let cs = pack_changeset(&db, 0).unwrap();

        let key = [0x11u8; 32];
        let wrong_key = [0x22u8; 32];
        let blob = encrypt_changeset(&cs, &key, "dev-alpha", 1).unwrap();
        let err = decrypt_changeset(&blob, &wrong_key, "dev-alpha", 1).unwrap_err();
        assert!(matches!(err, DatumError::Crypto(_)));
    }

    #[test]
    fn encrypt_with_wrong_aad_fails() {
        let db = fresh_db();
        db.insert_entry(&make_entry("coffee", 0)).unwrap();
        let cs = pack_changeset(&db, 0).unwrap();
        let key = [0x11u8; 32];
        let blob = encrypt_changeset(&cs, &key, "dev-alpha", 1).unwrap();

        // Tampering with device_id in the AAD surfaces as a decrypt error.
        assert!(matches!(
            decrypt_changeset(&blob, &key, "dev-beta", 1).unwrap_err(),
            DatumError::Crypto(_)
        ));
        // Tampering with seq surfaces as a decrypt error.
        assert!(matches!(
            decrypt_changeset(&blob, &key, "dev-alpha", 2).unwrap_err(),
            DatumError::Crypto(_)
        ));
        // Bit-flipping ciphertext surfaces as a decrypt error.
        let mut tampered = blob.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(matches!(
            decrypt_changeset(&EncryptedBlob::from_bytes(tampered), &key, "dev-alpha", 1)
                .unwrap_err(),
            DatumError::Crypto(_)
        ));
    }

    #[test]
    fn seq_advances_correctly() {
        let db = fresh_db();
        assert_eq!(get_push_seq(&db).unwrap(), 0);
        assert_eq!(get_pull_seq(&db).unwrap(), 0);
        assert!(get_last_push_at(&db).unwrap().is_none());

        advance_push_seq(&db, 5).unwrap();
        assert_eq!(get_push_seq(&db).unwrap(), 5);
        assert!(get_last_push_at(&db).unwrap().is_some());

        advance_push_seq(&db, 12).unwrap();
        assert_eq!(get_push_seq(&db).unwrap(), 12);

        // Rewind is rejected — monotonic watermark.
        assert!(advance_push_seq(&db, 10).is_err());
        assert_eq!(get_push_seq(&db).unwrap(), 12);

        // push and pull are independent columns.
        advance_pull_seq(&db, 7).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 7);
        assert_eq!(get_push_seq(&db).unwrap(), 12);
    }

    #[test]
    fn wrap_unwrap_master_key_round_trip() {
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
    fn unwrap_with_wrong_passphrase_fails_as_crypto_error() {
        let master = [0x77u8; 32];
        let wrapped = wrap_master_key("right", &master).unwrap();
        let err = unwrap_master_key("wrong", &wrapped).unwrap_err();
        assert!(matches!(err, DatumError::Crypto(_)));
    }

    #[test]
    fn unwrap_rejects_tampered_ciphertext() {
        let master = [0x33u8; 32];
        let mut wrapped = wrap_master_key("pass", &master).unwrap();
        // Flip a byte in the ciphertext.
        let mut ct = b64_decode(&wrapped.wrapped_key_b64).unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        wrapped.wrapped_key_b64 = b64_encode(&ct);
        let err = unwrap_master_key("pass", &wrapped).unwrap_err();
        assert!(matches!(err, DatumError::Crypto(_)));
    }

    #[test]
    fn key_fingerprint_is_deterministic() {
        let k = [0u8; 32];
        let fp1 = key_fingerprint(&k);
        let fp2 = key_fingerprint(&k);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64); // SHA-256 hex is 64 chars
        // Different key → different fingerprint.
        let fp3 = key_fingerprint(&[1u8; 32]);
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn apply_changeset_lww_skips_older_incoming() {
        let db_a = fresh_db();
        let db_b = fresh_db();

        // Two different bodies for the same slug; `newer` has later modified_at.
        let mut older = make_entry("coffee", 0);
        let mut newer = make_entry("coffee", 0);
        newer.modify_time = older.modify_time + Duration::seconds(60);
        newer.content_hash = "deadbeef".to_string();
        older.content_hash = "cafebabe".to_string();

        db_a.insert_entry(&newer).unwrap();

        // db_b receives the older one first, then the newer, then the older
        // again — the older second arrival must not clobber the newer state.
        let older_cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&older).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        let newer_cs = pack_changeset(&db_a, 0).unwrap();

        apply_changeset(&db_b, &older_cs).unwrap();
        apply_changeset(&db_b, &newer_cs).unwrap();
        apply_changeset(&db_b, &older_cs).unwrap();

        let hash: String = db_b
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE slug = ?1",
                params![newer.slug],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hash, "deadbeef");
    }
}
