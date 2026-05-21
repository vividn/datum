//! Command dispatch and argument → Entry conversion.

use anyhow::Result;
use datum_core::{
    db::Db,
    entry::Entry,
    field_spec::{self, InterpolationContext},
};

use crate::{
    args::{Cli, Command, LogArgs},
    commands::{helpers::load_registry_opt, render_helpers::build_render_contexts},
    render::{show_create_with_context, OutputArgs},
};

/// Drive a raw argv through the unified comma-split-first pipeline
/// against `db`. Goes through the same
/// [`crate::args::parse_invocation_with_db`] entry point as the binary,
/// so test coverage at this layer exercises the same call graph that
/// `main.rs` runs at runtime — no parallel "tests-only" path.
pub fn dispatch_argv(argv: Vec<String>, db: &Db) -> Result<()> {
    let invocation = crate::args::parse_invocation_with_db(argv, db)?;
    dispatch_invocation(invocation, db)
}

/// Dispatch a parsed [`crate::args::Invocation`]. Single-row invocations
/// route through [`dispatch`]; multi-row invocations open the
/// multi-collector window, run each pre-parsed side, then atomically
/// commit the buffered entries with shared-occur-time normalization
/// and same-field merge.
///
/// Failure path: if any side fails during dispatch, the multi-collector
/// is drained and the error propagates — the atomic commit never runs,
/// so nothing is written.
pub fn dispatch_invocation(invocation: crate::args::Invocation, db: &Db) -> Result<()> {
    match invocation {
        crate::args::Invocation::Single(cli) => dispatch(cli, db),
        crate::args::Invocation::Multi { sides, .. } => dispatch_multi_sides(sides, db),
    }
}

fn dispatch_multi_sides(sides: Vec<Cli>, db: &Db) -> Result<()> {
    crate::args::begin_multi_collector();

    let run_sides = || -> Result<()> {
        for cli in sides {
            dispatch(cli, db)?;
        }
        Ok(())
    };

    let result = run_sides();
    let collected = crate::args::end_multi_collector().unwrap_or_default();
    result?;

    if collected.is_empty() {
        // No side produced a write-able entry (e.g. all sides were
        // non-logging verbs like `tail`). Nothing to do.
        return Ok(());
    }

    // Per spec: all sides share one occur_time. Time-flag side-scoping
    // (da-gh0.D) will revisit this — for now the leftmost side's time
    // wins and propagates to every other side. We rebuild slug and
    // content_hash for any side whose occur_time changed so the
    // identity columns reflect the canonical time.
    let (entries, outputs): (Vec<_>, Vec<_>) = collected.into_iter().unzip();
    let normalized = normalize_shared_occur_time(entries);

    // da-dayk: bucket per-side `OutputArgs` by `(field, occur_time)`
    // in first-appearance order — exactly how `merge_same_field_groups`
    // groups entries before insert. The leftmost contributing side's
    // flags drive the resulting merged row's render. The hid order
    // returned by `insert_entries_atomic` matches this group order
    // 1:1, so we can pair them up directly when rendering.
    let group_outputs = leftmost_output_per_group(&normalized, &outputs);

    let hids = db.insert_entries_atomic(normalized)?;
    if !hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &hids);
    }

    // da-dayk: render every persisted row through the same
    // `show_create_with_context` path single-row dispatch uses, with
    // the per-group `OutputArgs` so `--show none` (and friends) still
    // silence as expected. `build_render_contexts` batches the
    // state-history probes across rows.
    render_multi_inserted(db, &hids, &group_outputs);

    Ok(())
}

/// Walk `entries` in order, returning the [`OutputArgs`] of the first
/// entry seen for each unique `(field, occur_time)` key. The output
/// vector aligns with `merge_same_field_groups`' group-order output,
/// which matches the `hids` returned by
/// [`datum_core::db::Db::insert_entries_atomic`].
fn leftmost_output_per_group(
    entries: &[datum_core::entry::Entry],
    outputs: &[OutputArgs],
) -> Vec<OutputArgs> {
    use std::collections::HashSet;
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut group_outputs: Vec<OutputArgs> = Vec::new();
    for (entry, out) in entries.iter().zip(outputs.iter()) {
        let occur = entry
            .occur_time
            .as_ref()
            .map(|t| t.utc.to_rfc3339())
            .unwrap_or_default();
        let key = (entry.field.clone(), occur);
        if seen.insert(key) {
            group_outputs.push(out.clone());
        }
    }
    group_outputs
}

/// da-dayk: render every freshly-inserted multi row to stdout. Mirrors
/// the post-insert render block in [`log_entry`] but driven from the
/// hid list returned by [`datum_core::db::Db::insert_entries_atomic`].
/// `RenderContext`s are batch-built so we share one state-history probe
/// per field instead of N point probes.
fn render_multi_inserted(db: &Db, hids: &[datum_core::ids::Hid], group_outputs: &[OutputArgs]) {
    if hids.is_empty() {
        return;
    }
    let registry = load_registry_opt(db);
    let entries: Vec<Entry> = hids
        .iter()
        .filter_map(|hid| db.get_entry(hid.as_str()).ok())
        .collect();
    if entries.is_empty() {
        return;
    }
    let contexts = build_render_contexts(&entries, db);
    for (idx, entry) in entries.iter().enumerate() {
        let output = group_outputs.get(idx).cloned().unwrap_or_default();
        let ctx = contexts.get(idx).cloned().unwrap_or_default();
        show_create_with_context(entry, registry.as_ref(), &ctx, &output);
    }
}

/// Force every entry in `entries` to share the leftmost entry's
/// `occur_time` (Section B: "multi_all_sides_share_one_occur_time").
/// Slug and content_hash both depend on occur_time, so they are
/// rebuilt whenever we change the time. Entries with a non-templated
/// slug get the default `<field>:<timestamp>` rebuild; entries
/// carrying an `id_structure` template are re-interpolated against
/// the post-mutation state via [`datum_core::field_spec::interpolate`].
fn normalize_shared_occur_time(
    mut entries: Vec<datum_core::entry::Entry>,
) -> Vec<datum_core::entry::Entry> {
    let Some(shared) = entries.first().and_then(|e| e.occur_time.clone()) else {
        return entries;
    };
    for entry in entries.iter_mut().skip(1) {
        if entry.occur_time.as_ref().map(|t| &t.utc) == Some(&shared.utc) {
            continue;
        }
        entry.occur_time = Some(shared.clone());
        // da-ivld: every entry now carries an id_structure; rebuild the
        // slug as `<field>:<body>` where body = interpolate(id_structure).
        if let Some(tmpl) = entry.id_structure.clone() {
            let uid = entry.short_uid();
            let ctx = InterpolationContext::from_entry(entry, &uid);
            let body = field_spec::interpolate(&tmpl, &ctx, ":");
            entry.slug = datum_core::ids::Slug(format!("{}:{}", entry.field, body));
        } else {
            entry.slug = datum_core::entry::build_slug(&entry.field, Some(&shared));
        }
        entry.content_hash = entry.expected_content_hash();
    }
    entries
}

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    use crate::commands;
    match cli.command {
        // Repl is intercepted by the binary entry point before dispatch
        // (see `datum-cli::dispatch_single`). Reaching here means a
        // bare-flag invocation was routed into in-process dispatch — error
        // out instead of pretending to do work.
        Command::Repl => Err(anyhow::anyhow!(
            "no subcommand given — `datum` with only global flags is REPL-only"
        )),
        Command::Occur(args) => run_log_with_alias(args, db, commands::occur::occur),
        Command::Start(args) => run_log_with_alias(args, db, commands::start::start),
        Command::Stop(args) => run_log_with_alias(args, db, commands::end::stop),
        Command::Switch(args) => run_log_with_alias(args, db, commands::switch::switch),
        Command::Add(args) => run_log_with_alias(args, db, commands::add::add),
        Command::Tail(args) => commands::tail::tail(args, db),
        Command::Head(args) => commands::head::head(args, db),
        Command::Now(args) => commands::now::now(args, db),
        Command::Get(args) => commands::get::get(args, db),
        Command::Delete(args) => commands::del::del(args, db),
        Command::Edit(args) => commands::edit::edit(args, db),
        Command::Update(args) => commands::update::update(args, db),
        Command::Merge(args) => commands::merge::merge(args, db),
        Command::Rekey(args) => commands::rekey::rekey(args, db),
        Command::Retime(args) => commands::retime::retime(args, db),
        Command::Undo(args) => commands::undo::undo(args, db),
        Command::History(args) => commands::history::history(args, db),
        Command::Why(args) => commands::why::why(args, db),
        Command::Spec(sub) => commands::spec::spec(sub, db),
        Command::Alias(sub) => commands::alias::alias(sub, db),
        Command::Kit(sub) => commands::kit::kit(sub, db),
        Command::Track(args) => commands::track::track(args, db),
        Command::Ignore(args) => commands::ignore::ignore(args, db),
        Command::Meta(sub) => commands::meta::meta(sub, db),
        Command::Grep(args) => commands::grep::grep(args, db),
        Command::Check(args) => commands::check::check(args, db),
        Command::Count(args) => commands::count::count(args, db),
        Command::Ids(args) => commands::ids::ids(args, db),
        Command::Dayview(args) => commands::dayview::dayview(args, db),
        Command::Nowview(args) => commands::nowview::nowview(args, db),
        Command::View(args) => commands::view::view(args, db),
        Command::Info(args) => commands::info::info(args, cli.db.as_deref(), db),
        Command::Db(sub) => commands::db::dispatch(sub, db),
        Command::Config(sub) => commands::config::config(sub, cli.db.as_deref()),
        Command::Sync(args) => commands::sync::dispatch(args, db),
        Command::Reparse(args) => commands::reparse::reparse(args, db),
    }
}

/// Wrap a logging-verb dispatcher so that `--alias <NAME>` (and
/// `--alias-overwrite`) on the same invocation is honoured: the inner
/// command runs first, then on success the canonical argv form is
/// persisted to the alias table via
/// [`crate::commands::alias::record_inline_alias`].
///
/// The raw pre-expansion argv must be captured *before* `inner` runs,
/// because `log_entry` consumes the [`crate::args::IntentContext`] off
/// the thread-local slot when stamping provenance onto the entry.
fn run_log_with_alias(args: LogArgs, db: &Db, inner: fn(LogArgs, &Db) -> Result<()>) -> Result<()> {
    let alias_name = args.alias_name.clone();
    let alias_overwrite = args.alias_overwrite;
    let raw_argv = crate::args::peek_intent_context()
        .map(|ctx| ctx.raw_argv)
        .unwrap_or_default();
    inner(args, db)?;
    crate::commands::alias::record_inline_alias(alias_name, alias_overwrite, &raw_argv, db)
}

// ─── Edit ────────────────────────────────────────────────────────────────────

// Open an entry in `$EDITOR` as pretty JSON; on save, re-parse and persist.
//
// Testing hook: `DATUM_EDITOR` takes precedence over `EDITOR`. Both are
// invoked as `sh -c "$editor \"$1\"" -- <tmpfile>`, so tests can set
// `DATUM_EDITOR="cp /path/to/prepared.json"` to simulate a scripted edit.
//
// `cmd_edit` was moved to `crate::commands::edit::edit` in da-4lov to
// route the editor flow through `UserEditableEntry`, which makes
// system-only fields (extra_metadata, hid, slug, content_hash, etc.)
// physically unreachable from the editor tempfile.

// ─── Update / Merge / Rekey ──────────────────────────────────────────────────
// Underlying logic lives in `commands::update_core::update_with_strategy`.
// See that module for the unified strategy function the wrappers (and S3
// batch mutations) call into.

// ─── Why ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{DeleteArgs, GetArgs, RetimeArgs, TailArgs, UndoArgs, UpdateArgs};
    use crate::commands::log_core::log_entry;
    use crate::commands::tail::tail_entries;
    use crate::commands::update_core::{update_from_args, UpdateStrategy};
    use datum_core::action::Action;
    use datum_core::entry::Verb;
    use serde_json::Value as JsonValue;

    #[test]
    fn test_get_and_delete() {
        use datum_core::{db::Db, entry::Entry, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t = DatumTime::now();
        let entry = Entry::new_legacy("test", Verb::Point, None, None, Some(t), None, "dev");
        let slug = entry.slug.clone();
        db.insert_entry(&entry).unwrap();

        // get
        let fetched = db.get_entry(&slug).unwrap();
        assert_eq!(fetched.slug, slug);

        // delete
        db.delete_entry(&slug).unwrap();
        let err = db.get_entry(&slug);
        assert!(matches!(
            err,
            Err(datum_core::error::DatumError::NotFound(_))
        ));
    }

    #[test]
    fn test_auto_track_on_occur() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Bare `occur coffee` — occur is stateless under da-k5o6 and rejects
        // bare positionals; the per-occurrence quantity, if any, would land
        // via `-k amount=2`. Auto-track depends on the verb, not the data.
        let args = LogArgs::fresh("coffee");
        log_entry(Action::Occur, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "coffee" && !f.ignored));
    }

    #[test]
    fn test_auto_track_on_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs::fresh("work");
        log_entry(Action::Start, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "work" && !f.ignored));
    }

    #[test]
    fn test_undo_flag_deletes_last_matching_entry() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // First insert: `datum switch coffee 2` (switch is the state-bearing
        // verb; `occur` carries no state under da-k5o6).
        let mut args = LogArgs::fresh("coffee");
        args.rest = vec!["2".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 1);

        // Undo via `-u`: same command form with `args.undo = true`. The
        // matching entry is soft-deleted, so the live count drops to 0.
        let mut undo_args = LogArgs::fresh("coffee");
        undo_args.rest = vec!["2".to_string()];
        undo_args.undo = true;
        log_entry(Action::Switch, undo_args, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 0);
    }

    #[test]
    fn test_undo_flag_state_specific() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two entries with different states under the same field.
        let mut a = LogArgs::fresh("coffee");
        a.rest = vec!["1".to_string()];
        log_entry(Action::Switch, a, &db).unwrap();
        let mut b = LogArgs::fresh("coffee");
        b.rest = vec!["2".to_string()];
        log_entry(Action::Switch, b, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 2);

        // Undo only `switch coffee 2` — the `switch coffee 1` entry stays put.
        let mut undo = LogArgs::fresh("coffee");
        undo.rest = vec!["2".to_string()];
        undo.undo = true;
        log_entry(Action::Switch, undo, &db).unwrap();
        let live = db.entries_for_field("coffee", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].state, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_undo_flag_errors_when_no_match() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("coffee");
        args.rest = vec!["3".to_string()];
        args.undo = true;
        let err = log_entry(Action::Switch, args, &db).unwrap_err();
        assert!(
            err.to_string().contains("no matching entry to undo"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_no_auto_track_on_note() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("journal");
        // v3 (da-5ns): bare positional under Action::Add is rejected
        // (the v=20 schema dropped the implicit `value` column). Stage
        // the note as a kv pair, and use `-T` so the entry is timeless
        // — without an `occur_time` event, the state compiler emits
        // nothing for a Point row, which is what the original
        // "add-only ⇒ ignored" semantic relied on.
        args.rest = vec!["text=some text".to_string()];
        args.time.omit_timestamp = true;
        log_entry(Action::Add, args, &db).unwrap();
        // da-2sr: an add-only field has no state-bearing entry, so state_at
        // returns None — the projection labels it ignored (the "no entries"
        // default). It still appears in `get_tracked_fields` so callers
        // know it exists, but `now` filters it out by default.
        let tracked = db.get_tracked_fields().unwrap();
        let journal = tracked.iter().find(|f| f.field == "journal");
        assert!(journal.is_some(), "field should be present after any write");
        assert!(
            journal.unwrap().ignored,
            "add-only field defaults to ignored"
        );
    }

    #[test]
    fn test_cmd_track_and_ignore() {
        use crate::args::{IgnoreArgs, TrackArgs};
        use datum_core::db::Db;
        use datum_core::test_util::{advance_time, set_time};
        let db = Db::open_in_memory().unwrap();

        // da-afk: cmd_track/cmd_ignore stamp entries with `DatumTime::now()`,
        // and the slug is `field:<ms-iso>`. Three back-to-back calls on a fast
        // machine can land in the same millisecond and hard-error on the slug
        // collision. Freeze the clock and step it between calls so the slugs
        // are distinct by construction.
        let _g = set_time("2026-01-15T12:00:00Z");

        let track_args = TrackArgs {
            field: "mood".to_string(),
            color: Some("ff6600".to_string()),
            state_colors: vec!["happy=00ff00".to_string(), "sad=ff0000".to_string()],
            tags: vec![],
        };
        crate::commands::track::track(track_args, &db).unwrap();

        // da-2sr: track wrote a switch entry (value=NULL) and a spec for the
        // colors. The derived projection sees mood as tracked.
        let tracked = db.get_tracked_fields().unwrap();
        let mood = tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let state_colors = db.get_all_state_colors().unwrap();
        assert_eq!(
            state_colors
                .get(&("mood".to_string(), "happy".to_string()))
                .map(|s| s.as_str()),
            Some("00ff00")
        );

        advance_time("1m");
        // Ignore writes a switch IGNORED entry — supersedes the prior
        // track switch — and leaves the spec (and its color) alone.
        crate::commands::ignore::ignore(
            IgnoreArgs {
                field: "mood".to_string(),
            },
            &db,
        )
        .unwrap();
        let tracked2 = db.get_tracked_fields().unwrap();
        let mood2 = tracked2.iter().find(|f| f.field == "mood").unwrap();
        assert!(mood2.ignored);
        // Color survives because it lives on FieldSpec, not in the entry log.
        assert_eq!(mood2.color.as_deref(), Some("ff6600"));

        advance_time("1m");
        // Re-track flips it back to tracked (most recent switch wins).
        crate::commands::track::track(
            TrackArgs {
                field: "mood".to_string(),
                color: None,
                state_colors: vec![],
                tags: vec![],
            },
            &db,
        )
        .unwrap();
        let tracked3 = db.get_tracked_fields().unwrap();
        assert!(!tracked3.iter().find(|f| f.field == "mood").unwrap().ignored);
    }

    #[test]
    fn test_cmd_now_no_crash() {
        use crate::args::NowArgs;
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Empty db
        crate::commands::now::now(
            NowArgs {
                show_all: false,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();

        // With some tracked fields and entries
        let mut args = LogArgs::fresh("mood");
        args.rest = vec!["happy".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        // da-2sr: writing the switch entry alone makes mood tracked; no
        // separate set_tracked / spec write is needed.

        crate::commands::now::now(
            NowArgs {
                show_all: false,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();
        crate::commands::now::now(
            NowArgs {
                show_all: true,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();
    }

    // ─── id-template tests (A1 acceptance) ──────────────────────────────────

    fn log_args(field: &str, rest: Vec<String>, id: Option<String>) -> LogArgs {
        let mut args = LogArgs::fresh(field);
        args.rest = rest;
        args.id = id;
        args
    }

    #[test]
    fn spec_id_template_is_default() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // da-ivld: id_structure is body-only; the `<field>:` prefix is implicit.
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.id_template = Some("%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(
            e.id_structure.as_deref(),
            Some("%uid%"),
            "spec template must be persisted as id_structure"
        );
        // slug = field ":" short_uid → must start with "coffee:" and end with 5-char uid
        let expected_uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:{expected_uid}"));
    }

    #[test]
    fn explicit_id_flag_overrides_spec_default() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // Spec has one template; --id passes a different one → --id wins.
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.id_template = Some("%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("override-%uid%".to_string())),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        assert_eq!(e.id_structure.as_deref(), Some("override-%uid%"));
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:override-{uid}"));
    }

    #[test]
    fn id_template_round_trip_through_db() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%uid%".to_string())),
            &db,
        )
        .unwrap();

        // Re-open registry path and fetch by slug — must still have id_structure.
        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let fetched = db.get_entry(&e.slug).unwrap();
        assert_eq!(fetched.id_structure.as_deref(), Some("%uid%"));
        assert_eq!(fetched.slug, e.slug);
    }

    #[test]
    fn no_template_keeps_default_slug() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        // da-ivld: no --id and no spec template ⇒ default-populate id_structure
        // from the entry's occur_time. Slug stays `<field>:<formatted occur_time>`.
        assert_eq!(e.id_structure.as_deref(), Some("%occur_time%"));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn missing_key_collapses_delimiter_via_cli() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // occur verb has no state → %state% empty → "::" collapses to ":"
        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%state%:%uid%".to_string())),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:{uid}"));
    }

    // ─── field-template (da-qjrc) tests ─────────────────────────────────────

    #[test]
    fn field_template_writes_field_structure_and_interpolates() {
        // Bead da-qjrc worked example. The slot-filling order of
        // `parse_log_args` consumes `-K` slots before `-k` slots, so the
        // test pins `srcLang` via the `srcLang=pt` reserved-key form to
        // avoid coupling to positional order — the headline behaviour
        // under test is template detection + field interpolation, not
        // slot-fill mechanics.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let mut args = LogArgs::fresh("word__%srcLang%");
        args.slug_keys = vec!["pos".to_string(), "source".to_string()];
        args.remainder = Some("en".to_string());
        args.rest = vec![
            "srcLang=pt".to_string(),
            "noun".to_string(),
            "fundo".to_string(),
            "fund".to_string(),
            "bottom".to_string(),
        ];

        log_entry(Action::Add, args, &db).unwrap();

        let entries = db.entries_for_field("word__pt", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.field, "word__pt");
        assert_eq!(e.field_structure.as_deref(), Some("word__%srcLang%"));
        assert_eq!(e.id_structure.as_deref(), Some("%pos%__%source%"));
        assert_eq!(e.slug, "word__pt:noun__fundo");
        let data = e.data.as_ref().expect("data populated");
        assert_eq!(data["srcLang"], serde_json::json!("pt"));
        assert_eq!(data["pos"], serde_json::json!("noun"));
        assert_eq!(data["source"], serde_json::json!("fundo"));
        assert_eq!(data["en"], serde_json::json!(["fund", "bottom"]));
    }

    #[test]
    fn field_template_update_cascades_to_field_and_slug() {
        // Re-interpolating field_structure on update must move the field
        // (and therefore the slug). Acceptance criterion from the bead.
        use crate::commands::update_core::{update_with_strategy, UpdateRequest, UpdateStrategy};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let mut args = LogArgs::fresh("word__%srcLang%");
        args.slug_keys = vec!["pos".to_string(), "source".to_string()];
        args.rest = vec![
            "srcLang=pt".to_string(),
            "noun".to_string(),
            "fundo".to_string(),
        ];
        log_entry(Action::Add, args, &db).unwrap();

        let original = db
            .entries_for_field("word__pt", None)
            .unwrap()
            .pop()
            .unwrap();

        update_with_strategy(
            UpdateRequest {
                id_or_slug: &original.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"srcLang": "es"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("word__es", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, "word__es");
        assert_eq!(live[0].field_structure.as_deref(), Some("word__%srcLang%"));
        assert_eq!(live[0].slug, "word__es:noun__fundo");
    }

    #[test]
    fn no_field_template_leaves_field_structure_none() {
        // Bare field (no `%key%`) ⇒ field_structure stays None.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        assert_eq!(e.field, "coffee");
        assert_eq!(e.field_structure, None);
    }

    // ─── head command tests ─────────────────────────────────────────────────

    fn make_entry_at(field: &str, secs_offset: i64, device: &str) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        // Fixed base time + offset keeps ids/slugs distinct and orderable.
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(
            field,
            Verb::Point,
            Some(serde_json::json!(secs_offset)),
            None,
            Some(t),
            None,
            device,
        )
    }

    /// Build a `TailArgs` in head-mode (oldest-first) with the given field and count.
    /// Used by head regression tests to exercise the shared tail codepath.
    fn head_args(field: Option<&str>, count: Option<usize>) -> TailArgs {
        TailArgs {
            field: field.map(String::from),
            count,
            oldest_first: true,
            ..TailArgs::fresh()
        }
    }

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

        // Verify via tail_entries that ordering is ASC by time_start (head mode).
        let entries = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        let values: Vec<i64> = entries
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 200, 300]);
    }

    #[test]
    fn head_respects_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..20 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }

        crate::commands::tail::tail(head_args(Some("mood"), Some(5)), &db).unwrap();

        let got = tail_entries(&head_args(Some("mood"), Some(5)), &db).unwrap();
        assert_eq!(got.len(), 5);
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![0, 60, 120, 180, 240]);
    }

    #[test]
    fn head_filters_by_field() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("coffee", 100, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();
        db.insert_entry(&make_entry_at("coffee", 300, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 400, "d1")).unwrap();

        crate::commands::tail::tail(head_args(Some("coffee"), Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(Some("coffee"), Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|e| e.field == "coffee"));
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 300]);
    }

    #[test]
    fn head_default_limit_is_10() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..15 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }
        // tail_params defaults count to 10 when no date window is set — same
        // default the old head command hardcoded.
        let got = tail_entries(&head_args(Some("mood"), None), &db).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(got[0].value.as_ref().unwrap().as_i64().unwrap(), 0);
    }

    #[test]
    fn head_returns_all_when_fewer_than_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
    }

    // ─── update / merge / rekey command tests ───────────────────────────────

    fn update_args(id: &str, rest: Vec<&str>) -> UpdateArgs {
        UpdateArgs {
            id_or_slug: Some(id.to_string()),
            rest: rest.into_iter().map(String::from).collect(),
            strategy: None,
            id: None,
            id_delimiter: None,
            keys: vec![],
            comment: Vec::new(),
            dry_run: false,
            strict: false,
            output: Default::default(),
        }
    }

    fn seed_entry(
        db: &datum_core::db::Db,
        field: &str,
        verb: Verb,
        value: Option<JsonValue>,
        extra: Option<JsonValue>,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new_legacy(field, verb, value, extra, Some(dt), None, "d1");
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn update_replaces_extra_keys() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({
                "weight": {"v": 70.0, "u": "kg"},
                "pose": "standing",
                "reps": 10,
            })),
        );
        // Let a moment pass so modified_at is observably greater than created_at.
        std::thread::sleep(std::time::Duration::from_millis(5));

        update_from_args(
            update_args(&e.slug, vec!["weight=80kg"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        // weight replaced
        assert_eq!(ex["weight"]["v"], serde_json::json!(80.0));
        assert_eq!(ex["weight"]["u"], serde_json::json!("kg"));
        // unmentioned keys preserved
        assert_eq!(ex["pose"], serde_json::json!("standing"));
        assert_eq!(ex["reps"], serde_json::json!(10));
        // modified_at bumped; content_hash refreshed
        assert!(after.modify_time >= e.modify_time);
        assert_ne!(after.content_hash, e.content_hash);
        assert_eq!(after.content_hash, after.expected_content_hash());
    }

    #[test]
    fn merge_preserves_untouched_keys() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({
                "nested": {"a": 1, "b": 2},
                "other": "keep",
            })),
        );

        update_from_args(
            update_args(&e.slug, vec!["tag=red"]),
            UpdateStrategy::Merge,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert_eq!(ex["tag"], serde_json::json!("red"));
        // Deep-merge does NOT disturb existing keys anywhere in the tree.
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["b"], serde_json::json!(2));
        assert_eq!(ex["other"], serde_json::json!("keep"));
    }

    #[test]
    fn merge_deep_merges_nested_objects() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({"nested": {"a": 1, "c": 3}})),
        );

        let mut args = update_args(&e.slug, vec![]);
        args.keys = vec!["nested.b=2".to_string()];
        // da-ju71.4: dotted paths are real paths now (the old flat_paths
        // compat shim is gone), so `nested.b=2` deep-sets into the
        // existing `nested` object. Original keys must survive the merge.
        update_from_args(args, UpdateStrategy::Merge, &db).unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        // Original scalar keys intact (deep-merge preserved them)
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["c"], serde_json::json!(3));
        // Newly-set dotted path lands inside the nested object
        assert_eq!(ex["nested"]["b"], serde_json::json!(2));
    }

    // `deep_merge_recurses_into_objects` lives in update_core.rs where the
    // helper now resides; this crate-level test would be redundant.

    #[test]
    fn update_empty_value_drops_key() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"foo": "bar", "baz": "qux"})),
        );

        update_from_args(
            update_args(&e.slug, vec!["foo="]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert!(ex.get("foo").is_none(), "foo= should drop the key");
        assert_eq!(ex["baz"], serde_json::json!("qux"));
    }

    #[test]
    fn rekey_renames_data_key_via_cli() {
        // CLI: `update <slug> --strategy rekey reps=repetitions` should
        // rename `data.reps` → `data.repetitions` in place. No new row.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10, "set": 3})),
        );

        update_from_args(
            update_args(&e.slug, vec!["reps=repetitions"]),
            UpdateStrategy::Rekey,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("gym", None).unwrap();
        assert_eq!(live.len(), 1, "rekey is a key-rename, not a copy");
        let data = live[0].data.as_ref().unwrap();
        assert_eq!(data["repetitions"], serde_json::json!(10));
        assert!(data.get("reps").is_none());
        assert_eq!(data["set"], serde_json::json!(3));
    }

    #[test]
    fn rekey_without_mapping_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
        );

        let err =
            update_from_args(update_args(&e.slug, vec![]), UpdateStrategy::Rekey, &db).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("rekey"),
            "expected rekey-mapping error, got: {err}"
        );
    }

    #[test]
    fn update_tombstoned_row_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({"reps": 10})),
        );
        db.soft_delete_entry(&e.slug).unwrap();

        // Use id (slug lookup would also find the tombstoned row via
        // get_entry_including_deleted).
        let err = update_from_args(
            update_args(&e.slug, vec!["reps=20"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("tombstoned"),
            "expected tombstone error, got: {err}"
        );

        // Merge and rekey must also reject tombstoned rows.
        let err = update_from_args(
            update_args(&e.slug, vec!["tag=red"]),
            UpdateStrategy::Merge,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));

        // Rekey: pass a valid mapping so the tombstone check (which fires
        // before strategy dispatch) is what trips, not the empty-mapping check.
        let rk = update_args(&e.slug, vec!["reps=repetitions"]);
        let err = update_from_args(rk, UpdateStrategy::Rekey, &db).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));
    }

    #[test]
    fn update_resolves_unique_prefix() {
        // C3 integration smoke: a unique prefix routes through update just
        // like the full id. Use the first 5 chars of the id — prefix matching
        // is anchored at the start by `LIKE '...%'`, and the ULID timestamp
        // leader makes the head of a single-row table unique.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "gym", Verb::Point, Some(serde_json::json!(1)), None);
        let prefix: String = e.slug.chars().take(5).collect();
        update_from_args(
            update_args(&prefix, vec!["note=fixed"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let fresh = db.get_entry(&e.slug).unwrap();
        assert_eq!(fresh.data.unwrap()["note"], serde_json::json!("fixed"),);
    }

    #[test]
    fn update_ambiguous_prefix_errors() {
        use datum_core::{db::Db, entry::Entry, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        // Two entries on the same field at distinct instants — both slugs
        // start with `gym:`, so that prefix matches multiple rows.
        let t0: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00.000Z".parse().unwrap();
        let t1: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00.001Z".parse().unwrap();
        for (t, v) in [(t0, 1), (t1, 2)] {
            let dt = DatumTime {
                utc: t,
                offset_minutes: 0,
                timezone: "UTC".to_string(),
            };
            let e = Entry::new_legacy(
                "gym",
                Verb::Point,
                Some(serde_json::json!(v)),
                None,
                Some(dt),
                None,
                "d1",
            );
            db.insert_entry(&e).unwrap();
        }
        let err = update_from_args(
            update_args("gym:", vec!["note=x"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("ambiguous"), "expected ambiguous, got: {err}");
    }

    #[test]
    fn update_nonexistent_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = update_from_args(
            update_args("no-such-id", vec!["foo=bar"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[test]
    fn update_strategy_flag_overrides_subcommand_default() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({"tag": {"color": "red", "size": "m"}})),
        );

        // Invoke via Update default but override strategy to Merge — the nested
        // {color, size} should survive because deep-merge preserves siblings.
        let mut args = update_args(&e.slug, vec![]);
        args.strategy = Some(crate::args::ConflictStrategyArg::Merge);
        args.keys = vec!["tag.priority=1".to_string()];
        update_from_args(args, UpdateStrategy::Replace, &db).unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert_eq!(ex["tag"]["color"], serde_json::json!("red"));
        assert_eq!(ex["tag"]["size"], serde_json::json!("m"));
        // da-ju71.4: dotted paths deep-set, so `tag.priority=1` lands as
        // `tag.priority` *inside* the existing nested `tag` object.
        assert_eq!(ex["tag"]["priority"], serde_json::json!(1));
    }

    #[test]
    fn update_value_replaces_primary() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );

        update_from_args(
            update_args(&e.slug, vec!["sad"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.value, Some(serde_json::json!("sad")));
    }

    #[test]
    fn update_auto_reinterps_slug_when_id_structure_present() {
        // Headline behavior: a regular `update` on a row whose id_structure
        // references a state-bearing template var should auto-rekey the
        // slug when that var changes — no manual --strategy rekey, no
        // --id flag. da-5ns.3: `state=` writes the column; `%state%`
        // resolves to `entry.state` for Verb::Switch.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
        e.id_structure = Some("%state%".to_string());
        db.save_existing_entry(&e).unwrap();

        update_from_args(
            update_args(&e.slug, vec!["state=sad"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1, "auto-rekey is in-place; no new row");
        assert_eq!(live[0].slug, "mood:sad");
    }

    #[test]
    fn rekey_renames_key_referenced_by_id_structure_cascades_to_slug() {
        // Renaming a data key that the id_structure template references
        // should cascade: the rename triggers auto-reinterp, which moves
        // the slug.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "task",
            Verb::Point,
            None,
            Some(serde_json::json!({"oldKind": "chore"})),
        );
        // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
        e.id_structure = Some("%kind%".to_string());
        db.save_existing_entry(&e).unwrap();

        update_from_args(
            update_args(&e.slug, vec!["oldKind=kind"]),
            UpdateStrategy::Rekey,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("task", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, "task:chore");
        let data = live[0].data.as_ref().unwrap();
        assert_eq!(data["kind"], serde_json::json!("chore"));
        assert!(data.get("oldKind").is_none());
    }

    // ─── B7: undo via soft-delete tombstone ─────────────────────────────────

    fn undo_args(slug: Option<&str>, force: bool, window: &str) -> UndoArgs {
        UndoArgs {
            id_or_slug: slug.map(str::to_string),
            to_version: None,
            force,
            window: window.to_string(),
            output: Default::default(),
        }
    }

    #[test]
    fn undo_restores_most_recent_delete_within_window() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Three entries; delete the middle one. `undo` with default window
        // must pick that one (the most recent tombstone) and bring it back.
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        let doomed = make_entry_at("mood", 200, "d1");
        let doomed_id = doomed.slug.clone();
        db.insert_entry(&doomed).unwrap();
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();

        db.soft_delete_entry(&doomed_id).unwrap();
        assert_eq!(db.entries_for_field("mood", None).unwrap().len(), 2);

        crate::commands::undo::undo(undo_args(None, false, "15m"), &db).unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 3);
        assert!(live.iter().any(|e| e.slug == doomed_id));
    }

    #[test]
    fn undo_outside_window_errors_without_force() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.slug.clone();
        db.insert_entry(&e).unwrap();
        db.soft_delete_entry(&id).unwrap();

        // Zero-length window: the tombstone we just created is already "too old"
        // (its deleted_at < now - 0). Undo without --force must bail out.
        let err = crate::commands::undo::undo(undo_args(None, false, "0s"), &db).unwrap_err();
        assert!(
            err.to_string().contains("use --force"),
            "error should suggest --force, got: {err}"
        );

        // Row is still tombstoned.
        assert!(db.entries_for_field("mood", None).unwrap().is_empty());

        // With --force, the same call restores it regardless of window.
        crate::commands::undo::undo(undo_args(None, true, "0s"), &db).unwrap();
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, id);
    }

    #[test]
    fn undo_specific_slug_targets_that_row() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two deletes; the most recent is B, but we want to restore A specifically.
        let a = make_entry_at("mood", 100, "d1");
        let a_id = a.slug.clone();
        let a_slug = a.slug.clone();
        let b = make_entry_at("mood", 200, "d1");
        let b_id = b.slug.clone();
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        db.soft_delete_entry(&a_id).unwrap();
        db.soft_delete_entry(&b_id).unwrap();

        crate::commands::undo::undo(undo_args(Some(&a_slug), false, "15m"), &db).unwrap();

        // A is back, B is still tombstoned.
        let live: Vec<_> = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, a_id);
        assert!(db
            .get_entry_including_deleted(&b_id)
            .unwrap()
            .deleted_at
            .is_some());
    }

    // ─── B3: retime ─────────────────────────────────────────────────────────

    fn retime_args(slug: Option<&str>, offset: &str) -> RetimeArgs {
        RetimeArgs {
            slug: slug.map(str::to_string),
            offset: Some(offset.to_string()),
            time: None,
            field: None,
            after: None,
            before: None,
        }
    }

    fn make_block(
        field: &str,
        start_secs: i64,
        end_secs: i64,
        device: &str,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let start = DatumTime {
            utc: base + chrono::Duration::seconds(start_secs),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let end = DatumTime {
            utc: base + chrono::Duration::seconds(end_secs),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(
            field,
            Verb::Switch,
            None,
            None,
            Some(start),
            Some(end),
            device,
        )
    }

    #[test]
    fn retime_single_slug_offsets_start_and_end() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_block("sleep", 0, 3600, "d1");
        let id = e.slug.clone();
        let t0_start = e.start_time.as_ref().unwrap().utc;
        let t0_end = e.end_time.as_ref().unwrap().utc;
        let hash_before = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        crate::commands::retime::retime(retime_args(Some(&id), "-5m"), &db).unwrap();

        let after = db.get_entry(&id).unwrap();
        let shift = chrono::Duration::minutes(-5);
        assert_eq!(after.start_time.as_ref().unwrap().utc, t0_start + shift);
        assert_eq!(after.end_time.as_ref().unwrap().utc, t0_end + shift);
        // content_hash follows the new timestamps; modified_at moves too.
        assert_ne!(after.content_hash, hash_before);
        assert_eq!(after.content_hash, after.expected_content_hash());
        assert!(after.modify_time >= e.modify_time);
    }

    #[test]
    fn retime_field_wide_offset_filtered_by_after() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two coffee rows straddling an --after cutoff, plus an unrelated
        // field. Only the coffee row past the cutoff should shift. Fixture
        // times sit days either side of the cutoff so local-midnight
        // interpretation doesn't muddle the test across timezones.
        let cutoff: chrono::DateTime<chrono::Utc> = "2026-04-01T00:00:00Z".parse().unwrap();
        let early = make_entry_at_utc("coffee", cutoff - chrono::Duration::days(2), "d1");
        let early_start = early.start_time.as_ref().unwrap().utc;
        let late = make_entry_at_utc("coffee", cutoff + chrono::Duration::days(2), "d1");
        let late_start = late.start_time.as_ref().unwrap().utc;
        let other = make_entry_at_utc("water", cutoff + chrono::Duration::days(3), "d1");
        let other_start = other.start_time.as_ref().unwrap().utc;
        db.insert_entry(&early).unwrap();
        db.insert_entry(&late).unwrap();
        db.insert_entry(&other).unwrap();

        let args = RetimeArgs {
            slug: None,
            offset: Some("+1h".to_string()),
            time: None,
            field: Some("coffee".to_string()),
            after: Some("2026-04-01".to_string()),
            before: None,
        };
        crate::commands::retime::retime(args, &db).unwrap();

        let early_after = db.get_entry(&early.slug).unwrap();
        let late_after = db.get_entry(&late.slug).unwrap();
        let other_after = db.get_entry(&other.slug).unwrap();
        assert_eq!(
            early_after.start_time.as_ref().unwrap().utc,
            early_start,
            "pre-cutoff row must not shift"
        );
        assert_eq!(
            late_after.start_time.as_ref().unwrap().utc,
            late_start + chrono::Duration::hours(1),
            "post-cutoff row shifts +1h"
        );
        assert_eq!(
            other_after.start_time.as_ref().unwrap().utc,
            other_start,
            "other field must not shift"
        );
    }

    #[test]
    fn retime_negative_offset() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("focus", 10_000, "d1");
        let id = e.slug.clone();
        let t0 = e.start_time.as_ref().unwrap().utc;
        db.insert_entry(&e).unwrap();

        crate::commands::retime::retime(retime_args(Some(&id), "-30m"), &db).unwrap();

        let after = db.get_entry(&id).unwrap();
        assert_eq!(
            after.start_time.as_ref().unwrap().utc,
            t0 - chrono::Duration::minutes(30)
        );
    }

    fn make_entry_at_utc(
        field: &str,
        utc: chrono::DateTime<chrono::Utc>,
        device: &str,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(field, Verb::Point, None, None, Some(t), None, device)
    }

    #[test]
    fn delete_then_undo_is_noop() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.slug.clone();
        let original_hash = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        crate::commands::del::del(
            DeleteArgs {
                id_or_slug: Some(id.to_string()),
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
        crate::commands::undo::undo(undo_args(None, false, "15m"), &db).unwrap();

        // Row is back, identical to the original insertion (including content_hash).
        let restored = db.get_entry(&id).unwrap();
        assert_eq!(restored.slug, id);
        assert_eq!(restored.content_hash, original_hash);
        assert!(restored.deleted_at.is_none());
        assert_eq!(db.count_entries().unwrap(), 1);
    }

    // ─── da-e65: _LAST / _LAST_WITH_PROTECTION default targeting ─────────────
    //
    // Acceptance from the bead: bare get/delete/update/undo target the last
    // entry the user touched, with a 15-minute protection window for the
    // destructive variants. Read-only browsing (tail/head/grep) does NOT
    // bump the pointer.

    /// Bare `get` after `coffee=2` returns the just-inserted coffee entry.
    /// (Acceptance #1.)
    #[test]
    fn last_docs_bare_get_returns_most_recent_insert() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
        let inserted = db.entries_for_field("coffee", None).unwrap()[0]
            .slug
            .clone();

        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        // Pointer hid resolves to the same entry the get rendered.
        let resolved = datum_core::ids::resolve_quickid_single(&db, &pointer.hids[0]).unwrap();
        assert_eq!(resolved.slug, inserted);
    }

    /// Bare `delete --force` deletes the just-inserted coffee entry without
    /// prompting. (Acceptance #3 — the prompt path is interactive and not
    /// covered here.)
    #[test]
    fn last_docs_bare_delete_force_removes_most_recent_insert() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 1);

        crate::commands::del::del(
            DeleteArgs {
                id_or_slug: None,
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 0);
    }

    /// `_LAST_WITH_PROTECTION` refuses to resolve when the pointer is older
    /// than the 15-minute window. The error mentions the bypass token so
    /// the user knows what to do.
    #[test]
    fn last_docs_protection_window_rejects_stale_pointer() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Stamp the pointer with a timestamp from yesterday by hand-rolling
        // the row — `set_last_docs` always writes "now".
        let stale = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        db.conn()
            .execute(
                "INSERT INTO last_docs (id, hids, time) VALUES (1, ?1, ?2)",
                rusqlite::params!["[\"abcdef012345\"]", stale],
            )
            .unwrap();

        let err = crate::commands::del::del(
            DeleteArgs {
                id_or_slug: None,
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("more than") && msg.contains("_LAST"),
            "stale-pointer error should explain the bypass: {msg}"
        );
    }

    /// `_LAST` (no protection) resolves regardless of staleness — and bare
    /// `get` defaults to it.
    #[test]
    fn last_docs_plain_last_ignores_window() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let entry = make_entry_at("mood", 100, "d1");
        let hid = entry.hid.clone();
        db.insert_entry(&entry).unwrap();

        // Stamp pointer at the entry but rewrite the time to be ancient.
        let stale = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        db.conn()
            .execute(
                "INSERT INTO last_docs (id, hids, time) VALUES (1, ?1, ?2)",
                rusqlite::params![format!("[\"{hid}\"]"), stale],
            )
            .unwrap();

        // get is non-destructive → defaults to _LAST → no protection check.
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
    }

    /// Bare `get` against a fresh DB (no pointer ever set) errors with a
    /// "no previous documents" message rather than panicking.
    #[test]
    fn last_docs_bare_get_on_fresh_db_errors_clearly() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No previous documents") || msg.contains("touch a document"),
            "fresh-DB get should surface NoLastDocsRef: {msg}"
        );
    }

    /// Read-only browsing commands (tail in particular) do NOT bump the
    /// pointer — only commands that target a specific entry do.
    #[test]
    fn last_docs_tail_does_not_update_pointer() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(Some("mood"), Some(10)), &db).unwrap();

        // Pointer must still be untouched after a pure browse.
        assert!(datum_core::last_docs::get_last_docs(&db).unwrap().is_none());
    }

    /// When the pointer holds multiple hids (a future multi-insert batch
    /// or a comma-separated update), bare `get` resolves to all of them.
    /// Acceptance #4: after `dm coffee=2,coffee=3` the pointer holds two
    /// hids and a bare `dm get` returns both.
    #[test]
    fn last_docs_bare_get_returns_full_batch() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let a = make_entry_at("coffee", 100, "d1");
        let b = make_entry_at("coffee", 200, "d1");
        let (a_hid, b_hid) = (a.hid.clone(), b.hid.clone());
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();
        // Simulate a batch insert by stamping both hids in the pointer.
        datum_core::last_docs::set_last_docs(&db, &[a_hid.clone(), b_hid.clone()]).unwrap();

        // Bare `get` resolves _LAST → both entries are returned and
        // re-stamped onto the pointer (idempotent).
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        let got: std::collections::HashSet<datum_core::ids::Hid> =
            pointer.hids.into_iter().collect();
        assert_eq!(
            got,
            std::collections::HashSet::from([a_hid, b_hid]),
            "bare get must surface every hid in the pointer"
        );
    }

    /// Update through the batch path stamps the pointer so a follow-up
    /// bare `get` reflects the just-mutated row.
    #[test]
    fn last_docs_update_stamps_pointer_for_followup_get() {
        use crate::commands::update_core::{update_batch_from_args, UpdateStrategy};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
        let slug = db.entries_for_field("coffee", None).unwrap()[0]
            .slug
            .clone();

        // Mutate via the explicit-id path; the batch should write the pointer.
        update_batch_from_args(update_args(&slug, vec!["3"]), UpdateStrategy::Replace, &db)
            .unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
    }

    // ─── da-5ns.3: insert/update path wires the new schema ───────────────
    //
    // CLI scenarios from the bead description: reserved keys land on
    // entry columns, the storage layer's `derive_verb` classifies the
    // verb, and an unbound positional on `add` errors instead of silently
    // dropping the value. Unknown keys (including `value=`) fall through
    // to the user-data payload (da-53rz).

    fn make_add_args(field: &str, rest: Vec<&str>) -> LogArgs {
        let mut a = LogArgs::fresh(field);
        a.rest = rest.into_iter().map(String::from).collect();
        a
    }

    #[test]
    fn add_with_state_and_dur_lands_block_verb() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // `dm add` is the shape-inferring verb that lets reserved keys
        // drive the truth-table classification. `Action::Occur` rejects
        // explicit `state=` under da-k5o6, so the prior use of Occur here
        // was incidental — the truth-table contract belongs to Add.
        log_entry(
            Action::Add,
            make_add_args("focus", vec!["state=true", "dur=PT1H"]),
            &db,
        )
        .unwrap();
        let live = db.entries_for_field("focus", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(e.verb, Verb::Block);
        assert_eq!(e.state, Some(serde_json::json!(true)));
        assert_eq!(
            e.dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT1H")
        );
    }

    #[test]
    fn add_with_state_zero_dur_following_state_lands_point_switch() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Add,
            make_add_args(
                "phase",
                vec!["state=true", "dur=PT0S", "following_state=false"],
            ),
            &db,
        )
        .unwrap();
        let live = db.entries_for_field("phase", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(e.verb, Verb::PointSwitch);
        assert_eq!(e.state, Some(serde_json::json!(true)));
        assert_eq!(
            e.dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT0S")
        );
        assert_eq!(e.following_state, Some(serde_json::json!(false)));
    }

    /// da-53rz: post-da-5ns the `value` column is gone, so `value=X`
    /// must fall through to the user-data payload like any other key
    /// (lands as `data.value = X`) instead of erroring as a reserved
    /// column-bound key. Hits real SQL via `log_entry` to catch any
    /// column-routing regression.
    #[test]
    fn datum_occur_with_user_value_key_lands_as_data_key() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(Action::Occur, make_add_args("mood", vec!["value=1"]), &db).unwrap();
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(
            e.data.as_ref().and_then(|d| d.get("value")),
            Some(&serde_json::json!(1)),
            "expected data.value=1, got data={:?}",
            e.data,
        );
    }

    #[test]
    fn add_rejects_unbound_positional_value() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = log_entry(Action::Add, make_add_args("focus", vec!["80"]), &db).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unbound") || msg.contains("positional"),
            "expected unbound-positional rejection, got: {err}"
        );
    }

    #[test]
    fn update_state_recomputes_verb() {
        use datum_core::{db::Db, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(dt),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        let after_seed = db.get_entry(&e.slug).unwrap();
        assert_eq!(after_seed.verb, Verb::Switch);

        // Mutate via update with a dur=PT30M — the row becomes a Block.
        crate::commands::update_core::update_from_args(
            update_args(&e.slug, vec!["dur=PT30M"]),
            crate::commands::update_core::UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let after_update = db.get_entry(&e.slug).unwrap();
        assert_eq!(after_update.verb, Verb::Block);
        assert_eq!(
            after_update
                .dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT30M")
        );
        assert_eq!(after_update.state, Some(serde_json::json!("happy")));
    }

    #[test]
    fn update_negative_dur_rejected() {
        use datum_core::{db::Db, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "focus",
            Verb::Switch,
            Some(serde_json::json!("on")),
            None,
            Some(dt),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        // Negative-dur rejection lives in the storage layer's
        // `derive_verb` call; it bubbles up from the update path.
        let err = crate::commands::update_core::update_from_args(
            update_args(&e.slug, vec!["dur=-PT5M"]),
            crate::commands::update_core::UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("negative") || msg.contains("dur"),
            "expected negative-dur rejection, got: {err}"
        );
    }
}
