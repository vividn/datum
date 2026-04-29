//! Protocol version constants. Bump these only when changing the wire
//! contract in a way that requires coordinated client/server upgrade.

/// Current protocol version. Encoded into HTTP paths (`/v1/...`) and into
/// the changeset magic prefix (`DSC1` — see [`crate::crypto::CHANGESET_MAGIC_V1`]).
pub const PROTOCOL_VERSION: &str = "v1";
