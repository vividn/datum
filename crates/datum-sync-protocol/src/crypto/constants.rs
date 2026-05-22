//! Wire-locked crypto constants. Every protocol-conforming implementation
//! must use these exact values; changing them requires a protocol-version
//! bump (see [`crate::version::PROTOCOL_VERSION`]).

/// AES-256-GCM standard nonce length (96 bits / 12 bytes). Every
/// protocol-conforming implementation MUST emit and accept exactly this
/// length; varying it would break decryption across clients regardless
/// of which AES-GCM library each side uses.
pub const NONCE_LEN: usize = 12;

/// Magic prefix for v1 changeset envelopes (`b"DSC1"`). Prepended to the
/// JSON payload before sealing — `pack_changeset` writes this; `apply_*`
/// rejects anything else.
pub const CHANGESET_MAGIC_V1: &[u8; 4] = b"DSC1";

/// Default Argon2id parameters for passphrase-based key wrap (matches the
/// proposal's recommended values: 64 MiB × 3 passes × p=1).
pub const ARGON2_DEFAULT_M_COST_KIB: u32 = 65536;
pub const ARGON2_DEFAULT_T_COST: u32 = 3;
pub const ARGON2_DEFAULT_P_COST: u32 = 1;
pub const ARGON2_DEFAULT_SALT_LEN: usize = 16;
