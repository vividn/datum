//! datum commands — the command layer between clap parsing (in `datum-cli`)
//! and the core data model (`datum-core`).
//!
//! For R1 this crate holds:
//! * `args` — clap argument definitions (migrated wholesale from `datum-cli`)
//! * `config` — XDG path resolution, db registry, datumrc.yml
//! * `render` — entry/table output formatting (formerly `output.rs`)
//! * `commands` — one module per command (check, sync, plus the legacy
//!   `cmd` dispatcher while R1 is in-flight)
//!
//! In R2 the bespoke parser replaces clap and each command grows typed +
//! string entry points. For R1 the clap structs in `args` double as the
//! typed API.

pub mod args;
pub mod cmd;
pub mod commands;
pub mod config;
pub mod render;
