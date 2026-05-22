//! Protocol version constants. Bump these only when changing the wire
//! contract in a way that requires coordinated client/server upgrade.

/// Current protocol version, as an integer (da-p3m9). Comparisons like
/// `client_v < server_v` should be arithmetic, not string-parsed —
/// keeping the canonical value typed avoids each new caller re-doing
/// `"v1".trim_start_matches('v').parse()` (often buggily). Today no
/// downstream code does version comparison; this exists so the
/// eventual v2 bump can add comparison logic without an interim
/// migration.
pub const PROTOCOL_VERSION: u32 = 1;

/// HTTP-path-friendly form of [`PROTOCOL_VERSION`] (`"v1"`) — encoded
/// into route prefixes and referenced by the changeset magic prefix
/// (`DSC1` — see [`crate::crypto::CHANGESET_MAGIC_V1`]). Kept as a
/// separate constant rather than computed at every call site because
/// (a) `format!` isn't `const`, and (b) the literal form avoids
/// pulling `const_format` into the dependency graph for what is in
/// practice a once-per-protocol-version change. The lock-step test
/// below pins it to [`PROTOCOL_VERSION`].
pub const PROTOCOL_VERSION_PATH: &str = "v1";

#[cfg(test)]
mod tests {
    use super::*;

    /// Bumping [`PROTOCOL_VERSION`] without updating
    /// [`PROTOCOL_VERSION_PATH`] (or vice versa) MUST fail tests rather
    /// than ship a version where the integer disagrees with the path
    /// string — `format!` isn't const, so this is the cheapest check.
    #[test]
    fn version_and_path_agree() {
        let expected = format!("v{PROTOCOL_VERSION}");
        assert_eq!(
            PROTOCOL_VERSION_PATH, expected,
            "PROTOCOL_VERSION and PROTOCOL_VERSION_PATH must agree; \
             update both when bumping the protocol version",
        );
    }
}
