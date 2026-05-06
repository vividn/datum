//! Color goldens — verify daydumb produces datum-byte-equal hex strings
//! across mySpec overrides and md5 fallbacks.
//!
//! These tests don't need the `parity-runtime` feature: the algorithms
//! (md5, YIQ contrast) are small enough to reimplement and check
//! directly. The `colors.tsv` fixture is the source of truth.

use datum_parity::colors;
use datum_parity::myspec;

const FIXTURE: &str = include_str!("../goldens/colors/colors.tsv");

#[derive(Debug)]
struct Row<'a> {
    lineno: usize,
    field: Option<&'a str>,
    state: Option<&'a str>,
    expected_hex: &'a str,
    source: &'a str,
    contrast_fg: &'a str,
    note: &'a str,
}

fn parse_rows(body: &str) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let mut header_seen = false;
    for (i, raw) in body.lines().enumerate() {
        let lineno = i + 1;
        let trim = raw.trim_end();
        if trim.is_empty() || trim.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trim.split('\t').collect();
        if !header_seen {
            assert_eq!(
                cols,
                vec!["field", "state", "expected_hex", "source", "contrast_fg", "note"],
                "header on line {lineno}"
            );
            header_seen = true;
            continue;
        }
        assert_eq!(
            cols.len(),
            6,
            "row on line {lineno} should have 6 columns: {trim:?}"
        );
        let field = if cols[0].is_empty() {
            None
        } else {
            Some(cols[0])
        };
        let state = match cols[1] {
            "-" | "" => None,
            other => Some(other),
        };
        rows.push(Row {
            lineno,
            field,
            state,
            expected_hex: cols[2],
            source: cols[3],
            contrast_fg: cols[4],
            note: cols[5],
        });
    }
    rows
}

#[test]
fn fixture_has_minimum_coverage() {
    let rows = parse_rows(FIXTURE);
    assert!(
        rows.len() >= 10,
        "color fixture must have ≥10 rows (bead acceptance criterion); got {}",
        rows.len()
    );
    let spec_rows = rows.iter().filter(|r| r.source == "spec").count();
    let md5_rows = rows.iter().filter(|r| r.source == "md5").count();
    assert!(
        spec_rows >= 5,
        "need ≥5 spec-override fixtures to lock mySpec loading; got {spec_rows}"
    );
    assert!(
        md5_rows >= 5,
        "need ≥5 md5-fallback fixtures; got {md5_rows}"
    );
}

#[test]
fn color_goldens_match_datum_algorithm() {
    let rows = parse_rows(FIXTURE);
    for row in &rows {
        // Look up spec overrides for this fixture.
        let field_spec_color: Option<&str> = row
            .field
            .and_then(myspec::field_spec)
            .and_then(|s| s.color);
        let state_spec_color: Option<&str> = match (row.field, row.state) {
            (Some(f), Some(s)) => myspec::state_spec(f, s).and_then(|ss| ss.color),
            _ => None,
        };

        let computed = match row.state {
            None => colors::field_color(row.field, field_spec_color),
            Some(state) => colors::state_color(
                Some(state),
                row.field,
                field_spec_color,
                state_spec_color,
            ),
        };

        assert_eq!(
            computed, row.expected_hex,
            "line {}: field={:?} state={:?} ({}): got {} want {}",
            row.lineno, row.field, row.state, row.note, computed, row.expected_hex
        );

        // Source classifier matches: a spec override either populates one
        // of the two spec-color slots, or the row is an md5 fallback.
        let any_spec = field_spec_color.is_some() || state_spec_color.is_some();
        match row.source {
            "spec" => assert!(
                any_spec,
                "line {} ({}): row is tagged `spec` but neither field nor state has a mySpec override",
                row.lineno, row.note,
            ),
            "md5" => assert!(
                row.state.is_some() || !any_spec,
                "line {} ({}): row is tagged `md5` but a field-level override would shadow it",
                row.lineno, row.note,
            ),
            other => panic!(
                "line {}: unknown source {:?}; valid: spec, md5",
                row.lineno, other,
            ),
        }
    }
}

#[test]
fn contrast_fg_matches_yiq_pick() {
    let rows = parse_rows(FIXTURE);
    for row in &rows {
        let want_fg = row.contrast_fg;
        let got_fg = colors::contrast_text_color(row.expected_hex);
        assert_eq!(
            got_fg, want_fg,
            "line {}: contrast_fg for {} ({}): got {} want {}",
            row.lineno, row.expected_hex, row.note, got_fg, want_fg,
        );
    }
}
