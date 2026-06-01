//! `datum sync` subcommand tree.
//!
//! Surfaces backed by the wire protocol in `docs/sync-protocol.md`:
//!
//! | Invocation                      | Purpose                                   |
//! |---------------------------------|-------------------------------------------|
//! | `datum sync`                    | bidirectional pull-then-push              |
//! | `datum sync --push`             | push only                                 |
//! | `datum sync --pull`             | pull only                                 |
//! | `datum sync --dry-run`          | report without mutating                   |
//! | `datum sync status`             | per-DB last-push/last-pull + conflicts    |
//! | `datum sync login`              | OAuth device-code flow → bearer token     |
//! | `datum sync config ...`         | Per-DB + global sync server management    |
//! | `datum sync key ...`            | Master-key init / unlock / rotate / fp    |
//!
//! The relay URL comes from YAML config or an explicit `--server` override.

use anyhow::Result;
use datum_core::db::Db;
use std::time::Duration;

use crate::args::{SyncArgs, SyncCommand, SyncConfigCommand, SyncKeyCommand};

pub mod cluster_ops;
pub mod config;
pub mod daemon_status;
pub mod install_daemon;
pub mod key;
pub mod login;
pub mod pull;
pub mod push;
pub mod status;

/// Token persisted per server, matching what `login` writes. Re-exported
/// from the client library so existing call sites keep working without
/// reaching into `datum_sync_client::protocol`.
pub use datum_sync_client::protocol::messages::AuthToken;

/// Entry point — dispatch `datum sync [flags] [subcommand]`.
pub fn dispatch(args: SyncArgs, db: &Db) -> Result<()> {
    match args.command {
        Some(SyncCommand::Login(a)) => login::run(a, db, &mut StdIoPrompter),
        Some(SyncCommand::Config(sub)) => match sub {
            SyncConfigCommand::SetServer { url, global } => config::set_server(url, global),
            SyncConfigCommand::Disable => config::disable(),
            SyncConfigCommand::Show => config::show(),
        },
        Some(SyncCommand::Key(sub)) => match sub {
            SyncKeyCommand::Init { server, force } => key::init(server, force, &mut StdIoPrompter),
            SyncKeyCommand::Unlock { server } => key::unlock(server, &mut StdIoPrompter),
            SyncKeyCommand::RotatePassphrase { server } => {
                key::rotate_passphrase(server, &mut StdIoPrompter)
            }
            SyncKeyCommand::ShowFingerprint { server } => key::show_fingerprint(server),
        },
        Some(SyncCommand::Status(a)) => status::run(a),
        Some(SyncCommand::InstallDaemon(a)) => install_daemon::run(a.show, a.uninstall),
        None => run_sync(args, db),
    }
}

/// Direction requested via CLI flags. `Both` is the default when neither
/// `--push` nor `--pull` is passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Both,
    PushOnly,
    PullOnly,
}

impl Direction {
    fn from_flags(push: bool, pull: bool) -> Self {
        match (push, pull) {
            (true, false) => Direction::PushOnly,
            (false, true) => Direction::PullOnly,
            _ => Direction::Both,
        }
    }
    pub fn does_push(self) -> bool {
        matches!(self, Direction::Both | Direction::PushOnly)
    }
    pub fn does_pull(self) -> bool {
        matches!(self, Direction::Both | Direction::PullOnly)
    }
}

/// Outcome of a single `datum sync` invocation — surfaces counts so the
/// caller can print a summary line and so tests can assert behaviour
/// without screen-scraping stdout.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub pushed: usize,
    pub pulled: usize,
    pub integrity_warnings_queued: usize,
    pub dry_run: bool,
}

fn run_sync(args: SyncArgs, db: &Db) -> Result<()> {
    let dir = Direction::from_flags(args.push, args.pull);
    let outcome = run_sync_with_client(args.server.as_deref(), dir, args.dry_run, db)?;

    let prefix = if outcome.dry_run { "[dry-run] " } else { "" };
    if dir.does_pull() {
        if outcome.integrity_warnings_queued > 0 {
            println!(
                "{prefix}↓ pulled {} change{} ({} integrity warning{} queued)",
                outcome.pulled,
                plural(outcome.pulled),
                outcome.integrity_warnings_queued,
                plural(outcome.integrity_warnings_queued),
            );
        } else {
            println!(
                "{prefix}↓ pulled {} change{}",
                outcome.pulled,
                plural(outcome.pulled),
            );
        }
    }
    if dir.does_push() {
        println!(
            "{prefix}↑ pushed {} change{}",
            outcome.pushed,
            plural(outcome.pushed),
        );
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Test hook: run a sync against the configured server without printing.
/// Returns the outcome so tests can make assertions directly.
pub fn run_sync_with_client(
    server_override: Option<&str>,
    direction: Direction,
    dry_run: bool,
    db: &Db,
) -> Result<SyncOutcome> {
    let server = resolve_server(server_override)?;
    let client = build_client()?;
    run_sync_inner(&server, &client, direction, dry_run, db)
}

pub fn run_sync_inner(
    server: &str,
    client: &reqwest::blocking::Client,
    direction: Direction,
    dry_run: bool,
    db: &Db,
) -> Result<SyncOutcome> {
    use datum_sync_client::sync_once::{self, Direction as ClientDirection, SyncConfig};

    let preflight = preflight_checks(server)?;
    let session = build_session(server, &preflight.token);
    let cfg = SyncConfig {
        server,
        session: &session,
        device_id: &preflight.device_id,
        master_key: &preflight.master_key,
        direction: match direction {
            Direction::Both => ClientDirection::Both,
            Direction::PushOnly => ClientDirection::PushOnly,
            Direction::PullOnly => ClientDirection::PullOnly,
        },
        dry_run,
    };
    let report = sync_once::sync_once(client, db, &cfg).map_err(rewrite_generation_mismatch)?;
    Ok(SyncOutcome {
        pushed: report.push.pushed,
        pulled: report.pull.applied,
        integrity_warnings_queued: report.pull.integrity_warnings_queued,
        dry_run,
    })
}

/// da-ws1q sub-5: when sync_once bubbles a SessionError::GenerationMismatch
/// up through anyhow, replace the raw protocol message with a CLI-actionable
/// "cluster forked" instruction. Anything else passes through untouched so
/// network / decode failures keep their original context.
fn rewrite_generation_mismatch(err: anyhow::Error) -> anyhow::Error {
    use datum_sync_client::http::SessionError;
    use datum_sync_client::SyncProtocolError;

    if let Some(SessionError::GenerationMismatch(SyncProtocolError::GenerationMismatch {
        expected,
        found,
        ..
    })) = err.downcast_ref::<SessionError>()
    {
        return anyhow::anyhow!(
            "cluster forked: server is on generation {expected} but this device sent {found}. \
             Another device purged or restored the cluster since your last sync. \
             Re-init this device: back up local-only work, then run `datum db purge` \
             (without --cluster) and `datum sync` to re-pull from generation {expected}."
        );
    }
    err
}

/// Build a [`Session`](datum_sync_client::http::Session) wired to
/// auto-persist rotated tokens back to `auth_token_path(host)` (mode
/// 0600). Used by every sync entry point so a 401 mid-cycle is
/// recovered transparently.
pub fn build_session(server: &str, token: &AuthToken) -> datum_sync_client::http::Session {
    use crate::config::{auth_token_path, server_host_from_url};
    use datum_sync_client::http::session::OnRefresh;
    use datum_sync_client::PersistedAuthToken;

    let server_string = server.to_string();
    let host = server_host_from_url(server);

    let on_refresh: OnRefresh = Box::new(move |new: &AuthToken| -> anyhow::Result<()> {
        // Re-derive the path on each rotation rather than capturing it —
        // tests that swap DATUM_CONFIG_HOME between sync calls expect the
        // current env var to win.
        let path = auth_token_path(&host)?;
        // da-ixmh: the OnRefresh callback receives the WIRE shape; the
        // CLI is the layer that owns persistence metadata, so we wrap
        // here. `from_wire` stamps `refreshed_at` to now and `server`
        // to the URL the session is bound to.
        let persisted = PersistedAuthToken::from_wire(new.clone(), &server_string);
        let body = serde_json::to_vec_pretty(&persisted).context("serialize auth token")?;
        write_private_file(&path, &body)?;
        Ok(())
    });

    datum_sync_client::http::Session::new(server, token, Some(on_refresh))
}

#[cfg(unix)]
fn write_private_file(path: &std::path::Path, body: &[u8]) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
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
fn write_private_file(path: &std::path::Path, body: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    std::fs::write(path, body).with_context(|| format!("write {path:?}"))?;
    Ok(())
}

/// Bundle of inputs every sync direction needs — token, key, device_id —
/// so we can validate them once and fail fast with a useful message.
pub struct Preflight {
    /// On-disk persistence shape (da-ixmh). `Deref`s to the wire
    /// `AuthToken` for HTTP call sites that only need the bearer +
    /// user_id + refresh_token; persistence-aware code reads
    /// `refreshed_at` / `server` directly.
    pub token: datum_sync_client::PersistedAuthToken,
    pub master_key: [u8; 32],
    pub device_id: String,
}

pub fn preflight_checks(server: &str) -> Result<Preflight> {
    use crate::commands::sync::key::load_token;
    use crate::config::{device_id, master_key_path, server_host_from_url};

    let token = load_token(server)?
        .ok_or_else(|| anyhow::anyhow!("not logged in — run `datum sync login` first"))?;

    let host = server_host_from_url(server);
    let key_path = master_key_path(&host)?;
    if !key_path.exists() {
        anyhow::bail!(
            "master key not unlocked — run `datum sync key init` or `datum sync key unlock`"
        );
    }
    let bytes = std::fs::read(&key_path)
        .with_context(|| format!("read master key at {}", key_path.display()))?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "master key at {} must be 32 bytes, got {}",
            key_path.display(),
            bytes.len()
        );
    }
    let mut master = [0u8; 32];
    master.copy_from_slice(&bytes);

    let device = device_id()?;
    Ok(Preflight {
        token,
        master_key: master,
        device_id: device,
    })
}

pub(crate) fn resolve_server(cli_server: Option<&str>) -> Result<String> {
    if let Some(s) = cli_server {
        return Ok(s.to_string());
    }
    let cfg = crate::config::load_raw_config_or_default()?;
    let db_name = std::env::var("DATUM_DB")
        .ok()
        .filter(|v| !v.is_empty() && !crate::config::looks_like_path(v));
    crate::config::resolve_sync_server(&cfg, db_name.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "no sync server configured; pass --server <url> or run \
             `datum sync config set-server <url> --global`"
        )
    })
}

pub(crate) fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")
}

use anyhow::Context;

/// Abstraction over user-facing I/O so tests can drive interactive flows
/// without attaching a TTY. Production uses [`StdIoPrompter`]; tests
/// substitute [`TestPrompter`].
pub trait Prompter {
    /// Show a URL + user-code prompt for the OAuth device flow.
    fn show_login_instructions(&mut self, uri: &str, user_code: &str);
    /// Read a passphrase (never echoed). `confirm = true` asks twice and
    /// rejects a mismatch.
    fn read_passphrase(&mut self, prompt: &str, confirm: bool) -> Result<String>;
    /// Print a status line while waiting on a background operation.
    fn status(&mut self, msg: &str);
    /// How long to sleep between device-code polls. Tests return quickly.
    fn sleep(&mut self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Production prompter: prints to stderr, reads via `rpassword`.
pub struct StdIoPrompter;

impl Prompter for StdIoPrompter {
    fn show_login_instructions(&mut self, uri: &str, user_code: &str) {
        eprintln!("Visit {uri} and enter code: {user_code}");
    }
    fn read_passphrase(&mut self, prompt: &str, confirm: bool) -> Result<String> {
        let pass = rpassword::prompt_password(prompt)?;
        if confirm {
            let again = rpassword::prompt_password("Confirm passphrase: ")?;
            if pass != again {
                anyhow::bail!("passphrases do not match");
            }
        }
        Ok(pass)
    }
    fn status(&mut self, msg: &str) {
        eprintln!("{msg}");
    }
}

/// Scripted prompter for tests — passphrases come from a preloaded queue
/// and status/login events are captured in `events`.
#[cfg(any(test, feature = "test-util"))]
pub struct TestPrompter {
    pub passphrases: std::collections::VecDeque<String>,
    pub events: Vec<String>,
}

#[cfg(any(test, feature = "test-util"))]
impl TestPrompter {
    pub fn new<I, S>(passphrases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            passphrases: passphrases.into_iter().map(Into::into).collect(),
            events: Vec::new(),
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Prompter for TestPrompter {
    fn show_login_instructions(&mut self, uri: &str, user_code: &str) {
        self.events.push(format!("login:{uri}:{user_code}"));
    }
    fn read_passphrase(&mut self, _prompt: &str, confirm: bool) -> Result<String> {
        let first = self
            .passphrases
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("no scripted passphrase"))?;
        if confirm {
            let again = self
                .passphrases
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("no scripted confirm"))?;
            if first != again {
                anyhow::bail!("passphrases do not match");
            }
        }
        Ok(first)
    }
    fn status(&mut self, msg: &str) {
        self.events.push(format!("status:{msg}"));
    }
    fn sleep(&mut self, _dur: Duration) {}
}
