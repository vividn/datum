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
///   that is no known subcommand, kit command or other resolvable alias.
///   Typical cause: a downstream alias's owning spec was deleted.
/// * `alias_cycle` — expanding a user/kit alias revisits a name in the
///   chain. The `expand_argv` pre-pass would bail at
///   [`MAX_ALIAS_HOPS`], but the cycle would otherwise go undetected.
/// * `alias_shadowed_by_command` — a user, kit or derived alias whose name
///   a command takes, which runs instead (da-kba2e); each is listed with
///   the full form that still reaches it.
/// * `kit_command_shadowed_by_field` — a kit command whose name is a
///   field's word, made after the install; typing it logs to the field
///   (da-9leah).
/// * `spec_alias_names_stale` — an error: `spec_alias_names` lacks a row a
///   fill writes, holds one it does not, or a name read by it is not the
///   index's (da-ppkv6).
///
/// `--fix` (`fix = true`) refills `spec_alias_names`, and annotates
/// state-collision examples with the explicit first-saved-wins
/// resolution, which is already deterministic. Orphans and cycles need
/// user judgment per the bead direction and are report-only.
pub(super) fn collect_alias_drift(db: &Db, fix: bool) -> Result<AliasReport> {
    let mut issues: Vec<Issue> = Vec::new();

    // 1. Cross-spec state-name collisions (already tracked by the index).
    let collisions: Vec<_> = {
        let index = db.spec_alias_index()?;
        index.collisions.clone()
    };
    if !collisions.is_empty() {
        let first = &collisions[0];
        let detail = if fix {
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

    // 1c. `spec_alias_names`, the names a command's first word is read
    // by, against the specs (da-ppkv6): a migration that rebuilt `entries`
    // and did not fill it again leaves a word that resolves to nothing.
    let drift = db.spec_alias_names_drift()?;
    if !drift.is_empty() {
        let row = |what: &str, (name, spec): &(String, i64)| {
            format!("'{name}' of spec row {spec} {what}")
        };
        let found: Vec<String> = (drift.missing.iter().map(|r| row("missing", r)))
            .chain(drift.stale.iter().map(|r| row("stale", r)))
            .chain(
                drift
                    .misread
                    .iter()
                    .map(|n| format!("'{n}' read by name is not the index's")),
            )
            .collect();
        if fix {
            db.refill_spec_alias_names()?;
        }
        issues.push(Issue {
            domain: Domain::Aliases,
            severity: Severity::Error,
            category: "spec_alias_names_stale",
            message: format!(
                "spec_alias_names disagrees with the specs in {n} place{s}{fixed}",
                n = found.len(),
                s = if found.len() == 1 { "" } else { "s" },
                fixed = if fix {
                    " (refilled)"
                } else {
                    " (--fix refills it)"
                },
            ),
            count: found.len(),
            example: Some(IssueExample {
                id: None,
                field: None,
                detail: Some(found[0].clone()),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    let user_aliases = db.list_user_aliases()?;

    // 1d. A name `alias set` and `kit install` now refuse, stored before
    // they did (da-uqv4r): nothing can type it as a first word.
    let unplain: Vec<&str> = user_aliases
        .iter()
        .map(|a| a.name.as_str())
        .filter(|n| !datum_parse::alias::is_plain_alias_name(n))
        .collect();
    if let Some(first) = unplain.first() {
        issues.push(Issue {
            domain: Domain::Aliases,
            severity: Severity::Warning,
            category: "alias_name_not_plain",
            message: format!(
                "{n} alias name{s} not a plain word; `datum alias remove` {it} \
                 and set {it} again under one",
                n = unplain.len(),
                s = if unplain.len() == 1 { " is" } else { "s are" },
                it = if unplain.len() == 1 { "it" } else { "them" },
            ),
            count: unplain.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(first.to_string()),
                detail: Some(
                    unplain
                        .iter()
                        .map(|n| format!("'{n}'"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            }),
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }

    // 1e. An alias a command took the name of (da-kba2e): the command runs
    // instead, so a user's alias saved before the command was, or a
    // state's, a field's own word, never fires by its name.
    let shadowed: Vec<datum_core::field_spec::Alias> = {
        let index = db.spec_alias_index()?;
        // A user or kit row (both `list_user_aliases`) shadows a derived
        // one of its name, as lookup does: that derived one is lost to the
        // row, which `setup` says, not to the command, so it is not listed.
        // Within a source a name is one alias: a user or kit row's slug is
        // its name, and `index.entries` is keyed by it (a second spec's is
        // in `collisions`, `alias_state_collision` above).
        let named: std::collections::BTreeMap<&str, &datum_core::field_spec::Alias> = index
            .entries
            .values()
            .chain(&user_aliases)
            .filter(|a| KNOWN_SUBCOMMANDS.contains(&a.name.as_str()))
            .map(|a| (a.name.as_str(), a))
            .collect();
        named.into_values().cloned().collect()
    };
    if let Some(first) = shadowed.first() {
        let line = |a: &datum_core::field_spec::Alias| {
            format!(
                "'{name}' → datum {to}: `datum {name}` runs the command; type `datum {to}`",
                name = a.name,
                to = a.expands_to,
            )
        };
        issues.push(Issue {
            domain: Domain::Aliases,
            severity: Severity::Warning,
            category: "alias_shadowed_by_command",
            message: format!(
                "{n} alias{es} named after a command, which runs instead",
                n = shadowed.len(),
                es = if shadowed.len() == 1 { "" } else { "es" },
            ),
            count: shadowed.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(first.name.clone()),
                detail: Some(line(first)),
            }),
            entries: Vec::new(),
            rendered_block: shadowed.iter().map(|a| line(a) + "\n").collect(),
        });
    }

    // 1f. A kit command a field took the name of, made after the install
    // refused one (da-9leah): the field's word is expanded first, so typing
    // the command logs to the field. A user or kit alias row of the name
    // takes the word before the field does, as lookup reads it.
    let mut dead: Vec<(String, String)> = Vec::new();
    for c in db.list_kit_commands()? {
        if user_aliases.iter().any(|a| a.name == c.name) {
            continue;
        }
        if let Some(a) = db.field_word(&c.name)? {
            dead.push((c.name, a.expands_to));
        }
    }
    if let Some((first, _)) = dead.first() {
        let line = |(name, to): &(String, String)| {
            format!("'{name}': `datum {name}` runs `datum {to}`, not the kit command")
        };
        issues.push(Issue {
            domain: Domain::Aliases,
            severity: Severity::Warning,
            category: "kit_command_shadowed_by_field",
            message: format!(
                "{n} kit command{s} named like a field's word, which logs to the field instead",
                n = dead.len(),
                s = if dead.len() == 1 { "" } else { "s" },
            ),
            count: dead.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(first.clone()),
                detail: Some(line(&dead[0])),
            }),
            entries: Vec::new(),
            rendered_block: dead.iter().map(|d| line(d) + "\n").collect(),
        });
    }

    // 2 + 3. Orphans and cycles in user/kit alias chains. Spec-derived
    // aliases never need this walk — their `expands_to` always begins with
    // a KNOWN_SUBCOMMAND.
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
                            "{path} → '{broken_at}' (no known alias, subcommand or kit command)",
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
    /// Chain reaches a known subcommand, a kit command's view or a positional
    /// placeholder.
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
/// expansion. Stops on a known subcommand, a kit command's view, a positional
/// placeholder, an unresolved name, a revisited name, or after
/// [`MAX_ALIAS_HOPS`] hops
/// (which we treat as a cycle — a chain that long is unreachable in
/// practice).
fn analyze_chain(db: &Db, start: &str) -> Result<ChainOutcome> {
    let mut visited: Vec<String> = vec![start.to_string()];
    let mut current = start.to_string();
    for _ in 0..MAX_ALIAS_HOPS {
        // An alias of the name takes the word first, then a kit command's
        // expansion, as the resolver reads them; a kit command with none
        // runs its view.
        let expands_to = if let Some(alias) = db.lookup_alias(&current)? {
            alias.expands_to
        } else if let Some(command) = db.get_kit_command(&current)? {
            let Some(expands_to) = command.expands_to else {
                return Ok(ChainOutcome::Resolved);
            };
            expands_to
        } else {
            // The starting alias is guaranteed resolvable by its caller (we
            // just enumerated it), so a None here means we hit an unresolved
            // hop mid-walk.
            return Ok(ChainOutcome::Orphan {
                broken_at: current,
                chain: visited,
            });
        };
        let tokens = shell_split(&expands_to);
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

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;
    use datum_core::field_spec::{Alias, FieldSpec};

    /// A state's alias and a user's stored before a command took its name
    /// are each listed with the full form that still reaches it (da-kba2e).
    #[test]
    fn alias_named_after_a_command_is_listed() {
        let db = Db::open_in_memory().unwrap();
        let mut payment = FieldSpec::new("payment", Action::Switch);
        payment.allowed_states = Some(vec!["cash".into(), "check".into()]);
        payment.regenerate_derived_aliases();
        db.save_field_spec(&payment).unwrap();
        db.save_alias(&Alias {
            name: "stats".to_string(),
            expands_to: "occur coffee".to_string(),
            description: None,
        })
        .unwrap();

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_shadowed_by_command")
            .expect("shadowed aliases must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 2);
        assert_eq!(
            issue.rendered_block,
            "'check' → datum switch payment check: `datum check` runs the command; \
             type `datum switch payment check`\n\
             'stats' → datum occur coffee: `datum stats` runs the command; \
             type `datum occur coffee`\n"
        );
    }

    /// A field made after a kit command of its name lists the command,
    /// which its word now hides (da-9leah).
    #[test]
    fn kit_command_named_like_a_later_field_is_listed() {
        let db = Db::open_in_memory().unwrap();
        let vocab: datum_core::kit::CommandEntry =
            toml::from_str("name = \"vocab\"\nexpands_to = \"tail word\"").unwrap();
        db.install_kit_commands("k", &[vocab], false).unwrap();
        let found = |db: &Db| {
            collect_alias_drift(db, false)
                .unwrap()
                .issues
                .into_iter()
                .find(|i| i.category == "kit_command_shadowed_by_field")
        };
        assert!(found(&db).is_none());

        let mut spec = FieldSpec::new("vocab", Action::Occur);
        spec.regenerate_derived_aliases();
        db.save_field_spec(&spec).unwrap();
        let issue = found(&db).expect("the hidden command must be reported");
        assert_eq!(issue.count, 1);
        assert_eq!(
            issue.rendered_block,
            "'vocab': `datum vocab` runs `datum occur vocab`, not the kit command\n"
        );

        // A user's alias of the name takes the word before the field does.
        db.save_alias(&Alias {
            name: "vocab".to_string(),
            expands_to: "tail word".to_string(),
            description: None,
        })
        .unwrap();
        assert!(found(&db).is_none());
    }

    /// An alias reaching a kit command, directly or through another alias,
    /// is no orphan, and one reaching nothing still is, through a kit
    /// command's expansion too (da-sunqa).
    #[test]
    fn alias_to_a_kit_command_is_no_orphan() {
        let db = Db::open_in_memory().unwrap();
        let command = |name: &str, expands_to: &str| -> datum_core::kit::CommandEntry {
            toml::from_str(&format!("name = \"{name}\"\nexpands_to = \"{expands_to}\"")).unwrap()
        };
        db.install_kit_commands(
            "k",
            &[
                command("ll", "tail"),
                command("broken", "nothing"),
                toml::from_str("name = \"seen\"\nview = \"v\"").unwrap(),
            ],
            false,
        )
        .unwrap();
        for (name, expands_to) in [
            ("лл", "ll"),
            ("l2", "лл --flag"),
            ("gone", "nothing"),
            ("via", "broken"),
            ("view", "seen"),
        ] {
            db.save_alias(&Alias {
                name: name.to_string(),
                expands_to: expands_to.to_string(),
                description: None,
            })
            .unwrap();
        }

        let report = collect_alias_drift(&db, false).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "alias_orphan")
            .expect("the alias to nothing must be reported");
        assert_eq!(issue.count, 2);
        let orphans: Vec<_> = db
            .list_user_aliases()
            .unwrap()
            .into_iter()
            .filter(|a| {
                matches!(
                    analyze_chain(&db, &a.name).unwrap(),
                    ChainOutcome::Orphan { .. }
                )
            })
            .map(|a| a.name)
            .collect();
        assert_eq!(orphans, ["gone", "via"]);
    }
}
