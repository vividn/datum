//! An op refused, by the chain or for its row, and which failures abort a
//! receive instead (da-0c2ne, da-xjfol, da-tk8g3).

use super::*;

/// Is `e` about the moment, not the row: the lock, the disk, memory, the
/// file itself? Only such a failure aborts a receive (da-tk8g3). SQLite's
/// code is read, not its kind of failure, and any code not listed is the
/// row's, a plain `SQLITE_ERROR` among them (malformed JSON, a kit index's
/// overflow); so is an error with no SQLite code: `derive_verb`'s, serde's,
/// rusqlite's own. Its op is refused as unwritten, said by `sync status` and
/// `check sync`, and tried again by the next build, so a bug in the write
/// is loud and mended by its fix. `Io` is the moment's by where it arises
/// in a write: the file, never a row's value.
pub(super) fn of_the_moment(e: &DatumError) -> bool {
    use rusqlite::ErrorCode::*;
    match e {
        DatumError::Io(_) => true,
        DatumError::Db(e) => matches!(
            e.sqlite_error_code(),
            Some(
                DatabaseBusy
                    | DatabaseLocked
                    | SystemIoFailure
                    | DiskFull
                    | OutOfMemory
                    | ReadOnly
                    | OperationInterrupted
                    | DatabaseCorrupt
                    | NotADatabase
                    | CannotOpen
                    | FileLockingProtocolFailed
            )
        ),
        _ => false,
    }
}

/// The notice kind of an op whose row this copy did not write.
pub(crate) const UNWRITTEN: &str = "unwritten";

/// The `_meta` key naming the build that last refused an op's row.
pub(super) const UNWRITTEN_BY: &str = "sync_unwritten_by";

/// Refuse an op the chain does not allow, or whose row this copy does not
/// write: it is kept, applied = 2, so it is not asked for again,
/// and a membership that later allows it lets it wait again
/// ([`reconsider`]), as does another build for a row ([`settle`]). The
/// first refused of an endpoint files its sync notice of `kind`, `refused`
/// (da-0c2ne) or [`UNWRITTEN`], which `datum sync chain ack` settles — but
/// for a purged endpoint's, set aside on purpose (da-ry11c).
pub(super) fn refuse(
    conn: &Connection,
    chain: &Chain,
    op: &Op,
    why: &str,
    kind: &str,
    s: &mut ApplySummary,
) -> Result<()> {
    conn.execute(
        "UPDATE ops SET applied = 2 WHERE endpoint = ?1 AND seq = ?2",
        params![op.endpoint, op.seq],
    )?;
    // An op refused again in one settle (a row's, tried again after each
    // record) is said once; a chain's refusal never repeats there.
    let line = format!("{}/{}: {why}", op.endpoint, op.seq);
    if !s.refused.contains(&line) {
        s.refused.push(line);
    }
    let member = chain.member(&op.endpoint);
    // A purged endpoint's refusals are on purpose, and warn of nothing.
    if member.is_some_and(|m| m.purged.is_some()) {
        return Ok(());
    }
    // An agent's copy a later restore replaced, writing after it: its code is
    // in use in two places (da-wtdw8), which says more than a refusal.
    let revoked = member.and_then(|m| m.revoked.as_deref());
    let after = revoked.is_some_and(|r| r <= op.hlc.as_str());
    if kind == "refused"
        && after
        && chain::in_use_twice(conn, chain, &op.endpoint, chain::hlc_time(&op.hlc))?
    {
        return Ok(());
    }
    // A device outside the chain has no label in it: the host it says it is.
    let host = op.origin.as_ref().and_then(|o| o["host"].as_str());
    let label = member.map_or(host.unwrap_or(&op.endpoint[..10]), |m| &m.label);
    conn.prepare_cached(
        "INSERT OR IGNORE INTO sync_notices (subject, kind, label, at, via)
         VALUES (?1, ?5, ?2, ?3, ?4)",
    )?
    .execute(params![
        format!("{kind}:{}", op.endpoint),
        label,
        chain::hlc_time(&op.hlc),
        why,
        kind
    ])?;
    Ok(())
}
