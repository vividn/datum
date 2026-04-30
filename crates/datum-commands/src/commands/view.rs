//! `datum view <name>` — execute a named SELECT and render the rows.
//!
//! The view's SQL text comes from the `views` table (installed via
//! `datum kit install` or, eventually, via `datum view set`). Slice 4
//! ships the read path only: the SQL is executed under
//! `PRAGMA query_only = 1` so even a malicious or buggy view can't issue
//! DDL/DML against the entries DB.
//!
//! Output formats:
//!
//! - `pretty`  — aligned columnar text, suitable for a terminal.
//! - `tsv`     — tab-separated, lossless for piping to `awk`/`cut`.
//! - `csv`     — RFC4180-ish; quoted when the cell contains `,` or `"`.
//! - `json`    — `[{column: value, …}, …]`, one record per row.
//! - `auto`    — pretty when stdout is a TTY, otherwise tsv. Default.
//!
//! `--watch` polls the query every two seconds and clears the screen
//! before each redraw. The bead notes a follow-up to swap the poll for
//! the daemon's entry-mutation hook.

use std::io::IsTerminal;

use anyhow::{Context, Result};
use datum_core::db::{Db, ViewResult};
use serde_json::Value as JsonValue;

use crate::args::{ViewArgs, ViewFormat};

/// Typed entry point — the dispatcher in `cmd.rs` calls this.
pub fn view(args: ViewArgs, db: &Db) -> Result<()> {
    if args.watch {
        loop {
            // ANSI clear-screen + cursor home so a long row count doesn't
            // accumulate scrollback. Inert on non-VT terminals.
            print!("\x1b[2J\x1b[H");
            render_once(&args, db)?;
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    } else {
        render_once(&args, db)
    }
}

pub fn view_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("view_from_argv: stub — R2 extracts bespoke parser")
}

pub fn view_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("view_from_string: stub — R2 extracts bespoke parser")
}

fn render_once(args: &ViewArgs, db: &Db) -> Result<()> {
    let row = db
        .get_view(&args.name)
        .with_context(|| format!("look up view '{}'", args.name))?
        .ok_or_else(|| anyhow::anyhow!("no view named '{}'", args.name))?;

    let result = db
        .execute_view(&row.sql_text)
        .with_context(|| format!("execute view '{}'", args.name))?;

    let format = resolve_format(args.format);
    match format {
        ViewFormat::Auto => unreachable!("resolve_format never returns Auto"),
        ViewFormat::Pretty => render_pretty(&result),
        ViewFormat::Tsv => render_tsv(&result),
        ViewFormat::Csv => render_csv(&result),
        ViewFormat::Json => render_json(&result)?,
    }
    Ok(())
}

/// Resolve `Auto` against the current stdout. All concrete formats pass
/// through unchanged.
fn resolve_format(f: ViewFormat) -> ViewFormat {
    match f {
        ViewFormat::Auto => {
            if std::io::stdout().is_terminal() {
                ViewFormat::Pretty
            } else {
                ViewFormat::Tsv
            }
        }
        other => other,
    }
}

/// Pretty: aligned columnar text. Column width = max(header, max cell).
/// Numeric-looking cells are right-aligned to keep the column readable.
fn render_pretty(r: &ViewResult) {
    if r.columns.is_empty() {
        println!("(no columns)");
        return;
    }
    if r.rows.is_empty() {
        println!("{}", r.columns.join("  "));
        println!("(0 rows)");
        return;
    }

    let cells: Vec<Vec<String>> = r
        .rows
        .iter()
        .map(|row| row.iter().map(format_cell_plain).collect())
        .collect();

    let mut widths: Vec<usize> = r.columns.iter().map(|c| c.chars().count()).collect();
    for row in &cells {
        for (i, c) in row.iter().enumerate() {
            let w = c.chars().count();
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }

    let numeric: Vec<bool> = (0..r.columns.len())
        .map(|i| {
            cells
                .iter()
                .all(|row| !row[i].is_empty() && is_numeric_cell(&row[i]))
        })
        .collect();

    print_row(&r.columns, &widths, &numeric);
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    print_row(&sep, &widths, &vec![false; widths.len()]);
    for row in &cells {
        print_row(row, &widths, &numeric);
    }
    println!(
        "({} row{})",
        r.rows.len(),
        if r.rows.len() == 1 { "" } else { "s" }
    );
}

fn print_row(cells: &[String], widths: &[usize], numeric: &[bool]) {
    let mut line = String::new();
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        if numeric[i] {
            line.push_str(&format!("{c:>w$}", w = widths[i]));
        } else {
            line.push_str(&format!("{c:<w$}", w = widths[i]));
        }
    }
    println!("{line}");
}

fn render_tsv(r: &ViewResult) {
    println!("{}", r.columns.join("\t"));
    for row in &r.rows {
        let cells: Vec<String> = row.iter().map(format_cell_tsv).collect();
        println!("{}", cells.join("\t"));
    }
}

fn render_csv(r: &ViewResult) {
    println!(
        "{}",
        r.columns
            .iter()
            .map(|c| csv_escape(c))
            .collect::<Vec<_>>()
            .join(",")
    );
    for row in &r.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|v| csv_escape(&format_cell_plain(v)))
            .collect();
        println!("{}", cells.join(","));
    }
}

fn render_json(r: &ViewResult) -> Result<()> {
    let arr: Vec<JsonValue> = r
        .rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::with_capacity(r.columns.len());
            for (i, col) in r.columns.iter().enumerate() {
                obj.insert(col.clone(), row[i].clone());
            }
            JsonValue::Object(obj)
        })
        .collect();
    let s = serde_json::to_string_pretty(&JsonValue::Array(arr))?;
    println!("{s}");
    Ok(())
}

/// Render a JSON cell value in the natural form for tabular output.
/// Strings drop their JSON quotes; everything else round-trips as JSON
/// so the user can still distinguish e.g. `null` from `"null"`.
fn format_cell_plain(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => "".to_string(),
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Tab/newline scrubbing for TSV cells. Keeps the column count intact
/// and prevents accidental row breaks from embedded literals.
fn format_cell_tsv(v: &JsonValue) -> String {
    let s = format_cell_plain(v);
    s.replace(['\t', '\n', '\r'], " ")
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn is_numeric_cell(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mk_result() -> ViewResult {
        ViewResult {
            columns: vec!["field".into(), "n".into()],
            rows: vec![
                vec![json!("coffee"), json!(7)],
                vec![json!("mood"), json!(3)],
            ],
        }
    }

    #[test]
    fn json_format_emits_array_of_objects() {
        let r = mk_result();
        let arr: Vec<JsonValue> = r
            .rows
            .iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (i, col) in r.columns.iter().enumerate() {
                    obj.insert(col.clone(), row[i].clone());
                }
                JsonValue::Object(obj)
            })
            .collect();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].get("field"), Some(&json!("coffee")));
        assert_eq!(arr[0].get("n"), Some(&json!(7)));
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_escape("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn format_cell_plain_strips_string_quotes() {
        assert_eq!(format_cell_plain(&json!("coffee")), "coffee");
        assert_eq!(format_cell_plain(&json!(42)), "42");
        assert_eq!(format_cell_plain(&json!(null)), "");
        assert_eq!(format_cell_plain(&json!(true)), "true");
    }

    #[test]
    fn tsv_scrubs_embedded_separators() {
        assert_eq!(format_cell_tsv(&json!("a\tb")), "a b");
        assert_eq!(format_cell_tsv(&json!("a\nb")), "a b");
    }

    #[test]
    fn is_numeric_cell_recognizes_floats_and_ints() {
        assert!(is_numeric_cell("42"));
        assert!(is_numeric_cell("3.14"));
        assert!(is_numeric_cell("-1"));
        assert!(!is_numeric_cell("hello"));
        assert!(!is_numeric_cell(""));
    }
}
