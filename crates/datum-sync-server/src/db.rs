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

        -- `uploaded_at` is RFC 3339 UTC (`...T...HH:MM:SSZ`) because it is
        -- surfaced on the wire in the Pull response (spec §Pull / §Portability:
        -- all wire timestamps are RFC 3339) and the client parses it with
        -- `parse_from_rfc3339`. SQLite's `datetime('now')` returns
        -- `YYYY-MM-DD HH:MM:SS` (space separator, no `Z`) which fails RFC 3339
        -- parsers — use `strftime('%Y-%m-%dT%H:%M:%SZ','now')` (da-grwx).
        -- `seq` is scoped per (device_id, cluster_id), not globally per
        -- device (da-hu5y): device_id is per-machine while the client's
        -- `device_seq` counter lives in each DB's sync_state, so two DBs
        -- (= two clusters, da-3fnd) on one machine count independently.
        -- A device-global seq would force a SeqMismatch re-base + re-
        -- encrypt on nearly every alternating-DB push. The AAD already
        -- binds cluster_id (da-3nkx), so per-cluster is the consistent
        -- model. cluster_id / generation are declared inline here for
        -- fresh DBs; `migrate_to_clusters` adds them to upgraded ones.
        CREATE TABLE IF NOT EXISTS changesets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            device_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            payload TEXT NOT NULL,
            uploaded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            cluster_id TEXT,
            generation INTEGER NOT NULL DEFAULT 0,
            UNIQUE(device_id, cluster_id, seq)
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
    // `generation_started_at` is RFC 3339 UTC (`...T...HH:MM:SSZ`) because
    // the value is surfaced on the wire in `SyncProtocolError::GenerationMismatch`
    // and the spec promises RFC 3339. SQLite's `datetime('now')` returns
    // `YYYY-MM-DD HH:MM:SS` (space separator, no Z) which fails RFC 3339
    // parsers — use `strftime('%Y-%m-%dT%H:%M:%SZ','now')` instead.
    // `created_at` is server-internal and may stay on the older format
    // until a separate cleanup; see follow-up bead.
    //
    // da-1m63: the PK is composite (cluster_id, user_id) — cluster names
    // are namespaced PER USER. Default cluster names come from DB
    // filenames client-side ('notes', 'journal', 'main' recur constantly);
    // a global namespace meant the first user to sync a name owned it
    // relay-wide and every later user 404'd forever. Per-user namespaces
    // also kill name-squatting.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clusters (
            cluster_id            TEXT NOT NULL,
            user_id               TEXT NOT NULL,
            generation            INTEGER NOT NULL DEFAULT 0,
            created_at            TEXT NOT NULL DEFAULT (datetime('now')),
            generation_started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
            PRIMARY KEY (cluster_id, user_id)
        );",
    )
    .context("create clusters")?;

    // Idempotent migration for existing relays — adds the
    // `generation_started_at` column tracking when the current generation
    // was first set (updates on every bump). Clients use this to bucket
    // local pre/post-reinit unsynced edits during recovery (da-74xx).
    if !column_exists(conn, "clusters", "generation_started_at")? {
        conn.execute(
            "ALTER TABLE clusters \
             ADD COLUMN generation_started_at TEXT NOT NULL \
             DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            [],
        )
        .context("add clusters.generation_started_at")?;
    }

    // da-1m63: relays created before the composite PK still carry
    // `cluster_id TEXT PRIMARY KEY` (global namespace). SQLite can't
    // rewrite a primary key in place — recreate and copy, same recipe as
    // the wrapped_keys migration below. Only runs once: post-migration,
    // user_id is part of the PK.
    if !is_pk_column(conn, "clusters", "user_id")? {
        conn.execute_batch(
            "CREATE TABLE clusters_new (
                cluster_id            TEXT NOT NULL,
                user_id               TEXT NOT NULL,
                generation            INTEGER NOT NULL DEFAULT 0,
                created_at            TEXT NOT NULL DEFAULT (datetime('now')),
                generation_started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                PRIMARY KEY (cluster_id, user_id)
            );
            INSERT INTO clusters_new
                (cluster_id, user_id, generation, created_at, generation_started_at)
                SELECT cluster_id, user_id, generation, created_at, generation_started_at
                FROM clusters;
            DROP TABLE clusters;
            ALTER TABLE clusters_new RENAME TO clusters;",
        )
        .context("recreate clusters with composite key")?;
    }

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

    // da-hu5y: seq scoping moves from per-device to per (device_id,
    // cluster_id) — see the schema comment in `init_db`. SQLite can't
    // rewrite a table-level UNIQUE in place, so upgraded relays whose
    // changesets table still carries UNIQUE(device_id, seq) get the
    // table recreated. Global `id`s are copied verbatim (pull
    // watermarks are id-based) and AUTOINCREMENT resumes past the max
    // copied id. Gated on the unique index shape so it runs once.
    if !changesets_unique_covers_cluster(conn)? {
        conn.execute_batch(
            "CREATE TABLE changesets_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                payload TEXT NOT NULL,
                uploaded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                cluster_id TEXT,
                generation INTEGER NOT NULL DEFAULT 0,
                UNIQUE(device_id, cluster_id, seq)
            );
            INSERT INTO changesets_new
                (id, device_id, seq, payload, uploaded_at, cluster_id, generation)
                SELECT id, device_id, seq, payload, uploaded_at, cluster_id, generation
                FROM changesets;
            DROP TABLE changesets;
            ALTER TABLE changesets_new RENAME TO changesets;",
        )
        .context("recreate changesets with per-cluster seq uniqueness")?;
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

/// Does any UNIQUE index on `changesets` include `cluster_id`? Detects
/// whether the da-hu5y per-cluster seq uniqueness is already in place
/// (the inline `UNIQUE(...)` materialises as a `sqlite_autoindex_*`
/// unique index), gating the one-time table recreate above.
fn changesets_unique_covers_cluster(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA index_list(changesets)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        let unique: i64 = row.get(2)?;
        if unique == 0 {
            continue;
        }
        let mut info = conn.prepare(&format!("PRAGMA index_info(\"{name}\")"))?;
        let mut cols = info.query([])?;
        while let Some(col) = cols.next()? {
            let col_name: String = col.get(2)?;
            if col_name == "cluster_id" {
                return Ok(true);
            }
        }
    }
    Ok(false)
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

/// True when `column` is part of `table`'s primary key. `PRAGMA
/// table_info`'s `pk` column is the 1-based position within the PK
/// (0 = not part of it) — this is how the da-1m63 migration detects a
/// pre-composite `clusters` table. Same literal-only caveat as
/// [`column_exists`].
fn is_pk_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            let pk: i64 = row.get(5)?;
            return Ok(pk > 0);
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

/// The `seq` the next push from `device_id` into `cluster_id` will be
/// assigned (`MAX(seq) + 1`, or 1 for a (device, cluster) pair that has
/// never pushed). The push handler reads this to validate the client's
/// `predicted_seq` *before* inserting, so a seq the client mispredicted
/// is rejected with [`SyncProtocolError::SeqMismatch`] instead of stored
/// with a corrupt AAD (da-ca4q). The whole handler holds the db lock, so
/// this peek and the subsequent insert see the same value.
///
/// Scoped per (device_id, cluster_id) — NOT device-global (da-hu5y):
/// the client counts `device_seq` per-DB (= per-cluster), so a global
/// scope would mismatch on every alternating-DB push from a multi-DB
/// machine. `cluster_id = None` matches only rows with NULL cluster_id
/// (legacy/test fixtures) via SQLite's NULL-aware `IS`.
pub fn peek_next_seq(conn: &Connection, device_id: &str, cluster_id: Option<&str>) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM changesets
         WHERE device_id = ?1 AND cluster_id IS ?2",
        params![device_id, cluster_id],
        |row| row.get(0),
    )
    .context("compute next seq")
}

/// Insert a changeset for a device. Returns (global_id, per_cluster_seq).
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

    // Resolve the cluster BEFORE peeking — the seq counter is scoped
    // per (device_id, cluster_id) (da-hu5y).
    let seq = peek_next_seq(conn, device_id, resolved_cluster.as_deref())?;

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
///
/// da-1m63: cluster names are namespaced per user, so the same
/// cluster_id can exist for two different users. The pull must
/// therefore ALSO scope to the calling user — via the changeset
/// device's owner — or same-named clusters would leak blobs across
/// users.
///
/// da-5ua6: returns at most `limit` rows, ascending by `id`, so a peer
/// with a long history is paged down rather than serialised into one
/// response. The caller re-asks from the last `id` it applied.
pub type ClusterChangesetRow = (i64, String, i64, String, String, String, i64);

pub fn pull_changesets_for_cluster(
    conn: &Connection,
    since: i64,
    excluding_device: &str,
    cluster_id: &str,
    user_id: &Uuid,
    limit: i64,
) -> Result<Vec<ClusterChangesetRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, seq, payload, uploaded_at, cluster_id, generation
         FROM changesets
         WHERE id > ?1 AND device_id != ?2 AND cluster_id = ?3
           AND device_id IN (SELECT id FROM devices WHERE user_id = ?4)
         ORDER BY id
         LIMIT ?5",
    )?;

    let rows = stmt.query_map(
        params![
            since,
            excluding_device,
            cluster_id,
            user_id.to_string(),
            limit
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;

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

/// The user_id (as a string) that owns `device_id`, or `None` if the device
/// is not yet registered. Lets `push` reject a changeset submitted under
/// another user's device identity before it writes (da-h4te).
pub fn device_owner(conn: &Connection, device_id: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT user_id FROM devices WHERE id = ?1",
            params![device_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

/// Ensure a device is registered for a user (auto-registers if not present).
/// Callers that accept attacker-supplied `device_id` (i.e. `push`) must
/// first reject foreign ownership via [`device_owner`] — this `INSERT OR
/// IGNORE` silently no-ops on a device_id already owned by someone else.
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

    /// `uploaded_at` is surfaced on the wire (Pull response) and parsed
    /// client-side with `parse_from_rfc3339`, so the server-stamped DEFAULT
    /// must be RFC 3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`), not SQLite's
    /// `datetime('now')` form (`YYYY-MM-DD HH:MM:SS`). Locks da-grwx.
    #[test]
    fn changeset_uploaded_at_is_rfc3339() {
        let conn = setup();
        let (id, _) = insert_changeset(&conn, "device-1", "payload").unwrap();
        let uploaded_at = pull_changesets(&conn, 0, "device-2")
            .unwrap()
            .into_iter()
            .find(|r| r.0 == id)
            .unwrap()
            .4;

        // RFC 3339 UTC shape: `YYYY-MM-DDTHH:MM:SSZ` — `T` separator, `Z`
        // suffix, no space. The space-separated SQLite form would fail all
        // three checks.
        assert_eq!(uploaded_at.len(), 20, "got: {uploaded_at}");
        assert_eq!(uploaded_at.as_bytes()[10], b'T', "got: {uploaded_at}");
        assert!(uploaded_at.ends_with('Z'), "got: {uploaded_at}");
        assert!(!uploaded_at.contains(' '), "got: {uploaded_at}");
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

    /// da-1m63: a relay created with the global-namespace schema
    /// (`cluster_id TEXT PRIMARY KEY`) gets its clusters table
    /// recreated with the composite (cluster_id, user_id) PK; existing
    /// rows survive, and a second user can then register the same
    /// cluster name.
    #[test]
    fn migration_upgrades_clusters_to_composite_pk() {
        let conn = Connection::open_in_memory().unwrap();
        // The pre-da-1m63 clusters table, populated.
        conn.execute_batch(
            "CREATE TABLE clusters (
                cluster_id            TEXT PRIMARY KEY,
                user_id               TEXT NOT NULL,
                generation            INTEGER NOT NULL DEFAULT 0,
                created_at            TEXT NOT NULL DEFAULT (datetime('now')),
                generation_started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );",
        )
        .unwrap();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        conn.execute(
            "INSERT INTO clusters (cluster_id, user_id, generation) VALUES ('notes', ?1, 7)",
            params![alice.to_string()],
        )
        .unwrap();

        init_db(&conn).unwrap();

        // user_id is now part of the PK…
        assert!(is_pk_column(&conn, "clusters", "user_id").unwrap());
        // …Alice's row survived with its generation…
        let gen_: i64 = conn
            .query_row(
                "SELECT generation FROM clusters WHERE cluster_id = 'notes' AND user_id = ?1",
                params![alice.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gen_, 7);
        // …and Bob can now register the same name (was a PK conflict).
        conn.execute(
            "INSERT INTO clusters (cluster_id, user_id, generation) VALUES ('notes', ?1, 0)",
            params![bob.to_string()],
        )
        .unwrap();
    }

    /// da-1m63: cluster names are per-user, so a pull scoped to
    /// (cluster, user) must not return another user's rows even when
    /// both users named their cluster identically.
    #[test]
    fn pull_for_cluster_isolates_same_named_clusters_across_users() {
        let conn = setup();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        ensure_device(&conn, "alice-dev", &alice).unwrap();
        ensure_device(&conn, "bob-dev", &bob).unwrap();
        insert_changeset_into_cluster(&conn, "alice-dev", "alice-payload", Some("notes"), 0)
            .unwrap();
        insert_changeset_into_cluster(&conn, "bob-dev", "bob-payload", Some("notes"), 0).unwrap();

        // Alice pulling 'notes' from a second device sees only her row.
        let rows =
            pull_changesets_for_cluster(&conn, 0, "alice-dev-2", "notes", &alice, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, "alice-payload");

        // Bob likewise.
        let rows = pull_changesets_for_cluster(&conn, 0, "bob-dev-2", "notes", &bob, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, "bob-payload");
    }

    // ─── da-hu5y: per-cluster seq scoping ───────────────────────────────────

    /// One device pushing into two clusters (= two local DBs on one
    /// machine, da-3fnd) keeps an independent seq counter per cluster.
    /// This is the model the client's per-DB `device_seq` assumes — a
    /// device-global counter would force a SeqMismatch re-base on every
    /// alternating-DB push.
    #[test]
    fn seq_is_per_cluster_for_one_device() {
        let conn = setup();

        // Alternate pushes between clusters, mirroring a multi-DB user.
        let (_, a1) =
            insert_changeset_into_cluster(&conn, "device-1", "a1", Some("alpha"), 0).unwrap();
        let (_, b1) =
            insert_changeset_into_cluster(&conn, "device-1", "b1", Some("beta"), 0).unwrap();
        let (_, a2) =
            insert_changeset_into_cluster(&conn, "device-1", "a2", Some("alpha"), 0).unwrap();
        let (_, b2) =
            insert_changeset_into_cluster(&conn, "device-1", "b2", Some("beta"), 0).unwrap();

        assert_eq!((a1, a2), (1, 2), "alpha counts independently");
        assert_eq!((b1, b2), (1, 2), "beta counts independently");

        // peek agrees with what the push handler will validate against.
        assert_eq!(peek_next_seq(&conn, "device-1", Some("alpha")).unwrap(), 3);
        assert_eq!(peek_next_seq(&conn, "device-1", Some("beta")).unwrap(), 3);
        // A cluster this device never pushed into starts at 1.
        assert_eq!(peek_next_seq(&conn, "device-1", Some("gamma")).unwrap(), 1);
    }

    /// An upgraded relay whose changesets table still carries the old
    /// `UNIQUE(device_id, seq)` gets the table recreated: global ids are
    /// preserved verbatim (pull watermarks are id-based) and the same
    /// (device, seq) pair becomes legal across different clusters.
    #[test]
    fn migration_rewrites_changesets_uniqueness_per_cluster() {
        let conn = Connection::open_in_memory().unwrap();
        // The post-da-og24, pre-da-hu5y shape: cluster columns exist but
        // the uniqueness is still device-global.
        conn.execute_batch(
            "CREATE TABLE changesets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                payload TEXT NOT NULL,
                uploaded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                cluster_id TEXT,
                generation INTEGER NOT NULL DEFAULT 0,
                UNIQUE(device_id, seq)
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO changesets (id, device_id, seq, payload, cluster_id)
             VALUES (7, 'dev-A', 1, 'old-payload', 'alpha')",
            [],
        )
        .unwrap();

        init_db(&conn).unwrap();

        // Row survived the recreate with its global id intact.
        let (id, payload): (i64, String) = conn
            .query_row(
                "SELECT id, payload FROM changesets WHERE device_id = 'dev-A'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((id, payload.as_str()), (7, "old-payload"));

        // Same (device, seq) in a different cluster — rejected by the old
        // uniqueness, legal under the per-cluster model.
        let (_, seq) =
            insert_changeset_into_cluster(&conn, "dev-A", "new-payload", Some("beta"), 0).unwrap();
        assert_eq!(seq, 1);

        // AUTOINCREMENT resumed past the copied id — no id reuse.
        let new_id: i64 = conn
            .query_row(
                "SELECT id FROM changesets WHERE payload = 'new-payload'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(new_id > 7, "new id {new_id} must not reuse copied ids");

        // Re-running init_db must not recreate again (idempotent gate).
        assert!(changesets_unique_covers_cluster(&conn).unwrap());
        init_db(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM changesets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
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
