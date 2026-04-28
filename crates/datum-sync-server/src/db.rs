use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{params, Connection};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS credentials (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            cred_data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS devices (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            label TEXT,
            registered_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS changesets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            device_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            payload TEXT NOT NULL,
            uploaded_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(device_id, seq)
        );

        CREATE TABLE IF NOT EXISTS wrapped_keys (
            user_id TEXT PRIMARY KEY,
            blob TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
    ",
    )
    .context("init_db")?;
    Ok(())
}

pub fn create_user(conn: &Connection, username: &str) -> Result<Uuid> {
    let user_id = Uuid::new_v4();
    conn.execute(
        "INSERT INTO users (id, username) VALUES (?1, ?2)",
        params![user_id.to_string(), username],
    )
    .context("create_user")?;
    Ok(user_id)
}

pub fn get_user_by_username(conn: &Connection, username: &str) -> Result<Option<Uuid>> {
    let mut stmt = conn.prepare("SELECT id FROM users WHERE username = ?1")?;
    let mut rows = stmt.query(params![username])?;
    if let Some(row) = rows.next()? {
        let id_str: String = row.get(0)?;
        Ok(Some(Uuid::parse_str(&id_str)?))
    } else {
        Ok(None)
    }
}

pub fn save_credential(conn: &Connection, user_id: &Uuid, passkey: &Passkey) -> Result<()> {
    let cred_id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_slice());
    let cred_data = serde_json::to_string(passkey)?;
    conn.execute(
        "INSERT OR REPLACE INTO credentials (id, user_id, cred_data) VALUES (?1, ?2, ?3)",
        params![cred_id, user_id.to_string(), cred_data],
    )
    .context("save_credential")?;
    Ok(())
}

pub fn get_passkeys(conn: &Connection, user_id: &Uuid) -> Result<Vec<Passkey>> {
    let mut stmt =
        conn.prepare("SELECT cred_data FROM credentials WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id.to_string()], |row| {
        row.get::<_, String>(0)
    })?;

    let mut passkeys = Vec::new();
    for row in rows {
        let cred_data = row?;
        let passkey: Passkey = serde_json::from_str(&cred_data)?;
        passkeys.push(passkey);
    }
    Ok(passkeys)
}

/// Insert a changeset for a device. Returns (global_id, per_device_seq).
pub fn insert_changeset(conn: &Connection, device_id: &str, payload: &str) -> Result<(i64, i64)> {
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM changesets WHERE device_id = ?1",
            params![device_id],
            |row| row.get(0),
        )
        .context("compute next seq")?;

    conn.execute(
        "INSERT INTO changesets (device_id, seq, payload) VALUES (?1, ?2, ?3)",
        params![device_id, seq, payload],
    )
    .context("insert_changeset")?;

    let id = conn.last_insert_rowid();
    Ok((id, seq))
}

/// Pull changesets with global id > since, from all devices except excluding_device.
/// Returns (id, device_id, seq, payload_b64, uploaded_at) tuples in ascending id order.
pub fn pull_changesets(
    conn: &Connection,
    since: i64,
    excluding_device: &str,
) -> Result<Vec<(i64, String, i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, seq, payload, uploaded_at
         FROM changesets
         WHERE id > ?1 AND device_id != ?2
         ORDER BY id",
    )?;

    let rows = stmt.query_map(params![since, excluding_device], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// Upsert the wrapped master key blob for a user. Body is opaque JSON
/// (the relay is content-blind about the inner shape per spec §Wrapped key exchange).
pub fn upsert_wrapped_key(conn: &Connection, user_id: &Uuid, blob: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO wrapped_keys (user_id, blob, updated_at)
         VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(user_id) DO UPDATE SET blob = excluded.blob, updated_at = excluded.updated_at",
        params![user_id.to_string(), blob],
    )
    .context("upsert_wrapped_key")?;
    Ok(())
}

/// Get the wrapped master key blob for a user, or None if unset.
pub fn get_wrapped_key(conn: &Connection, user_id: &Uuid) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT blob FROM wrapped_keys WHERE user_id = ?1")?;
    let mut rows = stmt.query(params![user_id.to_string()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Ensure a device is registered for a user (auto-registers if not present).
pub fn ensure_device(conn: &Connection, device_id: &str, user_id: &Uuid) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO devices (id, user_id) VALUES (?1, ?2)",
        params![device_id, user_id.to_string()],
    )
    .context("ensure_device")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn test_insert_and_pull_changeset() {
        let conn = setup();
        let (id, seq) = insert_changeset(&conn, "device-1", "payload-1").unwrap();
        assert_eq!(seq, 1);
        assert!(id > 0);

        let rows = pull_changesets(&conn, 0, "device-2").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, id);
        assert_eq!(rows[0].1, "device-1");
        assert_eq!(rows[0].2, 1);
        assert_eq!(rows[0].3, "payload-1");
    }

    #[test]
    fn test_pull_excludes_own_device() {
        let conn = setup();
        insert_changeset(&conn, "device-1", "payload-1").unwrap();

        // Pulling as device-1 should return nothing
        let rows = pull_changesets(&conn, 0, "device-1").unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn test_multi_device_push_pull() {
        let conn = setup();

        // Push 3 from device-1
        for i in 1..=3 {
            insert_changeset(&conn, "device-1", &format!("payload-{}", i)).unwrap();
        }

        // Pull from device-2 — should get all 3
        let rows = pull_changesets(&conn, 0, "device-2").unwrap();
        assert_eq!(rows.len(), 3);

        // Check seq numbers are monotonic per device
        assert_eq!(rows[0].2, 1);
        assert_eq!(rows[1].2, 2);
        assert_eq!(rows[2].2, 3);
    }

    #[test]
    fn test_since_filter() {
        let conn = setup();
        let (id1, _) = insert_changeset(&conn, "device-1", "payload-1").unwrap();
        insert_changeset(&conn, "device-1", "payload-2").unwrap();
        insert_changeset(&conn, "device-1", "payload-3").unwrap();

        // since=id1 means we want rows with global id > id1
        let rows = pull_changesets(&conn, id1, "device-2").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].3, "payload-2");
        assert_eq!(rows[1].3, "payload-3");
    }

    #[test]
    fn test_wrapped_key_roundtrip() {
        let conn = setup();
        let user_id = Uuid::new_v4();
        // Need user row to exist? wrapped_keys table doesn't FK; skip user.
        assert!(get_wrapped_key(&conn, &user_id).unwrap().is_none());
        upsert_wrapped_key(&conn, &user_id, r#"{"wrapped_key_b64":"abc"}"#).unwrap();
        assert_eq!(
            get_wrapped_key(&conn, &user_id).unwrap().as_deref(),
            Some(r#"{"wrapped_key_b64":"abc"}"#)
        );
        // Upsert overwrites.
        upsert_wrapped_key(&conn, &user_id, r#"{"wrapped_key_b64":"xyz"}"#).unwrap();
        assert_eq!(
            get_wrapped_key(&conn, &user_id).unwrap().as_deref(),
            Some(r#"{"wrapped_key_b64":"xyz"}"#)
        );
    }

    #[test]
    fn test_per_device_seq_is_independent() {
        let conn = setup();
        // device-1 gets seq 1,2
        insert_changeset(&conn, "device-1", "a1").unwrap();
        insert_changeset(&conn, "device-1", "a2").unwrap();
        // device-2 gets seq 1,2 independently
        let (_, seq1) = insert_changeset(&conn, "device-2", "b1").unwrap();
        let (_, seq2) = insert_changeset(&conn, "device-2", "b2").unwrap();
        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
    }
}
