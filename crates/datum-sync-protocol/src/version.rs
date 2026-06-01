//! Protocol version constants. Bump these only when changing the wire
//! contract in a way that requires coordinated client/server upgrade.

/// Current protocol version, as an integer (da-p3m9). Comparisons like
/// `client_v < server_v` should be arithmetic, not string-parsed —
/// keeping the canonical value typed avoids each new caller re-doing
/// `"v0".trim_start_matches('v').parse()` (often buggily). Today no
/// downstream code does version comparison; this exists so the eventual
/// promotion to v1 (and any later bump) can add comparison logic without
/// an interim migration.
///
/// `0` is the pre-production, unstable version (da-sshi): the wire
/// contract may still change in place. It freezes and promotes to `1`
/// when sync ships to production.
pub const PROTOCOL_VERSION: u32 = 0;

/// HTTP-path-friendly form of [`PROTOCOL_VERSION`] (`"v0"`) — encoded
/// into route prefixes. Kept as a separate constant rather than computed
/// at every call site because (a) `format!` isn't `const`, and (b) the
/// literal form avoids pulling `const_format` into the dependency graph
/// for what is in practice a once-per-protocol-version change. The
/// lock-step test below pins it to [`PROTOCOL_VERSION`].
///
/// Orthogonal to the changeset payload-format version (the `DSC1` magic /
/// [`crate::crypto::CHANGESET_MAGIC_V1`]): that versions the encrypted
/// body, this versions the HTTP route surface; they move independently.
pub const PROTOCOL_VERSION_PATH: &str = "v0";

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
