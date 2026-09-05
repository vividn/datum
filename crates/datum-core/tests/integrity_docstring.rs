//! da-grkx: `integrity.rs`'s module docstring used to say cross-device sync
//! conflicts had no home yet. The commit that gave them one (da-uxgv) touched
//! neither this file nor that docstring — a commit that falsifies a NEGATIVE
//! claim made elsewhere leaves no trace in its own diff, so reviewing the diff
//! could never have caught it. The guard instead keeps the prose and the
//! column joined by name: a grep for `superseded_by` finds both, and removing
//! either side fails here.

const INTEGRITY_RS: &str = include_str!("../src/integrity.rs");
const SCHEMA_SQL: &str = include_str!("../src/schema.sql");

#[test]
fn integrity_docstring_names_the_supersession_column() {
    let doc = INTEGRITY_RS
        .lines()
        .take_while(|l| l.starts_with("//!") || l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        doc.contains("superseded_by"),
        "integrity.rs's module docstring must name `superseded_by`: it is the \
         file a reader consults to ask whether cross-device conflicts are \
         handled, and the column is the answer"
    );
    assert!(
        SCHEMA_SQL.contains("superseded_by"),
        "schema.sql must still declare `superseded_by` — if the column moved, \
         the docstring above now points at nothing"
    );
    assert!(
        !doc.contains("will live in a separate table"),
        "the docstring must not re-assert that sync conflicts have no home"
    );
}
