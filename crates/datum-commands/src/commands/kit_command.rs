//! Level 2 kit command renderer (slice 6 / da-d4t.6).
//!
//! Dispatches an unknown verb that resolves to a Level-2 kit command
//! (the entry `command:<name>`, da-fwwtn): runs the named view, projects/labels/sorts/colorises
//! the rows, and prints. Level 1 (alias-style) commands never reach this
//! module — they're handled by the alias-expansion pre-pass.
//!
//! Output goes through the same `render_pretty` style as `datum view`,
//! plus optional ANSI colour applied per row when `color_rules` matches.
//! The renderer is read-only: it can only execute the named view via
//! `Db::execute_view`, which flips `PRAGMA query_only = 1`.

use anyhow::{Context, Result};
use chrono::Utc;
use datum_core::db::{Db, InstalledKitCommand, ViewResult};
use datum_core::kit_color_rule::{self, ColorRule, RowLookup};
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::render::visible_width;

/// Run a Level 2 command by name. `extra_args` carries any tokens after
/// the command name on the original argv (e.g. `--watch`); for slice 6
/// the only flag we honour is `--watch`, gated by `flags = ["watch"]`.
pub fn run(name: &str, extra_args: &[String], db: &Db) -> Result<()> {
    let cmd = db
        .get_kit_command(name)
        .with_context(|| format!("look up kit command '{name}'"))?
        .ok_or_else(|| anyhow::anyhow!("no kit command named '{name}'"))?;

    if cmd.script_text.is_some() {
        anyhow::bail!(
            "kit command '{}': scripted commands not yet supported (Level 3 deferred)",
            cmd.name
        );
    }
    if cmd.expands_to.is_some() {
        // Level 1 should have been handled by the alias-style pre-pass;
        // arriving here means the dispatcher logic is wrong. Surface it
        // rather than silently doing nothing.
        anyhow::bail!(
            "internal: kit command '{}' is Level 1 (expands_to); should be handled by pre-pass",
            cmd.name
        );
    }

    let view_name = cmd.view_name.as_deref().ok_or_else(|| {
        anyhow::anyhow!("kit command '{}' has neither expands_to nor view", cmd.name)
    })?;

    let watch_flag_allowed = cmd.flags.iter().any(|f| f == "watch");
    let want_watch = extra_args.iter().any(|a| a == "--watch");
    if want_watch && !watch_flag_allowed {
        anyhow::bail!(
            "kit command '{}': --watch is not enabled (declare flags = [\"watch\"])",
            cmd.name
        );
    }

    if want_watch {
        loop {
            print!("\x1b[2J\x1b[H");
            // Flush so the clear is visible even if the render errors
            // before printing anything.
            let _ = std::io::Write::flush(&mut std::io::stdout());
            // A watch must be the most resilient surface, not the least
            // (da-sd25): show errors in place (transient SQLITE_BUSY, a
            // momentarily-broken view, even a deleted view the user may
            // recreate) and retry on the next tick rather than dying.
            if let Err(e) = render_once(&cmd, view_name, db) {
                eprintln!("watch '{}': {e:#}", cmd.name);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    } else {
        render_once(&cmd, view_name, db)
    }
}

fn render_once(cmd: &InstalledKitCommand, view_name: &str, db: &Db) -> Result<()> {
    let view_row = db
        .get_view(view_name)
        .with_context(|| format!("look up view '{view_name}'"))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "kit command '{}' references missing view '{view_name}'",
                cmd.name
            )
        })?;
    let result = db
        .execute_view(&view_row.sql_text)
        .with_context(|| format!("execute view '{view_name}'"))?;
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    print!("{}", render(cmd, result, tty)?);
    Ok(())
}

/// Sort and colour read the FULL view row; `columns` decides only what is
/// shown (da-ggzv1). A sort column or rule identifier the view lacks is
/// refused, naming it, rather than silently ignored.
fn render(cmd: &InstalledKitCommand, mut view: ViewResult, color: bool) -> Result<String> {
    let rules = parse_rules(&cmd.color_rules)?;
    for id in rules.iter().flat_map(|r| r.when.idents()) {
        col_index(&cmd.name, &view.columns, id, "a color rule")?;
    }
    if let Some(col) = cmd.sort_col.as_deref() {
        let idx = col_index(&cmd.name, &view.columns, col, "sort")?;
        view.rows.sort_by(|a, b| compare_cells(&a[idx], &b[idx]));
    }
    let now = Utc::now();
    let colors: Vec<Option<&str>> = view
        .rows
        .iter()
        .map(|row| {
            color
                .then(|| row_color(&rules, &view.columns, row, now))
                .flatten()
        })
        .collect();
    let projected = project(&cmd.name, &view, &cmd.columns)?;
    let headers = header_labels(&projected.columns, &cmd.labels);
    let formatted = format_cells(&projected, &cmd.formats);
    Ok(render_pretty(&headers, &formatted, &colors))
}

/// Where `name` sits in the view's row, or an error naming the command,
/// the column and what named it.
fn col_index(cmd: &str, columns: &[String], name: &str, by: &str) -> Result<usize> {
    columns.iter().position(|c| c == name).ok_or_else(|| {
        anyhow::anyhow!("kit command '{cmd}': its view has no column '{name}' (named by {by})")
    })
}

fn parse_rules(entries: &[datum_core::kit::ColorRuleEntry]) -> Result<Vec<ColorRule>> {
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        out.push(ColorRule::parse(&e.when, &e.color)?);
    }
    Ok(out)
}

/// Project the view's columns down to `cols` (or pass through when empty).
/// Missing columns are an error — silently dropping them would mask typos.
fn project(cmd: &str, view: &ViewResult, cols: &[String]) -> Result<ViewResult> {
    if cols.is_empty() {
        return Ok(view.clone());
    }
    let indices = cols
        .iter()
        .map(|c| col_index(cmd, &view.columns, c, "columns"))
        .collect::<Result<Vec<_>>>()?;
    let columns = cols.to_vec();
    let rows = view
        .rows
        .iter()
        .map(|row| indices.iter().map(|i| row[*i].clone()).collect())
        .collect();
    Ok(ViewResult { columns, rows })
}

fn compare_cells(a: &JsonValue, b: &JsonValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (JsonValue::Null, JsonValue::Null) => Ordering::Equal,
        (JsonValue::Null, _) => Ordering::Greater,
        (_, JsonValue::Null) => Ordering::Less,
        (JsonValue::Number(x), JsonValue::Number(y)) => {
            let xf = x.as_f64().unwrap_or(0.0);
            let yf = y.as_f64().unwrap_or(0.0);
            xf.partial_cmp(&yf).unwrap_or(Ordering::Equal)
        }
        (JsonValue::String(x), JsonValue::String(y)) => x.cmp(y),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

fn header_labels(
    columns: &[String],
    labels: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    columns
        .iter()
        .map(|c| labels.get(c).cloned().unwrap_or_else(|| c.clone()))
        .collect()
}

/// Apply per-column format hints. Returns a parallel matrix of strings.
fn format_cells(
    view: &ViewResult,
    formats: &std::collections::BTreeMap<String, String>,
) -> Vec<Vec<String>> {
    view.rows
        .iter()
        .map(|row| {
            view.columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let raw = &row[i];
                    match formats.get(col).map(|s| s.as_str()) {
                        Some("smart_date") => format_smart_date(raw),
                        Some("duration") => format_duration(raw),
                        Some(other) => format!("?{other}({})", json_to_plain(raw)),
                        None => json_to_plain(raw),
                    }
                })
                .collect()
        })
        .collect()
}

fn json_to_plain(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn format_smart_date(v: &JsonValue) -> String {
    let Some(s) = v.as_str() else {
        return json_to_plain(v);
    };
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) else {
        return s.to_string();
    };
    let now = Utc::now();
    let delta = dt.with_timezone(&Utc).signed_duration_since(now);
    let secs = delta.num_seconds();
    let abs = secs.abs();
    let prefix = if secs < 0 { "" } else { "in " };
    let suffix = if secs < 0 { " ago" } else { "" };
    let body = if abs < 60 {
        format!("{abs}s")
    } else if abs < 3600 {
        format!("{}m", abs / 60)
    } else if abs < 86_400 {
        format!("{}h", abs / 3600)
    } else {
        format!("{}d", abs / 86_400)
    };
    format!("{prefix}{body}{suffix}")
}

fn format_duration(v: &JsonValue) -> String {
    let secs_opt: Option<i64> = match v {
        JsonValue::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        JsonValue::String(s) => s.parse::<i64>().ok(),
        _ => None,
    };
    let Some(secs) = secs_opt else {
        return json_to_plain(v);
    };
    format_human_duration(secs, DurationPrecision::Seconds)
}

/// Resolve a colour name or hex into the closest ANSI sequence. Used
/// only for terminal output — the colour rules round-trip the literal
/// string in TOML so the kit author's intent survives export.
fn ansi_for(color: &str) -> &'static str {
    match color.to_ascii_lowercase().as_str() {
        "red" => "\x1b[31m",
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "magenta" | "purple" => "\x1b[35m",
        "cyan" => "\x1b[36m",
        "white" => "\x1b[37m",
        "gray" | "grey" => "\x1b[90m",
        _ => "\x1b[37m",
    }
}

const ANSI_RESET: &str = "\x1b[0m";

fn render_pretty(headers: &[String], cells: &[Vec<String>], colors: &[Option<&str>]) -> String {
    if headers.is_empty() {
        return "(no columns)\n".to_string();
    }
    // Column widths in display columns, not chars (da-dhtp) — emoji/CJK
    // cells occupy two terminal columns each.
    let mut widths: Vec<usize> = headers.iter().map(|c| visible_width(c)).collect();
    for row in cells {
        for (i, c) in row.iter().enumerate() {
            let w = visible_width(c);
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }

    let mut out = print_row(headers, &widths, None);
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out += &print_row(&sep, &widths, None);
    for (row, color) in cells.iter().zip(colors) {
        out += &print_row(row, &widths, *color);
    }
    out += &format!(
        "({} row{})\n",
        cells.len(),
        if cells.len() == 1 { "" } else { "s" }
    );
    out
}

fn print_row(cells: &[String], widths: &[usize], color: Option<&str>) -> String {
    let mut line = String::new();
    if let Some(c) = color {
        line.push_str(c);
    }
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        // Pad by display width (da-dhtp), not `format!`'s char-count spec.
        line.push_str(c);
        line.push_str(&" ".repeat(widths[i].saturating_sub(visible_width(c))));
    }
    if color.is_some() {
        line.push_str(ANSI_RESET);
    }
    line.push('\n');
    line
}

fn row_color(
    rules: &[ColorRule],
    columns: &[String],
    raw: &[JsonValue],
    now: chrono::DateTime<Utc>,
) -> Option<&'static str> {
    let mut map: HashMap<&str, &JsonValue> = HashMap::new();
    for (i, c) in columns.iter().enumerate() {
        map.insert(c.as_str(), &raw[i]);
    }
    let lookup: &dyn RowLookup = &map;
    let color_name = kit_color_rule::pick_color(rules, lookup, now)?;
    Some(ansi_for(color_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mk_view() -> ViewResult {
        ViewResult {
            columns: vec!["field".into(), "n".into(), "extra".into()],
            rows: vec![
                vec![json!("coffee"), json!(7), json!("a")],
                vec![json!("mood"), json!(3), json!("b")],
            ],
        }
    }

    fn mk_cmd(columns: &[&str], sort: Option<&str>, rules: &[(&str, &str)]) -> InstalledKitCommand {
        InstalledKitCommand {
            name: "k".into(),
            kit_id: None,
            expands_to: None,
            view_name: Some("v".into()),
            help: None,
            columns: columns.iter().map(|c| c.to_string()).collect(),
            labels: Default::default(),
            formats: Default::default(),
            sort_col: sort.map(str::to_string),
            flags: vec![],
            color_rules: rules
                .iter()
                .map(|(when, color)| datum_core::kit::ColorRuleEntry {
                    when: when.to_string(),
                    color: color.to_string(),
                })
                .collect(),
            script_text: None,
        }
    }

    #[test]
    fn project_passes_through_when_empty() {
        let v = mk_view();
        let p = project("k", &v, &[]).unwrap();
        assert_eq!(p.columns, v.columns);
        assert_eq!(p.rows, v.rows);
    }

    #[test]
    fn project_subset_in_order() {
        let v = mk_view();
        let p = project("k", &v, &["n".into(), "field".into()]).unwrap();
        assert_eq!(p.columns, vec!["n".to_string(), "field".to_string()]);
        assert_eq!(p.rows[0], vec![json!(7), json!("coffee")]);
    }

    #[test]
    fn project_missing_column_errors() {
        let v = mk_view();
        let err = project("k", &v, &["bogus".into()]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "kit command 'k': its view has no column 'bogus' (named by columns)"
        );
    }

    #[test]
    fn sort_by_column_orders_rows() {
        let out = render(&mk_cmd(&[], Some("n"), &[]), mk_view(), false).unwrap();
        assert!(
            out.find("mood").unwrap() < out.find("coffee").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn sort_on_hidden_column_orders_rows() {
        let out = render(&mk_cmd(&["field"], Some("n"), &[]), mk_view(), false).unwrap();
        assert!(!out.contains('7'), "n stays hidden: {out}");
        assert!(
            out.find("mood").unwrap() < out.find("coffee").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn rule_on_hidden_column_paints_its_row() {
        let cmd = mk_cmd(&["field"], None, &[("extra = 'b'", "red")]);
        let out = render(&cmd, mk_view(), true).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[2], "coffee", "{out}");
        assert_eq!(lines[3], "\x1b[31mmood  \x1b[0m", "{out}");
    }

    #[test]
    fn a_name_the_view_lacks_is_refused() {
        let cmd = mk_cmd(&["field"], None, &[("n > 1 && bogus = 1", "red")]);
        let err = render(&cmd, mk_view(), false).unwrap_err();
        assert_eq!(
            err.to_string(),
            "kit command 'k': its view has no column 'bogus' (named by a color rule)"
        );
        let err = render(&mk_cmd(&["field"], Some("nope"), &[]), mk_view(), false).unwrap_err();
        assert_eq!(
            err.to_string(),
            "kit command 'k': its view has no column 'nope' (named by sort)"
        );
    }

    #[test]
    fn header_labels_substitutes() {
        let cols = vec!["next_due_raw".to_string(), "iti".to_string()];
        let mut labels = std::collections::BTreeMap::new();
        labels.insert("next_due_raw".into(), "due".into());
        labels.insert("iti".into(), "freq".into());
        let h = header_labels(&cols, &labels);
        assert_eq!(h, vec!["due".to_string(), "freq".to_string()]);
    }

    #[test]
    fn format_duration_renders_hms() {
        assert_eq!(format_duration(&json!(3725)), "1h2m");
        assert_eq!(format_duration(&json!(125)), "2m5s");
        assert_eq!(format_duration(&json!(45)), "45s");
    }
}
