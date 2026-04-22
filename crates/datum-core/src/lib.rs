pub mod doc_ref;
pub mod entry;
pub mod field_spec;
pub mod db;
pub mod error;
pub mod ids;
pub mod slug;
pub mod state;
pub mod views;
pub mod migrations;
pub mod history;
pub mod conflicts;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

/// Re-export the pure time module from `datum-parse` so existing callers
/// (`datum_core::time::DatumTime`, etc.) keep compiling.
pub use datum_parse::time;
