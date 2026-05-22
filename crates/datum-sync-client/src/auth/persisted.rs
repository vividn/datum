//! On-disk shape for auth tokens (da-ixmh).
//!
//! Wraps the wire-only [`AuthToken`] with the metadata the CLI adds at
//! persistence time: when the token was minted and which server it was
//! minted against. `serde(flatten)` keeps the on-disk JSON layout
//! identical to the legacy single-struct shape so existing
//! `$XDG_CONFIG_HOME/datum/auth/<host>.json` files keep parsing without
//! a migration step.
//!
//! The `Deref<Target = AuthToken>` impl lets call sites that today take
//! `&AuthToken` keep working when handed a `&PersistedAuthToken` — the
//! refactor moves the responsibility for "do you want the wire view or
//! the persisted view?" to the *type the caller asks for*, not the
//! fields they read.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::protocol::messages::AuthToken;

/// The on-disk auth token shape. The CLI writes one of these to
/// `auth/<host>.json` after every successful login or refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedAuthToken {
    /// Wire contract — flattened so the JSON keys stay at the top level
    /// (matching the legacy single-struct on-disk shape).
    #[serde(flatten)]
    pub wire: AuthToken,
    /// RFC 3339 timestamp set by the CLI when this token was minted or
    /// refreshed. Used by `datum sync status` to surface staleness.
    pub refreshed_at: String,
    /// Origin server URL the token was minted against. Lets callers
    /// sanity-check token / server mismatches before firing a request,
    /// and lets refresh paths reconstruct the URL without re-reading
    /// config.
    pub server: String,
}

impl PersistedAuthToken {
    /// Wrap a server-issued [`AuthToken`] with the CLI's persistence
    /// metadata. The `refreshed_at` stamp is `chrono::Utc::now()` so
    /// callers don't need to import chrono just to write a token.
    pub fn from_wire(wire: AuthToken, server: &str) -> Self {
        Self {
            wire,
            refreshed_at: chrono::Utc::now().to_rfc3339(),
            server: server.to_string(),
        }
    }

    /// Borrow the wire shape — for HTTP call sites that only need the
    /// bearer / refresh token / user_id, not the persistence metadata.
    pub fn as_wire(&self) -> &AuthToken {
        &self.wire
    }
}

/// Deref so existing `&AuthToken` consumers keep working when handed a
/// `&PersistedAuthToken` — accessing `.token`, `.user_id`, etc.
/// transparently reads through to the inner wire shape.
impl Deref for PersistedAuthToken {
    type Target = AuthToken;
    fn deref(&self) -> &AuthToken {
        &self.wire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `serde(flatten)` keeps the on-disk JSON identical to what the
    /// pre-da-ixmh single-struct shape produced. Pin the layout so a
    /// careless `#[serde(rename = ...)]` doesn't silently break every
    /// existing auth file.
    #[test]
    fn persisted_serializes_flat_with_legacy_layout() {
        let p = PersistedAuthToken {
            wire: AuthToken {
                token: "bearer".to_string(),
                user_id: "u-1".to_string(),
                refresh_token: "r-1".to_string(),
                cluster_id: Some("c-1".to_string()),
            },
            refreshed_at: "2026-05-22T12:00:00Z".to_string(),
            server: "https://relay.test".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&p).unwrap();
        // All keys at the top level — no `"wire": { ... }` envelope.
        assert!(
            json.get("token").is_some(),
            "token must be top-level: {json}"
        );
        assert!(json.get("user_id").is_some(), "user_id top-level: {json}");
        assert!(
            json.get("refresh_token").is_some(),
            "refresh_token top-level: {json}"
        );
        assert!(
            json.get("cluster_id").is_some(),
            "cluster_id top-level: {json}"
        );
        assert!(
            json.get("refreshed_at").is_some(),
            "refreshed_at top-level: {json}"
        );
        assert!(json.get("server").is_some(), "server top-level: {json}");
        assert!(json.get("wire").is_none(), "no wire envelope: {json}");
    }

    /// Round-trip: write then read returns the same struct field-by-field.
    /// Locks the on-disk format against subtle drift between the two paths.
    #[test]
    fn persisted_round_trips_through_json() {
        let p = PersistedAuthToken {
            wire: AuthToken {
                token: "bearer".to_string(),
                user_id: "u-1".to_string(),
                refresh_token: "r-1".to_string(),
                cluster_id: None,
            },
            refreshed_at: "2026-05-22T12:00:00Z".to_string(),
            server: "https://relay.test".to_string(),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: PersistedAuthToken = serde_json::from_str(&s).unwrap();
        assert_eq!(back.wire.token, p.wire.token);
        assert_eq!(back.wire.user_id, p.wire.user_id);
        assert_eq!(back.wire.refresh_token, p.wire.refresh_token);
        assert_eq!(back.wire.cluster_id, p.wire.cluster_id);
        assert_eq!(back.refreshed_at, p.refreshed_at);
        assert_eq!(back.server, p.server);
    }

    /// `serde(alias = "user")` survives the wrap: a legacy on-disk file
    /// that wrote `"user"` (not `"user_id"`) still parses into the new
    /// PersistedAuthToken via the wire type's alias.
    #[test]
    fn persisted_reads_legacy_user_alias() {
        let legacy = r#"{
            "token": "bearer",
            "user": "u-old",
            "refresh_token": "r",
            "refreshed_at": "2026-05-22T12:00:00Z",
            "server": "https://relay.test"
        }"#;
        let p: PersistedAuthToken = serde_json::from_str(legacy).unwrap();
        assert_eq!(p.wire.user_id, "u-old");
    }
}
