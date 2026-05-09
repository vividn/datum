//! journalctl-friendly logging bootstrap.
//!
//! systemd user services capture stderr line-by-line and annotate each
//! line with unit metadata. The daemon uses `tracing` for structured
//! events and bridges them to stderr via `tracing-subscriber`. Output
//! format:
//!
//! ```text
//! 2026-04-22T15:06:12Z INFO datum_sync_agent::main started pid=12345 watching=3
//! 2026-04-22T15:06:13Z WARN datum_sync_agent::worker push_failed db=main err="relay 503"
//! ```
//!
//! Fields on each span / event are serialized inline. `RUST_LOG` is
//! honored for filter overrides.

use tracing_subscriber::{fmt, EnvFilter};

/// Install the global tracing subscriber. Safe to call exactly once.
pub fn init(verbose: bool) {
    let default = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_level(true)
        .compact()
        .try_init()
        .ok();
}
