//! The `integrity` checker: the pending stored warnings, the live repeated
//! states (da-acft) and the contested keys sync left (da-05rkv).

use std::collections::BTreeSet;

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::Pending;
use datum_core::time::DatumTime;
use serde_json::Value as JsonValue;

use super::kept::Plan;
use super::{
    render_flagged_block, render_state_for_flag, rows_with, Domain, FlaggedEntry, Issue,
    IssueExample, Severity, Source,
};

/// Surface pending rows from `integrity_warnings` (`resolution IS NULL`)
/// plus the live `repeated_state` computation. These are local
/// data-integrity findings (filed by the post-insert validator or
/// derived live from current entries), not cross-device sync
/// conflicts. The canonical scenario is a repeated state where the
/// user forgot to log an intermediate transition; the fix is usually
/// `datum switch <field> <state> -t -<n>` to backfill the missing
/// entry.
///
/// Resolved rows (`resolution IS NOT NULL`) are excluded — the user
/// already triaged them. The detection cost is already paid in the
/// insert path; this checker just makes the queue visible alongside
/// other `datum check` findings.
///
/// da-acft: `repeated_state` is computed live from the `entries`
/// table via [`integrity::list_repeated_states`] rather than read
/// from `integrity_warnings`. Stored same_state_repeat rows are no
/// longer written at insert (m27→28 cleans up stale ones), so when a
/// user backfills the missing transition or deletes the duplicate
/// the warning vanishes on the next `datum check` — there's no
/// stored row left behind.
#[cfg(test)]
pub(super) fn collect_integrity_warnings(db: &Db) -> Result<Vec<Issue>> {
    // da-0lful: what the dayview marks, read once for both.
    integrity_issues(db, datum_core::integrity::pending(db.conn(), None)?)
}

/// What the integrity checker reads (da-3zq6g): every field's repeats, or
/// those kept from the last run in fields unchanged since and those of the
/// fields changed, a field's repeats and their spans being read from its own
/// rows. And what to keep: each repeat, `[hid, from, to]`.
pub(super) fn integrity_pending(db: &Db, plan: &Plan) -> Result<(Pending, Vec<String>)> {
    use datum_core::integrity::{pending, Repeat};
    type Kept = (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let (mut found, kept) = match plan.since(db, "integrity") {
        None => (pending(db.conn(), None)?, Vec::new()),
        Some((kept, changed)) => {
            let fields: BTreeSet<String> = (changed.into_iter())
                .map(|(_, field, _)| field)
                .filter(|f| !f.is_empty())
                .collect();
            let kept: Vec<Kept> = kept
                .iter()
                .filter_map(|k| serde_json::from_str(k).ok())
                .collect();
            let hids: Vec<String> = kept.iter().map(|k| k.0.clone()).collect();
            let rows: std::collections::HashMap<String, Entry> =
                (db.entries_of(&hids)?.into_iter())
                    .map(|e| (e.hid.to_string(), e))
                    .collect();
            let unchanged = (kept.into_iter())
                .filter_map(|(hid, from, to)| {
                    let entry = rows.get(&hid)?.clone();
                    let span = from.map(|from| (from, to));
                    (!fields.contains(&entry.field)).then_some(Repeat { entry, span })
                })
                .collect();
            let fields: Vec<String> = fields.into_iter().collect();
            (pending(db.conn(), Some(&fields))?, unchanged)
        }
    };
    found.repeats.extend(kept);
    found.repeats.sort_by_cached_key(|r| {
        let e = &r.entry;
        (e.occur_time.as_ref().map(|t| t.utc), e.slug.to_string())
    });
    let kept = (found.repeats.iter())
        .filter_map(|r| {
            let (from, to) = r.span.map_or((None, None), |(from, to)| (Some(from), to));
            serde_json::to_string(&(r.entry.hid.to_string(), from, to)).ok()
        })
        .collect();
    Ok((found, kept))
}

/// The integrity checker's issues from what it read.
pub(super) fn integrity_issues(db: &Db, found: Pending) -> Result<Vec<Issue>> {
    use datum_core::integrity::{IntegrityKind, IntegrityWarning};
    // da-m34q7: a row that does not decode is a finding of its own, and the
    // rows that do are still checked.
    let mut pending = Vec::new();
    let mut unreadable = Vec::new();
    for row in found.stored {
        match row {
            Ok(warning) => pending.push(warning),
            Err((id, e)) => {
                // The decoder's own words, without SQLite's column wrapper.
                let why = std::error::Error::source(&e).map_or(e.to_string(), |s| s.to_string());
                unreadable.push(format!("  {id}: {why}"));
            }
        }
    }

    // Group pending warnings by kind; preserve insertion order within each
    // group so per-entry render output matches the queue order. A stored
    // SameStateRepeat is not among them — it's computed live below
    // (da-acft), and `pending` leaves out any row that slipped past the
    // cleanup migration.
    let mut by_kind: std::collections::BTreeMap<
        (&'static str, &'static str),
        Vec<&IntegrityWarning>,
    > = std::collections::BTreeMap::new();
    for w in &pending {
        by_kind
            .entry((w.kind.as_str(), w.detected_by.as_str()))
            .or_default()
            .push(w);
    }

    let mut issues = Vec::new();
    if !unreadable.is_empty() {
        let count = unreadable.len();
        issues.push(Issue {
            domain: Domain::Entries,
            severity: Severity::Error,
            category: "integrity_warning_unreadable",
            message: format!(
                "{count} stored integrity warning{s} cannot be read:\n{}",
                unreadable.join("\n"),
                s = if count == 1 { "" } else { "s" },
            ),
            count,
            example: None,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }
    for (_, group) in by_kind {
        let first = group[0];
        let count = group.len();
        let kind = first.kind;
        let message = match kind {
            IntegrityKind::SameStateRepeat => format!(
                "{count} entr{y} repeat the prior state on their field. \
                 Look at the surrounding data; the right fix is usually a backfill (missing transition) \
                 or a delete (true duplicate).",
                y = if count == 1 { "y" } else { "ies" },
            ),
            IntegrityKind::DoubleOpen => format!(
                "{count} field{s} opened a second time while already open",
                s = if count == 1 { "" } else { "s" },
            ),
            IntegrityKind::OrphanEnd => format!(
                "{count} close{s} arrived on a field with no prior open",
                s = if count == 1 { "" } else { "s" },
            ),
            IntegrityKind::Overlap => format!(
                "{count} block{s} overlap another block on the same field",
                s = if count == 1 { "" } else { "s" },
            ),
            IntegrityKind::LastStateMismatch => format!(
                "{count} entr{y} carried a --last-state assertion that disagrees with history",
                y = if count == 1 { "y" } else { "ies" },
            ),
        };
        let (domain, category) = Source::Integrity(kind).section();
        // da-mkes: collect one FlaggedEntry per warning. The state column
        // gets a leading `!` for `repeated_state` rows so the human render
        // shares the same marker the live renderer uses for repeated
        // switches (qfv0).
        let mark_repeat = matches!(kind, IntegrityKind::SameStateRepeat);
        let entries: Vec<FlaggedEntry> = group
            .iter()
            .map(|w| {
                let state_str = match w.local_entry.state.as_ref() {
                    Some(v) => render_state_for_flag(v),
                    None => String::new(),
                };
                let state = if mark_repeat {
                    format!("!{state_str}")
                } else {
                    state_str
                };
                FlaggedEntry {
                    hid: w.hid.clone(),
                    field: w.local_entry.field.clone(),
                    state,
                }
            })
            .collect();
        // da-cr4n: pre-render the flagged entries through the same table
        // formatter `datum tail` uses, so column alignment, state-cell
        // background colour, and the `!` repeated-state marker render
        // identically. Re-fetch each trigger by slug so the entry carries
        // the canonical post-pivot columns (occur_time, dur, etc.) —
        // `IntegrityWarning::local_entry` was reconstructed from the
        // table-stored JSON which `#[serde(skip)]`s the compat shims
        // (`start_time`, `entry_shape`, …) the renderer's extract path
        // still reads. `build_render_contexts` populates each row's
        // RenderContext from the field's state-event history, so
        // `is_state_repeat` fires on exactly the same shape as the
        // SameStateRepeat detector. Header suppressed — issue rows live
        // inside an already-introduced "[WARN]" block.
        let full_entries: Vec<Entry> = group
            .iter()
            .filter_map(|w| db.get_entry(w.entry_slug.as_str()).ok())
            .collect();
        let rendered_block = render_flagged_block(db, &full_entries);
        issues.push(Issue {
            domain,
            severity: Severity::Warning,
            category,
            message,
            count,
            example: Some(IssueExample {
                id: Some(first.hid.clone()),
                field: Some(first.local_entry.field.clone()),
                detail: Some(format!(
                    "trigger slug={slug}  detected={detected}",
                    slug = first.entry_slug,
                    detected = first.detected_at.to_rfc3339(),
                )),
            }),
            entries,
            rendered_block,
        });
    }

    // da-acft: repeated_state is computed live so backfilling the
    // missing transition (or deleting the duplicate) makes the
    // warning disappear on the next run — there's no stored row to
    // mark resolved.
    issues.extend(collect_repeated_state_live(db, found.repeats)?);
    issues.extend(collect_sync_conflicts(db)?);
    Ok(issues)
}

/// da-05rkv: the entries whose ops leave a key contested — read off their
/// `conflicts` column, which the sync fold keeps, so it is the same on every
/// copy and goes when an edit that saw both sides arrives.
fn collect_sync_conflicts(db: &Db) -> Result<Option<Issue>> {
    let conflicted = db.sync_conflicts()?;
    let Some(first) = conflicted.first() else {
        return Ok(None);
    };
    let count = conflicted.len();
    let (domain, category) = Source::Conflicts.section();
    let keys = |e: &Entry| {
        let c = e.conflicts.as_ref().and_then(JsonValue::as_object);
        let keys: Vec<&str> = c.into_iter().flatten().map(|(k, _)| k.as_str()).collect();
        keys.join(", ")
    };
    Ok(Some(Issue {
        domain,
        severity: Severity::Warning,
        category,
        message: format!(
            "{count} entr{y} hold{s} values two copies set without seeing each other's; \
             `datum conflicts` lists each side and resolves them.",
            y = if count == 1 { "y" } else { "ies" },
            s = if count == 1 { "s" } else { "" },
        ),
        count,
        example: Some(IssueExample {
            id: Some(first.short_uid()),
            field: Some(first.field.clone()),
            detail: Some(format!("contested: {}", keys(first))),
        }),
        entries: conflicted
            .iter()
            .map(|e| FlaggedEntry {
                hid: e.hid.to_string(),
                field: e.field.clone(),
                state: keys(e),
            })
            .collect(),
        rendered_block: render_flagged_block(db, &conflicted),
    }))
}

/// The live `repeated_state` rows (da-acft) as one issue (da-wdfsw): each
/// repeated switch with the span it sits in, longest first, so a block left
/// open for days stands above a double switch seconds apart (da-vwymi), then
/// each repeated end, which is usually a duplicate.
fn collect_repeated_state_live(
    db: &Db,
    repeats: Vec<datum_core::integrity::Repeat>,
) -> Result<Option<Issue>> {
    use datum_core::integrity::Repeat;
    use datum_parse::time::{format_human_duration, DurationPrecision};
    let (mut opens, closes): (Vec<Repeat>, Vec<Repeat>) =
        repeats.into_iter().partition(|r| r.span.is_some());
    let now = DatumTime::now().utc;
    type Span = (
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let secs = |(from, to): Span| (to.unwrap_or(now) - from).num_seconds();
    opens.sort_by_key(|r| std::cmp::Reverse(r.span.map_or(0, secs)));
    let clock = |t: chrono::DateTime<chrono::Utc>| {
        datum_core::time::Zone::host()
            .local_naive(t)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    };
    let issue = |repeats: Vec<Repeat>, category, message: &dyn Fn(usize) -> String| {
        let first = repeats.first()?;
        let count = repeats.len();
        let state = |e: &Entry| {
            e.state
                .as_ref()
                .map(render_state_for_flag)
                .unwrap_or_default()
        };
        // One line under each tail row: the span, said once for the human
        // and once for the JSON, its state on one line as the span's is.
        let spans: Vec<Option<String>> = repeats
            .iter()
            .map(|r| {
                r.span.map(|(from, to)| {
                    format!(
                        "{} for {}: {} → {}",
                        state(&r.entry).replace('\n', "\\n"),
                        format_human_duration(secs((from, to)), DurationPrecision::Minutes),
                        clock(from),
                        to.map_or("now".to_string(), clock),
                    )
                })
            })
            .collect();
        let entries: Vec<Entry> = repeats.iter().map(|r| r.entry.clone()).collect();
        let rendered_block = rows_with(db, &entries, &spans);
        Some(Issue {
            domain: Domain::Entries,
            severity: Severity::Warning,
            category,
            message: message(count),
            count,
            example: Some(IssueExample {
                id: Some(first.entry.short_uid()),
                field: Some(first.entry.field.clone()),
                detail: Some(format!("trigger slug={}", first.entry.slug)),
            }),
            entries: entries
                .iter()
                .zip(spans)
                .map(|(e, span)| FlaggedEntry {
                    hid: e.hid.to_string(),
                    field: e.field.clone(),
                    state: format!("!{}", span.unwrap_or_else(|| state(e))),
                })
                .collect(),
            rendered_block,
        })
    };
    let one = |count: usize| count == 1;
    opens.extend(closes);
    Ok(issue(opens, "repeated_state", &|count| {
        format!(
            "{count} entr{} repeat{} the state {} field was already in: each switch with the \
             span it sits in, longest first, then each end. Before a repeated switch a close or \
             switch is usually MISSING, and `stats` counts the whole span as that state until it \
             is backfilled (`datum end <field> -t <when>`, or `datum switch <field> <state> -t \
             <when>`). A repeated end is usually a duplicate, which changes no state or `stats`; `datum del <hid>` clears one.",
            if one(count) { "y" } else { "ies" },
            if one(count) { "s" } else { "" },
            if one(count) { "its" } else { "their" },
        )
    }))
}
