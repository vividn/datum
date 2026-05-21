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

        CREATE TABLE IF NOT EXISTS refresh_tokens (
            token_hash TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
    ",
    )
    .context("init_db")?;

    migrate_to_clusters(conn).context("migrate_to_clusters")?;
    Ok(())
}

/// da-og24 (sub-2 of multi-DB epic da-u6lm): introduce the `clusters`
/// concept so a single user can hold multiple logical sync clusters
/// (multi-DB). Idempotent; safe to call on a fresh or populated relay.
///
/// Backfill rule (transitional): every existing user gets a default
/// cluster with `cluster_id = user_id`, `generation = 0`. Existing
/// wrapped-key and changeset rows are pinned to that default. Sub-3+
/// surfaces real cluster creation; until then every server-side path
/// silently maps `cluster_id := user_id`.
fn migrate_to_clusters(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clusters (
            cluster_id TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL,
            generation INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .context("create clusters")?;

    // ALTER ADD COLUMN is idempotent only via the `IF NOT EXISTS` shape
    // (SQLite 3.35+) — older builds error. PRAGMA table_info is the
    // portable way to gate the migration.
    if !column_exists(conn, "changesets", "cluster_id")? {
        conn.execute("ALTER TABLE changesets ADD COLUMN cluster_id TEXT", [])
            .context("add changesets.cluster_id")?;
        // Backfill from devices: each changeset's cluster is the
        // changeset device's owning user (transitional 1:1).
        conn.execute(
            "UPDATE changesets
             SET cluster_id = (
                 SELECT user_id FROM devices WHERE devices.id = changesets.device_id
             )
             WHERE cluster_id IS NULL",
            [],
        )
        .context("backfill changesets.cluster_id")?;
    }
    if !column_exists(conn, "changesets", "generation")? {
        conn.execute(
            "ALTER TABLE changesets ADD COLUMN generation INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("add changesets.generation")?;
    }

    // `wrapped_keys` keys on user_id today; multi-DB means (user_id,
    // cluster_id) pair. SQLite doesn't let us rewrite a primary key in
    // place — recreate the table, copy rows with cluster_id = user_id,
    // swap names. Only runs once because the post-migration table
    // already carries cluster_id.
    if !column_exists(conn, "wrapped_keys", "cluster_id")? {
        conn.execute_batch(
            "CREATE TABLE wrapped_keys_new (
                user_id    TEXT NOT NULL,
                cluster_id TEXT NOT NULL,
                blob       TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (user_id, cluster_id)
            );
            INSERT INTO wrapped_keys_new (user_id, cluster_id, blob, updated_at)
                SELECT user_id, user_id, blob, updated_at FROM wrapped_keys;
            DROP TABLE wrapped_keys;
            ALTER TABLE wrapped_keys_new RENAME TO wrapped_keys;",
        )
        .context("recreate wrapped_keys with composite key")?;
    }

    // Default-cluster backfill. Runs every init — `INSERT OR IGNORE`
    // means already-seeded users stay untouched, but a user inserted
    // before this migration ran still gets their default cluster row
    // the first time the server boots after the upgrade.
    conn.execute(
        "INSERT OR IGNORE INTO clusters (cluster_id, user_id, generation)
         SELECT id, id, 0 FROM users",
        [],
    )
    .context("backfill default clusters")?;

    Ok(())
}

/// PRAGMA-based column lookup. Lets the migration stay idempotent
/// against any SQLite version (no reliance on `ALTER TABLE ... IF NOT
/// EXISTS COLUMN`, which is 3.35+).
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // `PRAGMA table_info` doesn't take bind parameters; the table name
    // is interpolated. Caller passes hard-coded literals so SQL
    // injection isn't a concern here.
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Persist a refresh token by its SHA-256 hash. Storing the hash means
/// a server-DB read does NOT yield bearer-equivalent material.
/// `expires_at` is a unix timestamp in seconds.
pub fn insert_refresh_token(
    conn: &Connection,
    token_hash: &str,
    user_id: &Uuid,
    expires_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO refresh_tokens (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
        params![token_hash, user_id.to_string(), expires_at],
    )
    .context("insert_refresh_token")?;
    Ok(())
}

/// Look up a refresh token by hash. Returns `(user_id, expires_at)` or
/// `None` if the token has been rotated out (or never existed).
pub fn lookup_refresh_token(conn: &Connection, token_hash: &str) -> Result<Option<(Uuid, i64)>> {
    let mut stmt =
        conn.prepare("SELECT user_id, expires_at FROM refresh_tokens WHERE token_hash = ?1")?;
    let mut rows = stmt.query(params![token_hash])?;
    if let Some(row) = rows.next()? {
        let id_str: String = row.get(0)?;
        let expires_at: i64 = row.get(1)?;
        Ok(Some((Uuid::parse_str(&id_str)?, expires_at)))
    } else {
        Ok(None)
    }
}

/// Delete a refresh token by its hash. Used to enforce single-use
/// rotation on every successful refresh.
pub fn delete_refresh_token(conn: &Connection, token_hash: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM refresh_tokens WHERE token_hash = ?1",
        params![token_hash],
    )
    .context("delete_refresh_token")?;
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
    let mut stmt = conn.prepare("SELECT cred_data FROM credentials WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id.to_string()], |row| row.get::<_, String>(0))?;

    let mut passkeys = Vec::new();
    for row in rows {
        let cred_data = row?;
        let passkey: Passkey = serde_json::from_str(&cred_data)?;
        passkeys.push(passkey);
    }
    Ok(passkeys)
}

/// Insert a changeset for a device. Returns (global_id, per_device_seq).
/// Defaults `cluster_id` to the device's owning user and `generation`
/// to 0 — the transitional v1 shape from da-og24. Callers that have
/// resolved the cluster + generation explicitly use
/// [`insert_changeset_into_cluster`].
pub fn insert_changeset(conn: &Connection, device_id: &str, payload: &str) -> Result<(i64, i64)> {
    insert_changeset_into_cluster(conn, device_id, payload, None, 0)
}

/// da-tat1: cluster-aware insert. `cluster_id = None` falls back to the
/// transitional 1:1 mapping (`devices.user_id`) so v1 clients (no
/// cluster_id on the wire) keep working unchanged. `generation` is
/// stamped from the server's authoritative value at write time so a
/// later pull can confirm the row was accepted into the expected
/// generation.
pub fn insert_changeset_into_cluster(
    conn: &Connection,
    device_id: &str,
    payload: &str,
    cluster_id: Option<&str>,
    generation: i64,
) -> Result<(i64, i64)> {
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM changesets WHERE device_id = ?1",
            params![device_id],
            |row| row.get(0),
        )
        .context("compute next seq")?;

    // When the caller doesn't supply a cluster_id, fall back to the
    // device's owning user — the da-og24 transitional default. If
    // there's no device row (legacy / test fixtures bypassing
    // `ensure_device`), the SQL subquery returns NULL and the row
    // stays cluster-less; the wire contract still accepts that since
    // `cluster_id` is `Option<String>` on the protocol type.
    let resolved_cluster: Option<String> = match cluster_id {
        Some(c) => Some(c.to_string()),
        None => conn
            .query_row(
                "SELECT user_id FROM devices WHERE id = ?1",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .ok(),
    };

    conn.execute(
        "INSERT INTO changesets (device_id, seq, payload, cluster_id, generation)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![device_id, seq, payload, resolved_cluster, generation],
    )
    .context("insert_changeset")?;

    let id = conn.last_insert_rowid();
    Ok((id, seq))
}

/// One row from the changesets table as returned by [`pull_changesets`].
/// Tuple positions: `(global_id, device_id, seq, payload_b64, uploaded_at)`.
pub type ChangesetRow = (i64, String, i64, String, String);

/// Pull changesets with global id > since, from all devices except excluding_device.
/// Returns rows in ascending id order.
pub fn pull_changesets(
    conn: &Connection,
    since: i64,
    excluding_device: &str,
) -> Result<Vec<ChangesetRow>> {
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

/// da-tat1: cluster-scoped pull. Projects cluster_id + generation
/// alongside the base columns so the wire response can carry them.
/// Filters rows to the requested cluster — multi-DB isolation guarantee.
pub type ClusterChangesetRow = (i64, String, i64, String, String, String, i64);

pub fn pull_changesets_for_cluster(
    conn: &Connection,
    since: i64,
    excluding_device: &str,
    cluster_id: &str,
) -> Result<Vec<ClusterChangesetRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, seq, payload, uploaded_at, cluster_id, generation
         FROM changesets
         WHERE id > ?1 AND device_id != ?2 AND cluster_id = ?3
         ORDER BY id",
    )?;

    let rows = stmt.query_map(params![since, excluding_device, cluster_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
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
///
/// da-og24 (sub-2 of multi-DB epic): `wrapped_keys` now keys on
/// (user_id, cluster_id). This function targets the user's default
/// cluster (`cluster_id = user_id`) as the transitional shape; sub-3+
/// adds explicit per-cluster wrap APIs.
pub fn upsert_wrapped_key(conn: &Connection, user_id: &Uuid, blob: &str) -> Result<()> {
    let uid = user_id.to_string();
    conn.execute(
        "INSERT INTO wrapped_keys (user_id, cluster_id, blob, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(user_id, cluster_id)
         DO UPDATE SET blob = excluded.blob, updated_at = excluded.updated_at",
        params![uid, uid, blob],
    )
    .context("upsert_wrapped_key")?;
    Ok(())
}

/// Get the wrapped master key blob for a user, or None if unset.
///
/// da-og24: scoped to the user's default cluster. Sub-3+ adds a
/// cluster-aware variant.
pub fn get_wrapped_key(conn: &Connection, user_id: &Uuid) -> Result<Option<String>> {
    let uid = user_id.to_string();
    let mut stmt =
        conn.prepare("SELECT blob FROM wrapped_keys WHERE user_id = ?1 AND cluster_id = ?2")?;
    let mut rows = stmt.query(params![uid, uid])?;
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

    // ─── da-og24: clusters schema migration ─────────────────────────────────

    /// Brand-new init produces a `clusters` table and the cluster
    /// columns on the dependent tables. Pins the post-migration
    /// surface so future schema changes don't silently lose it.
    #[test]
    fn migration_creates_clusters_table_and_columns() {
        let conn = setup();
        assert!(column_exists(&conn, "clusters", "cluster_id").unwrap());
        assert!(column_exists(&conn, "clusters", "user_id").unwrap());
        assert!(column_exists(&conn, "clusters", "generation").unwrap());
        assert!(column_exists(&conn, "changesets", "cluster_id").unwrap());
        assert!(column_exists(&conn, "changesets", "generation").unwrap());
        assert!(column_exists(&conn, "wrapped_keys", "cluster_id").unwrap());
    }

    /// New inserts route through the migration: the changeset's
    /// cluster_id is derived from the device's owning user, and the
    /// generation defaults to 0. Locks the transitional 1:1 mapping.
    #[test]
    fn insert_changeset_populates_cluster_from_device_owner() {
        let conn = setup();
        let user_id = Uuid::new_v4();
        ensure_device(&conn, "device-1", &user_id).unwrap();
        insert_changeset(&conn, "device-1", "payload").unwrap();

        let (cluster_id, generation): (String, i64) = conn
            .query_row(
                "SELECT cluster_id, generation FROM changesets WHERE device_id = ?1",
                params!["device-1"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cluster_id, user_id.to_string());
        assert_eq!(generation, 0);
    }

    /// Wrapped-key writes target (user_id, cluster_id = user_id) as
    /// the transitional default. Round-trip continues to work through
    /// the unchanged public API.
    #[test]
    fn wrapped_key_round_trips_under_composite_key() {
        let conn = setup();
        let user_id = Uuid::new_v4();
        upsert_wrapped_key(&conn, &user_id, "{\"k\":\"v\"}").unwrap();
        assert_eq!(
            get_wrapped_key(&conn, &user_id).unwrap().as_deref(),
            Some("{\"k\":\"v\"}")
        );

        // Direct query confirms cluster_id = user_id was persisted.
        let cluster_id: String = conn
            .query_row(
                "SELECT cluster_id FROM wrapped_keys WHERE user_id = ?1",
                params![user_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cluster_id, user_id.to_string());
    }

    /// Migration must preserve rows in a v1-shaped DB and backfill
    /// cluster_id / generation. Simulates a relay upgraded in place:
    /// build the pre-migration schema by hand, populate it, then run
    /// `init_db` and assert every old row survives with the
    /// transitional cluster assignment.
    #[test]
    fn migration_preserves_existing_rows_and_backfills_clusters() {
        let conn = Connection::open_in_memory().unwrap();
        // Build the v1 schema exactly as it was pre-da-og24.
        conn.execute_batch(
            "CREATE TABLE users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE devices (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                label TEXT,
                registered_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE changesets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                payload TEXT NOT NULL,
                uploaded_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(device_id, seq)
            );
            CREATE TABLE wrapped_keys (
                user_id TEXT PRIMARY KEY,
                blob TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();

        let user_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO users (id, username) VALUES (?1, ?2)",
            params![user_id.to_string(), "alice"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO devices (id, user_id) VALUES (?1, ?2)",
            params!["dev-A", user_id.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO changesets (device_id, seq, payload) VALUES (?1, ?2, ?3)",
            params!["dev-A", 1_i64, "legacy-payload"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO wrapped_keys (user_id, blob) VALUES (?1, ?2)",
            params![user_id.to_string(), "legacy-blob"],
        )
        .unwrap();

        // The in-place upgrade.
        init_db(&conn).unwrap();

        // Legacy changeset row is preserved AND has its cluster_id /
        // generation populated from the device's owner.
        let (payload, cluster_id, generation): (String, String, i64) = conn
            .query_row(
                "SELECT payload, cluster_id, generation FROM changesets WHERE device_id = ?1",
                params!["dev-A"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(payload, "legacy-payload");
        assert_eq!(cluster_id, user_id.to_string());
        assert_eq!(generation, 0);

        // Legacy wrapped_key survives the table recreate; cluster_id
        // = user_id is the transitional default.
        let (blob, cluster_id): (String, String) = conn
            .query_row(
                "SELECT blob, cluster_id FROM wrapped_keys WHERE user_id = ?1",
                params![user_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(blob, "legacy-blob");
        assert_eq!(cluster_id, user_id.to_string());

        // The default cluster row got backfilled.
        let (gen_, owner): (i64, String) = conn
            .query_row(
                "SELECT generation, user_id FROM clusters WHERE cluster_id = ?1",
                params![user_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(gen_, 0);
        assert_eq!(owner, user_id.to_string());
    }

    /// Migration is idempotent — running init_db twice on a populated
    /// DB must not duplicate cluster rows or fail with "duplicate
    /// column".
    #[test]
    fn migration_is_idempotent() {
        let conn = setup();
        let user_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO users (id, username) VALUES (?1, ?2)",
            params![user_id.to_string(), "bob"],
        )
        .unwrap();

        init_db(&conn).unwrap();
        init_db(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clusters WHERE user_id = ?1",
                params![user_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "default cluster must not duplicate");
    }
}
