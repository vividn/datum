//! SQLite database layer for datum.
//!
//! All entry data, field specs, and aliases live in a single SQLite file.
//! The schema is designed to be CRDT-friendly (append-mostly, ULID-ordered).
//!
//! FieldSpec stores a field's schema (default verb, allowed states, color,
//! derived aliases). Tracking state — whether a field is tracked or ignored
//! — is NOT on the spec; it derives from the entry log via [`Db::state_at`].
//! [`Db::get_tracked_fields`] is a thin projection that walks every distinct
//! field in `entries` plus every declared spec, and labels each ignored or
//! tracked according to its most recent state-bearing entry — ignored only
//! when that entry says so, so a field without one is tracked (da-m1l8).
//!
//! ## Module layout
//!
//! The implementation is split across siblings inside `db/`. Each submodule
//! adds its own `impl Db { … }` block; this top-level file only carries the
//! `Db` struct, lifecycle (open / migrate / schema_version), and the
//! re-exports that make `datum_core::db::TimeMetric` etc. resolve at the
//! historical paths.

mod alias;
mod definition;
mod holder;
mod insert;
mod integrity;
mod kit;
mod kit_command;
mod kit_layout;
mod list;
mod lookup;
mod multi;
mod now;
mod query;
mod row_mapper;
pub(crate) mod spec;
mod spec_alias_index;
mod spool;
mod state;
mod stats_cache;
pub(crate) use stats_cache::STATS_CACHE_SQL;
mod sync;
mod time;
mod tombstone;
mod types;
mod view;
pub(crate) mod write;
mod zone_timeline;
pub use write::{Seen, View, WriteBatch};

use std::cell::{Cell, OnceCell, RefCell};

use rusqlite::Connection;

use crate::error::{DatumError, Result};

pub use crate::migrations::{MigrationReport, MAX_PRE_MIGRATION_BACKUPS};

pub use alias::{InstallAliasesReport, KitAliasOutcome, ALIAS_FIELD};
pub use definition::{COMMAND_FIELD, INDEX_FIELD, KIT_FIELD, VIEW_FIELD};
pub use holder::lock_holder;
pub use kit::{InstallReport, InstalledKit, InstalledKitIndex, KitFieldOutcome};
pub use kit_command::{
    validate_command, InstallCommandsReport, InstalledKitCommand, KitCommandOutcome,
};
pub use kit_layout::{InstallLayoutsReport, KitLayoutOutcome, LAYOUT_FIELD, LAYOUT_KIT_KEY};
pub use multi::{merge_group_key, merge_same_field_groups};
pub use now::{
    NowFieldActive, NowFieldIgnored, NowFieldTrackedFalse, NowFilter, NowSnapshot,
    SCHEMA_ID as NOW_SCHEMA_ID,
};
pub use row_mapper::row_to_entry_public;
pub use spec::{is_meta_field, META_FIELDS, SPEC_FIELD};
pub use spool::{busy, Drained, Spool};
pub use stats_cache::{CachedDay, CachedState, CachedSum, Dirty, Refill};
pub(crate) use sync::derive_all_conflicts;
pub use sync::{
    canonical_json, chain, peers, row_diff, set_client_source, set_invocation, ApplySummary,
    DebugConflicts, FoldCheck, Named, Orphan, Stale, Version,
};
pub use tombstone::{revert_in_tx, soft_delete_in_tx};
pub use types::{
    AliasConflict, AliasConflictReason, ConflictStrategy, GroupColumn, HardReplaceCounts,
    InsertOutcome, ListParams, ReplaceCounts, RetimeFilter, Retimed, SaveSpecReport,
    SpecAliasCollision, SpecAliasIndex, TimeMetric, TimeOrderIssue, TimelineEntry, TrackedField,
    IGNORED_STATE_VALUE, NOT_FLAG_OR_META_SQL,
};
pub use view::{InstallViewsReport, InstalledView, KitViewOutcome, ViewResult};
pub use zone_timeline::{agreed_zone, NEIGHBOURS};

// `try_insert` is a `pub(crate)` free function in `insert.rs`; re-export it
// at `crate::db::try_insert` to preserve the historical import path used by
// `crate::migrations`.
pub(crate) use alias::new_alias_entry;
pub(crate) use definition::{definition_data, new_definition_entry};
pub(crate) use insert::try_insert;
pub(crate) use spec::{new_spec_entry, NOT_META_FIELD_SQL};
// The state-row column reader, shared by the SQL window behind `state_at`
// and the per-field cache in `crate::state` (da-u5l1o).
pub(crate) use state::{decode_state_row, read_state_columns, StateColumns};

// Canonical state-participation predicate + ordering (da-ylet), shared by
// every state oracle inside and outside the db module (`crate::state`,
// `crate::integrity`).
pub(crate) use types::{
    STATE_ORDER_NEWEST_FIRST, STATE_ORDER_OLDEST_FIRST, STATE_PARTICIPATION_SQL,
};

pub struct Db {
    conn: Connection,
    /// Records this handle's holds of the write lock (da-hyem1).
    holds: Option<std::sync::Arc<holder::Watch>>,
    /// Lazy cache of the spec-derived alias index, stamped with the
    /// connection's `total_changes()` when it was built: any write since
    /// makes it stale, so it is never a second copy of the spec entries
    /// that can drift from them (da-lc26c).
    spec_alias_cache: RefCell<Option<(u64, SpecAliasIndex)>>,
    /// The client a spec entry this handle writes is stamped with
    /// ([`Db::set_client_id`]); unset, the write says `local`.
    client_id: OnceCell<String>,
    /// What this open did to the schema, if anything. `None` when the DB
    /// was already current. datum-core never prints; the CLI reads this
    /// and tells the user (da-ksao).
    last_migration: Option<MigrationReport>,
    /// This open made the file: nothing was at its path before it.
    created: bool,
    /// Why this open's refold under a newer fold rule failed, when it did
    /// (da-sqpjt): the open goes on, and the CLI says so.
    refold_failed: RefCell<Option<String>>,
    /// The file this handle was opened at, as it was named: what datum
    /// writes beside a database is beside this. `None` in memory.
    path: Option<std::path::PathBuf>,
    /// When this handle's run of sync apply chunks with no pause between
    /// them began, and when it last committed one (`sync_apply`, da-2ejbs).
    pub(crate) apply_run: Cell<Option<(std::time::Instant, std::time::Instant)>>,
}

impl Db {
    /// Configure a file-backed connection for concurrent use (da-11qq).
    ///
    /// datum's design makes concurrency the normal mode — `--watch` loops
    /// read while the CLI writes — so every
    /// connection gets, explicitly and before any migration write:
    ///
    /// * `busy_timeout = 5000` — absorbs writer-vs-writer overlap instead
    ///   of surfacing SQLITE_BUSY. rusqlite happens to set this same 5s
    ///   default on open, but that is an internal detail of rusqlite —
    ///   datum's concurrency story shouldn't hinge on it. `sync run` alone
    ///   lowers it on its own connection, so a sync gives way to a person's
    ///   write rather than the reverse (`datum_sync::YIELD`, da-lk1s2).
    /// * `journal_mode = WAL` — readers never block the writer (in
    ///   rollback-journal mode a reader holding a transaction BUSYs any
    ///   writer). Persistent in the DB file, but set here rather than in
    ///   schema.sql so unmigrated opens and the migration walk itself run
    ///   under WAL too.
    /// * `synchronous = NORMAL` — the standard WAL pairing; fewer fsyncs
    ///   per commit with no integrity loss (WAL stays consistent on power
    ///   loss; at worst the last commit rolls back).
    ///
    /// All three pragmas cost microseconds — the CLI hot path stays
    /// instantaneous. (`execute_batch` because these pragmas return rows.)
    ///
    /// On wasm there is a fourth, `temp_store = MEMORY` (da-uhur). A WASI
    /// process has exactly one preopened directory and no temp directory at
    /// all, so the first query whose ephemeral table or sorter outgrows the
    /// page cache asks the VFS for a temp path and is answered
    /// `SQLITE_IOERR_GETTEMPPATH` (6410) — "disk I/O error", fatal, on a
    /// query that is merely large. `stats` did this on a ~20k-entry
    /// database in the browser. In memory there is nothing to ask the VFS
    /// for, and no query can reach that error again.
    ///
    /// `target_family = "wasm"` rather than `target_os = "wasi"`: the
    /// premise is not which WASI preview the shim implements but that a
    /// wasm build of datum has no host filesystem to spill into, which is
    /// true of every wasm target the CLI is compiled for. Native keeps
    /// SQLite's compile-time default (FILE), where a genuinely huge sort
    /// should spill to the disk that exists rather than eat the machine.
    ///
    /// And a fifth, `locking_mode = EXCLUSIVE` (da-o3sns): the connection
    /// takes its lock at the first read and keeps it until it closes. The
    /// browser already gives one process the files at a time, so the lock
    /// shuts nobody out — but released, SQLite took it again for every
    /// statement and re-read the file's header and size to check its cache:
    /// 100-250 times a command, each an OPFS call, ~3 ms of a command in a
    /// real profile and 90-280 ms in a private window, where every call is
    /// an IPC.
    ///
    /// And a sixth, `cache_size` of 64 MiB rather than SQLite's 2 MiB
    /// (da-89ujs). A process's cache is gone when it exits, and a write
    /// bigger than the cache read its pages again and spilled them early,
    /// each page an OPFS call: a 2k-op sync apply read 7,190 pages and wrote
    /// 2,410, against 152 and 1,740 with this. The cache grows only as pages
    /// are read; a small command stays small.
    ///
    /// Transactions are opened by `db::write` alone (da-8i7f0): a write
    /// IMMEDIATE, taking the lock at `BEGIN` where `busy_timeout` waits for
    /// it (da-5pulr: a DEFERRED write that read first fails at once with
    /// SQLITE_BUSY when another connection committed meanwhile), and a read
    /// DEFERRED, which takes no lock a writer waits on.
    fn configure_connection(conn: &mut Connection) -> Result<()> {
        // A bulk write runs a few dozen statements a row: the default 16
        // slots evicted each before its next use, and a statement on
        // `entries` compiles its triggers too (da-f4vrr).
        conn.set_prepared_statement_cache_capacity(256);
        conn.execute_batch(
            "PRAGMA busy_timeout = 5000;\n\
             PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;",
        )?;
        #[cfg(target_family = "wasm")]
        conn.execute_batch(
            "PRAGMA temp_store = MEMORY;\nPRAGMA locking_mode = EXCLUSIVE;\nPRAGMA cache_size = -65536;",
        )?;
        Ok(())
    }

    /// On WASI, every file opens through SQLite's `unix-none` VFS rather
    /// than the `unix-dotfile` its WASI build defaults to (da-dfmg8). That
    /// VFS reads the lock directory a connection made for itself as another
    /// process's reserved lock, and SQLite looks for a hot journal only
    /// once it holds that lock: so no journal was ever hot, and a write cut
    /// short — a tab closed, killed or crashed mid-commit — left its
    /// half-written pages for every process after to read, «database disk
    /// image is malformed». Nothing is given up: the lock directory lived
    /// in each process's own memory, so it never kept one process from
    /// another. The browser's OPFS handles and its Web Lock do that.
    fn choose_vfs() {
        #[cfg(target_os = "wasi")]
        {
            static ONCE: std::sync::Once = std::sync::Once::new();
            // SAFETY: a VFS SQLite registered itself, found by name and made
            // the default before any connection is opened.
            ONCE.call_once(|| unsafe {
                let vfs = rusqlite::ffi::sqlite3_vfs_find(c"unix-none".as_ptr());
                assert!(!vfs.is_null(), "SQLite's WASI build has no unix-none VFS");
                rusqlite::ffi::sqlite3_vfs_register(vfs, 1);
            });
        }
    }

    /// Open (or create) the database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        Self::choose_vfs();
        let created = !std::path::Path::new(path).exists();
        let mut conn = Connection::open(path)?;
        Self::configure_connection(&mut conn)?;
        let mut db = Db {
            holds: holder::watch(&conn, std::path::Path::new(path)),
            conn,
            spec_alias_cache: RefCell::new(None),
            client_id: OnceCell::new(),
            apply_run: Cell::new(None),
            last_migration: None,
            created,
            refold_failed: Default::default(),
            path: Some(path.into()),
        };
        db.last_migration = db.migrate(Some(std::path::Path::new(path)))?;
        Ok(db)
    }

    /// Open the database without running migrations. Used by destructive
    /// recovery paths (`datum db purge`) that will rebuild the schema from
    /// scratch — auto-migrate on a half-state DB can fail before the
    /// recovery code has a chance to run. It still sets the journal mode,
    /// which is in the file's header: a reader that must not rewrite what
    /// it reads takes [`Db::open_read_only`].
    pub fn open_unmigrated(path: &str) -> Result<Self> {
        Self::choose_vfs();
        let mut conn = Connection::open(path)?;
        Self::configure_connection(&mut conn)?;
        Ok(Db {
            holds: holder::watch(&conn, std::path::Path::new(path)),
            conn,
            spec_alias_cache: RefCell::new(None),
            client_id: OnceCell::new(),
            apply_run: Cell::new(None),
            last_migration: None,
            created: false,
            refold_failed: Default::default(),
            path: Some(path.into()),
        })
    }

    /// Open an existing database for reading only: no migration, no
    /// schema, no pragma that writes, and SQLite refuses every write on the
    /// handle. For readers that must not touch what they read, such as
    /// shell completion (`datum __complete`, da-bu09u). A missing file is an
    /// error, never a new database.
    ///
    /// A read-only open of a WAL database still creates its `-wal` and
    /// `-shm` files when they are absent. Absent, and with no rollback
    /// journal to read either (WASI's every write, da-yajgu), the file is
    /// opened `immutable` and nothing is created beside it. Beside a hot
    /// journal SQLite refuses the open (`SQLITE_READONLY_ROLLBACK`), as a
    /// read-only handle cannot roll it back: an error, never a cut write.
    pub fn open_read_only(path: &str) -> Result<Self> {
        use rusqlite::OpenFlags;
        let checkpointed = !std::path::Path::new(&format!("{path}-wal")).exists()
            && std::fs::metadata(format!("{path}-journal")).map_or(true, |m| m.len() == 0);
        let escaped = path
            .replace('%', "%25")
            .replace('?', "%3f")
            .replace('#', "%23");
        let uri = format!(
            "file:{escaped}?mode=ro{}",
            if checkpointed { "&immutable=1" } else { "" }
        );
        Self::choose_vfs();
        let conn = Connection::open_with_flags(
            uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        Ok(Db {
            conn,
            holds: None,
            spec_alias_cache: RefCell::new(None),
            client_id: OnceCell::new(),
            apply_run: Cell::new(None),
            last_migration: None,
            created: false,
            refold_failed: Default::default(),
            path: Some(path.into()),
        })
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Db {
            conn,
            holds: None,
            spec_alias_cache: RefCell::new(None),
            client_id: OnceCell::new(),
            apply_run: Cell::new(None),
            last_migration: None,
            created: false,
            refold_failed: Default::default(),
            path: None,
        };
        db.last_migration = db.migrate(None)?;
        Ok(db)
    }

    /// Call `log` with the time held and the rows written per table each
    /// time this handle lets go of the write lock after holding it `after`
    /// or longer (da-hyem1). A file-backed handle only, and not on wasm.
    pub fn on_long_hold(
        &self,
        after: std::time::Duration,
        log: impl Fn(std::time::Duration, &str) + Send + 'static,
    ) {
        if let Some(holds) = &self.holds {
            holds.on_long_hold(after, Box::new(log));
        }
    }

    /// Did this open make the file? `db migrate` says so (da-zg5ut).
    pub fn created(&self) -> bool {
        self.created
    }

    /// What the migration walk during [`Db::open`] did, or `None` if the
    /// database was already at the current schema version.
    pub fn last_migration(&self) -> Option<&MigrationReport> {
        self.last_migration.as_ref()
    }

    /// Why the refold this open ran under a newer fold rule failed, if it
    /// did: the open went on, stamped (da-sqpjt).
    pub fn refold_failed(&self) -> Option<String> {
        self.refold_failed.borrow().clone()
    }

    /// Bring the schema to [`crate::migrations::CURRENT_SCHEMA_VERSION`],
    /// snapshotting the file first when there is real work to do.
    ///
    /// `db_path` is `None` only for in-memory databases, which are always
    /// created fresh at the current version and so never migrate.
    fn migrate(&self, db_path: Option<&std::path::Path>) -> Result<Option<MigrationReport>> {
        // Reject pre-refactor entries tables (which have an `id` column):
        // this refactor drops ULID entirely and slug is now PK. Silent
        // migration would lose or re-key data unpredictably; the user must
        // delete the DB and re-create it.
        let legacy_entries_sql: String = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='entries'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        if !legacy_entries_sql.is_empty() {
            let upper = legacy_entries_sql.to_uppercase();
            // The pre-refactor schema named the ULID column `id` as the PK.
            // Our new schema has no such column.
            if upper.contains(" ID ") || upper.contains("\tID\t") || upper.contains("(ID ") {
                return Err(DatumError::IncompatibleSchema(
                    "legacy entries table with `id` column found; this version drops ULID \
                     and uses slug as PK. Delete the DB and recreate it, or migrate from a \
                     backup with `datum db v2import`."
                        .to_string(),
                ));
            }
        }

        // Stamp / advance the schema version. `infer_legacy_version`
        // short-circuits a fresh DB (no datum tables yet) to
        // CURRENT_SCHEMA_VERSION; legacy DBs walk their column-add probes
        // forward. Migrations add or rebuild the tables they need, so we
        // run them BEFORE schema.sql — no pre-flight ALTER hacks required.
        // Snapshot BEFORE the first write. A migration that runs without
        // its backup is the failure this guards against, so a backup
        // error aborts the open rather than being logged and ignored.
        let from = crate::migrations::peek_schema_version(&self.conn)?;
        let to = crate::migrations::CURRENT_SCHEMA_VERSION;
        let backup_path = match db_path {
            Some(path) if from < to => Some(crate::migrations::backup_before_migration(
                &self.conn, path, from, to,
            )?),
            _ => None,
        };

        let applied = crate::migrations::migrate_to_current(&self.conn)?;
        // schema.sql lands the final shape: any indexes / tables the
        // migration registry doesn't create (because they were always part
        // of schema.sql since they were introduced) are filled in here.
        // CREATE … IF NOT EXISTS keeps the pass idempotent on already-current
        // databases.
        self.conn.execute_batch(include_str!("../schema.sql"))?;
        self.conn.execute_batch(STATS_CACHE_SQL)?;
        // A kit index is this device's, derived from the definitions the
        // file holds (da-fwwtn). One that will not build is no reason not
        // to open: it is only an index.
        let _ = self.derive_kit_indices();
        self.refold_for_rule()?;

        if applied.is_empty() {
            return Ok(None);
        }
        // Retention runs only after the migration succeeded — we never
        // delete an old snapshot to make room for one that may be needed.
        let pruned = db_path
            .map(crate::migrations::prune_pre_migration_backups)
            .unwrap_or_default();
        Ok(Some(MigrationReport {
            from_version: from,
            to_version: to,
            backup_path,
            applied,
            pruned,
        }))
    }

    /// Borrow the underlying connection. Low-level — callers should prefer
    /// the typed API; this is intended for `datum db migrate`, `datum info`,
    /// and other schema-level tooling.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// The file this handle was opened at; `None` for an in-memory database.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// Current stored schema version, read fresh from `_meta`.
    pub fn schema_version(&self) -> Result<u32> {
        match crate::migrations::read_schema_version(&self.conn)? {
            Some(v) => Ok(v),
            None => Ok(crate::migrations::CURRENT_SCHEMA_VERSION),
        }
    }

    /// Drop every table in the file and rebuild the schema from scratch.
    /// Used by `datum db purge`. Caller is responsible for warning about
    /// data loss before invoking this.
    ///
    /// Every table, read from the file rather than listed here: sync's
    /// `ops` carry each change's effect and the chain, and `sync_local` the
    /// endpoint key, so a purge that kept them kept the data and the
    /// membership both (da-8v7ua). The purged copy mints a new key on its
    /// next sync and is a new copy, in no chain.
    ///
    /// And nothing of them stays in the file, as deleting it would leave
    /// nothing (da-nc8i1): a dropped table's pages are only marked free, so
    /// `VACUUM` rewrites the file without them, and the `-wal`, which holds
    /// earlier images of those pages, is checkpointed and truncated. The
    /// file itself stays: a running `sync run` holds it open, and reads its
    /// changed endpoint key through that handle. A reader that will not let
    /// the checkpoint finish within `busy_timeout` is an error, and a
    /// second purge finishes the job.
    ///
    /// The drops and the new schema are ONE transaction (da-dfmg8): dropped
    /// one at a time, a DROP that failed — on a corrupt page, which DROP
    /// walks every page of a table to meet — kept the tables after it and
    /// not `_meta` or `entries` before them, and every open after read that
    /// file as schema v7 and refused to migrate it.
    pub fn force_recreate_schema(&self) -> Result<Vec<String>> {
        self.recreate_schema_with(|_| Ok(()))
    }

    /// [`Db::force_recreate_schema`], with `within` run in its transaction
    /// on the new schema before it commits: what it writes lands with the
    /// purge or not at all (`db purge --rejoin`, da-qjxfx).
    pub(crate) fn recreate_schema_with(
        &self,
        within: impl FnOnce(&Connection) -> Result<()>,
    ) -> Result<Vec<String>> {
        // One transaction over the whole file, by design (da-dfmg8 above).
        let tables = write::whole(&self.conn, |tx| {
            let tables: Vec<String> = tx
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
                )?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            // IF EXISTS: dropping a virtual table drops its shadow tables,
            // which are later in the list (da-k3uqd).
            for t in &tables {
                let t = t.replace('"', "\"\"");
                tx.execute(&format!("DROP TABLE IF EXISTS \"{t}\""), [])?;
            }
            tx.execute_batch(include_str!("../schema.sql"))?;
            tx.execute_batch(STATS_CACHE_SQL)?;
            crate::migrations::set_schema_version(tx, crate::migrations::CURRENT_SCHEMA_VERSION)?;
            within(tx)?;
            Ok(tables)
        })?;
        self.conn.execute_batch("VACUUM")?;
        // A kit index is this device's, derived from the definitions the
        // file holds (da-fwwtn). One that will not build is no reason not
        // to open: it is only an index.
        let _ = self.derive_kit_indices();
        // The first column is `busy`: 1 when a reader kept the checkpoint
        // from finishing. No row from an SQLite built without WAL,
        // datum.day's: it has no `-wal` to truncate.
        use rusqlite::OptionalExtension;
        let busy: Option<i64> = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .optional()?;
        if busy.is_some_and(|busy| busy != 0) {
            return Err(DatumError::Other(
                "the database is purged and usable, empty, but another process is reading \
                 it, so its -wal file still holds old pages: run the purge again to clear them"
                    .to_string(),
            ));
        }
        Ok(tables)
    }
}
