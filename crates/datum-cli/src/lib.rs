//! Library view of the `datum` CLI — lets integration tests dispatch through
//! the full arg-parse → command-dispatch path without spawning a subprocess.

pub mod args;
pub mod check;
pub mod cmd;
pub mod config;
pub mod output;
