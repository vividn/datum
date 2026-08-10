//! CLI-side cluster control: bump-generation roundtrips for `datum db
//! purge --cluster` / `datum db restore --cluster`. (da-ws1q sub-5 of
//! multi-DB epic da-u6lm.)
//!
//! Sits next to the sync push/pull commands rather than under
//! `commands/db/` so the HTTP plumbing (session, token refresh, server
//! resolution) lives alongside the rest of the network surface.

use anyhow::{anyhow, Context, Result};
use datum_sync_client::http::{self, Session, SessionError};

use super::{build_client, build_session, require_token, resolve_server};

/// Bump the cluster's generation on whichever server the active DB is
/// configured to sync against. Returns the new generation the server
/// landed on. The caller is responsible for stamping it into local
/// `sync_state` (the purge path needs to do that AFTER it has wiped
/// and re-created the table; the restore path can stamp directly).
///
/// Resolution mirrors `run_sync_with_client`: explicit `--server` flag
/// is not exposed for these one-shot ops yet — they always go to the
/// DB's configured default. (Pass `--server` to a follow-up `datum
/// sync` if you need to retarget.)
pub fn bump_generation_for_current_server(cluster_id: &str) -> Result<u64> {
    let server = resolve_server(None, None)?;
    let client = build_client()?;
    let token = require_token(&server)?;
    let session = build_session(&server, &token);
    let new_generation = run_bump(&client, &server, &session, cluster_id)
        .with_context(|| format!("bump cluster {cluster_id} generation against {server}"))?;
    Ok(new_generation)
}

/// Inner helper — split so tests can drive it directly with a Session
/// pointed at a stub relay. Translates [`SessionError`] into anyhow so
/// the CLI's existing error printer renders it uniformly with the
/// other commands.
pub fn run_bump(
    client: &reqwest::blocking::Client,
    server: &str,
    session: &Session,
    cluster_id: &str,
) -> Result<u64> {
    session
        .run(client, |bearer| {
            http::bump_generation(client, server, bearer, cluster_id)
        })
        .map_err(|e| match e {
            SessionError::GenerationMismatch(p) => {
                // Bumping is the SOURCE of mismatches; receiving one
                // here means the cluster was already bumped by another
                // device between our preflight and this call. Surface
                // it explicitly so the CLI message remains accurate.
                anyhow!("cluster generation mismatch — relay already on a newer generation: {p}")
            }
            other => anyhow::Error::new(other),
        })
}
