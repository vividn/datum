//! HTTP client surface. Reqwest-based (blocking) — NOT WASM-safe.
//!
//! Everything in this module sits behind the `http` cargo feature. A future
//! WASM build should depend on `datum-sync-client` with
//! `--no-default-features --features wasm` and bring its own fetch shim.

pub mod device_flow;
pub mod keys;
pub mod push_pull;

pub use device_flow::{run_device_flow, DeviceFlowPrompter};
pub use keys::{fetch_wrapped, upload_wrapped};
