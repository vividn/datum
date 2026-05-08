#![doc = include_str!("../README.md")]

//! datum commands — the command layer between argv parsing (in `datum-cli`
//! via `datum-parse`) and the core data model (`datum-core`).
//!
//! For R1 this crate holds:
//! * `args` — typed argument structs (migrated wholesale from `datum-cli`)
//! * `config` — XDG path resolution, db registry, datumrc.toml
//! * `render` — entry/table output formatting (formerly `output.rs`)
//! * `commands` — one module per command (check, sync, plus the legacy
//!   `cmd` dispatcher while R1 is in-flight)
//!
//! In R2 a bespoke parser powers each command, which grows typed +
//! string entry points. For R1 the arg structs in `args` double as the
//! typed API.

pub mod args;
pub mod cmd;
pub mod commands;
pub mod config;
pub mod render;
