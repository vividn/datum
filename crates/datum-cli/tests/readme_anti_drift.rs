//! Anti-drift guard for the auto-generated Commands section in the
//! top-level `README.md`.
//!
//! The README contains a region bracketed by:
//!     <!-- BEGIN AUTOGEN: COMMANDS -->
//!     <!-- END AUTOGEN: COMMANDS -->
//! whose body is produced by `datum_parse::help::render_markdown(None)`
//! (with the title/tagline stripped). Manual edits inside that region
//! will fail this test. To regenerate the section, run
//! `cargo xtask readme`.

const README_BEGIN: &str = "<!-- BEGIN AUTOGEN: COMMANDS -->";
const README_END: &str = "<!-- END AUTOGEN: COMMANDS -->";

fn workspace_readme() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR resolves to crates/datum-cli; the workspace
    // root is two levels up.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("README.md")
}

fn extract_section(readme: &str) -> &str {
    let begin = readme
        .find(README_BEGIN)
        .expect("README is missing the BEGIN AUTOGEN marker");
    let end = readme
        .find(README_END)
        .expect("README is missing the END AUTOGEN marker");
    let after_begin = begin + README_BEGIN.len();
    assert!(end >= after_begin, "AUTOGEN markers are out of order");
    &readme[after_begin..end]
}

fn expected_body() -> String {
    // `render_markdown(None)` is a self-contained document; for embedding
    // we want only the structural body, starting at `## Usage`.
    let full = datum_parse::help::render_markdown(None);
    let idx = full
        .find("## Usage")
        .expect("render_markdown lacks `## Usage`");
    full[idx..].to_string()
}

#[test]
fn readme_commands_section_in_sync() {
    let readme = std::fs::read_to_string(workspace_readme()).expect("reading workspace README.md");
    let actual = extract_section(&readme).trim_matches('\n');
    let expected = expected_body();
    let expected = expected.trim_matches('\n');
    assert_eq!(
        actual, expected,
        "README.md `<!-- BEGIN AUTOGEN: COMMANDS --> ... <!-- END AUTOGEN: COMMANDS -->` \
         section drifted from `datum_parse::help::render_markdown(None)`. \
         Run `cargo xtask readme` to regenerate."
    );
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
