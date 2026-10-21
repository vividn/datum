//! Anti-drift guard for the auto-generated regions of the docs: each lives
//! in the file `xtask::AUTOGEN_BLOCKS` names for it (`docs/commands.md`,
//! `docs/aliases.md`, `docs/configuration.md`; all three were in the top-level
//! `README.md` until 2026-10-20).
//!
//! Each region is bracketed by:
//!     <!-- BEGIN AUTOGEN: NAME -->
//!     <!-- END AUTOGEN: NAME -->
//! and its body is produced by `xtask::autogen_section(NAME)` — the very
//! function `cargo xtask readme` writes with, called here rather than
//! reimplemented, because a test that reimplements the generator only
//! proves the two reimplementations agree (da-ce7n3).
//!
//! Manual edits inside a region fail this test. To regenerate them all,
//! run `cargo xtask readme`.

fn workspace_file(rel: &str) -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR resolves to crates/datum-cli; the workspace
    // root is two levels up.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(workspace_file(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

fn extract_section<'a>(readme: &'a str, block: &str) -> &'a str {
    let begin_marker = format!("<!-- BEGIN AUTOGEN: {block} -->");
    let end_marker = format!("<!-- END AUTOGEN: {block} -->");
    let begin = readme
        .find(&begin_marker)
        .unwrap_or_else(|| panic!("the file is missing `{begin_marker}`"));
    let end = readme
        .find(&end_marker)
        .unwrap_or_else(|| panic!("the file is missing `{end_marker}`"));
    let after_begin = begin + begin_marker.len();
    assert!(
        end >= after_begin,
        "`{block}` AUTOGEN markers are out of order"
    );
    &readme[after_begin..end]
}

/// Every generated region still says what the generator says. `COMMANDS` was the only one guarded for a long time, and the
/// regions outside it drifted exactly as you would expect: an 80-line
/// hand copy of `datum help alias` that reached the same false claim
/// from the other direction, and an env-var table that never learned
/// about `DATUM_ID_DELIMITER`.
#[test]
fn readme_autogen_sections_in_sync() {
    for (file, block) in xtask::AUTOGEN_BLOCKS {
        let doc = read(file);
        let actual = extract_section(&doc, block).trim_matches('\n');
        let expected = xtask::autogen_section(block).expect("generator for a listed block");
        let expected = expected.trim_matches('\n');
        assert_eq!(
            actual, expected,
            "{file} `<!-- BEGIN AUTOGEN: {block} --> … <!-- END AUTOGEN: {block} -->` \
             drifted from `xtask::autogen_section(\"{block}\")`. \
             Run `cargo xtask readme` to regenerate."
        );
    }
}

/// Every block the generator knows about is actually spliced into its
/// file. Without this, adding a generator and forgetting the markers
/// would leave the section hand-written and silently unguarded — the
/// exact failure this bead removed.
#[test]
fn every_generated_block_has_markers_in_its_file() {
    for (file, block) in xtask::AUTOGEN_BLOCKS {
        assert!(
            read(file).contains(&format!("<!-- BEGIN AUTOGEN: {block} -->")),
            "`{block}` has a generator but no region in {file}"
        );
    }
}

/// Round-trip sanity: every command in the table renders both Markdown
/// and roff for `Some(name)` without panicking, and the roff page
/// references the same command name in its `.SH NAME` section. This
/// guards against renderer regressions across the introspection table.
#[test]
fn render_adapters_round_trip_command_names() {
    for spec in datum_parse::help::COMMANDS {
        let md = datum_parse::help::render_markdown(Some(spec.name));
        assert!(
            md.contains(&format!("# `datum {}`", spec.name)),
            "markdown for `{}` lost its header",
            spec.name
        );
        let roff = datum_parse::help::render_roff(Some(spec.name));
        let expected_th = format!(".TH \"DATUM-{}\"", spec.name.to_ascii_uppercase());
        assert!(
            roff.starts_with(&expected_th),
            "roff for `{}` lost its .TH header (got: {:.40}…)",
            spec.name,
            roff,
        );
    }
}
