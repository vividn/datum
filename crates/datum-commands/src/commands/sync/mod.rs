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
pub mod recovery;
pub mod status;

/// Token persisted per server, matching what `login` writes. Re-exported
/// from the client library so existing call sites keep working without
/// reaching into `datum_sync_client::protocol`.
pub use datum_sync_client::protocol::messages::AuthToken;

/// Entry point — dispatch `datum sync [flags] [subcommand]`.
///
/// `cli_db` is the global `--db <value>` flag (da-cyn4): per-DB-aware
/// subcommands must scope by it, not by `DATUM_DB` alone.
pub fn dispatch(args: SyncArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    match args.command {
        Some(SyncCommand::Login(a)) => login::run(a, cli_db, db, &mut StdIoPrompter),
        Some(SyncCommand::Config(sub)) => match sub {
            SyncConfigCommand::SetServer { url, global } => config::set_server(url, global, cli_db),
            SyncConfigCommand::Disable => config::disable(cli_db),
            SyncConfigCommand::Show => config::show(cli_db),
        },
        Some(SyncCommand::Key(sub)) => match sub {
            SyncKeyCommand::Init { server, force } => {
                key::init(server, force, cli_db, &mut StdIoPrompter)
            }
            SyncKeyCommand::Unlock { server } => key::unlock(server, cli_db, &mut StdIoPrompter),
            SyncKeyCommand::RotatePassphrase { server } => {
                key::rotate_passphrase(server, cli_db, &mut StdIoPrompter)
            }
            SyncKeyCommand::ShowFingerprint { server } => key::show_fingerprint(server, cli_db),
        },
        Some(SyncCommand::Status(a)) => status::run(a, cli_db),
        Some(SyncCommand::InstallDaemon(a)) => install_daemon::run(a.show, a.uninstall),
        None => run_sync(args, cli_db, db),
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
    /// `Some` when the push recovered from a seq desync by re-basing to
    /// the relay's authoritative seq (da-ca4q). The CLI prints this as a
    /// stderr warning; the library never prints (da-60gf).
    pub seq_rebased: Option<datum_sync_client::sync_once::SeqRebase>,
}

fn run_sync(args: SyncArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    let dir = Direction::from_flags(args.push, args.pull);
    // Resolve the relay HERE, --db-aware (da-cyn4), and hand it down as
    // an explicit override — the inner runner's own resolve_server falls
    // back to env-only detection and must not be reached from the CLI
    // path when a per-DB server is pinned.
    let server = resolve_server(args.server.as_deref(), cli_db)?;
    // CLI path: pass a real Prompter so GenerationMismatch recovery
    // (da-74xx) can run interactively. Headless / test callers that go
    // through the bare `run_sync_with_client` get the pre-recovery
    // behaviour (the error surfaces with the "cluster forked" message).
    let outcome = run_sync_with_client_and_prompter(
        Some(&server),
        dir,
        args.dry_run,
        db,
        &mut StdIoPrompter,
    )?;

    if let Some(r) = &outcome.seq_rebased {
        eprintln!(
            "warning: sync seq diverged for device {} \
             (predicted {}, relay expected {}); re-based and retried",
            r.device_id, r.predicted, r.expected,
        );
    }
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
/// Returns the outcome so tests can make assertions directly. Backwards-
/// compatible — does NOT attempt da-74xx recovery on
/// `GenerationMismatch`. Use [`run_sync_with_client_and_prompter`] when
/// the caller can drive interactive prompts.
pub fn run_sync_with_client(
    server_override: Option<&str>,
    direction: Direction,
    dry_run: bool,
    db: &Db,
) -> Result<SyncOutcome> {
    run_sync_with_client_inner(server_override, direction, dry_run, db, None)
}

/// Prompter-aware variant: the CLI dispatch path uses this with
/// `StdIoPrompter` so a `GenerationMismatch` triggers the da-74xx
/// recovery flow (prompt → optional JSONL export → purge pre-reinit →
/// reset watermark → adopt new generation → retry the sync). Pass a
/// `TestPrompter` from integration tests to drive the prompts.
pub fn run_sync_with_client_and_prompter(
    server_override: Option<&str>,
    direction: Direction,
    dry_run: bool,
    db: &Db,
    prompter: &mut dyn Prompter,
) -> Result<SyncOutcome> {
    run_sync_with_client_inner(server_override, direction, dry_run, db, Some(prompter))
}

fn run_sync_with_client_inner(
    server_override: Option<&str>,
    direction: Direction,
    dry_run: bool,
    db: &Db,
    prompter: Option<&mut dyn Prompter>,
) -> Result<SyncOutcome> {
    let server = resolve_server(server_override, None)?;
    let client = build_client()?;
    run_sync_inner_with_recovery(&server, &client, direction, dry_run, db, prompter)
}

pub fn run_sync_inner(
    server: &str,
    client: &reqwest::blocking::Client,
    direction: Direction,
    dry_run: bool,
    db: &Db,
) -> Result<SyncOutcome> {
    run_sync_inner_with_recovery(server, client, direction, dry_run, db, None)
}

fn run_sync_inner_with_recovery(
    server: &str,
    client: &reqwest::blocking::Client,
    direction: Direction,
    dry_run: bool,
    db: &Db,
    prompter: Option<&mut dyn Prompter>,
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

    // On the happy path this is one call to `sync_once`. On a
    // `GenerationMismatch` *with* a prompter, run the da-74xx recovery
    // flow and retry once. Without a prompter, fall back to the original
    // "cluster forked" CLI message — preserving the test-callable shape.
    let report = match sync_once::sync_once(client, db, &cfg) {
        Ok(r) => r,
        Err(e) => match prompter {
            Some(p) => match try_recover_and_retry(&e, db, p, client, &cfg)? {
                Some(retry) => retry,
                None => return Err(rewrite_generation_mismatch(e)),
            },
            None => return Err(rewrite_generation_mismatch(e)),
        },
    };
    Ok(SyncOutcome {
        pushed: report.push.pushed,
        pulled: report.pull.applied,
        integrity_warnings_queued: report.pull.integrity_warnings_queued,
        dry_run,
        seq_rebased: report.push.seq_rebased,
    })
}

/// Attempt da-74xx recovery for a `GenerationMismatch` error and retry
/// the sync. Returns `Ok(Some(report))` when both succeed, `Ok(None)`
/// when the inbound error isn't a `GenerationMismatch` (the caller
/// should fall through to its non-recovery error path), or `Err(...)`
/// when recovery itself failed (user declined, IO failure, or the retry
/// surfaced another error — all propagated so the user sees the cause).
fn try_recover_and_retry(
    err: &anyhow::Error,
    db: &Db,
    prompter: &mut dyn Prompter,
    client: &reqwest::blocking::Client,
    cfg: &datum_sync_client::sync_once::SyncConfig<'_>,
) -> Result<Option<datum_sync_client::sync_once::SyncReport>> {
    use datum_sync_client::http::SessionError;
    use datum_sync_client::SyncProtocolError;
    let (expected, generation_started_at) = match err.downcast_ref::<SessionError>() {
        Some(SessionError::GenerationMismatch(SyncProtocolError::GenerationMismatch {
            expected,
            generation_started_at,
            ..
        })) => (*expected, generation_started_at.clone()),
        _ => return Ok(None),
    };
    let _rep = recovery::run(db, expected, &generation_started_at, prompter)?;
    // Pass the retry's error through `rewrite_generation_mismatch` so a
    // rare second mismatch (another reinit landed *during* recovery)
    // still surfaces the actionable "cluster forked" CLI message rather
    // than the raw protocol error (Copilot review on PR #8).
    let retry = datum_sync_client::sync_once::sync_once(client, db, cfg)
        .map_err(rewrite_generation_mismatch)?;
    Ok(Some(retry))
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
        crate::config::write_atomic(&path, &body, 0o600)
            .with_context(|| format!("write {path:?}"))?;
        Ok(())
    });

    datum_sync_client::http::Session::new(server, token, Some(on_refresh))
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

/// Load the persisted bearer for `server`, or explain — accurately — how to
/// get one.
///
/// The supported bootstrap on a default relay is an admin-minted token
/// exported as `DATUM_SYNC_TOKEN`; `datum sync login` (the passkey /
/// device-code path) only completes once the relay enables it. Lead with the
/// working path so a new user isn't sent to a dead end (da-gqp5).
pub(crate) fn require_token(server: &str) -> Result<datum_sync_client::PersistedAuthToken> {
    crate::commands::sync::key::load_token(server)?.ok_or_else(|| {
        anyhow::anyhow!(
            "no sync credentials for {server}.\n\
             Get a token from the relay admin — on the relay host, run:\n    \
             datum-sync-server mint-token <your-username>\n\
             then export it on this device and re-run:\n    \
             export DATUM_SYNC_TOKEN=<token>\n\
             (`datum sync login` is the passkey path — available once the relay enables it.)"
        )
    })
}

pub fn preflight_checks(server: &str) -> Result<Preflight> {
    use crate::config::{device_id, master_key_path, server_host_from_url};

    let token = require_token(server)?;

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

/// Resolve the relay URL: explicit `--server` wins, then the per-DB /
/// global config for the active DB. `cli_db` is the global `--db` flag —
/// it beats `DATUM_DB` when naming the active DB (da-cyn4).
pub(crate) fn resolve_server(cli_server: Option<&str>, cli_db: Option<&str>) -> Result<String> {
    if let Some(s) = cli_server {
        return Ok(s.to_string());
    }
    let cfg = crate::config::load_raw_config_or_default()?;
    let db_name = crate::config::active_db_name(cli_db);
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
    /// Ask a yes/no question; return `true` for an affirmative answer.
    /// Default impl reads a line from stdin and accepts `y` / `yes`
    /// (case-insensitive). Used by the GenerationMismatch recovery
    /// prompt (da-74xx) — and any future interactive decision point.
    fn confirm(&mut self, prompt: &str) -> Result<bool> {
        use std::io::{BufRead, Write};
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt} [y/N]: ")?;
        stderr.flush()?;
        let stdin = std::io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let answer = line.trim().to_lowercase();
        Ok(answer == "y" || answer == "yes")
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

/// Scripted prompter for tests — passphrases and confirm answers come
/// from preloaded queues; status/login/confirm events are captured in
/// `events` for assertions.
#[cfg(any(test, feature = "test-util"))]
pub struct TestPrompter {
    pub passphrases: std::collections::VecDeque<String>,
    pub confirms: std::collections::VecDeque<bool>,
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
            confirms: std::collections::VecDeque::new(),
            events: Vec::new(),
        }
    }

    /// Queue an answer for the next `confirm(...)` call. Chainable for
    /// concise test setup.
    pub fn push_confirm(&mut self, answer: bool) -> &mut Self {
        self.confirms.push_back(answer);
        self
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
    fn confirm(&mut self, prompt: &str) -> Result<bool> {
        self.events.push(format!("confirm:{prompt}"));
        self.confirms
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("no scripted confirm answer for: {prompt}"))
    }
}
