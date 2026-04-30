//! Integration tests for `datum view <name>` (slice 4 / da-d4t.4).
//!
//! Exercises the full parser → dispatch → output path. Stdout capture in
//! Rust's test harness is fiddly, so most assertions go against the DB
//! state and the parsed `ViewArgs` shape; the renderer's pure helpers are
//! covered by unit tests inside `datum-commands::commands::view`.

use datum_cli::args::{Cli, CliExt, Command, ViewFormat};
use datum_core::db::Db;
use datum_core::kit::ViewEntry;

mod common;

use common::run_cli;

fn parsed(args: &[&str]) -> Cli {
    let argv: Vec<String> = std::iter::once("datum".to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    Cli::try_parse_argv(argv).expect("parse failed")
}

#[test]
fn parse_view_requires_name() {
    let argv: Vec<String> = vec!["datum".into(), "view".into()];
    assert!(
        Cli::try_parse_argv(argv).is_err(),
        "view with no name must error",
    );
}

#[test]
fn parse_view_takes_name_format_watch() {
    let cli = parsed(&["view", "chores", "--format", "json", "--watch"]);
    match cli.command {
        Command::View(v) => {
            assert_eq!(v.name, "chores");
            assert_eq!(v.format, ViewFormat::Json);
            assert!(v.watch);
        }
        other => panic!("expected Command::View, got {other:?}"),
    }
}

#[test]
fn parse_view_format_attached_form() {
    let cli = parsed(&["view", "x", "--format=tsv"]);
    match cli.command {
        Command::View(v) => assert_eq!(v.format, ViewFormat::Tsv),
        _ => panic!("expected View"),
    }
}

#[test]
fn parse_view_rejects_unknown_format() {
    let argv: Vec<String> = ["datum", "view", "x", "--format", "yaml"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(
        Cli::try_parse_argv(argv).is_err(),
        "yaml is not a supported view format",
    );
}

#[test]
fn dispatch_unknown_view_errors_with_clear_message() {
    let db = Db::open_in_memory().unwrap();
    let err = run_cli(&["view", "ghost"], &db).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("ghost") || msg.contains("no view"),
        "expected unknown-view error, got: {msg}",
    );
}

#[test]
fn dispatch_known_view_succeeds() {
    let db = Db::open_in_memory().unwrap();
    let views = vec![ViewEntry {
        name: "all_one".into(),
        sql: "SELECT 1 AS v".into(),
    }];
    db.install_kit_views("test", &views, false).unwrap();
    // tsv keeps the test deterministic regardless of TTY autodetect.
    run_cli(&["view", "all_one", "--format", "tsv"], &db).unwrap();
}

#[test]
fn dispatch_view_blocks_writes() {
    let db = Db::open_in_memory().unwrap();
    let views = vec![ViewEntry {
        name: "evil".into(),
        sql: "DELETE FROM entries".into(),
    }];
    db.install_kit_views("test", &views, false).unwrap();
    let err = run_cli(&["view", "evil"], &db).unwrap_err();
    // anyhow's `:#` formatter walks the cause chain; the `with_context`
    // wraps the rusqlite error which is what actually carries the
    // "attempt to write a readonly database" / "query_only" string.
    let msg = format!("{err:#}").to_lowercase();
    assert!(
        msg.contains("read") || msg.contains("query_only") || msg.contains("attempt to write"),
        "expected query_only rejection, got: {msg}",
    );
}
