//! One module per `datum` command.
//!
//! Each subcommand exposes a typed entry point (`<cmd>(args, db)`) that
//! delegates to a heavyweight handler in `crate::cmd`. These files are the
//! thin, permanent public surface.

pub mod add;
pub mod alias;
pub mod backup_restore;
pub mod check;
pub mod config;
pub mod count;
pub mod dayview;
pub mod db;
pub mod del;
pub mod edit;
pub mod end;
pub mod get;
pub mod get_search;
pub mod grep;
pub mod head;
pub mod helpers;
pub mod history;
pub mod ids;
pub mod ignore;
pub mod info;
pub mod kit;
pub mod kit_command;
pub mod log_core;
pub mod merge;
pub mod meta;
pub mod migrate;
pub mod now;
pub mod nowview;
pub mod occur;
pub mod rekey;
pub mod render_helpers;
pub mod reparse;
pub mod retime;
pub mod setup;
pub mod spec;
pub mod start;
pub mod switch;
pub mod sync;
pub mod tail;
pub mod track;
pub mod undo;
pub mod update;
pub mod update_core;
pub mod view;
pub mod watch;
pub mod why;
