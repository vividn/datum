#![doc = include_str!("../README.md")]

pub mod action;
pub mod conflicts;
pub mod db;
pub mod doc_ref;
pub mod dur;
pub mod entry;
pub mod error;
pub mod field_spec;
pub mod history;
pub mod ids;
pub mod kit;
pub mod kit_color;
pub mod kit_color_rule;
pub mod last_docs;
pub mod migrations;
pub mod shape;
pub mod slug;
pub mod state;
pub mod verb;
pub mod views;

pub use verb::derive_verb;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

/// Re-export the pure time module from `datum-parse` so existing callers
/// (`datum_core::time::DatumTime`, etc.) keep compiling.
pub use datum_parse::time;
