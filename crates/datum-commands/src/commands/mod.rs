//! One module per `datum` command.
//!
//! Every subcommand exposes a typed entry point (`<cmd>(args, db)`) and
//! `*_from_argv` / `*_from_string` stubs that R2 will flesh out once the
//! bespoke parser lands. For R1 the heavyweight handlers still live in
//! `crate::cmd`; these files are the thin, permanent public surface.

pub mod add;
pub mod alias;
pub mod backup;
pub mod check;
pub mod conflicts;
pub mod config;
pub mod count;
pub mod dayview;
pub mod del;
pub mod edit;
pub mod end;
pub mod get;
pub mod grep;
pub mod head;
pub mod history;
pub mod ids;
pub mod ignore;
pub mod info;
pub mod merge;
pub mod migrate;
pub mod now;
pub mod nowview;
pub mod occur;
pub mod rekey;
pub mod restore;
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
pub mod why;
