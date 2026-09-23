//! The documented environment-variable table has to keep up with the
//! config layer that reads them.
//!
//! The README's table was maintained by hand and fell behind:
//! `DATUM_ID_DELIMITER` has been user-facing since R6 (`config.rs`
//! resolves it for every slug template) and was never listed, and
//! `DATUM_EDITOR` was written up as `EDITOR`. Generating the table from
//! `datum_parse::help::ENV_VARS` removes the README's copy; this test is
//! the other half — it removes the chance of the TABLE being the stale
//! one (da-ce7n3).
//!
//! The check is one-directional on purpose: every `DATUM_*` the config
//! layer reads must be documented, while the table may document more
//! (`DATUM_COLOR` is read by the output layer, not by `Config::load`).

/// The one module that layers CLI flag > env > toml > default. A
/// user-facing `DATUM_*` variable is read here or it is not user-facing.
///
/// ⚠ The scan below reads this file as TEXT, so it sees a variable only
/// where the name is spelled out as a literal. That is sound today
/// because every `read_env` / `env::var` call site in `config.rs` passes
/// one — and it is a property of that code, not something anything here
/// enforces. A `read_env(&format!("DATUM_{suffix}"))`, or the names moved
/// to consts in another module, would make this scan go blind while the
/// test stayed green: exactly the failure of the hand-kept table it
/// replaced. If you write such a call site, this guard is what you are
/// breaking, and it needs to read the names rather than the file.
const CONFIG_SOURCE: &str = include_str!("../../datum-commands/src/config.rs");

/// Variables `config.rs` mentions that are not settings a user sets.
/// Keep this list short and justified — it is the escape hatch, and an
/// entry added without a reason is how the guard stops guarding.
const NOT_A_USER_SETTING: &[&str] = &[
    // Named only in the doc comment that explains the precedence order.
    "DATUM_",
];

fn documented() -> std::collections::BTreeSet<&'static str> {
    datum_parse::help::ENV_VARS.iter().map(|v| v.name).collect()
}

/// Every `DATUM_*` literal in the config layer, in source order.
fn env_vars_config_reads() -> Vec<String> {
    let mut found = Vec::new();
    let bytes = CONFIG_SOURCE.as_bytes();
    let mut i = 0;
    while let Some(off) = CONFIG_SOURCE[i..].find("DATUM_") {
        let start = i + off;
        let mut end = start + "DATUM_".len();
        while end < bytes.len() && (bytes[end].is_ascii_uppercase() || bytes[end] == b'_') {
            end += 1;
        }
        let name = &CONFIG_SOURCE[start..end];
        if !NOT_A_USER_SETTING.contains(&name) && !found.iter().any(|f: &String| f == name) {
            found.push(name.to_string());
        }
        i = end;
    }
    found
}

#[test]
fn env_table_covers_what_the_config_layer_reads() {
    let documented = documented();
    let read = env_vars_config_reads();
    assert!(
        !read.is_empty(),
        "found no DATUM_* names in config.rs — the scan broke, not the table"
    );
    let undocumented: Vec<&String> = read
        .iter()
        .filter(|name| !documented.contains(name.as_str()))
        .collect();
    assert!(
        undocumented.is_empty(),
        "the config layer reads {undocumented:?}, which `datum_parse::help::ENV_VARS` \
         does not document — add them there (the README's table is generated from it, \
         via `cargo xtask readme`)"
    );
}

/// The two names the audit caught, pinned by name so a rename cannot
/// quietly drop them again.
#[test]
fn the_table_names_id_delimiter_and_datum_editor() {
    let documented = documented();
    assert!(
        documented.contains("DATUM_ID_DELIMITER"),
        "DATUM_ID_DELIMITER is user-facing (config.rs resolves it for slug templates)"
    );
    let editor = datum_parse::help::ENV_VARS
        .iter()
        .find(|v| v.name == "DATUM_EDITOR")
        .expect("DATUM_EDITOR is documented");
    assert!(
        editor.summary.contains("FIRST"),
        "`datum edit` reads DATUM_EDITOR BEFORE EDITOR; the summary has to say so, \
         got: {}",
        editor.summary
    );
}
