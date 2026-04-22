//! Changeset push + pull HTTP clients.
//!
//! Reserved for C4c (the wire-up bead). Leaving this as a scaffold so R4
//! closes the crate split without waiting on the C4c implementation.
//!
//! When filled in, this module owns:
//! - `POST /v1/push` — upload encrypted changesets; on success the relay
//!   echoes the assigned global `seq`, which the caller feeds into
//!   [`crate::sync_state::advance_push_seq`].
//! - `GET /v1/pull?since_seq=...` — fetch peers' changesets in ascending
//!   order; after applying each, advance the local pull watermark via
//!   [`crate::sync_state::advance_pull_seq`].
