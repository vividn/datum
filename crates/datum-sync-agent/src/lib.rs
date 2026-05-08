#![doc = include_str!("../README.md")]

//! `datum-sync-agent` — background daemon that wraps `datum-sync-client` to
//! push-on-write and pull on a periodic interval without requiring the CLI
//! to be open.
//!
//! The library target (this file) only exposes **pure** scheduling /
//! debounce / backoff types so they can be unit-tested without an async
//! runtime or filesystem. The binary in `main.rs` wires them to a real
//! tokio runtime, a `notify`-based file watcher, and a pid-file.
//!
//! Shape:
//!
//! | Module       | Purpose                                              |
//! |--------------|------------------------------------------------------|
//! | `debounce`   | Quiet-window write coalescing                        |
//! | `backoff`    | Exponential retry schedule (5s → 15s → 60s → 5min)   |
//! | `scheduler`  | Per-DB next-action policy driving push / pull        |
//! | `liveness`   | PID-file + alive-check                               |
//! | `install`    | systemd / launchd unit generation (install-daemon)   |
//! | `log`        | tracing-subscriber bootstrap for the daemon          |
//! | `targets`    | Enumerate auto-sync DBs from `datumrc.toml`          |
//! | `status`     | Daemon-liveness readout for `datum sync status`      |

pub mod backoff;
pub mod debounce;
pub mod liveness;
pub mod log;
pub mod notify;
pub mod scheduler;
pub mod status;
pub mod targets;
