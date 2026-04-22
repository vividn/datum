//! One module per `datum` command.
//!
//! R1 split: pure-data commands (check, sync) live here as first-class
//! modules. The monolithic log/retime/etc. handlers remain in the sibling
//! `cmd` module during R1 and will migrate file-by-file.

pub mod check;
pub mod sync;
