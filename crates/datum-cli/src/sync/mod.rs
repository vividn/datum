//! `datum sync` subcommand tree.
//!
//! Three surfaces, all backed by the wire protocol in `docs/sync-protocol.md`:
//!
//! | Subcommand               | Purpose                                     |
//! |--------------------------|---------------------------------------------|
//! | `datum sync login`       | OAuth device-code flow → bearer token       |
//! | `datum sync config ...`  | Per-DB + global sync server management       |
//! | `datum sync key ...`     | Master-key init / unlock / rotate / show-fp |
//!
//! The relay URL comes from YAML config or an explicit `--server` override.

use anyhow::Result;
use std::time::Duration;

use crate::args::{SyncCommand, SyncConfigCommand, SyncKeyCommand};

pub mod config;
pub mod key;
pub mod login;

/// Token persisted per server, matching what `login` writes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub refreshed_at: String,
    pub server: String,
    #[serde(default)]
    pub user: Option<String>,
}

/// Entry point — dispatch `datum sync <subcommand>`.
pub fn dispatch(cmd: SyncCommand) -> Result<()> {
    match cmd {
        SyncCommand::Login(args) => login::run(args, &mut StdIoPrompter),
        SyncCommand::Config(sub) => match sub {
            SyncConfigCommand::SetServer { url, global } => {
                config::set_server(url, global)
            }
            SyncConfigCommand::Disable => config::disable(),
            SyncConfigCommand::Show => config::show(),
        },
        SyncCommand::Key(sub) => match sub {
            SyncKeyCommand::Init { server, force } => {
                key::init(server, force, &mut StdIoPrompter)
            }
            SyncKeyCommand::Unlock { server } => key::unlock(server, &mut StdIoPrompter),
            SyncKeyCommand::RotatePassphrase { server } => {
                key::rotate_passphrase(server, &mut StdIoPrompter)
            }
            SyncKeyCommand::ShowFingerprint { server } => {
                key::show_fingerprint(server)
            }
        },
    }
}

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
