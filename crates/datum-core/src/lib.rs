#![doc = include_str!("../README.md")]

pub mod action;
pub mod db;
pub mod doc_ref;
pub mod entry;
pub mod error;
pub mod field_spec;
pub mod history;
pub mod ids;
pub mod integrity;
pub mod kit;
pub mod kit_color;
pub mod kit_color_rule;
pub mod last_docs;
pub mod migrations;
pub mod shape;
pub mod slug;
pub mod state;
pub mod sync_state;
pub mod views;

pub use verb::derive_verb;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

/// Re-export the pure modules that live in `datum-parse` so existing
/// callers (`datum_core::time::DatumTime`, `datum_core::dur::Dur`,
/// `datum_core::verb::derive_verb`, …) keep compiling.
///
/// They are defined there, not here, because they are pure — no database,
/// no I/O — and the browser reaches them through `datum-parse-wasm`
/// (da-7scm). One definition per rule; this crate is one of its callers.
pub use datum_parse::{dur, time, verb};
