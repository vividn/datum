use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub type Db = Arc<Mutex<Connection>>;

const SCHEMA: &str = r#"
PRAGMA journal_mode=WAL;

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS credentials (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    cred_data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    label TEXT NOT NULL DEFAULT '',
    registered_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS changesets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL REFERENCES devices(id),
    seq INTEGER NOT NULL,
    payload TEXT NOT NULL,
    uploaded_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(device_id, seq)
);
"#;

pub fn init_db(path: &str) -> anyhow::Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(Arc::new(Mutex::new(conn)))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub id: String,
    pub user_id: String,
    pub label: String,
    pub registered_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Changeset {
    pub device_id: String,
    pub seq: i64,
    pub payload: String,
}

// --- users ---

pub fn create_user(db: &Db, user_id: &str) -> anyhow::Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "INSERT OR IGNORE INTO users (id) VALUES (?1)",
        params![user_id],
    )?;
    Ok(())
}

pub fn user_exists(db: &Db, user_id: &str) -> anyhow::Result<bool> {
    let db = db.lock().unwrap();
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM users WHERE id = ?1",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

// --- credentials ---

pub fn store_credential(db: &Db, id: &str, user_id: &str, cred_data: &str) -> anyhow::Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "INSERT OR REPLACE INTO credentials (id, user_id, cred_data) VALUES (?1, ?2, ?3)",
        params![id, user_id, cred_data],
    )?;
    Ok(())
}

pub fn update_credential(db: &Db, id: &str, cred_data: &str) -> anyhow::Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "UPDATE credentials SET cred_data = ?2 WHERE id = ?1",
        params![id, cred_data],
    )?;
    Ok(())
}

pub fn get_credentials_for_user(db: &Db, user_id: &str) -> anyhow::Result<Vec<(String, String)>> {
    let db = db.lock().unwrap();
    let mut stmt =
        db.prepare("SELECT id, cred_data FROM credentials WHERE user_id = ?1")?;
    let creds = stmt
        .query_map(params![user_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(creds)
}

// --- devices ---

pub fn register_device(db: &Db, id: &str, user_id: &str, label: &str) -> anyhow::Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "INSERT OR IGNORE INTO devices (id, user_id, label) VALUES (?1, ?2, ?3)",
        params![id, user_id, label],
    )?;
    Ok(())
}

pub fn get_devices_for_user(db: &Db, user_id: &str) -> anyhow::Result<Vec<Device>> {
    let db = db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT id, user_id, label, registered_at FROM devices WHERE user_id = ?1",
    )?;
    let devices = stmt
        .query_map(params![user_id], |row| {
            Ok(Device {
                id: row.get(0)?,
                user_id: row.get(1)?,
                label: row.get(2)?,
                registered_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(devices)
}

pub fn device_belongs_to_user(db: &Db, device_id: &str, user_id: &str) -> anyhow::Result<bool> {
    let db = db.lock().unwrap();
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM devices WHERE id = ?1 AND user_id = ?2",
        params![device_id, user_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

// --- changesets ---

pub fn next_seq_for_device(db: &Db, device_id: &str) -> anyhow::Result<i64> {
    let db = db.lock().unwrap();
    let seq: i64 = db.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM changesets WHERE device_id = ?1",
        params![device_id],
        |row| row.get(0),
    )?;
    Ok(seq)
}

pub fn push_changeset(db: &Db, device_id: &str, seq: i64, payload: &str) -> anyhow::Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "INSERT INTO changesets (device_id, seq, payload) VALUES (?1, ?2, ?3)",
        params![device_id, seq, payload],
    )?;
    Ok(())
}

pub fn pull_changesets(
    db: &Db,
    user_id: &str,
    exclude_device: &str,
    since: i64,
) -> anyhow::Result<Vec<Changeset>> {
    let db = db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT c.device_id, c.seq, c.payload
         FROM changesets c
         JOIN devices d ON c.device_id = d.id
         WHERE d.user_id = ?1
           AND c.device_id != ?2
           AND c.seq > ?3
         ORDER BY c.id ASC",
    )?;
    let changesets = stmt
        .query_map(params![user_id, exclude_device, since], |row| {
            Ok(Changeset {
                device_id: row.get(0)?,
                seq: row.get(1)?,
                payload: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(changesets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Db {
        init_db(":memory:").expect("in-memory db")
    }

    #[test]
    fn test_changeset_roundtrip() {
        let db = setup();

        create_user(&db, "alice").unwrap();
        register_device(&db, "dev1", "alice", "laptop").unwrap();
        register_device(&db, "dev2", "alice", "phone").unwrap();

        let s1 = next_seq_for_device(&db, "dev1").unwrap();
        push_changeset(&db, "dev1", s1, "payload_1").unwrap();
        let s2 = next_seq_for_device(&db, "dev1").unwrap();
        push_changeset(&db, "dev1", s2, "payload_2").unwrap();
        let s3 = next_seq_for_device(&db, "dev1").unwrap();
        push_changeset(&db, "dev1", s3, "payload_3").unwrap();

        let result = pull_changesets(&db, "alice", "dev2", 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].payload, "payload_1");
        assert_eq!(result[1].payload, "payload_2");
        assert_eq!(result[2].payload, "payload_3");
        assert_eq!(result[2].seq, 3);
    }

    #[test]
    fn test_pull_since() {
        let db = setup();

        create_user(&db, "bob").unwrap();
        register_device(&db, "dA", "bob", "").unwrap();
        register_device(&db, "dB", "bob", "").unwrap();

        for i in 1..=5i64 {
            push_changeset(&db, "dA", i, &format!("p{}", i)).unwrap();
        }

        let result = pull_changesets(&db, "bob", "dB", 3).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].seq, 4);
        assert_eq!(result[1].seq, 5);
    }

    #[test]
    fn test_device_isolation() {
        let db = setup();

        create_user(&db, "user1").unwrap();
        create_user(&db, "user2").unwrap();
        register_device(&db, "d1u1", "user1", "").unwrap();
        register_device(&db, "d2u1", "user1", "").unwrap();
        register_device(&db, "d1u2", "user2", "").unwrap();

        push_changeset(&db, "d1u1", 1, "user1_payload").unwrap();

        // user2 should not see user1's changesets
        let result = pull_changesets(&db, "user2", "d1u2", 0).unwrap();
        assert_eq!(result.len(), 0);

        // user1's second device should see the changeset
        let result = pull_changesets(&db, "user1", "d2u1", 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].payload, "user1_payload");
    }

    #[test]
    fn test_integration_push_pull() {
        // Integration test: register 2 devices, push 3 changesets from device1,
        // pull from device2, verify all received
        let db = setup();

        create_user(&db, "carol").unwrap();
        register_device(&db, "carol-laptop", "carol", "laptop").unwrap();
        register_device(&db, "carol-phone", "carol", "phone").unwrap();

        // Push 3 changesets from laptop (encrypted blobs — server is content-blind)
        let payloads = ["enc_aGVsbG8=", "enc_d29ybGQ=", "enc_dGVzdA=="];
        for (i, payload) in payloads.iter().enumerate() {
            let seq = i as i64 + 1;
            push_changeset(&db, "carol-laptop", seq, payload).unwrap();
        }

        // Pull from phone — should receive all 3
        let received = pull_changesets(&db, "carol", "carol-phone", 0).unwrap();
        assert_eq!(received.len(), 3);
        for (i, cs) in received.iter().enumerate() {
            assert_eq!(cs.device_id, "carol-laptop");
            assert_eq!(cs.seq, i as i64 + 1);
            assert_eq!(cs.payload, payloads[i]);
        }
    }
}
