//! `datum check` — database consistency checks.
//!
//! Runs a battery of SQL- and Rust-level sanity queries and produces a
//! grouped report. Errors set the process exit code to 1; warnings do not.

use anyhow::Result;
use datum_core::db::{Db, ListParams};
use datum_core::derive_verb;
use datum_core::entry::Entry;
use datum_core::field_spec::{self, InterpolationContext};
use datum_core::ids::Slug;
use datum_core::time::DatumTime;
use datum_parse::alias::{shell_split, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS};
use datum_parse::cli::KNOWN_CHECKERS;
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::args::CheckArgs;
use crate::commands::helpers::load_registry_opt;
use crate::commands::render_helpers::build_render_contexts;
use crate::render::{format_table_with, OutputArgs, TableArgs, TimeMetricCol};

/// Typed API for `datum check`. Exits the process with status 1 when the
/// report contains any [`Severity::Error`] items.
///
/// Positional checker names narrow the report: `dm check verbs` runs only
/// verb-drift detection; `dm check aliases` runs only spec-vs-user alias
/// divergence checks (state-name collisions, orphans, cycles). With no
/// positionals, the full default battery runs. `--fix` rewrites the
/// stored `verb` column for any drifted live row to match `derive_verb`
/// and, for the `aliases` checker, annotates each state collision with
/// the explicit first-saved-wins resolution (no DB mutation — precedence
/// is already deterministic).
pub fn check(args: CheckArgs, db: &Db) -> Result<()> {
    if args.list {
        if args.json {
            println!("{}", serde_json::to_string(KNOWN_CHECKERS)?);
        } else {
            for name in KNOWN_CHECKERS {
                println!("{name}");
            }
        }
        return Ok(());
    }

    let run_verbs = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "verbs");
    let run_aliases = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "aliases");
    let run_slugs = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "slugs");
    let run_integrity = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "integrity");
    let run_flags = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "flags");
    let run_tz = args.checkers.is_empty() || args.checkers.iter().any(|c| c == "tz");
    let run_default_battery = args.checkers.is_empty();

    let mut issues: Vec<Issue> = Vec::new();
    let mut fix_reports: Vec<FixReport> = Vec::new();
    if run_default_battery {
        issues.extend(collect_issues(db)?.issues);
        // da-adyj: legacy rows from before da-r569 carry a content_hash
        // that disagrees with `Entry::expected_content_hash()`. The
        // default battery already surfaces these as
        // `content_hash_mismatch`; under `--fix` we additionally
        // recompute and overwrite the column so dedup lookups (which
        // use the post-r569 hash) start finding the rows.
        //
        // da-4c1c follow-up: the fix originally reported a single
        // repair count and trusted the UPDATE. The new flow tracks
        // detect → apply → verify counts separately so a regression
        // where the rewrite silently no-ops (the bug Vremya hit on
        // real legacy data) prints a loud diagnostic instead of
        // pretending success.
        if args.fix {
            fix_reports.push(apply_content_hash_fixes(db)?);
        }
    }
    if run_verbs {
        let verb_report = collect_verb_drift(db)?;
        if args.fix {
            fix_reports.push(apply_verb_fixes(db, &verb_report.fixes)?);
        }
        issues.extend(verb_report.issues);
    }
    if run_aliases {
        let alias_report = collect_alias_drift(db, args.fix)?;
        issues.extend(alias_report.issues);
    }
    if run_slugs {
        let delim = crate::config::resolve_id_delimiter(None);
        let slug_report = collect_slug_drift(db, &delim)?;
        if args.fix {
            fix_reports.push(apply_slug_fixes(db, &slug_report.fixes, &delim)?);
        }
        issues.extend(slug_report.issues);
    }
    if run_integrity {
        issues.extend(collect_integrity_warnings(db)?);
    }
    if run_flags {
        issues.extend(collect_pending_flags(db)?);
    }
    if run_tz {
        issues.extend(collect_tz_drift(db)?);
    }
    let report = Report {
        issues,
        summary: Summary {
            errors: 0,
            warnings: 0,
            clean: false,
        },
    }
    .with_recomputed_summary();

    if args.json {
        println!("{}", render_json(&report)?);
    } else {
        print!("{}", render_human(&report));
        for fix in &fix_reports {
            print_fix_summary(fix);
        }
    }
    if report.has_errors() {
        std::process::exit(1);
    }
    Ok(())
}

/// da-4c1c: detect/apply/verify counters for a single `--fix` run.
/// Distinguishing them lets us catch the "applied but didn't stick"
/// regression that Vremya hit (the apply call returned Ok but the
/// re-read showed the row still mismatched). da-6xvh generalized the
/// struct from the content_hash applier to all three `--fix` appliers
/// so verb and slug repairs share the same loud error posture.
#[derive(Debug, Clone)]
struct FixReport {
    /// What this pass repairs (e.g. "content_hash") — names the
    /// summary line.
    label: &'static str,
    /// Next-step guidance printed after the leftover samples when the
    /// verify pass finds surviving mismatches.
    hint: &'static str,
    /// Rows the pre-fix scan decided needed repair.
    detected: usize,
    /// Rows the per-row apply call actually succeeded for.
    applied: usize,
    /// Rows the post-fix re-scan still found mismatched.
    still_mismatched: usize,
    /// First few (slug, stored, expected) tuples for the remaining
    /// mismatches — enough context for a hand-debug without dumping
    /// hundreds of rows.
    leftover_samples: Vec<(String, String, String)>,
}

/// da-6xvh: shared detect → apply → verify driver for the `--fix`
/// appliers. The error posture is deliberate (cf. da-sunq, where a
/// user-facing batch wants tx atomicity instead): these repairs are
/// idempotent and re-runnable, so per-row tolerance + post-verify +
/// loud report beats a transaction — one bad row must not abort the
/// rest of the batch, and the verify re-scan catches both rows whose
/// apply failed and rows whose apply "succeeded" but didn't stick.
fn run_fix_pass<F>(
    label: &'static str,
    hint: &'static str,
    fixes: &[F],
    mut apply_one: impl FnMut(&F) -> Result<()>,
    verify: impl FnOnce() -> Result<Vec<(String, String, String)>>,
) -> Result<FixReport> {
    let mut report = FixReport {
        label,
        hint,
        detected: fixes.len(),
        applied: 0,
        still_mismatched: 0,
        leftover_samples: Vec::new(),
    };
    for fix in fixes {
        // Per-row tolerance: a failed apply (e.g. NotFound after a
        // concurrent or cascading rekey) is counted by the verify
        // re-scan below instead of bailing the whole fix run.
        match apply_one(fix) {
            Ok(()) => report.applied += 1,
            Err(_) => { /* counted in verify pass below */ }
        }
    }
    let leftovers = verify()?;
    report.still_mismatched = leftovers.len();
    report.leftover_samples = leftovers.into_iter().take(3).collect();
    Ok(report)
}

/// da-adyj + da-4c1c: walk every row, rewrite the `content_hash`
/// column when the stored value disagrees with
/// [`Entry::expected_content_hash`], then re-walk to confirm the
/// rewrites took. Touches no other columns and writes no history row.
///
/// The two-pass design (detect → apply → verify) is the da-4c1c
/// upgrade: the original (da-adyj) one-pass trusted the UPDATE and
/// only printed the apply count, which silently hid the case where a
/// row's `expected_content_hash` came out differently across two reads
/// of the same data (the actual bug Vremya hit on real legacy rows).
/// Now we count remaining mismatches after the apply pass and print a
/// loud diagnostic if any survived.
fn apply_content_hash_fixes(db: &Db) -> Result<FixReport> {
    let scan = |db: &Db| -> Result<Vec<(String, String, String)>> {
        Ok(db
            .list_including_deleted()?
            .iter()
            .filter_map(|e| {
                let expected = e.expected_content_hash();
                (expected != e.content_hash)
                    .then(|| (e.slug.to_string(), e.content_hash.clone(), expected))
            })
            .collect())
    };
    let fixes = scan(db)?;
    run_fix_pass(
        "content_hash",
        "File a bug with the failing row's `state` / `data` JSON — \
         expected_content_hash() may be returning different values on \
         successive reads of the same row.",
        &fixes,
        |(slug, _, expected)| {
            db.set_entry_content_hash(slug, expected)?;
            Ok(())
        },
        || {
            // Truncate the 64-char hashes for the sample lines.
            Ok(scan(db)?
                .into_iter()
                .map(|(slug, stored, expected)| {
                    (
                        slug,
                        truncate_for_sample(&stored),
                        truncate_for_sample(&expected),
                    )
                })
                .collect())
        },
    )
}

/// Shorten a long opaque value (content hash) for a leftover-sample line.
fn truncate_for_sample(v: &str) -> String {
    format!("{}…", &v[..v.len().min(12)])
}

fn print_fix_summary(fix: &FixReport) {
    // Always print under `--fix` (even when detected == 0) so the user
    // can distinguish "ran and found nothing" from "didn't run."
    let entry_word = if fix.applied == 1 { "y" } else { "ies" };
    println!(
        "✓ {} repaired: {} entr{entry_word} ({} detected, {} still mismatched)",
        fix.label, fix.applied, fix.detected, fix.still_mismatched,
    );
    if fix.still_mismatched > 0 {
        eprintln!(
            "⚠ {} entr{} could not be repaired. Sample rows:",
            fix.still_mismatched,
            if fix.still_mismatched == 1 {
                "y"
            } else {
                "ies"
            },
        );
        for (slug, stored, expected) in &fix.leftover_samples {
            eprintln!("  - {slug}: stored={stored}  expected={expected}");
        }
        eprintln!("  {}", fix.hint);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueExample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One flagged entry, rendered inline under its issue in human output.
/// Carries enough to identify the row without re-querying the DB.
#[derive(Debug, Clone, Serialize)]
pub struct FlaggedEntry {
    /// Short 5-char hid prefix.
    pub hid: String,
    pub field: String,
    /// Pre-rendered state value (`!`-prefixed for `repeated_state` rows so
    /// the marker matches the renderer's repeated-state convention).
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<IssueExample>,
    /// All entries the issue applies to (one line each in human output).
    /// Empty for non-per-entry issues (e.g. summary categories).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<FlaggedEntry>,
    /// Pre-rendered tail-formatter block for the flagged entries (da-cr4n).
    /// Populated at collect time when [`entries`] is non-empty so the human
    /// renderer can embed the real `datum tail` columns (time / field /
    /// state / dur / hid, plus the `!` repeated-state marker) without
    /// re-querying the DB at render time. Skipped from JSON — programmatic
    /// consumers read [`entries`].
    #[serde(skip)]
    #[serde(default)]
    pub rendered_block: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub summary: Summary,
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.summary.errors > 0
    }

    /// Recompute the summary counts from the current `issues`. Used after
    /// merging verb-drift issues into a previously-built report.
    fn with_recomputed_summary(mut self) -> Self {
        let errors = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let warnings = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count();
        self.summary = Summary {
            errors,
            warnings,
            clean: errors == 0 && warnings == 0,
        };
        self
    }
}

/// One drifted row queued for `--fix` to rewrite. `stored_verb` is
/// carried only for the verify-pass sample lines (da-6xvh).
#[derive(Debug, Clone)]
struct VerbFix {
    slug: String,
    stored_verb: String,
    expected_verb: String,
}

#[derive(Debug, Default)]
struct VerbReport {
    issues: Vec<Issue>,
    fixes: Vec<VerbFix>,
}

/// Walk every live entry and compare its stored `verb` against
/// `derive_verb(state, dur, following_state)`. Produces:
///
/// * a `verb_drift` ERROR (with the count of drifted rows) when one or
///   more rows have a stored verb that differs from the derived verb,
/// * a `verb_corruption` ERROR when a row is in a shape that
///   `derive_verb` rejects per the truth table (e.g. `state=NULL`,
///   `dur=NULL`, `following_state=set`). Such rows should never reach
///   the DB; their presence is a corruption signal.
///
/// Drifted rows are accumulated into `fixes` so `--fix` can rewrite
/// them in a single pass. Corruption-shape rows are *not* fixed —
/// there is no derivable verb to write.
fn collect_verb_drift(db: &Db) -> Result<VerbReport> {
    let entries = db.list_including_deleted()?;

    let mut drift_count: usize = 0;
    let mut drift_example: Option<IssueExample> = None;
    let mut corrupt_count: usize = 0;
    let mut corrupt_example: Option<IssueExample> = None;
    let mut fixes: Vec<VerbFix> = Vec::new();

    for e in &entries {
        // Skip tombstones — they are historical rows, not live data;
        // reviving one through restore re-runs the insert path and
        // re-derives the verb.
        if e.deleted_at.is_some() {
            continue;
        }
        match derive_verb(e.state.clone(), e.dur.as_ref(), e.following_state.clone()) {
            Ok((expected, _canonical_state)) => {
                let stored = e.verb.to_string();
                let expected_str = expected.to_string();
                if stored != expected_str {
                    drift_count += 1;
                    if drift_example.is_none() {
                        drift_example = Some(IssueExample {
                            id: Some(e.slug.to_string()),
                            field: Some(e.field.clone()),
                            detail: Some(format!("stored={stored}  expected={expected_str}")),
                        });
                    }
                    fixes.push(VerbFix {
                        slug: e.slug.to_string(),
                        stored_verb: stored,
                        expected_verb: expected_str,
                    });
                }
            }
            Err(err) => {
                corrupt_count += 1;
                if corrupt_example.is_none() {
                    corrupt_example = Some(IssueExample {
                        id: Some(e.slug.to_string()),
                        field: Some(e.field.clone()),
                        detail: Some(err.to_string()),
                    });
                }
            }
        }
    }

    let mut issues = Vec::new();
    if drift_count > 0 {
        issues.push(Issue {
            severity: Severity::Error,
            category: "verb_drift",
            message: format!(
                "{} entr{} have a stored verb that disagrees with derive_verb",
                drift_count,
                if drift_count == 1 { "y" } else { "ies" },
            ),
            count: drift_count,
            example: drift_example,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }
    if corrupt_count > 0 {
        issues.push(Issue {
            severity: Severity::Error,
            category: "verb_corruption",
            message: format!(
                "{} entr{} are in a shape derive_verb rejects \
                 (state/dur/following_state truth-table violation)",
                corrupt_count,
                if corrupt_count == 1 { "y" } else { "ies" },
            ),
            count: corrupt_count,
            example: corrupt_example,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    Ok(VerbReport { issues, fixes })
}

/// Rewrite the stored `verb` column for each queued drifted row, then
/// re-scan to confirm the rewrites took (da-6xvh — the da-4c1c
/// detect/apply/verify posture, shared via [`run_fix_pass`]).
fn apply_verb_fixes(db: &Db, fixes: &[VerbFix]) -> Result<FixReport> {
    run_fix_pass(
        "verb",
        "File a bug with the failing row's `state` / `dur` / \
         `following_state` — derive_verb may be returning different \
         values on successive reads of the same row.",
        fixes,
        |fix| {
            db.set_entry_verb(&fix.slug, &fix.expected_verb)?;
            Ok(())
        },
        || {
            Ok(collect_verb_drift(db)?
                .fixes
                .into_iter()
                .map(|f| (f.slug, f.stored_verb, f.expected_verb))
                .collect())
        },
    )
}

/// One drifted row queued for `--fix` to rewrite. Carries the existing
/// slug so the rekey path can find the row. `expected` is carried only
/// for the verify-pass sample lines (da-6xvh) — the rekey itself
/// re-derives the slug through the auto-reinterp path.
#[derive(Debug, Clone)]
struct SlugFix {
    existing: Slug,
    expected: String,
}

#[derive(Debug, Default)]
struct SlugReport {
    issues: Vec<Issue>,
    fixes: Vec<SlugFix>,
}

/// Walk every live entry and compare its stored `slug` to the slug
/// recomputed from the row's templates:
///
/// * `field = interpolate(field_structure, ctx, delim)` if
///   `field_structure` is set, else the stored `field`.
/// * `body  = interpolate(id_structure, ctx, delim)` if `id_structure`
///   is set, else `None` and the row is skipped (no template to
///   recompute against).
/// * `expected = "<field>:<body>"`.
///
/// A row drifts when `stored_slug != expected`. Drifted rows are
/// accumulated into `fixes` so `--fix` can rekey them in a single pass.
/// Tombstones are skipped — they are historical rows and re-deriving
/// their slug would produce a meaningless rekey.
fn collect_slug_drift(db: &Db, delim: &str) -> Result<SlugReport> {
    let entries = db.list_including_deleted()?;

    let mut drift_count: usize = 0;
    let mut drift_example: Option<IssueExample> = None;
    let mut fixes: Vec<SlugFix> = Vec::new();

    for e in &entries {
        if e.deleted_at.is_some() {
            continue;
        }
        // Skip rows that carry no structure at all — there is nothing to
        // recompute the slug against.
        if e.id_structure.is_none() && e.field_structure.is_none() {
            continue;
        }
        let uid = e.short_uid();
        let ctx = InterpolationContext::from_entry(e, &uid);

        let expected_field = match &e.field_structure {
            Some(tmpl) => field_spec::interpolate(tmpl, &ctx, delim),
            None => e.field.clone(),
        };
        let Some(id_tmpl) = e.id_structure.as_ref() else {
            // field_structure-only rows can still drift on the field
            // half; build the slug body from the existing stored body
            // (everything after the first `:`). If the stored slug has
            // no `:` the row is malformed — skip it.
            let Some((_, body)) = e.slug.as_str().split_once(':') else {
                continue;
            };
            let expected = format!("{expected_field}:{body}");
            let stored = e.slug.to_string();
            if stored != expected {
                drift_count += 1;
                if drift_example.is_none() {
                    drift_example = Some(IssueExample {
                        id: Some(stored.clone()),
                        field: Some(e.field.clone()),
                        detail: Some(format!("stored={stored}  expected={expected}")),
                    });
                }
                fixes.push(SlugFix {
                    existing: e.slug.clone(),
                    expected,
                });
            }
            continue;
        };
        let body = field_spec::interpolate(id_tmpl, &ctx, delim);
        let expected = format!("{expected_field}:{body}");
        let stored = e.slug.to_string();
        if stored != expected {
            drift_count += 1;
            if drift_example.is_none() {
                drift_example = Some(IssueExample {
                    id: Some(stored.clone()),
                    field: Some(e.field.clone()),
                    detail: Some(format!("stored={stored}  expected={expected}")),
                });
            }
            fixes.push(SlugFix {
                existing: e.slug.clone(),
                expected,
            });
        }
    }

    let mut issues = Vec::new();
    if drift_count > 0 {
        issues.push(Issue {
            severity: Severity::Error,
            category: "slug_drift",
            message: format!(
                "{} entr{} have a stored slug that disagrees with its templated id/field structure",
                drift_count,
                if drift_count == 1 { "y" } else { "ies" },
            ),
            count: drift_count,
            example: drift_example,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    Ok(SlugReport { issues, fixes })
}

/// Rekey each drifted row by re-running the standard auto-reinterp
/// path. This mirrors the update path (`save_existing_entry_with_auto_reinterp`)
/// so the fix produces the same slug a future write would, and so
/// history rows are stamped consistently. `hid` is preserved.
///
/// da-6xvh: slug fixes can self-interfere — `fixes` carries pre-scan
/// slugs, so an earlier rekey in this same loop can invalidate the
/// `existing` slug a later fix references (get_entry → NotFound).
/// [`run_fix_pass`] tolerates the per-row failure instead of aborting
/// mid-repair; the verify re-scan counts whatever is still drifted and
/// the hint tells the user a re-run picks it up (repairs are
/// idempotent).
fn apply_slug_fixes(db: &Db, fixes: &[SlugFix], delim: &str) -> Result<FixReport> {
    run_fix_pass(
        "slug",
        "Re-run `datum check --fix` — slug rekeys can cascade, so a row \
         whose pre-scan slug was invalidated by an earlier rekey is \
         picked up on the next pass.",
        fixes,
        |fix| {
            let entry = db.get_entry(&fix.existing)?;
            db.save_existing_entry_with_auto_reinterp(&fix.existing, entry, delim)?;
            Ok(())
        },
        || {
            Ok(collect_slug_drift(db, delim)?
                .fixes
                .into_iter()
                .map(|f| (f.existing.to_string(), f.existing.to_string(), f.expected))
                .collect())
        },
    )
}

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
fn collect_integrity_warnings(db: &Db) -> Result<Vec<Issue>> {
    use datum_core::integrity::{IntegrityKind, IntegrityWarning, ListFilter};
    let pending = db.list_integrity_warnings(&ListFilter {
        kind: None,
        pending: Some(true),
    })?;

    // Group pending warnings by kind; preserve insertion order within each
    // group so per-entry render output matches the queue order. SameStateRepeat
    // is intentionally skipped — it's computed live below (da-acft); any rows
    // that slipped past the cleanup migration are ignored here.
    let mut by_kind: std::collections::BTreeMap<&'static str, Vec<&IntegrityWarning>> =
        std::collections::BTreeMap::new();
    for w in &pending {
        if matches!(w.kind, IntegrityKind::SameStateRepeat) {
            continue;
        }
        by_kind.entry(w.kind.as_str()).or_default().push(w);
    }

    let mut issues = Vec::new();
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
            IntegrityKind::LwwOverwrite => format!(
                "{count} local edit{s} {were} overwritten by a newer remote edit during sync",
                s = if count == 1 { "" } else { "s" },
                were = if count == 1 { "was" } else { "were" },
            ),
        };
        let category: &'static str = match kind {
            IntegrityKind::SameStateRepeat => "repeated_state",
            IntegrityKind::DoubleOpen => "integrity_double_open",
            IntegrityKind::OrphanEnd => "integrity_orphan_end",
            IntegrityKind::Overlap => "integrity_overlap",
            IntegrityKind::LastStateMismatch => "integrity_last_state_mismatch",
            IntegrityKind::LwwOverwrite => "integrity_lww_overwrite",
        };
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
    if let Some(issue) = collect_repeated_state_live(db)? {
        issues.push(issue);
    }
    Ok(issues)
}

/// Live counterpart of the stored integrity-warning surface for the
/// `repeated_state` category (da-acft). Walks `entries` through
/// [`integrity::list_repeated_states`] and builds the same Issue
/// shape — category, count, example, FlaggedEntry list, rendered
/// block — that the legacy stored-row path emitted.
fn collect_repeated_state_live(db: &Db) -> Result<Option<Issue>> {
    let triggers = datum_core::integrity::list_repeated_states(db.conn())?;
    if triggers.is_empty() {
        return Ok(None);
    }
    let count = triggers.len();
    let message = format!(
        "{count} entr{y} repeat the prior state on their field. \
         Look at the surrounding data; the right fix is usually a backfill (missing transition) \
         or a delete (true duplicate).",
        y = if count == 1 { "y" } else { "ies" },
    );
    let entries: Vec<FlaggedEntry> = triggers
        .iter()
        .map(|e| {
            let state_str = match e.state.as_ref() {
                Some(v) => render_state_for_flag(v),
                None => String::new(),
            };
            FlaggedEntry {
                hid: e.short_uid(),
                field: e.field.clone(),
                state: format!("!{state_str}"),
            }
        })
        .collect();
    let first = &triggers[0];
    let example = Some(IssueExample {
        id: Some(first.short_uid()),
        field: Some(first.field.clone()),
        detail: Some(format!("trigger slug={slug}", slug = first.slug)),
    });
    let rendered_block = render_flagged_block(db, &triggers);
    Ok(Some(Issue {
        severity: Severity::Warning,
        category: "repeated_state",
        message,
        count,
        example,
        entries,
        rendered_block,
    }))
}

/// da-ghlx: surface every live row whose `extra_metadata.flag` is JSON
/// `true` as a `flagged_entry` warning. These rows are the `datum flag
/// <field> [comment]` review markers (da-dnnn) — they live in the DB
/// alongside tracked data but are hidden from the default tail /
/// dayview / nowview surface, so `datum check flags` is the canonical
/// way to remind the user that pending reviews exist.
///
/// A single roll-up issue is emitted (one `flagged_entry` row, count =
/// #flagged), with the rendered block carrying one line per flagged
/// entry through the real tail formatter so column alignment matches
/// the live view (da-cr4n).
fn collect_pending_flags(db: &Db) -> Result<Vec<Issue>> {
    // Pull the full set of flagged rows in one go. `include_meta = true`
    // bypasses the default-hide filter (da-ghlx) so this query actually
    // returns flagged rows; we still post-filter to `extra_metadata.flag
    // = true` because `include_meta = true` *also* exposes
    // `extra_metadata.meta = true` rows (those belong to the parallel
    // `meta` notes surface — da-xn8j — not to the flag-review queue).
    let params = ListParams {
        metric: datum_core::db::TimeMetric::Hybrid,
        include_meta: true,
        ..Default::default()
    };
    let flagged: Vec<Entry> = db
        .list_entries(&params)?
        .into_iter()
        .filter(|e| {
            e.extra_metadata
                .as_ref()
                .and_then(|m| m.get("flag"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .collect();
    if flagged.is_empty() {
        return Ok(Vec::new());
    }
    let count = flagged.len();
    let message = format!(
        "{count} entr{y} flagged for review. Inspect with `datum check flags`; \
         clear by deleting or updating the row.",
        y = if count == 1 { "y" } else { "ies" },
    );
    let first = &flagged[0];
    let example = Some(IssueExample {
        id: Some(first.slug.to_string()),
        field: Some(first.field.clone()),
        detail: first
            .extra_metadata
            .as_ref()
            .and_then(|m| m.get("comment"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    });
    // FlaggedEntry triplet for the JSON output — same shape as the
    // integrity-warning rows so consumers don't branch on category.
    let entries: Vec<FlaggedEntry> = flagged
        .iter()
        .map(|e| FlaggedEntry {
            hid: e.short_uid(),
            field: e.field.clone(),
            // Surface the comment if present, otherwise leave empty.
            // Render-block (below) carries the canonical row layout.
            state: e
                .extra_metadata
                .as_ref()
                .and_then(|m| m.get("comment"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    let rendered_block = render_flagged_block(db, &flagged);
    Ok(vec![Issue {
        severity: Severity::Warning,
        category: "flagged_entry",
        message,
        count,
        example,
        entries,
        rendered_block,
    }])
}

/// Recompute each live entry's `occur_time` offset from its `timezone` at
/// `utc` and flag any row whose stored `offset_minutes` disagrees — the
/// `tz` checker. A `DatumTime`'s three fields are not independent
/// (`crates/datum-parse/src/time/datum_time.rs`): `offset_minutes` must
/// equal the offset `timezone` produces at `utc`. v3 derives the offset at
/// construction so a freshly-written row is always in sync, but a legacy
/// import, a sync merge, a hand-edited DB row, or a time mutation that
/// skipped `refresh_offset` can leave a row desynced — and a wrong stored
/// offset renders as a wrong wall-clock time, the cardinal error for a
/// time tracker. This is the `check`-time complement to v3's
/// prevention-by-construction (and the replacement for v2's dropped
/// read-time mismatch warn).
///
/// A row with an unparseable `timezone` is also flagged here: the offset
/// can't be verified and `local_time` is meaningless until the name is
/// fixed. Tombstones are skipped — historical rows, not live data.
///
/// Report-only: the "which field is authoritative — tz or offset?" call
/// deserves a human, so there is deliberately no `--fix` re-derivation yet
/// (a future `--fix` would trust `timezone` and re-run `refresh_offset`).
fn collect_tz_drift(db: &Db) -> Result<Vec<Issue>> {
    let entries = db.list_including_deleted()?;

    let mut flagged: Vec<Entry> = Vec::new();
    let mut example: Option<IssueExample> = None;
    for e in &entries {
        if e.deleted_at.is_some() {
            continue;
        }
        let Some(occur) = e.occur_time.as_ref() else {
            continue;
        };
        let detail = match occur.expected_offset_minutes() {
            Ok(expected) if expected == occur.offset_minutes => continue,
            Ok(expected) => format!(
                "tz={tz}  stored_offset={stored}  expected_offset={expected}",
                tz = occur.timezone,
                stored = occur.offset_minutes,
            ),
            Err(err) => format!(
                "tz={tz}  stored_offset={stored}  {err}",
                tz = occur.timezone,
                stored = occur.offset_minutes,
            ),
        };
        if example.is_none() {
            example = Some(IssueExample {
                id: Some(e.slug.to_string()),
                field: Some(e.field.clone()),
                detail: Some(detail),
            });
        }
        flagged.push(e.clone());
    }

    if flagged.is_empty() {
        return Ok(Vec::new());
    }
    let count = flagged.len();
    let entries_out: Vec<FlaggedEntry> = flagged
        .iter()
        .map(|e| FlaggedEntry {
            hid: e.short_uid(),
            field: e.field.clone(),
            // occur_time is always Some here (filtered above); show the
            // stored→expected offset delta so the row's problem is legible
            // without re-reading the DB.
            state: e
                .occur_time
                .as_ref()
                .map(|t| match t.expected_offset_minutes() {
                    Ok(exp) => format!("{}→{exp}", t.offset_minutes),
                    Err(_) => format!("{}→? (bad tz)", t.offset_minutes),
                })
                .unwrap_or_default(),
        })
        .collect();
    let rendered_block = render_flagged_block(db, &flagged);
    Ok(vec![Issue {
        severity: Severity::Error,
        category: "tz_offset_mismatch",
        message: format!(
            "{count} entr{y} have a stored UTC offset that disagrees with their timezone at that instant",
            y = if count == 1 { "y" } else { "ies" },
        ),
        count,
        example,
        entries: entries_out,
        rendered_block,
    }])
}

/// Format flagged entries through the live tail table formatter. Returns the
/// fully rendered text block (one line per entry, no header). Empty input
/// yields an empty string so callers can `if !block.is_empty()` to decide
/// whether to emit a sub-section.
fn render_flagged_block(db: &Db, entries: &[Entry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let ctxs = build_render_contexts(entries, db);
    let registry = load_registry_opt(db);
    let now = DatumTime::now();
    let tbl = TableArgs {
        output: OutputArgs::default(),
        metric: TimeMetricCol::Hybrid,
        columns: Vec::new(),
        suppress_header: true,
        max_total_width: None,
    };
    format_table_with(entries, Some(&ctxs), Some(&now), registry.as_ref(), &tbl).unwrap_or_default()
}

/// Render a state JSON value into the short string used in flagged-entry
/// lines: scalar bool/number/string unwrapped, anything structured
/// serialized compactly. The shape mirrors what the live renderer prints
/// in the state column.
fn render_state_for_flag(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Default)]
struct AliasReport {
    issues: Vec<Issue>,
}

/// Walk every field spec and every user/kit alias to surface three
/// divergence classes:
///
/// * `alias_state_collision` — two specs declare the same state name as a
///   derived `State` alias. The [`SpecAliasIndex`] resolves these
///   deterministically by `field_specs.rowid` (UPSERT preserves rowid, so
///   "first-saved wins"); the loser's derived alias is shadowed and is
///   reachable only via its owning spec's bare-field expansion. Pre-v21
///   the loser's row was silently overwritten in the materialized aliases
///   table, hence "first-saved-wins" was a silent collision (Appendix C
///   jade + garnet retros).
/// * `alias_orphan` — a user/kit alias's expansion chain ends in a name
///   that is neither a known subcommand nor any other resolvable alias.
///   Typical cause: a downstream alias's owning spec was deleted.
/// * `alias_cycle` — expanding a user/kit alias revisits a name in the
///   chain. The `expand_argv` pre-pass would bail at
///   [`MAX_ALIAS_HOPS`], but the cycle would otherwise go undetected.
///
/// `--fix` (`fix_collisions = true`) annotates state-collision examples
/// with the explicit first-saved-wins resolution. There is no DB
/// mutation: the precedence is already deterministic and forcing a
/// rebuild of the index would produce the same outcome. Orphans and
/// cycles need user judgment per the bead direction and are report-only.
fn collect_alias_drift(db: &Db, fix_collisions: bool) -> Result<AliasReport> {
    let mut issues: Vec<Issue> = Vec::new();

    // 1. Cross-spec state-name collisions (already tracked by the index).
    let collisions: Vec<_> = {
        let index = db.spec_alias_index()?;
        index.collisions.clone()
    };
    if !collisions.is_empty() {
        let first = &collisions[0];
        let detail = if fix_collisions {
            format!(
                "alias '{name}' kept by '{winner}' (first-saved-wins); displaced from '{loser}'",
                name = first.name,
                winner = first.winner,
                loser = first.loser,
            )
        } else {
            format!(
                "alias '{name}' owned by '{winner}'; '{loser}' displaced",
                name = first.name,
                winner = first.winner,
                loser = first.loser,
            )
        };
        issues.push(Issue {
            severity: Severity::Warning,
            category: "alias_state_collision",
            message: format!(
                "{n} cross-spec state-name collision{s} (first-saved precedence applied)",
                n = collisions.len(),
                s = if collisions.len() == 1 { "" } else { "s" },
            ),
            count: collisions.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(first.loser.clone()),
                detail: Some(detail),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    // 1b. Spec-v2 migration (docs/trailing-and-slots.md): a literal `--`
    // in a stored alias body used to mean promote-to-all; it now means
    // unix end-of-flags. Flag any body still carrying one so the user
    // can decide whether it wants a `,,` rewrite.
    {
        let legacy: Vec<datum_core::field_spec::Alias> = db
            .list_user_aliases()?
            .into_iter()
            .filter(|a| {
                datum_parse::alias::shell_split(&a.expands_to)
                    .iter()
                    .any(|t| t == "--")
            })
            .collect();
        if !legacy.is_empty() {
            let first = &legacy[0];
            issues.push(Issue {
                severity: Severity::Warning,
                category: "alias_legacy_double_dash",
                message: format!(
                    "{n} alias bod{ies} contain{s} `--`, which is now unix \
                     end-of-flags (spec v2); rewrite with `,,` if shared \
                     trailing was intended",
                    n = legacy.len(),
                    ies = if legacy.len() == 1 { "y" } else { "ies" },
                    s = if legacy.len() == 1 { "s" } else { "" },
                ),
                count: legacy.len(),
                example: Some(IssueExample {
                    id: None,
                    field: None,
                    detail: Some(format!(
                        "alias '{}' → datum {}",
                        first.name, first.expands_to
                    )),
                }),
                entries: Vec::new(),
                rendered_block: String::new(),
            });
        }
    }

    // 2 + 3. Orphans and cycles in user/kit alias chains. Spec-derived
    // aliases never need this walk — their `expands_to` always begins with
    // a KNOWN_SUBCOMMAND.
    let user_aliases = db.list_user_aliases()?;
    let mut orphan_count: usize = 0;
    let mut orphan_example: Option<IssueExample> = None;
    let mut cycle_count: usize = 0;
    let mut cycle_example: Option<IssueExample> = None;

    for alias in &user_aliases {
        match analyze_chain(db, &alias.name)? {
            ChainOutcome::Resolved => {}
            ChainOutcome::Orphan { broken_at, chain } => {
                orphan_count += 1;
                if orphan_example.is_none() {
                    orphan_example = Some(IssueExample {
                        id: None,
                        field: Some(alias.name.clone()),
                        detail: Some(format!(
                            "{path} → '{broken_at}' (no known alias or subcommand)",
                            path = chain.join(" → "),
                        )),
                    });
                }
            }
            ChainOutcome::Cycle(path) => {
                cycle_count += 1;
                if cycle_example.is_none() {
                    cycle_example = Some(IssueExample {
                        id: None,
                        field: Some(alias.name.clone()),
                        detail: Some(format!("cycle: {}", path.join(" → "))),
                    });
                }
            }
        }
    }

    if orphan_count > 0 {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "alias_orphan",
            message: format!(
                "{n} user/kit alias{s} reference an unknown name (likely a deleted spec)",
                n = orphan_count,
                s = if orphan_count == 1 { "" } else { "es" },
            ),
            count: orphan_count,
            example: orphan_example,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }
    if cycle_count > 0 {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "alias_cycle",
            message: format!(
                "{n} user/kit alias chain{s} form a cycle",
                n = cycle_count,
                s = if cycle_count == 1 { "" } else { "s" },
            ),
            count: cycle_count,
            example: cycle_example,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    Ok(AliasReport { issues })
}

/// Outcome of walking a single alias's expansion chain.
#[derive(Debug, Clone)]
enum ChainOutcome {
    /// Chain reaches a known subcommand or a positional placeholder.
    Resolved,
    /// Chain terminates at an unresolvable name partway through.
    Orphan {
        broken_at: String,
        chain: Vec<String>,
    },
    /// Chain re-visits a name. `path` is the visited list with the
    /// repeated name appended at the end.
    Cycle(Vec<String>),
}

/// Walk an alias's chain by repeatedly looking up the head of each
/// expansion. Stops on a known subcommand, a positional placeholder, an
/// unresolved name, a revisited name, or after [`MAX_ALIAS_HOPS`] hops
/// (which we treat as a cycle — a chain that long is unreachable in
/// practice).
fn analyze_chain(db: &Db, start: &str) -> Result<ChainOutcome> {
    let mut visited: Vec<String> = vec![start.to_string()];
    let mut current = start.to_string();
    for _ in 0..MAX_ALIAS_HOPS {
        let alias = match db.lookup_alias(&current)? {
            Some(a) => a,
            None => {
                // The starting alias is guaranteed resolvable by its
                // caller (we just enumerated it), so a None here means
                // we hit an unresolved hop mid-walk.
                return Ok(ChainOutcome::Orphan {
                    broken_at: current,
                    chain: visited,
                });
            }
        };
        let tokens = shell_split(&alias.expands_to);
        let Some(head) = tokens.first() else {
            return Ok(ChainOutcome::Resolved);
        };
        if KNOWN_SUBCOMMANDS.contains(&head.as_str()) {
            return Ok(ChainOutcome::Resolved);
        }
        if head.starts_with('$') {
            // Positional placeholder; substitution happens at expansion
            // time, the chain is well-formed.
            return Ok(ChainOutcome::Resolved);
        }
        if visited.iter().any(|v| v == head) {
            visited.push(head.clone());
            return Ok(ChainOutcome::Cycle(visited));
        }
        visited.push(head.clone());
        current = head.clone();
    }
    // Hit the hop budget without resolving — treat as cycle.
    Ok(ChainOutcome::Cycle(visited))
}

/// Run every consistency check and return a structured report.
pub fn collect_issues(db: &Db) -> Result<Report> {
    let mut issues: Vec<Issue> = Vec::new();

    // 1. time_end < time_start (ERROR)
    let time_bad = db.find_time_end_before_start()?;
    if !time_bad.is_empty() {
        let first = &time_bad[0];
        issues.push(Issue {
            severity: Severity::Error,
            category: "time_end_before_start",
            message: format!(
                "{} entr{} have time_end earlier than time_start",
                time_bad.len(),
                if time_bad.len() == 1 { "y" } else { "ies" },
            ),
            count: time_bad.len(),
            example: Some(IssueExample {
                id: Some(first.slug.to_string()),
                field: Some(first.field.clone()),
                detail: Some(format!(
                    "start={}  end={}",
                    first.start_time, first.end_time
                )),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    // 2. content_hash mismatches (ERROR)
    let entries = db.list_including_deleted()?;
    let mut mismatches: Vec<(String, String, String, String)> = Vec::new();
    for e in &entries {
        let expected = e.expected_content_hash();
        if expected != e.content_hash {
            mismatches.push((
                e.slug.to_string(),
                e.field.clone(),
                e.content_hash.clone(),
                expected,
            ));
        }
    }
    if !mismatches.is_empty() {
        let (slug, field, stored, expected) = mismatches[0].clone();
        issues.push(Issue {
            severity: Severity::Error,
            category: "content_hash_mismatch",
            message: format!(
                "{} entr{} have a stored content_hash that disagrees with its fields",
                mismatches.len(),
                if mismatches.len() == 1 { "y" } else { "ies" },
            ),
            count: mismatches.len(),
            example: Some(IssueExample {
                id: Some(slug),
                field: Some(field),
                detail: Some(format!(
                    "stored={}…  expected={}…",
                    &stored[..stored.len().min(12)],
                    &expected[..expected.len().min(12)],
                )),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    // Load registry once for the remaining checks.
    let registry = db.load_registry()?;

    // 3. unknown fields — have entries but no field_spec (WARNING)
    let fields = db.distinct_fields()?;
    let unknown: Vec<String> = fields
        .iter()
        .filter(|f| registry.field_spec(f).is_none())
        .cloned()
        .collect();
    if !unknown.is_empty() {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "unknown_field",
            message: format!(
                "{} field{} not registered in any spec",
                unknown.len(),
                if unknown.len() == 1 { "" } else { "s" },
            ),
            count: unknown.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(unknown[0].clone()),
                detail: Some(unknown.join(", ")),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    // 4. orphaned extra_data keys — field has a spec but extra_data contains
    //    keys that aren't declared in the spec's subfields (WARNING).
    //
    // Only fires for specs explicitly marked `strict = true`. Most fields are
    // freeform and their data shape evolves; flagging every undeclared key by
    // default would be over-aggressive. Strict mode lets the user lock down a
    // spec once the shape stabilizes.
    let mut orphan_detail: Option<(String, String, String)> = None; // (id, field, key)
    let mut orphan_count = 0usize;
    for e in &entries {
        let Some(spec) = registry.field_spec(&e.field) else {
            continue;
        };
        if !spec.strict {
            continue;
        }
        let Some(extra) = e.data.as_ref() else {
            continue;
        };
        if let JsonValue::Object(map) = extra {
            for key in map.keys() {
                if !spec.subfields.contains_key(key) {
                    orphan_count += 1;
                    if orphan_detail.is_none() {
                        orphan_detail = Some((e.slug.to_string(), e.field.clone(), key.clone()));
                    }
                }
            }
        }
    }
    if orphan_count > 0 {
        let (id, field, key) = orphan_detail.unwrap();
        issues.push(Issue {
            severity: Severity::Warning,
            category: "orphaned_extra_data_key",
            message: format!(
                "{} extra_data key{} not declared in the owning field's spec",
                orphan_count,
                if orphan_count == 1 { "" } else { "s" },
            ),
            count: orphan_count,
            example: Some(IssueExample {
                id: Some(id),
                field: Some(field),
                detail: Some(format!("key={key}")),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    let errors = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let warnings = issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    Ok(Report {
        issues,
        summary: Summary {
            errors,
            warnings,
            clean: errors == 0 && warnings == 0,
        },
    })
}

/// Human-readable report, grouped by category with per-row count and example.
pub fn render_human(report: &Report) -> String {
    if report.issues.is_empty() {
        return "✓ No consistency issues found.\n".to_string();
    }
    let mut out = String::new();
    for issue in &report.issues {
        let tag = match issue.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
        };
        out.push_str(&format!(
            "[{tag}] {category} (x{count}): {msg}\n",
            category = issue.category,
            count = issue.count,
            msg = issue.message,
        ));
        // da-cr4n: when collect-time populated `rendered_block` (integrity
        // warnings, today), emit it verbatim — it came from the same
        // table formatter `datum tail` uses, so columns, colour, time
        // stamps, and the `!` repeated-state marker match the live view.
        // Indent each line by 4 to nest visually under the [WARN]/[ERROR]
        // header. The block already ends in `\n`, so `lines()` strips it
        // and the per-line push re-adds the newline.
        if !issue.rendered_block.is_empty() {
            for line in issue.rendered_block.lines() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        } else if let Some(ex) = &issue.example {
            if let Some(id) = &ex.id {
                out.push_str(&format!("    example: {id}\n"));
            }
            if let Some(field) = &ex.field {
                out.push_str(&format!("    example field: {field}\n"));
            }
            if let Some(detail) = &ex.detail {
                out.push_str(&format!("    {detail}\n"));
            }
        }
    }
    out.push_str(&format!(
        "\nSummary: {} error{}, {} warning{}.\n",
        report.summary.errors,
        if report.summary.errors == 1 { "" } else { "s" },
        report.summary.warnings,
        if report.summary.warnings == 1 {
            ""
        } else {
            "s"
        },
    ));
    out
}

/// JSON report for machine consumption.
pub fn render_json(report: &Report) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;
    use datum_core::db::Db;
    use datum_core::entry::{Entry, Verb};
    use datum_core::field_spec::{FieldSpec, SubfieldSpec, SubfieldType};
    use datum_core::time::DatumTime;
    use std::collections::HashMap;

    fn t_at(iso: &str) -> DatumTime {
        let utc: chrono::DateTime<chrono::Utc> = iso.parse().unwrap();
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
    }

    #[test]
    fn check_clean_db_exits_zero() {
        let db = Db::open_in_memory().unwrap();
        let report = collect_issues(&db).unwrap();
        assert!(!report.has_errors(), "empty db must have no errors");
        assert!(report.issues.is_empty(), "empty db must produce no issues");
        assert!(report.summary.clean);
    }

    /// A row whose stored `occur_time_offset` disagrees with the offset its
    /// `occur_time_tz` produces at that instant is flagged by the `tz`
    /// checker as a `tz_offset_mismatch` ERROR. v3 never writes such a row
    /// (the offset is derived at construction), so the test seeds the
    /// desync directly into the column — exactly the legacy-import /
    /// machine-timezone-seam class this checker exists to catch.
    #[test]
    fn tz_check_flags_desynced_offset() {
        let db = Db::open_in_memory().unwrap();
        // America/Chicago at this instant is CST (-360). new_legacy derives
        // the correct offset; we then corrupt the stored column to +0.
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-01-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        assert_eq!(occur.offset_minutes, -360);
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_offset = 0 WHERE field = 'work'",
                [],
            )
            .unwrap();

        let issues = collect_tz_drift(&db).unwrap();
        assert_eq!(issues.len(), 1, "exactly one tz issue");
        let issue = &issues[0];
        assert_eq!(issue.category, "tz_offset_mismatch");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.count, 1);
        let detail = issue
            .example
            .as_ref()
            .and_then(|e| e.detail.as_deref())
            .unwrap_or_default();
        assert!(
            detail.contains("stored_offset=0") && detail.contains("expected_offset=-360"),
            "detail names both offsets: {detail}",
        );
    }

    /// An in-sync row (offset derived correctly at construction) produces no
    /// `tz` issue — the checker only fires on genuine desync.
    #[test]
    fn tz_check_clean_db_emits_nothing() {
        let db = Db::open_in_memory().unwrap();
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-07-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        assert!(collect_tz_drift(&db).unwrap().is_empty());
    }

    /// An unparseable stored `occur_time_tz` can't be reconciled against the
    /// offset, so the row is flagged too — `local_time` is meaningless until
    /// the name is repaired.
    #[test]
    fn tz_check_flags_unparseable_timezone() {
        let db = Db::open_in_memory().unwrap();
        let entry = Entry::new_legacy(
            "work",
            Verb::Switch,
            None,
            None,
            Some(t_at("2024-01-15T12:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_tz = 'Mars/Olympus_Mons' WHERE field = 'work'",
                [],
            )
            .unwrap();
        let issues = collect_tz_drift(&db).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].category, "tz_offset_mismatch");
    }

    /// Tombstoned rows are historical, not live data — the `tz` checker
    /// skips them even when their stored offset is desynced.
    #[test]
    fn tz_check_skips_tombstoned_rows() {
        let db = Db::open_in_memory().unwrap();
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-01-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_offset = 0, \
                 deleted_at = '2024-01-16T00:00:00+00:00' WHERE field = 'work'",
                [],
            )
            .unwrap();
        assert!(
            collect_tz_drift(&db).unwrap().is_empty(),
            "desynced tombstone must not be flagged",
        );
    }

    #[test]
    fn check_reports_time_end_before_start() {
        let db = Db::open_in_memory().unwrap();
        // Under v3 (da-5ns) duration is captured by an ISO-string `dur`
        // column and `Entry::new_legacy` clamps a negative time delta to
        // zero before insert (`(end - start).num_seconds().max(0)`).
        // Combined with `derive_verb`'s rejection of negative ISO
        // durations, "end before start" can no longer reach the entries
        // table — `find_time_end_before_start` returns empty. Locking
        // in: the check pass succeeds without flagging this category.
        let start = t_at("2024-01-01T10:00:00+00:00");
        let end = t_at("2024-01-01T09:00:00+00:00");
        let entry = Entry::new_legacy(
            "work",
            Verb::Switch,
            None,
            None,
            Some(start),
            Some(end),
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(
            report
                .issues
                .iter()
                .all(|i| i.category != "time_end_before_start"),
            "time_end_before_start is unrepresentable under v3 — check should not flag it",
        );
    }

    #[test]
    fn check_reports_unknown_field_as_warning() {
        let db = Db::open_in_memory().unwrap();
        // Insert an entry for "mood" without registering a spec.
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(
            !report.has_errors(),
            "unknown fields are warnings, not errors"
        );
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "unknown_field")
            .expect("unknown_field warning must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        assert_eq!(
            issue.example.as_ref().and_then(|e| e.field.as_deref()),
            Some("mood")
        );
    }

    #[test]
    fn check_json_output_shape() {
        let db = Db::open_in_memory().unwrap();
        // Populate one error-level and one warning-level issue.
        let bad = Entry::new_legacy(
            "work",
            Verb::Switch,
            None,
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            Some(t_at("2024-01-01T09:00:00+00:00")),
            "d1",
        );
        db.insert_entry(&bad).unwrap();
        // Force a content_hash_mismatch error by stamping a known-bad hash
        // directly into the column. (Live writes can no longer produce this
        // drift — da-r569 — so the test seeds it explicitly.)
        db.conn()
            .execute(
                "UPDATE entries SET content_hash = 'deadbeef' WHERE field = 'work'",
                [],
            )
            .unwrap();
        // Unknown-field warning on another field.
        let unknown = Entry::new_legacy(
            "mystery",
            Verb::Point,
            None,
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&unknown).unwrap();

        let report = collect_issues(&db).unwrap();
        let json = render_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Top-level shape
        assert!(parsed.is_object());
        assert!(parsed.get("issues").is_some(), "report must have 'issues'");
        assert!(
            parsed.get("summary").is_some(),
            "report must have 'summary'"
        );

        // Summary shape
        let summary = parsed.get("summary").unwrap();
        assert!(summary.get("errors").unwrap().is_number());
        assert!(summary.get("warnings").unwrap().is_number());
        assert!(summary.get("clean").unwrap().is_boolean());
        assert!(summary.get("errors").unwrap().as_u64().unwrap() >= 1);
        assert!(summary.get("warnings").unwrap().as_u64().unwrap() >= 1);
        assert_eq!(summary.get("clean").unwrap().as_bool(), Some(false));

        // Issue shape
        let issues = parsed.get("issues").unwrap().as_array().unwrap();
        assert!(!issues.is_empty());
        for issue in issues {
            assert!(issue.get("severity").unwrap().is_string());
            assert!(issue.get("category").unwrap().is_string());
            assert!(issue.get("message").unwrap().is_string());
            assert!(issue.get("count").unwrap().is_number());
            let sev = issue.get("severity").unwrap().as_str().unwrap();
            assert!(sev == "error" || sev == "warning");
        }
    }

    #[test]
    fn check_reports_orphaned_extra_data_key_as_warning() {
        let db = Db::open_in_memory().unwrap();
        // Register a spec for "mood" that declares only `intensity` as a subfield.
        let mut subfields = HashMap::new();
        subfields.insert(
            "intensity".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::Number,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.subfields = subfields;
        // da-mkes: the orphan check only fires for strict specs.
        spec.strict = true;
        db.save_field_spec(&spec).unwrap();

        // Entry carries an *undeclared* extra_data key "weather".
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"weather": "sunny"})),
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(
            !report.has_errors(),
            "orphaned keys are warnings, not errors"
        );
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "orphaned_extra_data_key")
            .expect("orphaned_extra_data_key warning must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
    }

    /// da-mkes: non-strict specs (the default) silently accept undeclared
    /// extra_data keys — no warning fires.
    #[test]
    fn check_skips_orphaned_extra_data_key_when_spec_not_strict() {
        let db = Db::open_in_memory().unwrap();
        let mut subfields = HashMap::new();
        subfields.insert(
            "intensity".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::Number,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.subfields = subfields;
        // strict defaults to false — flag should not fire.
        db.save_field_spec(&spec).unwrap();

        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"weather": "sunny"})),
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(
            !report
                .issues
                .iter()
                .any(|i| i.category == "orphaned_extra_data_key"),
            "orphaned_extra_data_key must not fire for non-strict specs, got: {:?}",
            report.issues,
        );
    }

    // ─── da-5ns sub-bead 7: verb drift detection + --fix ────────────────

    /// Hand-corrupt a row's stored verb and verify
    /// `collect_verb_drift` flags it as `verb_drift` with the
    /// expected severity, count, and example detail.
    #[test]
    fn verb_drift_detected_when_stored_verb_disagrees() {
        let db = Db::open_in_memory().unwrap();
        // A switch-shape entry: state=set, dur=NULL, fs=NULL → derive_verb=Switch.
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        let slug = entry.slug.to_string();
        db.insert_entry(&entry).unwrap();
        // The insert path currently writes the stub verb "point" (sub-bead 3
        // wires derive_verb later); regardless, force the column to a value
        // that derive_verb would NOT produce so the test is independent of
        // sub-bead 3 status.
        db.set_entry_verb(&slug, "block").unwrap();

        let report = collect_verb_drift(&db).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "verb_drift")
            .expect("verb_drift issue must be reported");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.count, 1);
        let example = issue.example.as_ref().unwrap();
        assert_eq!(example.id.as_deref(), Some(slug.as_str()));
        let detail = example.detail.as_deref().unwrap();
        assert!(detail.contains("stored=block"), "got: {detail}");
        assert!(detail.contains("expected=switch"), "got: {detail}");
    }

    /// `--fix` rewrites a hand-corrupted verb back to what
    /// `derive_verb` produces, and a re-run of the check finds no drift.
    #[test]
    fn verb_fix_rewrites_drifted_verb_to_derived() {
        let db = Db::open_in_memory().unwrap();
        // Block-shape: state=set, dur=positive, fs=NULL → derive_verb=Block.
        let start = t_at("2024-01-01T10:00:00+00:00");
        let end = t_at("2024-01-01T10:30:00+00:00");
        let entry = Entry::new_legacy(
            "work",
            Verb::Block,
            Some(serde_json::json!("on")),
            None,
            Some(start),
            Some(end),
            "d1",
        );
        let slug = entry.slug.to_string();
        db.insert_entry(&entry).unwrap();
        // Hand-corrupt: write a verb derive_verb would never produce here.
        db.set_entry_verb(&slug, "switch").unwrap();

        // Pre-fix: drift detected.
        let report = collect_verb_drift(&db).unwrap();
        let drift = report
            .issues
            .iter()
            .find(|i| i.category == "verb_drift")
            .expect("pre-fix run must report drift");
        assert_eq!(drift.count, 1);
        assert_eq!(report.fixes.len(), 1);
        assert_eq!(report.fixes[0].slug, slug);
        assert_eq!(report.fixes[0].expected_verb, "block");

        // Apply the fix.
        apply_verb_fixes(&db, &report.fixes).unwrap();

        // Post-fix: no drift, and the row's stored verb is now `block`.
        let report2 = collect_verb_drift(&db).unwrap();
        assert!(
            !report2.issues.iter().any(|i| i.category == "verb_drift"),
            "post-fix drift must be zero, got issues: {:?}",
            report2.issues,
        );
        let row = db.get_entry(&slug).unwrap();
        assert_eq!(row.verb, Verb::Block);
    }

    /// `dm check verbs` (typed API) narrows the report to the verb-drift
    /// category — other categories (e.g. `unknown_field`) are excluded.
    #[test]
    fn check_verbs_only_narrows_to_verb_category() {
        let db = Db::open_in_memory().unwrap();
        // Insert a clean switch entry (no drift expected from derive_verb).
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("calm")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        let slug = entry.slug.to_string();
        db.insert_entry(&entry).unwrap();
        db.set_entry_verb(&slug, "point").unwrap(); // force drift

        let report = collect_verb_drift(&db).unwrap();
        // Only verb-related categories should appear.
        for issue in &report.issues {
            assert!(
                issue.category == "verb_drift" || issue.category == "verb_corruption",
                "unexpected category in `check verbs` run: {}",
                issue.category,
            );
        }
        assert!(
            report.issues.iter().any(|i| i.category == "verb_drift"),
            "verb_drift must be present"
        );
    }

    /// Tombstoned rows are skipped — verb drift on a soft-deleted row
    /// is historical and not actionable.
    #[test]
    fn verb_drift_skips_tombstoned_rows() {
        let db = Db::open_in_memory().unwrap();
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        let slug = entry.slug.to_string();
        db.insert_entry(&entry).unwrap();
        db.set_entry_verb(&slug, "block").unwrap(); // drift
        db.soft_delete_entry(&slug).unwrap();

        let report = collect_verb_drift(&db).unwrap();
        assert!(
            !report.issues.iter().any(|i| i.category == "verb_drift"),
            "tombstoned rows must not contribute to drift count, got: {:?}",
            report.issues,
        );
        assert!(report.fixes.is_empty());
    }

    // ─── da-c706: alias divergence detection ────────────────────────────

    use datum_core::field_spec::Alias;

    fn switch_spec(field: &str, states: &[&str]) -> FieldSpec {
        let mut spec = FieldSpec::new(field, Action::Switch);
        spec.allowed_states = Some(states.iter().map(|s| s.to_string()).collect());
        spec.regenerate_derived_aliases();
        spec
    }

    /// Two specs both declare a state named `done`. The first-saved spec
    /// wins by rowid; the loser appears in `SpecAliasIndex.collisions`,
    /// and `dm check aliases` surfaces it as `alias_state_collision`.
    #[test]
    fn alias_state_collision_detected_across_specs() {
        let db = Db::open_in_memory().unwrap();
        // mood is saved first → wins the `done` state alias.
        db.save_field_spec(&switch_spec("mood", &["happy", "done"]))
            .unwrap();
        db.save_field_spec(&switch_spec("task", &["wip", "done"]))
            .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_state_collision")
            .expect("state collision must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        let detail = issue.example.as_ref().unwrap().detail.as_deref().unwrap();
        assert!(detail.contains("'done'"), "got: {detail}");
        assert!(detail.contains("mood"), "got: {detail}");
        assert!(detail.contains("task"), "got: {detail}");
    }

    /// `--fix` (fix_collisions=true) annotates collisions with the
    /// explicit first-saved-wins resolution. No DB mutation occurs —
    /// re-running the same check produces the same collision count.
    #[test]
    fn alias_state_collision_fix_annotates_resolution() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&switch_spec("mood", &["done"])).unwrap();
        db.save_field_spec(&switch_spec("task", &["done"])).unwrap();

        let report = collect_alias_drift(&db, true).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_state_collision")
            .expect("state collision must be reported");
        let detail = issue.example.as_ref().unwrap().detail.as_deref().unwrap();
        assert!(
            detail.contains("first-saved-wins"),
            "fix-mode detail must mention the resolution: {detail}",
        );

        // Re-run is idempotent — collisions are not mutated away.
        let report2 = collect_alias_drift(&db, true).unwrap();
        let issue2 = report2
            .issues
            .iter()
            .find(|i| i.category == "alias_state_collision")
            .expect("collision still present after --fix");
        assert_eq!(issue2.count, 1);
    }

    /// A user alias whose chain ends in an unresolvable name (typically a
    /// deleted spec's bare-field alias) is flagged as `alias_orphan`.
    #[test]
    fn alias_orphan_detected_when_chain_breaks() {
        let db = Db::open_in_memory().unwrap();
        // User alias 'm' → 'mood'. mood was a spec but is now gone, and
        // there is no user alias by that name either.
        db.save_alias(&Alias {
            name: "m".to_string(),
            expands_to: "mood".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_orphan")
            .expect("orphan must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        let example = issue.example.as_ref().unwrap();
        assert_eq!(example.field.as_deref(), Some("m"));
        let detail = example.detail.as_deref().unwrap();
        assert!(detail.contains("mood"), "got: {detail}");
    }

    /// A user alias that resolves through another known alias to a
    /// subcommand head is NOT an orphan. (`m` → `happy` → `switch mood
    /// happy` is fine when `mood` spec exists with state `happy`.)
    #[test]
    fn alias_resolved_via_spec_alias_is_not_orphan() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&switch_spec("mood", &["happy"]))
            .unwrap();
        db.save_alias(&Alias {
            name: "h".to_string(),
            expands_to: "happy".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        assert!(
            !report.issues.iter().any(|i| i.category == "alias_orphan"),
            "no orphan expected, got: {:?}",
            report.issues,
        );
    }

    /// A self-referential alias (`a → a`) forms a one-step cycle and is
    /// flagged as `alias_cycle`.
    #[test]
    fn alias_cycle_self_loop_detected() {
        let db = Db::open_in_memory().unwrap();
        db.save_alias(&Alias {
            name: "a".to_string(),
            expands_to: "a".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_cycle")
            .expect("cycle must be reported");
        assert_eq!(issue.count, 1);
        let detail = issue.example.as_ref().unwrap().detail.as_deref().unwrap();
        assert!(detail.contains("a"), "got: {detail}");
    }

    /// A two-step cycle (`a → b → a`) is detected. Both aliases get
    /// flagged because we walk every alias's chain independently.
    #[test]
    fn alias_cycle_two_step_detected() {
        let db = Db::open_in_memory().unwrap();
        db.save_alias(&Alias {
            name: "a".to_string(),
            expands_to: "b".to_string(),
            description: None,
        })
        .unwrap();
        db.save_alias(&Alias {
            name: "b".to_string(),
            expands_to: "a".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_cycle")
            .expect("cycle must be reported");
        // Both `a` and `b` walk into the cycle from their own start
        // points, so the count is 2.
        assert_eq!(issue.count, 2);
    }

    /// A clean DB with well-formed specs and aliases produces no
    /// alias-related issues.
    #[test]
    fn alias_check_clean_db_emits_nothing() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&switch_spec("mood", &["happy", "sad"]))
            .unwrap();
        db.save_alias(&Alias {
            name: "m".to_string(),
            expands_to: "switch mood happy".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        assert!(
            report.issues.is_empty(),
            "clean db must yield no alias issues, got: {:?}",
            report.issues,
        );
    }

    /// `dm check aliases` narrows the report to alias categories only —
    /// verb drift and other default-battery categories are excluded.
    #[test]
    fn alias_check_args_narrows_to_alias_categories() {
        let db = Db::open_in_memory().unwrap();
        // Trigger an unknown_field warning (default battery) AND an
        // alias_orphan warning. Only the latter should appear.
        let entry = Entry::new_legacy(
            "ghost",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
        db.save_alias(&Alias {
            name: "m".to_string(),
            expands_to: "mood".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        for issue in &report.issues {
            assert!(
                matches!(
                    issue.category,
                    "alias_state_collision" | "alias_orphan" | "alias_cycle",
                ),
                "unexpected category in `check aliases` run: {}",
                issue.category,
            );
        }
        assert!(
            report.issues.iter().any(|i| i.category == "alias_orphan"),
            "alias_orphan must be present"
        );
    }

    /// Positional placeholders (`$1`) at the chain head are treated as
    /// resolved — substitution happens at expansion time.
    #[test]
    fn alias_positional_placeholder_is_resolved() {
        let db = Db::open_in_memory().unwrap();
        db.save_alias(&Alias {
            name: "p".to_string(),
            expands_to: "$1 thing".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        assert!(
            !report.issues.iter().any(|i| i.category == "alias_orphan"),
            "positional-headed alias must not be orphan: {:?}",
            report.issues,
        );
    }

    // ─── da-noz9: slug drift detection + --fix ──────────────────────────

    /// A row whose stored slug doesn't match `interpolate(id_structure)`
    /// is flagged as `slug_drift` with the expected severity, count, and
    /// example detail.
    #[test]
    fn slug_drift_detected_when_stored_slug_disagrees() {
        let db = Db::open_in_memory().unwrap();
        let mut entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        // Mirror the cmd.rs insert path: a row with an occur_time gets
        // `id_structure = "%occur_time%"` and slug `field:<occur_time>`.
        entry.id_structure = Some("%occur_time%".to_string());
        entry.slug = Slug(format!("mood:{}", entry.short_uid()));
        entry.field_structure = None;
        // The checker will recompute the body from `%occur_time%` and
        // discover it disagrees with the `short_uid` body we wrote here.
        // (The point is just to land a row whose stored slug differs from
        // its template-derived slug; the exact corruption shape doesn't
        // matter.)
        db.insert_entry(&entry).unwrap();

        let report = collect_slug_drift(&db, ":").unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "slug_drift")
            .expect("slug_drift issue must be reported");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.count, 1);
        let example = issue.example.as_ref().unwrap();
        let detail = example.detail.as_deref().unwrap();
        assert!(detail.contains("stored=mood:"), "got: {detail}");
        assert!(detail.contains("expected=mood:"), "got: {detail}");
        assert_eq!(report.fixes.len(), 1);
    }

    /// `--fix` rewrites a drifted slug back to the template-derived
    /// value, and a re-run of the check finds no drift. The row's `hid`
    /// is preserved.
    #[test]
    fn slug_fix_rekeys_drifted_row_to_template() {
        let db = Db::open_in_memory().unwrap();
        let mut entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("calm")),
            None,
            Some(t_at("2024-01-02T08:00:00+00:00")),
            None,
            "d1",
        );
        entry.id_structure = Some("%occur_time%".to_string());
        // Hand-corrupt: the slug body is `short_uid`, but the template
        // expects `%occur_time%`. The expected slug after fix is
        // `mood:<formatted occur_time>`.
        entry.slug = Slug(format!("mood:{}", entry.short_uid()));
        let hid_before = entry.hid.clone();
        db.insert_entry(&entry).unwrap();

        // Pre-fix: drift detected.
        let report = collect_slug_drift(&db, ":").unwrap();
        assert_eq!(report.fixes.len(), 1, "pre-fix run must queue one fix");

        // Apply the fix.
        apply_slug_fixes(&db, &report.fixes, ":").unwrap();

        // Post-fix: no drift, row's hid preserved, slug matches the
        // template-derived value.
        let report2 = collect_slug_drift(&db, ":").unwrap();
        assert!(
            !report2.issues.iter().any(|i| i.category == "slug_drift"),
            "post-fix drift must be zero, got issues: {:?}",
            report2.issues,
        );
        let row = db.get_entry(&hid_before).unwrap();
        assert_eq!(row.hid, hid_before, "hid must survive the rekey");
        assert!(
            row.slug.as_str().starts_with("mood:2024-01-02T08"),
            "post-fix slug body must be the rendered occur_time, got: {}",
            row.slug,
        );
    }

    /// `dm check slugs` narrows the report to the slug-drift category —
    /// other categories (e.g. `unknown_field`) are excluded.
    #[test]
    fn check_slugs_only_narrows_to_slug_category() {
        let db = Db::open_in_memory().unwrap();
        let mut entry = Entry::new_legacy(
            "ghost",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
            Some(t_at("2024-03-04T05:00:00+00:00")),
            None,
            "d1",
        );
        entry.id_structure = Some("%occur_time%".to_string());
        entry.slug = Slug(format!("ghost:{}", entry.short_uid()));
        db.insert_entry(&entry).unwrap();

        let report = collect_slug_drift(&db, ":").unwrap();
        for issue in &report.issues {
            assert_eq!(
                issue.category, "slug_drift",
                "unexpected category in `check slugs` run: {}",
                issue.category,
            );
        }
        assert!(
            report.issues.iter().any(|i| i.category == "slug_drift"),
            "slug_drift must be present"
        );
    }

    /// Tombstoned rows are skipped — slug drift on a soft-deleted row is
    /// historical and not actionable.
    #[test]
    fn slug_drift_skips_tombstoned_rows() {
        let db = Db::open_in_memory().unwrap();
        let mut entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        entry.id_structure = Some("%occur_time%".to_string());
        entry.slug = Slug(format!("mood:{}", entry.short_uid()));
        let slug = entry.slug.clone();
        db.insert_entry(&entry).unwrap();
        db.soft_delete_entry(&slug).unwrap();

        let report = collect_slug_drift(&db, ":").unwrap();
        assert!(
            !report.issues.iter().any(|i| i.category == "slug_drift"),
            "tombstoned rows must not contribute to drift count, got: {:?}",
            report.issues,
        );
        assert!(report.fixes.is_empty());
    }

    /// Rows with no `id_structure` and no `field_structure` are skipped
    /// — there is no template to recompute the slug from.
    #[test]
    fn slug_drift_skips_rows_without_structure() {
        let db = Db::open_in_memory().unwrap();
        // Entry::new_legacy leaves id_structure/field_structure as None.
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_slug_drift(&db, ":").unwrap();
        assert!(
            report.issues.is_empty(),
            "rows without templates must produce no drift, got: {:?}",
            report.issues,
        );
        assert!(report.fixes.is_empty());
    }

    /// A clean DB whose stored slugs already match their templates
    /// produces no slug-drift issues.
    #[test]
    fn slug_check_clean_db_emits_nothing() {
        let db = Db::open_in_memory().unwrap();
        let occur = t_at("2024-05-06T07:08:09+00:00");
        let mut entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(occur.clone()),
            None,
            "d1",
        );
        entry.id_structure = Some("%occur_time%".to_string());
        // Mirror cmd.rs: slug body is the interpolated id_structure.
        let uid = entry.short_uid();
        let ctx = InterpolationContext::from_entry(&entry, &uid);
        let body = field_spec::interpolate("%occur_time%", &ctx, ":");
        entry.slug = Slug(format!("mood:{body}"));
        db.insert_entry(&entry).unwrap();

        let report = collect_slug_drift(&db, ":").unwrap();
        assert!(
            report.issues.is_empty(),
            "clean db must yield no slug-drift issues, got: {:?}",
            report.issues,
        );
    }

    // ─── da-ytj4: pending integrity warnings surfaced by `datum check` ───

    /// Two `switch mood happy` entries back-to-back file a
    /// `same_state_repeat` integrity row; `collect_integrity_warnings`
    /// surfaces it as a WARN with the matching category and count.
    #[test]
    fn integrity_check_reports_pending_same_state_repeat() {
        let db = Db::open_in_memory().unwrap();
        let e1 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e1).unwrap();
        let e2 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T11:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e2).unwrap();

        let issues = collect_integrity_warnings(&db).unwrap();
        let issue = issues
            .iter()
            .find(|i| i.category == "repeated_state")
            .expect("repeated_state must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        assert_eq!(
            issue.example.as_ref().and_then(|e| e.field.as_deref()),
            Some("mood")
        );
        // da-mkes: per-entry rendering — the flagged entry list is populated
        // with the triggering row, and the state column carries the leading
        // `!` marker that mirrors the live renderer's repeated-state cue.
        assert_eq!(issue.entries.len(), 1);
        assert_eq!(issue.entries[0].field, "mood");
        assert!(
            issue.entries[0].state.starts_with('!'),
            "repeated_state entry must carry leading `!`, got: {}",
            issue.entries[0].state
        );
        // da-cr4n: the rendered_block is built through the real tail table
        // formatter at collect time. Strip ANSI before asserting so the
        // assertion survives colour drift (the renderer paints `!` and the
        // state-cell background; we only care that the row is in there).
        let block = strip_ansi(&issue.rendered_block);
        assert!(!block.is_empty(), "rendered_block must be populated");
        assert!(
            block.contains("mood"),
            "rendered block missing field name 'mood': {block:?}"
        );
        assert!(
            block.contains("happy"),
            "rendered block missing state 'happy': {block:?}"
        );
        assert!(
            block.contains('!'),
            "rendered block must carry the `!` repeated-state marker: {block:?}"
        );
    }

    /// Strip ANSI CSI escape sequences (`\x1b[...m`) from a rendered block
    /// so golden assertions don't depend on the renderer's colour palette.
    /// Scoped to `tests` because no production caller needs ANSI-free
    /// output — colours are part of the artifact `datum check` prints.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                i += 2;
                while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                i += 1; // consume the final letter
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    /// Resolved integrity rows (`resolution IS NOT NULL`) are excluded —
    /// the user already triaged them. Exercised against `DoubleOpen`, the
    /// canonical stored kind: da-acft moved `repeated_state` to a live
    /// computation, so it has no per-row resolution surface anymore.
    #[test]
    fn integrity_check_skips_resolved_rows() {
        use datum_core::integrity::{IntegrityKind, ListFilter, Resolution};
        let db = Db::open_in_memory().unwrap();
        let e1 = Entry::new_legacy(
            "task",
            Verb::Switch,
            Some(serde_json::json!(true)),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e1).unwrap();
        let e2 = Entry::new_legacy(
            "task",
            Verb::Switch,
            Some(serde_json::json!(true)),
            None,
            Some(t_at("2024-01-01T11:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e2).unwrap();

        let pending = db
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::DoubleOpen),
                pending: Some(true),
            })
            .unwrap();
        assert_eq!(
            pending.len(),
            1,
            "exactly one pending DoubleOpen row expected"
        );
        db.resolve_integrity_warning(&pending[0].id, Resolution::KeepBoth)
            .unwrap();

        let issues = collect_integrity_warnings(&db).unwrap();
        assert!(
            issues.iter().all(|i| i.category != "integrity_double_open"),
            "resolved DoubleOpen rows must not surface, got: {issues:?}",
        );
    }

    /// da-acft: `repeated_state` is computed live from `entries`, so
    /// backfilling the missing transition makes the warning disappear
    /// on the next check — there is no stored row left behind.
    #[test]
    fn repeated_state_clears_when_user_backfills_missing_transition() {
        let db = Db::open_in_memory().unwrap();
        // Two consecutive `happy` switches: a repeat (the user forgot
        // to log a transition through some other state).
        let e1 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e1).unwrap();
        let e2 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T12:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e2).unwrap();
        assert!(
            collect_integrity_warnings(&db)
                .unwrap()
                .iter()
                .any(|i| i.category == "repeated_state"),
            "repeated_state should fire on consecutive `happy` switches",
        );

        // User remembers the intermediate transition and backfills it.
        let between = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("focused")),
            None,
            Some(t_at("2024-01-01T11:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&between).unwrap();

        let issues = collect_integrity_warnings(&db).unwrap();
        assert!(
            issues.iter().all(|i| i.category != "repeated_state"),
            "repeated_state must disappear once the missing transition is backfilled, got: {issues:?}",
        );
    }

    /// A clean DB with no pending integrity rows produces no issues.
    #[test]
    fn integrity_check_clean_db_emits_nothing() {
        let db = Db::open_in_memory().unwrap();
        let issues = collect_integrity_warnings(&db).unwrap();
        assert!(
            issues.is_empty(),
            "clean db must yield no integrity issues, got: {issues:?}",
        );
    }

    /// The integrity-only helper emits only categories sourced from the
    /// integrity_warnings table — it must not leak unrelated default-battery
    /// findings (e.g. `unknown_field`). da-mkes renamed `same_state_repeat`
    /// to the prefix-free `repeated_state`; the assertion accepts both
    /// surface forms.
    #[test]
    fn integrity_check_narrows_to_integrity_categories() {
        let db = Db::open_in_memory().unwrap();
        let e1 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e1).unwrap();
        let e2 = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T11:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e2).unwrap();

        let issues = collect_integrity_warnings(&db).unwrap();
        assert!(!issues.is_empty(), "expected at least one integrity issue");
        for issue in &issues {
            assert!(
                issue.category.starts_with("integrity_") || issue.category == "repeated_state",
                "unexpected category in integrity-only run: {}",
                issue.category,
            );
        }
    }

    // ─── da-ghlx: flagged-entry checker + default meta filter ───────────

    /// One row tagged via `datum flag mood "review later"` shows up in
    /// `collect_pending_flags` as a single `flagged_entry` warning with
    /// count=1, the field name in the example, and the comment threaded
    /// through `IssueExample::detail`.
    #[test]
    fn flagged_entries_surface_as_pending_review_warning() {
        let db = Db::open_in_memory().unwrap();
        // Build a flagged entry the same way `datum flag` does: occur on
        // the field with `extra_metadata = { flag: true, comment: "..." }`.
        let mut e = Entry::new_legacy(
            "mood",
            Verb::Point,
            None,
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        e.extra_metadata = Some(serde_json::json!({
            "flag": true,
            "comment": "review later"
        }));
        db.insert_entry(&e).unwrap();

        let issues = collect_pending_flags(&db).unwrap();
        assert_eq!(issues.len(), 1, "exactly one rolled-up flag issue");
        let issue = &issues[0];
        assert_eq!(issue.category, "flagged_entry");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        assert_eq!(
            issue.example.as_ref().and_then(|ex| ex.field.as_deref()),
            Some("mood")
        );
        assert_eq!(
            issue.example.as_ref().and_then(|ex| ex.detail.as_deref()),
            Some("review later"),
        );
        // FlaggedEntry list mirrors the rendered rows.
        assert_eq!(issue.entries.len(), 1);
        assert_eq!(issue.entries[0].field, "mood");
    }

    /// Clean DB → no flag issues, no panic.
    #[test]
    fn flagged_entries_clean_db_returns_empty() {
        let db = Db::open_in_memory().unwrap();
        let issues = collect_pending_flags(&db).unwrap();
        assert!(issues.is_empty(), "no flagged rows → no issue: {issues:?}");

        // An ordinary (non-flagged) row must not surface as a flag review.
        let e = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        let issues = collect_pending_flags(&db).unwrap();
        assert!(
            issues.is_empty(),
            "non-flagged rows must not surface: {issues:?}"
        );
    }

    /// `extra_metadata.meta = true` rows (da-xn8j protocol notes) are
    /// NOT pulled into the flag-review queue — they live in their own
    /// surface (`datum meta show`). The flag checker must filter on
    /// the `flag` key, not on the broader "meta-like" presence.
    #[test]
    fn meta_notes_do_not_appear_in_flag_review() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new_legacy(
            "mood",
            Verb::Point,
            None,
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        e.extra_metadata = Some(serde_json::json!({
            "meta": true,
            "text": "switch happy ⇒ unhappy means low energy",
        }));
        db.insert_entry(&e).unwrap();
        let issues = collect_pending_flags(&db).unwrap();
        assert!(issues.is_empty(), "meta notes must not surface as flags");
    }

    // ─── da-ghlx: default-hide filter on Db::list_entries ──────────────

    /// By default, `Db::list_entries` (the query that backs tail / head /
    /// dayview / nowview) must exclude rows whose `extra_metadata.flag`
    /// or `extra_metadata.meta` is JSON `true`. Set
    /// `ListParams::include_meta = true` to bypass.
    #[test]
    fn list_entries_hides_flag_and_meta_rows_by_default() {
        let db = Db::open_in_memory().unwrap();
        // Plain tracked row — should always appear.
        let plain = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&plain).unwrap();
        // Flagged row.
        let mut flagged = Entry::new_legacy(
            "mood",
            Verb::Point,
            None,
            None,
            Some(t_at("2024-01-01T11:00:00+00:00")),
            None,
            "d1",
        );
        flagged.extra_metadata = Some(serde_json::json!({"flag": true}));
        db.insert_entry(&flagged).unwrap();
        // Meta-note row.
        let mut meta = Entry::new_legacy(
            "mood",
            Verb::Point,
            None,
            None,
            Some(t_at("2024-01-01T12:00:00+00:00")),
            None,
            "d1",
        );
        meta.extra_metadata = Some(serde_json::json!({"meta": true, "text": "x"}));
        db.insert_entry(&meta).unwrap();

        // Default: only the plain row survives.
        let default_rows = db
            .list_entries(&ListParams {
                metric: datum_core::db::TimeMetric::Hybrid,
                ..Default::default()
            })
            .unwrap();
        let default_slugs: Vec<_> = default_rows.iter().map(|e| e.slug.to_string()).collect();
        assert_eq!(
            default_rows.len(),
            1,
            "default include_meta=false hides flag+meta rows; got {default_slugs:?}",
        );
        assert_eq!(default_rows[0].slug, plain.slug);

        // include_meta=true: every row is visible.
        let all_rows = db
            .list_entries(&ListParams {
                metric: datum_core::db::TimeMetric::Hybrid,
                include_meta: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            all_rows.len(),
            3,
            "include_meta=true must surface flag + meta rows",
        );
    }

    /// A row whose `extra_metadata.flag` is JSON `false` is tracked
    /// data, not a review marker — it must still appear in the default
    /// list. Guards against an `IS NULL` / `IS NOT NULL` regression.
    #[test]
    fn list_entries_keeps_rows_with_flag_explicitly_false() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        e.extra_metadata = Some(serde_json::json!({"flag": false}));
        db.insert_entry(&e).unwrap();
        let rows = db
            .list_entries(&ListParams {
                metric: datum_core::db::TimeMetric::Hybrid,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "flag=false rows are tracked data, must remain"
        );
    }

    // ─── da-adyj: content_hash repair under --fix ────────────────────────

    /// `apply_content_hash_fixes` rewrites every row whose stored
    /// `content_hash` disagrees with `Entry::expected_content_hash()`.
    /// Mirrors the legacy-row repair path vividn asked for.
    #[test]
    fn apply_content_hash_fixes_rewrites_drifted_rows_and_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new_legacy(
            "work",
            Verb::Switch,
            Some(serde_json::json!("focus")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();

        // Stomp the stored hash to simulate a pre-r569 legacy row.
        let bogus = "deadbeefcafef00d";
        db.conn()
            .execute(
                "UPDATE entries SET content_hash = ?1 WHERE field = 'work'",
                rusqlite::params![bogus],
            )
            .unwrap();
        let stored: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE field = 'work'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, bogus, "fixture: hash forced");

        let fix = apply_content_hash_fixes(&db).unwrap();
        assert_eq!(fix.detected, 1, "exactly one drifted row was detected");
        assert_eq!(fix.applied, 1, "and applied");
        assert_eq!(fix.still_mismatched, 0, "and the rewrite stuck");

        // Hash now matches Entry::expected_content_hash().
        let after: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE field = 'work'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(after, bogus, "hash must change");
        let expected = db.list_including_deleted().unwrap()[0].expected_content_hash();
        assert_eq!(after, expected, "hash now matches expected_content_hash");

        // Second pass is a no-op — idempotence is part of the contract.
        let second = apply_content_hash_fixes(&db).unwrap();
        assert_eq!(second.detected, 0, "idempotent: nothing left to repair");
        assert_eq!(second.applied, 0);
        assert_eq!(second.still_mismatched, 0);
    }

    /// Clean DBs (every row's hash already matches its expected hash —
    /// i.e. anything written post-r569) report zero repairs. Guards
    /// against accidentally rewriting healthy rows.
    #[test]
    fn apply_content_hash_fixes_leaves_clean_db_untouched() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new_legacy(
            "work",
            Verb::Switch,
            Some(serde_json::json!("focus")),
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        let pre: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE field = 'work'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let fix = apply_content_hash_fixes(&db).unwrap();
        assert_eq!(fix.detected, 0, "no repairs on clean DB");
        assert_eq!(fix.applied, 0);
        assert_eq!(fix.still_mismatched, 0);

        let post: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE field = 'work'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pre, post, "stored hash untouched");
    }

    /// da-4c1c: rows whose `data` carries a complex nested-object JSON
    /// (multi-key, mixed types) still round-trip through the fix path.
    /// This guards against the hypothesis-2 failure where
    /// `expected_content_hash` would return different values on
    /// successive reads of the same row (e.g. via key-order drift in
    /// the deserialized JsonValue).
    #[test]
    fn apply_content_hash_fixes_repairs_rows_with_complex_data_json() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new_legacy(
            "weight",
            Verb::Point,
            Some(serde_json::json!(75.2)),
            Some(serde_json::json!({
                "unit": "kg",
                "note": "morning",
                "tags": ["calibrated", "indoor"],
                "nested": {"sensor": "scale-v2", "battery": 87}
            })),
            Some(t_at("2024-01-01T07:30:00+00:00")),
            None,
            "d1",
        );
        // After insert, content_hash should match expected_content_hash —
        // we'll stomp it to simulate a legacy pre-r569 row.
        db.insert_entry(&e).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET content_hash = 'legacy-broken-hash' WHERE field = 'weight'",
                [],
            )
            .unwrap();

        // Sanity: detector path agrees the row needs repair.
        let entries_pre = db.list_including_deleted().unwrap();
        assert_eq!(entries_pre.len(), 1);
        let detector_says_mismatch =
            entries_pre[0].expected_content_hash() != entries_pre[0].content_hash;
        assert!(
            detector_says_mismatch,
            "fixture must produce a detected mismatch"
        );

        let fix = apply_content_hash_fixes(&db).unwrap();
        assert_eq!(fix.detected, 1);
        assert_eq!(fix.applied, 1);
        assert_eq!(
            fix.still_mismatched, 0,
            "the post-fix verify pass must show zero mismatches; \
             a non-zero count would mean expected_content_hash() drifted \
             across two reads of the same row (da-4c1c)"
        );

        // Suppress unused-mut warning on `e`.
        e.content_hash = "drop".to_string();
        let _ = e;
    }

    // ─── da-6xvh: shared detect/apply/verify posture for verb + slug ─────

    /// A failing per-row apply (NotFound on a stale slug) must not abort
    /// the rest of the verb-fix batch: later rows still get repaired, and
    /// the verify re-scan counts the survivor with a sample line instead
    /// of bailing mid-repair (the pre-da-6xvh `?` posture).
    #[test]
    fn apply_verb_fixes_tolerates_per_row_failure_and_reports_leftovers() {
        let db = Db::open_in_memory().unwrap();
        let mk = |field: &str, hour: u32| {
            // Block-shape rows (state=set, dur>0) so derive_verb=Block.
            let entry = Entry::new_legacy(
                field,
                Verb::Block,
                Some(serde_json::json!("on")),
                None,
                Some(t_at(&format!("2024-01-01T{hour:02}:00:00+00:00"))),
                Some(t_at(&format!("2024-01-01T{hour:02}:30:00+00:00"))),
                "d1",
            );
            let slug = entry.slug.to_string();
            db.insert_entry(&entry).unwrap();
            db.set_entry_verb(&slug, "switch").unwrap(); // force drift
            slug
        };
        let slug_a = mk("alpha", 10);
        let slug_b = mk("beta", 11);

        let mut fixes = collect_verb_drift(&db).unwrap().fixes;
        assert_eq!(fixes.len(), 2, "fixture: both rows drifted");
        // Sabotage row A's fix: point it at a slug that doesn't exist so
        // its apply fails with NotFound mid-batch.
        let a_fix = fixes.iter_mut().find(|f| f.slug == slug_a).unwrap();
        a_fix.slug = "alpha:no-such-row".to_string();

        let report = apply_verb_fixes(&db, &fixes).unwrap();
        assert_eq!(report.detected, 2);
        assert_eq!(
            report.applied, 1,
            "row B must be repaired despite row A's failure"
        );
        assert_eq!(report.still_mismatched, 1, "verify counts the survivor");
        assert_eq!(report.leftover_samples.len(), 1);
        assert_eq!(report.leftover_samples[0].0, slug_a);

        // Row B is actually repaired in the DB.
        assert_eq!(db.get_entry(&slug_b).unwrap().verb, Verb::Block);
    }

    /// A SlugFix whose pre-scan `existing` slug no longer resolves —
    /// da-6xvh's self-interference hazard, where an earlier rekey in the
    /// same batch invalidates the slug a later fix references — must not
    /// abort the batch: the NotFound is tolerated, remaining rows are
    /// rekeyed, and the verify re-scan counts the survivor (a re-run
    /// picks it up; repairs are idempotent).
    #[test]
    fn apply_slug_fixes_tolerates_notfound_and_repairs_rest() {
        let db = Db::open_in_memory().unwrap();
        let mk = |field: &str, iso: &str| {
            let mut entry = Entry::new_legacy(
                field,
                Verb::Switch,
                Some(serde_json::json!("x")),
                None,
                Some(t_at(iso)),
                None,
                "d1",
            );
            entry.id_structure = Some("%occur_time%".to_string());
            // Hand-corrupt: slug body is short_uid, template expects
            // %occur_time% — same fixture shape as the da-noz9 tests.
            entry.slug = Slug(format!("{field}:{}", entry.short_uid()));
            let slug = entry.slug.clone();
            db.insert_entry(&entry).unwrap();
            slug
        };
        let slug_a = mk("alpha", "2024-01-01T10:00:00+00:00");
        let slug_b = mk("beta", "2024-01-01T11:00:00+00:00");

        let mut fixes = collect_slug_drift(&db, ":").unwrap().fixes;
        assert_eq!(fixes.len(), 2, "fixture: both rows drifted");
        // Simulate the cascade: row A's pre-scan slug has been
        // invalidated by the time its fix runs (get_entry → NotFound).
        let a_fix = fixes
            .iter_mut()
            .find(|f| f.existing.as_str() == slug_a.as_str())
            .unwrap();
        a_fix.existing = Slug("alpha:rekeyed-away".to_string());

        let report = apply_slug_fixes(&db, &fixes, ":").unwrap();
        assert_eq!(report.detected, 2);
        assert_eq!(
            report.applied, 1,
            "row B must be rekeyed despite row A's NotFound"
        );
        assert_eq!(report.still_mismatched, 1, "verify counts the survivor");
        assert_eq!(report.leftover_samples.len(), 1);
        assert_eq!(report.leftover_samples[0].0, slug_a.to_string());

        // Row B is actually rekeyed: only row A remains drifted.
        let remaining = collect_slug_drift(&db, ":").unwrap().fixes;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].existing.as_str(), slug_a.as_str());
        assert!(
            db.get_entry(slug_b.as_str()).is_err(),
            "row B's old slug must be gone after the rekey"
        );
    }
}
