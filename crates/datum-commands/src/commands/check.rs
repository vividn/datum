//! `datum check` — database consistency checks.
//!
//! Runs a battery of SQL- and Rust-level sanity queries and produces a
//! grouped report. Errors set the process exit code to 1; warnings do not.

use anyhow::Result;
use datum_core::db::Db;
use datum_core::derive_verb;
use datum_parse::alias::{shell_split, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS};
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::args::CheckArgs;

/// Typed API for `datum check`. Exits the process with status 1 when the
/// report contains any [`Severity::Error`] items.
///
/// `--verbs` narrows the report to verb-drift detection only.
/// `--aliases` narrows to spec-vs-user alias divergence checks
/// (state-name collisions, orphans, cycles).
/// `--fix` rewrites the stored `verb` column for any drifted live row to
/// match `derive_verb`; for `--aliases`, it adds an explicit
/// first-saved-wins resolution annotation to each state collision (no DB
/// mutation — the precedence is already deterministic).
pub fn check(args: CheckArgs, db: &Db) -> Result<()> {
    let report = if args.verbs {
        let verb_report = collect_verb_drift(db)?;
        if args.fix {
            apply_verb_fixes(db, &verb_report.fixes)?;
        }
        Report {
            issues: verb_report.issues,
            summary: Summary {
                errors: 0, // recomputed below
                warnings: 0,
                clean: false,
            },
        }
        .with_recomputed_summary()
    } else if args.aliases {
        let alias_report = collect_alias_drift(db, args.fix)?;
        Report {
            issues: alias_report.issues,
            summary: Summary {
                errors: 0,
                warnings: 0,
                clean: false,
            },
        }
        .with_recomputed_summary()
    } else {
        let mut report = collect_issues(db)?;
        let verb_report = collect_verb_drift(db)?;
        if args.fix {
            apply_verb_fixes(db, &verb_report.fixes)?;
        }
        report.issues.extend(verb_report.issues);
        let alias_report = collect_alias_drift(db, args.fix)?;
        report.issues.extend(alias_report.issues);
        report.with_recomputed_summary()
    };

    if args.json {
        println!("{}", render_json(&report)?);
    } else {
        print!("{}", render_human(&report));
    }
    if report.has_errors() {
        std::process::exit(1);
    }
    Ok(())
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

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<IssueExample>,
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

/// One drifted row queued for `--fix` to rewrite.
#[derive(Debug, Clone)]
struct VerbFix {
    slug: String,
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
    let entries = db.all_entries()?;

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
        });
    }

    Ok(VerbReport { issues, fixes })
}

/// Rewrite the stored `verb` column for each queued drifted row.
fn apply_verb_fixes(db: &Db, fixes: &[VerbFix]) -> Result<()> {
    for fix in fixes {
        db.set_entry_verb(&fix.slug, &fix.expected_verb)?;
    }
    Ok(())
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
        });
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
        });
    }

    // 2. content_hash mismatches (ERROR)
    let entries = db.all_entries()?;
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
        });
    }

    // 4. orphaned extra_data keys — field has a spec but extra_data contains
    //    keys that aren't declared in the spec's subfields (WARNING)
    let mut orphan_detail: Option<(String, String, String)> = None; // (id, field, key)
    let mut orphan_count = 0usize;
    for e in &entries {
        let Some(spec) = registry.field_spec(&e.field) else {
            continue;
        };
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
        });
    }

    // 5. tombstones — rows with deleted_at set (WARNING)
    let tombstones = db.tombstone_ids()?;
    if !tombstones.is_empty() {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "tombstone_without_live_row",
            message: format!(
                "{} tombstone{} present (deleted rows with no surviving live counterpart)",
                tombstones.len(),
                if tombstones.len() == 1 { "" } else { "s" },
            ),
            count: tombstones.len(),
            example: Some(IssueExample {
                id: Some(tombstones[0].clone()),
                field: None,
                detail: None,
            }),
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
        if let Some(ex) = &issue.example {
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

    /// `check --verbs` (typed API) narrows the report to the verb-drift
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
                "unexpected category in --verbs run: {}",
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
    /// and `dm check --aliases` surfaces it as `alias_state_collision`.
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

    /// `dm check --aliases` (`args.aliases = true`) narrows the report to
    /// alias categories only — verb drift and other default-battery
    /// categories are excluded.
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
                "unexpected category in --aliases run: {}",
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
}
