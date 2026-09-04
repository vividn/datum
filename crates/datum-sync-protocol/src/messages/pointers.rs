//! The claim that every wire type points back at the spec, made auditable.
//!
//! `docs/sync-protocol.md` says the wire types are kept in step with it by
//! hand and that each one's doc comment says so. That sentence is a
//! universal quantifier wearing prose: it reads like description, so nobody
//! checks it, and when it was written seven of the thirteen types carried no
//! pointer at all (da-yvu5). This module is the check. A new wire type with
//! no `docs/sync-protocol.md` in its doc comment fails the build.
//!
//! A type that is *not* locked by the document is fine — it just has to say
//! so, in those words, rather than stay silent. Silence is the failure mode:
//! a reader who finds no pointer concludes the field is free to rename.

/// The `pub mod` declarations `mod.rs` makes, so a wire-type module added
/// later cannot dodge the sweep by not being listed here.
const COVERED: &[(&str, &str)] = &[
    ("auth", include_str!("auth.rs")),
    ("sync", include_str!("sync.rs")),
];

/// Top-level `pub struct` / `pub enum` / `pub const` names in `src`, each
/// paired with the doc comment directly above it.
fn documented_items(src: &str) -> Vec<(String, String)> {
    let mut items = Vec::new();
    let mut doc = String::new();
    for line in src.lines() {
        if let Some(text) = line.strip_prefix("/// ").or(line.strip_prefix("///")) {
            doc.push_str(text);
            doc.push('\n');
        } else if line.starts_with("#[") || line.starts_with("//!") || line.is_empty() {
            // Attributes and blank lines sit between a doc block and its
            // item; module docs are not any item's doc.
        } else if let Some(name) = item_name(line) {
            items.push((name, std::mem::take(&mut doc)));
        } else {
            doc.clear();
        }
    }
    items
}

/// `pub struct Foo {` -> `Foo`, for declarations at column 0 only (fields
/// are indented, so this skips them).
fn item_name(line: &str) -> Option<String> {
    let rest = ["pub struct ", "pub enum ", "pub const "]
        .iter()
        .find_map(|kw| line.strip_prefix(kw))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[test]
fn every_wire_type_names_the_spec() {
    let mut missing = Vec::new();
    let mut checked = 0;
    for (module, src) in COVERED {
        for (name, doc) in documented_items(src) {
            checked += 1;
            if !doc.contains("docs/sync-protocol.md") {
                missing.push(format!("{module}::{name}"));
            }
        }
    }
    assert!(
        checked >= 13,
        "sweep found only {checked} wire items — parser broke?"
    );
    assert!(
        missing.is_empty(),
        "these wire types carry no pointer to docs/sync-protocol.md: {missing:?}\n\
         Add one — `Wire-locked by `docs/sync-protocol.md` §<section>.`, or a \
         sentence saying the document does not lock this shape. The doc's header \
         promises the reader every type says which it is.",
    );
}

#[test]
fn the_sweep_covers_every_module_mod_rs_declares() {
    let declared: Vec<&str> = include_str!("mod.rs")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub mod "))
        .filter_map(|r| r.split(';').next())
        .collect();
    let covered: Vec<&str> = COVERED.iter().map(|(m, _)| *m).collect();
    assert_eq!(
        declared, covered,
        "messages/mod.rs declares modules the pointer sweep does not read; \
         add them to COVERED",
    );
}
