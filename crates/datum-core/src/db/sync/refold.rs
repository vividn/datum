//! Refusing ops already applied, and folding their entries again without
//! them: a membership that moved ([`super::reconsider`]), or a create that
//! came to an entry a writer's ops were judged on (da-lgvu8).

use std::collections::BTreeSet;

use rusqlite::{params, Connection};

use super::chain::Chain;
use super::*;
use crate::error::Result;

/// Create `op` came to its entry: the other endpoints' ops on it are judged
/// again, a writer's by the creates now held, and the entry refolded
/// without those refused now (da-lgvu8). The creates judged by are every
/// one held, refused too ([`writer::creates_of`]), so they only grow, a
/// verdict only tightens, no verdict here depends on the order ops are
/// judged in, and every copy holding them ends alike. With no other
/// endpoint's op applied there is nothing to refuse.
pub(super) fn rejudge(
    conn: &Connection,
    chain: &Chain,
    op: &Op,
    s: &mut ApplySummary,
) -> Result<()> {
    let others = "SELECT 1 FROM ops WHERE hid = ?1 AND applied = 1 AND endpoint != ?2";
    if !conn
        .prepare_cached(others)?
        .exists(params![op.hid, op.endpoint])?
    {
        return Ok(());
    }
    let ops = ops_of(conn, &op.hid)?;
    let mut refused = false;
    for o in ops.iter().filter(|o| o.endpoint != op.endpoint) {
        if let Err(why) = writer::allowed(conn, chain, o)? {
            refuse(conn, chain, o, &why, "refused", s)?;
            refused = true;
        }
    }
    if refused {
        without(conn, BTreeSet::from([op.hid.clone()]))?;
    }
    Ok(())
}

/// Each of `hids` folded again from the ops still applied, after some were
/// refused. An entry left with no create is gone, and the rows merged into
/// it stand on their own again.
pub(super) fn without(conn: &Connection, hids: BTreeSet<String>) -> Result<()> {
    let mut touched = Vec::new();
    for hid in hids {
        let pre = fetch_by_hid(conn, &hid)?;
        touched.push(hid.clone());
        match fold(&ops_of(conn, &hid)?, None) {
            Some(r) => write_merged(conn, pre.as_ref(), entry_from(r)?, ChangeKind::SyncMerge)?,
            None => {
                let Some(pre) = pre else { continue };
                history::record(conn, &pre, ChangeKind::SyncMerge)?;
                let freed: Vec<Entry> = conn
                    .prepare_cached("SELECT * FROM entries WHERE superseded_by = ?1")?
                    .query_map(params![hid], row_to_entry)?
                    .collect::<rusqlite::Result<_>>()?;
                conn.execute("DELETE FROM entries WHERE hid = ?1", params![hid])?;
                // One at a time, each still pointing at the gone row — out of
                // the identity indexes — while `write_merged` decides it: two
                // freed rows may share an identity, and the later merges into
                // the earlier. One that stands alone points at nothing after.
                for mut f in freed {
                    touched.push(f.hid.0.clone());
                    f.superseded_by = None;
                    let own = own(conn, &f)?;
                    write_merged(conn, Some(&f), own, ChangeKind::SyncMerge)?;
                    conn.execute(
                        "UPDATE entries SET superseded_by = NULL WHERE hid = ?1 AND superseded_by = ?2",
                        params![f.hid, hid],
                    )?;
                }
                if let Some(kept) = &pre.superseded_by {
                    touched.push(kept.0.clone());
                    rederive(conn, kept.as_str(), ChangeKind::SyncMerge)?;
                }
            }
        }
    }
    for hid in touched {
        refresh_conflicts(conn, &hid)?;
    }
    Ok(())
}
