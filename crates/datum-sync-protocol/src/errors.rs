//! Wire-level error variants the relay and clients agree on.
//!
//! Distinct from [`crate::crypto::CryptoError`], which collapses all
//! crypto failures to a single opaque variant (padding-oracle hygiene).
//! Sync-control-plane errors are *not* secret — clients need to
//! distinguish them so the UX can react (e.g. prompt the user before
//! resetting the local watermark on a [`SyncProtocolError::GenerationMismatch`]).
//!
//! Bead da-soeu (sub-1 of the multi-DB epic da-u6lm): adding this enum
//! up-front so subsequent server / endpoint / client sub-beads pattern-
//! match against the same variant.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors the sync protocol surfaces over the wire. Variants serialize
/// as `{ "kind": "...", ...payload }` so clients can route by `kind`
/// regardless of which transport (HTTP status, websocket frame, queued
/// event) delivered them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncProtocolError {
    /// The client's view of the cluster generation disagrees with the
    /// server's authoritative value. Triggered when a cluster is
    /// reinitialised (factory reset, restore-from-encrypted-backup) so
    /// pre-reset changesets from other devices must be invalidated.
    ///
    /// Consumers (client + server) MUST treat this as terminal for the
    /// current sync attempt — the client resets its watermark and
    /// re-pulls from `generation = expected`. Old changesets stay on the
    /// server until garbage-collected by generation.
    #[error("generation mismatch: client {found}, server {expected}")]
    GenerationMismatch {
        /// The generation the server expected the client to be on
        /// (the cluster's current authoritative generation).
        expected: u64,
        /// The generation the client sent.
        found: u64,
    },
}

#[cfg(test)]
mod tests {
    //! Wire-format pin for [`SyncProtocolError`]. The tagged-enum shape
    //! lets clients route on `kind` regardless of transport, so a
    //! rename or shape change is a protocol-version-bumping event.

    use super::*;

    #[test]
    fn generation_mismatch_round_trips() {
        let err = SyncProtocolError::GenerationMismatch {
            expected: 7,
            found: 5,
        };
        let json = serde_json::to_string(&err).unwrap();
        // Tagged shape — clients route on `kind`.
        assert!(
            json.contains("\"kind\":\"generation_mismatch\""),
            "expected tagged kind, got: {json}",
        );
        assert!(json.contains("\"expected\":7"), "wire key: {json}");
        assert!(json.contains("\"found\":5"), "wire key: {json}");

        let back: SyncProtocolError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }

    #[test]
    fn generation_mismatch_display_names_both_sides() {
        let err = SyncProtocolError::GenerationMismatch {
            expected: 9,
            found: 4,
        };
        let msg = format!("{err}");
        // The display must surface BOTH sides so logs are debuggable
        // without parsing the structured payload.
        assert!(msg.contains("9"), "expected side missing: {msg}");
        assert!(msg.contains("4"), "found side missing: {msg}");
    }
}
