//! `datum sync login` — CLI-side thin wrapper over
//! [`datum_sync_client::http::run_device_flow`]. This file owns only the
//! three CLI-side concerns:
//! 1. resolving which relay URL to talk to (flag > yaml config);
//! 2. bridging the CLI's `Prompter` trait to the client library's
//!    `DeviceFlowPrompter`;
//! 3. persisting the minted token under the XDG auth path (mode 0600).

use anyhow::{anyhow, Context, Result};
use datum_core::db::Db;
use datum_sync_client::http::{run_device_flow, DeviceFlowPrompter};
use datum_sync_client::protocol::messages::AuthToken;
use datum_sync_client::{sync_state, PersistedAuthToken};
use reqwest::blocking::Client;
use std::time::Duration;

use crate::args::SyncLoginArgs;
use crate::commands::sync::Prompter;
use crate::config::{auth_token_path, load_raw_config_or_default, server_host_from_url};

/// Production entry point.
pub fn run(args: SyncLoginArgs, db: &Db, prompter: &mut impl Prompter) -> Result<()> {
    let server = resolve_server(args.server.as_deref())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    let token = run_with_client(&server, &client, prompter)?;
    adopt_cluster_for_user(db, &token);
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

/// da-ws1q sub-5: after a successful login, adopt the cluster
/// registration locally. The transitional default (per da-og24 + the
/// server's push/pull handlers) is `cluster_id = user_id` — so the
/// server-side `ensure_cluster` that runs on first push/pull is
/// deterministic, and we can record the same value here so the very
/// first sync sends a stable identifier instead of `None`.
///
/// Failures here are non-fatal — login itself has succeeded. The next
/// push/pull will set cluster_id from the server's echo (via da-eu7o)
/// anyway; this is a UX nicety so the user sees "Cluster <id> bound to
/// <db>" immediately after login.
fn adopt_cluster_for_user(db: &Db, token: &AuthToken) {
    // da-ixmh: user_id is now a required field of the wire shape, so
    // the previous "server returned no user_id" branch is dead — keep
    // the call site simple.
    let user_id = &token.user_id;
    if let Err(e) = sync_state::set_cluster_id(db, user_id) {
        eprintln!("⚠ login: could not record cluster_id locally: {e:#}");
        return;
    }
    let db_path = current_db_path(db);
    match db_path {
        Some(path) => println!("✓ Cluster {user_id} bound to {path}"),
        None => println!("✓ Cluster {user_id} registered"),
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

/// Resolve the relay URL: explicit `--server` wins, otherwise YAML config.
fn resolve_server(cli_server: Option<&str>) -> Result<String> {
    if let Some(s) = cli_server {
        return Ok(s.to_string());
    }
    let cfg = load_raw_config_or_default()?;
    cfg.sync
        .as_ref()
        .and_then(|s| s.default_server.clone())
        .filter(|v| !v.eq_ignore_ascii_case("none"))
        .ok_or_else(|| {
            anyhow!(
                "no sync server configured; pass --server <url> or run \
                 `datum sync config set-server <url> --global`"
            )
        })
}

/// Persist the (already-wrapped) token atomically under
/// `auth_token_path(host)`. Takes [`PersistedAuthToken`] specifically —
/// callers are responsible for stamping the persistence metadata via
/// [`PersistedAuthToken::from_wire`] first (da-ixmh).
fn write_persisted_token(server: &str, token: &PersistedAuthToken) -> Result<()> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    let body = serde_json::to_vec_pretty(token).context("serialize auth token")?;
    write_private_file(&path, &body)?;
    Ok(())
}

fn write_private_file(path: &std::path::Path, body: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("open {path:?}"))?;
        std::io::Write::write_all(&mut f, body)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, body).with_context(|| format!("write {path:?}"))?;
        Ok(())
    }
}

/// Load the persisted token for `server`. Returns `Ok(None)` if there's
/// no file. da-ixmh: this returns the on-disk shape
/// ([`PersistedAuthToken`]); call sites that only need the bearer use
/// the `Deref<Target = AuthToken>` impl, while sync_state-aware paths
/// can read `refreshed_at` / `server`.
#[allow(dead_code)]
pub fn load_token(server: &str) -> Result<Option<PersistedAuthToken>> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read token at {path:?}"))?;
    let token: PersistedAuthToken =
        serde_json::from_slice(&bytes).context("parse stored auth token")?;
    Ok(Some(token))
}
