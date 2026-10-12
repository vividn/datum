//! The `aliases` checker: spec-vs-user alias divergence — cross-spec
//! state-name collisions, orphaned chains and cycles.

use anyhow::Result;
use datum_core::db::Db;
use datum_parse::alias::{shell_split, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS};

use super::{Domain, Issue, IssueExample, Severity};

#[derive(Debug, Default)]
pub(super) struct AliasReport {
    pub(super) issues: Vec<Issue>,
    /// User/kit aliases walked for orphans and cycles.
    pub(super) aliases: usize,
}

/// Walk every field spec and every user/kit alias to surface three
/// divergence classes:
///
/// * `alias_state_collision` — two specs declare the same state name as a
///   derived `State` alias. The [`SpecAliasIndex`] resolves these
///   deterministically by the spec entry's rowid (an edit keeps it, so
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
pub(super) fn collect_alias_drift(db: &Db, fix_collisions: bool) -> Result<AliasReport> {
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
            domain: Domain::Aliases,
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
                domain: Domain::Aliases,
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
            domain: Domain::Aliases,
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
            domain: Domain::Aliases,
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

    Ok(AliasReport {
        issues,
        aliases: user_aliases.len(),
    })
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
