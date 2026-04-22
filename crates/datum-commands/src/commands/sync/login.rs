//! `datum sync login` — CLI-side thin wrapper over
//! [`datum_sync_client::http::run_device_flow`]. This file owns only the
//! three CLI-side concerns:
//! 1. resolving which relay URL to talk to (flag > yaml config);
//! 2. bridging the CLI's `Prompter` trait to the client library's
//!    `DeviceFlowPrompter`;
//! 3. persisting the minted token under the XDG auth path (mode 0600).

use anyhow::{anyhow, Context, Result};
use datum_sync_client::http::{run_device_flow, DeviceFlowPrompter};
use datum_sync_client::protocol::messages::AuthToken;
use reqwest::blocking::Client;
use std::time::Duration;

use crate::args::SyncLoginArgs;
use crate::commands::sync::Prompter;
use crate::config::{auth_token_path, load_raw_config_or_default, server_host_from_url};

/// Production entry point.
pub fn run(args: SyncLoginArgs, prompter: &mut impl Prompter) -> Result<()> {
    let server = resolve_server(args.server.as_deref())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    run_with_client(&server, &client, prompter)?;
    Ok(())
}

/// Split from `run` so tests can inject a client pointed at a stub relay.
pub fn run_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<AuthToken> {
    let mut bridge = PrompterBridge { inner: prompter };
    let token = run_device_flow(server, client, &mut bridge)?;
    write_token(server, &token)?;
    Ok(token)
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

/// Persist the token atomically under `auth_token_path(host)`.
fn write_token(server: &str, token: &AuthToken) -> Result<()> {
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
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, body).with_context(|| format!("write {path:?}"))?;
        Ok(())
    }
}

/// Load a previously-saved token for `server`. Returns `None` if missing.
#[allow(dead_code)]
pub fn load_token(server: &str) -> Result<Option<AuthToken>> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read token at {path:?}"))?;
    let token: AuthToken =
        serde_json::from_slice(&bytes).context("parse stored auth token")?;
    Ok(Some(token))
}
