//! da-6v7z1: datum has no units, and this file is what keeps them gone.
//!
//! A unit of measure on a field (`FieldSpec.unit`, «Default unit for the
//! primary value») or a subfield (`SubfieldSpec.unit`) was a leftover of
//! the old primary-VALUE model: nothing read it, and it kept coming back
//! because every «declare the whole spec» task copied the struct's fields.
//! vividn, 2026-09-26: «I think a deeper scrub to get rid of unit is needed
//! because it keeps appearing.» The only planned form is a typed value
//! (`u://3kg`), which is not built.
//!
//! If a test here failed after you added a unit back, the answer is not to
//! relax it: a quantity's unit is part of the value the person typed
//! (`weight=75.2kg` stores `"75.2kg"`), not a column of the spec.

use std::path::{Path, PathBuf};

use crate::common;

use common::run_cli;
use datum_core::test_util::fresh_db;

#[test]
fn spec_set_refuses_unit_as_an_unknown_flag() {
    let db = fresh_db();
    for argv in [
        vec!["spec", "set", "tea", "--unit", "cups"],
        vec!["spec", "set", "tea", "--unit=cups"],
    ] {
        let err = run_cli(&argv, &db)
            .map(|_| String::new())
            .unwrap_or_else(|e| format!("{e:#}"));
        assert!(
            err.contains("--unit"),
            "`datum {}` must refuse `--unit` by name, got: {err:?}",
            argv.join(" "),
        );
    }
}

/// Where the word means something else, and why each is left alone. Only
/// paths the patterns below actually hit are listed.
const KEEP: &[(&str, &str)] = &[
    (
        "crates/datum-core/src/migrations/",
        "history: the migrations that drop entries.unit from old databases",
    ),
    (
        "crates/datum-core/tests/migration_roundtrip.rs",
        "tests those migrations",
    ),
    ("crates/datum-parse/src/time/", "time and duration units"),
    (
        "crates/datum-parity/goldens/",
        "the original datum's documents, content-hash pinned: a person's data",
    ),
    (
        "crates/datum-cli/tests/occur_cmd.rs",
        "proves `unit=` is an ordinary data key, not a reserved one",
    ),
    (
        "crates/datum-cli/tests/stored_spec_compat.rs",
        "specs stored with the old key, which must still load",
    ),
    ("crates/datum-cli/tests/unit_removed.rs", "this file"),
    ("web/src/lib/DebugBar.svelte", "a tuning knob's px / ms"),
];

/// The measurement meaning in code: a `.unit` member, the `--unit` flag, a
/// `"unit"` key, or a `unit` field declared or filled.
fn measures(line: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let bounded = |pat: &str, tail: &dyn Fn(char) -> bool| {
        line.match_indices(pat).any(|(i, _)| {
            line[i + pat.len()..]
                .chars()
                .next()
                .is_none_or(|c| !tail(c))
        })
    };
    bounded(".unit", &ident)
        || bounded("--unit", &|c| ident(c) || c == '-')
        || line.contains("\"unit\"")
        || ["Option", "string", "None", "null"]
            .iter()
            .any(|t| line.contains(&format!("unit: {t}")))
}

#[test]
fn the_guard_sees_the_shapes_it_looks_for() {
    for hit in [
        "    pub unit: Option<String>,",
        "\tunit: string | null;",
        "            unit: None,",
        "if let Some(u) = &spec.unit {",
        "vec![\"spec\", \"set\", \"tea\", \"--unit\", \"cups\"]",
        r#"{"field":"tea","unit":"cups"}"#,
    ] {
        assert!(measures(hit), "missed: {hit}");
    }
    for miss in [
        "fn unit(n: i64, name: &str) -> String {",
        "let (num, unit) = match rest {",
        ".units { color: red }",
        "--units-of-work",
        "a unit test",
    ] {
        assert!(!measures(miss), "false hit: {miss}");
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn no_measurement_unit_outside_the_keep_list() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "web/src", "docs"] {
        walk(&root.join(dir), &mut files);
    }
    assert!(files.len() > 500, "the walk found {} files", files.len());

    let mut found = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if KEEP.iter().any(|(p, _)| rel.starts_with(p)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if measures(line) {
                found.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "datum has no units (da-6v7z1); found:\n{}",
        found.join("\n")
    );
}
