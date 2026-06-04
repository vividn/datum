//! `datum sync login` — CLI-side thin wrapper over
//! [`datum_sync_client::http::run_device_flow`]. This file owns only the
//! three CLI-side concerns:
//! 1. resolving which relay URL to talk to (via the shared
//!    [`resolve_server`]: flag > per-DB config > global default);
//! 2. bridging the CLI's `Prompter` trait to the client library's
//!    `DeviceFlowPrompter`;
//! 3. persisting the minted token under the XDG auth path (mode 0600).

use anyhow::{Context, Result};
use datum_core::db::Db;
use datum_sync_client::http::{run_device_flow, DeviceFlowPrompter};
use datum_sync_client::protocol::messages::AuthToken;
use datum_sync_client::{sync_state, PersistedAuthToken};
use reqwest::blocking::Client;
use std::time::Duration;

use crate::args::SyncLoginArgs;
use crate::commands::sync::{build_client, resolve_server, Prompter};
use crate::config::{auth_token_path, server_host_from_url};

/// Production entry point.
///
/// `cli_db` is the global `--db` flag — it beats `DATUM_DB` when naming
/// the active DB for per-DB server resolution (da-cyn4).
pub fn run(
    args: SyncLoginArgs,
    cli_db: Option<&str>,
    db: &Db,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(args.server.as_deref(), cli_db)?;
    let client = build_client()?;
    run_with_client(&server, &client, prompter)?;
    adopt_cluster(db, args.cluster.as_deref());
    Ok(())
}

/// Split from `run` so tests can inject a client pointed at a stub relay.
/// Returns the freshly-issued wire `AuthToken` — the persisted shape
/// has already been written to disk by this point.
pub fn run_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<AuthToken> {
    let mut bridge = PrompterBridge { inner: prompter };
    let token = run_device_flow(server, client, &mut bridge)?;
    // da-ixmh: persistence is the CLI's job, not the protocol type's.
    // Wrap the wire token in PersistedAuthToken (which stamps
    // `refreshed_at` + `server`) before writing.
    let persisted = PersistedAuthToken::from_wire(token.clone(), server);
    write_persisted_token(server, &persisted)?;
    Ok(token)
}

/// After a successful login, bind this DB to a sync cluster locally so
/// the very first push/pull sends a stable identifier instead of `None`.
///
/// da-3fnd: the default cluster is the DB's file name (e.g. `vremya`) —
/// two devices on a same-named DB auto-share a cluster with zero config.
/// `sync login --cluster <name>` overrides that to merge differently-
/// named DBs onto one cluster (or split one off).
///
/// Failures here are non-fatal — login itself has succeeded. The next
/// push/pull will set cluster_id from the server's echo (via da-eu7o)
/// anyway; this is a UX nicety so the user sees "Cluster <id> bound to
/// <db>" immediately after login.
fn adopt_cluster(db: &Db, explicit: Option<&str>) {
    let cluster = match explicit {
        Some(name) => name.to_string(),
        None => sync_state::default_cluster_id(db),
    };
    if let Err(e) = sync_state::set_cluster_id(db, &cluster) {
        eprintln!("⚠ login: could not record cluster_id locally: {e:#}");
        return;
    }
    let db_path = current_db_path(db);
    match db_path {
        Some(path) => println!("✓ Cluster {cluster} bound to {path}"),
        None => println!("✓ Cluster {cluster} registered"),
    }
}

/// Pull the active SQLite file path off `pragma_database_list`. None
/// for an in-memory DB (tests) — caller falls back to a path-free
/// message in that case.
fn current_db_path(db: &Db) -> Option<String> {
    let path: String = db
        .conn()
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |row| row.get(0),
        )
        .ok()?;
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Adapter: expose the CLI's richer `Prompter` as the client library's
/// narrower `DeviceFlowPrompter`. Keeps the sync-client free of rpassword
/// and other CLI-only deps.
struct PrompterBridge<'a, P: Prompter + ?Sized> {
    inner: &'a mut P,
}

impl<'a, P: Prompter + ?Sized> DeviceFlowPrompter for PrompterBridge<'a, P> {
    fn show_login_instructions(&mut self, uri: &str, user_code: &str) {
        self.inner.show_login_instructions(uri, user_code);
    }
    fn status(&mut self, msg: &str) {
        self.inner.status(msg);
    }
    fn sleep(&mut self, dur: Duration) {
        self.inner.sleep(dur);
    }
}

/// Persist the (already-wrapped) token atomically under
/// `auth_token_path(host)`. Takes [`PersistedAuthToken`] specifically —
/// callers are responsible for stamping the persistence metadata via
/// [`PersistedAuthToken::from_wire`] first (da-ixmh).
fn write_persisted_token(server: &str, token: &PersistedAuthToken) -> Result<()> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    let body = serde_json::to_vec_pretty(token).context("serialize auth token")?;
    crate::config::write_atomic(&path, &body, 0o600).with_context(|| format!("write {path:?}"))
}
