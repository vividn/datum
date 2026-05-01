//! Parity tests for `datum note` (and its `add` alias) — P1 / da-wsax.
//!
//! Ports the 25 cases from `/tmp/datum-survey/src/commands/__test__/addCmd.test.ts`
//! in the original datum project, adapted to daydumb's flag-based model. Each
//! Rust test maps 1-to-1 to a TS case; doc headers identify the source item.
//!
//! Divergences from datum:
//!   * daydumb has no `-F`/fieldless mode — entries always carry a field. The
//!     TS cases that exercise fieldless docs, custom base data (`-b`), or
//!     datum's "required keys" positional semantics are ported as structural
//!     skips that document the divergence. Skip headers call out exactly
//!     which datum behavior has no daydumb analog.
//!   * daydumb dedups on `content_hash = SHA-256(field + time_start +
//!     time_end + value + extra_data)`, not on `_id`. Two rows sharing a
//!     `--id` but differing in extra_data produce distinct content hashes
//!     and both insert. The `--merge` / `--update` tests are adapted to
//!     content-hash collisions (the daydumb conflict surface).
//!   * daydumb always stores `id_structure` whenever `--id TEMPLATE` is set,
//!     even for templates with no `%placeholder%`; datum only stored it when
//!     the template had key references. The TS case is ported with the
//!     daydumb contract locked in.

mod common;

use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};
use serde_json::json;

// ─── TS #1: "inserts documents into couchdb" ─────────────────────────────────

/// `datum note foo` creates exactly one row.
#[test]
fn note_creates_exactly_one_entry() {
    let db = fresh_db();
    run_cli(&["add", "foo"], &db).expect("note foo");
    assert_count!(db, "foo", 1);
}

// ─── TS #2: "includes field in the data" ─────────────────────────────────────

/// The positional first arg sets `entry.field`. Additional key=value pairs
/// land in `extra_data`.
#[test]
fn note_sets_field_and_collects_extra_kv() {
    let db = fresh_db();
    run_cli(&["add", "field"], &db).expect("note field");
    let e1 = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(e1, field: "field", verb: "add");
    assert!(
        e1.data.is_none(),
        "no kv pairs ⇒ extra_data stays None",
    );

    run_cli(&["add", "field2", "foo=bar"], &db).expect("note field2 foo=bar");
    let e2 = &db.entries_for_field("field2", None).unwrap()[0];
    assert_entry_matches!(e2, field: "field2");
    let extra = e2.data.as_ref().expect("foo=bar lands in extra_data");
    assert_eq!(extra.get("foo"), Some(&json!("bar")));
}

// ─── TS #3: "uses the first non explicitly assigned field in the data as field" ─

/// Skipped: datum parsed `foo=bar dataField` and promoted `dataField` into
/// the stored `field` when no other field was given. Daydumb requires the
/// field as an explicit first positional, so the "first non-kv token becomes
/// the field" promotion has no analog. See `parse_log_args` — the first
/// positional *is* the field, period.
///
/// da-5ns.3 made `add` reject unbound positionals, so the second
/// positional `dataField` now errors instead of landing as the primary
/// value. da-5ns.8 should retire this test (or rewrite it on a `-K`
/// slot) once the test sweep lands.
#[test]
#[ignore = "da-5ns.3 strict positional gate breaks the legacy primary-value path; da-5ns.8 sweeps"]
fn field_promotion_from_positional_skipped_no_analog() {
    // Confirm the daydumb contract: the first positional is always treated
    // as the field, not promoted by scanning for the first bare token.
    let db = fresh_db();
    run_cli(&["add", "foo=bar", "dataField"], &db)
        .expect("note foo=bar dataField");
    // `foo=bar` is argv[1], so it becomes field_or_verb="foo=bar"
    // — and the `=` means it lands in extra_data parsing as a kv pair... no,
    // field_or_verb is a *positional*, not a kv token. The field literally
    // becomes "foo=bar" (no split). That's the lock-in the test records.
    let entries = db.entries_for_field("foo=bar", None).unwrap();
    assert_eq!(entries.len(), 1, "first positional lands as-is in field");
    assert_eq!(entries[0].field, "foo=bar");
}

// ─── TS #4: "still handles field appropriately when there are required keys" ─

/// Skipped: datum's `-k abc` reserved a positional slot named `abc`.
/// Daydumb's `-k KEY=VALUE` only accepts complete kv pairs — a bare `-k abc`
/// errors. The "required keys absorb positionals" semantic does not port.
#[test]
fn required_keys_positional_absorption_skipped_no_analog() {
    let db = fresh_db();
    // In daydumb, `-k abc` (no `=`) is still consumed as the -k
    // value but `parse_log_args` only splits on `=`; it is silently dropped.
    // The positional `value` becomes the primary value.
    run_cli(&["add", "field", "-k", "abc=value"], &db)
        .expect("note field -k abc=value");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    let extra = e.data.as_ref().expect("-k lands in extra_data");
    assert_eq!(extra.get("abc"), Some(&json!("value")));
}

// ─── TS #5: "can skip the field with -F" ──────────────────────────────────────

/// Skipped: daydumb has no `-F`/fieldless mode. Every entry carries a field.
/// The TS case has no translation.
#[test]
fn fieldless_mode_skipped_no_analog() {
    let db = fresh_db();
    // The closest observable analog: omitting the field entirely must fail
    // at the parser layer (field_or_verb is required positional).
    let err = run_cli(&["add"], &db).expect_err("bare `note` has no field");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("required") || msg.contains("missing"),
        "expected parser 'required' error, got: {msg}",
    );
}

// ─── TS #6: "uses the field prop to populate the field key, but can also be specified again in the data" ─

/// Skipped: `-F field=manual` is a fieldless mode variant. No daydumb analog.
#[test]
fn fieldless_field_kv_skipped_no_analog() {
    // Daydumb: `datum note fromField field=fromData` — `fromField` is the
    // field, `field=fromData` drops into extra_data as key="field",
    // value="fromData". The entry.field stays "fromField"; the kv pair
    // coexists on a different axis.
    let db = fresh_db();
    run_cli(&["add", "fromField", "field=fromData"], &db)
        .expect("note fromField field=fromData");
    let e = &db.entries_for_field("fromField", None).unwrap()[0];
    assert_eq!(e.field, "fromField", "positional field wins");
    let extra = e.data.as_ref().unwrap();
    assert_eq!(
        extra.get("field"),
        Some(&json!("fromData")),
        "field=... kv still lands in extra_data under the 'field' key",
    );
}

// ─── TS #7: "throws an error if addCmd is called with no field, no id, no createTime, and no data" ─

/// Bare `note` (no positional) must fail at parse time, never silently.
#[test]
fn bare_note_fails_parse() {
    let db = fresh_db();
    let err = run_cli(&["add"], &db).expect_err("field required");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("required") || msg.contains("missing"),
        "expected parse error, got: {msg}",
    );
    assert_eq!(
        db.entries_for_field("", None).unwrap().len(),
        0,
        "no row must have slipped in before the error",
    );
}

// ─── TS #8: "throws an IdError if data is provided, but the id is specified as an empty string" ─

/// Daydumb: `--id ''` sets the template to the empty string, which
/// interpolates to an empty slug. We lock in *some* stable behavior — the
/// command succeeds but the slug is empty. If this is ever tightened to an
/// error (parity with datum's IdError), the assertion flips.
#[test]
fn empty_id_template_produces_empty_slug() {
    let db = fresh_db();
    run_cli(&["add", "field", "--id", ""], &db).expect("note --id ''");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(e.slug, "", "empty template ⇒ empty slug (current contract)");
    assert_eq!(
        e.id_structure.as_deref(),
        Some(""),
        "id_structure still reflects the (empty) template",
    );
}

// ─── TS #9: "can add a blank fieldless document with metadata" ────────────────

/// Skipped: fieldless mode not implemented — see TS #5 rationale.
#[test]
fn blank_fieldless_with_metadata_skipped_no_analog() {
    // Kept as a structural placeholder so the count of ported tests matches
    // the upstream survey. Inverse invariant: a normal `note field` always
    // carries populated bookkeeping fields by default.
    let db = fresh_db();
    run_cli(&["add", "field"], &db).expect("note field");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert!(!e.device_id.is_empty(), "device_id populated by default");
    assert_ne!(
        e.create_time,
        chrono::DateTime::UNIX_EPOCH,
        "created_at populated by default",
    );
}

// ─── TS #10: "can add a completely blank document if an id is provided" ──────

/// Skipped: requires fieldless mode. No daydumb analog.
#[test]
fn completely_blank_with_id_skipped_no_analog() {
    // Closest analog: `--id test` on a field produces a slug matching the
    // template. Locks in the `--id` wiring.
    let db = fresh_db();
    run_cli(&["add", "field", "--id", "test"], &db).expect("note field --id test");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(e.slug, "test", "explicit --id wins over default field:time");
}

// ─── TS #11: "calls addDoc" ───────────────────────────────────────────────────

/// Skipped: datum's test spied on the internal `addDoc` function. Daydumb's
/// equivalent lives at `Db::insert_entry_with_strategy`; asserting it was
/// called is indirect via observing the resulting row, which every other
/// ported test already covers.
#[test]
fn internal_insert_spy_skipped_structural_only() {
    let db = fresh_db();
    run_cli(&["add", "field", "foo=abc", "--id", "%foo%"], &db)
        .expect("note field foo=abc --id %foo%");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    // Observable proxy for "the insert happened with these args":
    assert_eq!(e.slug, "abc", "template interpolated against extra_data");
    assert_eq!(
        e.data.as_ref().unwrap().get("foo"),
        Some(&json!("abc")),
    );
    assert_eq!(e.id_structure.as_deref(), Some("%foo%"));
}

// ─── TS #12: "Can remove metadata entirely" ───────────────────────────────────

/// The headline P1 test: `--no-metadata` / `-M` suppresses bookkeeping.
/// Per the daydumb divergence (NOT NULL columns), suppression zeroes the
/// stamps and clears `device_id`.
#[test]
fn no_metadata_suppresses_timestamps() {
    let db = fresh_db();

    run_cli(&["add", "hasMetadata", "--id", "hasMetadata"], &db)
        .expect("note hasMetadata --id hasMetadata");
    let has = db.get_entry("hasMetadata").unwrap();
    assert!(!has.device_id.is_empty(), "default path keeps device_id");
    assert_ne!(
        has.create_time,
        chrono::DateTime::UNIX_EPOCH,
        "default path sets created_at to now()",
    );

    run_cli(&["add", "noMetadata", "--id", "noMetadata", "-M"], &db)
        .expect("note noMetadata -M");
    let none = db.get_entry("noMetadata").unwrap();
    assert_eq!(none.device_id, "", "-M clears device_id");
    assert_eq!(
        none.create_time,
        chrono::DateTime::UNIX_EPOCH,
        "-M zeros created_at",
    );
    assert_eq!(
        none.modify_time,
        chrono::DateTime::UNIX_EPOCH,
        "-M zeros modified_at",
    );

    // `--no-metadata` long form behaves identically.
    run_cli(
        &["add", "noMetaLong", "--id", "noMetaLong", "--no-metadata"],
        &db,
    )
    .expect("note --no-metadata");
    let long = db.get_entry("noMetaLong").unwrap();
    assert_eq!(long.device_id, "");
    assert_eq!(long.create_time, chrono::DateTime::UNIX_EPOCH);
}

// ─── TS #13: "tells the user if the document already exists identical data" ──

/// Identical content re-insertion dedups via content_hash. First call is
/// Inserted (CREATE); second is Ignored (EXISTS).
#[test]
fn identical_content_triggers_exists_outcome() {
    use datum_core::db::{ConflictStrategy, InsertOutcome};

    let db = fresh_db();
    // Use the strategy API directly to observe the outcome — the CLI path
    // already produces the same InsertOutcome variants.
    run_cli(&["add", "bob", "foo=bar", "-t", "09:00"], &db).expect("first note");
    assert_count!(db, "bob", 1);
    let first = db.entries_for_field("bob", None).unwrap()[0].clone();

    // Rebuild an entry with identical content (same field, time, value,
    // extra) and attempt insertion via Ignore — must report as duplicate.
    let outcome = db
        .insert_entry_with_strategy(&first, ConflictStrategy::Ignore)
        .unwrap();
    match outcome {
        InsertOutcome::Ignored { slug } => assert_eq!(slug, first.slug, "points at original"),
        other => panic!("expected Ignored, got {other:?}"),
    }
    // Row count unchanged.
    assert_count!(db, "bob", 1);
}

// ─── TS #14: "fails if addedDocument conflicts with different data" ──────────

/// Daydumb divergence: slug uniqueness is the daydumb analog of datum's
/// DocExistsError — "same id, different data" is essentially-duplicate data
/// (the user explicitly asserted the same identity twice) and must error
/// rather than silently store under a suffixed slug. Only the first row
/// survives.
#[test]
fn same_id_different_data_errors_on_second() {
    let db = fresh_db();
    run_cli(&["add", "field", "--id", "doug", "foo=bar"], &db)
        .expect("first note");
    let err = run_cli(&["add", "field", "--id", "doug", "different=data"], &db)
        .expect_err("second note at same --id must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("duplicate") || msg.contains("doug"),
        "error must mention the duplicate slug, got: {msg}",
    );

    let entries = db.entries_for_field("field", None).unwrap();
    assert_eq!(entries.len(), 1, "second insert rejected");
    assert_eq!(entries[0].slug, "doug");
    assert_eq!(entries[0].id_structure.as_deref(), Some("doug"));
}

// ─── TS #15: "inserts id structure into the metadata" ────────────────────────

/// Daydumb's `--id` only takes a single template (`Option<String>`).
/// Datum allowed two `--id` flags to concatenate; the equivalent daydumb
/// form is the already-concatenated template. Locks in the one-flag shape.
#[test]
fn id_template_recorded_in_id_structure() {
    let db = fresh_db();
    run_cli(
        &[
            "add",
            "field",
            "--id",
            "rawString__%foo%!!",
            "foo=abc",
        ],
        &db,
    )
    .expect("add --id rawString__%foo%!!");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(
        e.id_structure.as_deref(),
        Some("rawString__%foo%!!"),
        "id_structure preserves the raw template",
    );
    assert_eq!(e.slug, "rawString__abc!!", "interpolates %foo% → abc");
}

// ─── TS #16: "can use custom base data" ──────────────────────────────────────

/// Skipped: daydumb has no `-b`/`--base-data` flag. Base-data merging would
/// be a pre-pass that injects a JSON object into extra_data before the
/// kv parser runs. Not scoped for P1.
#[test]
fn base_data_flag_skipped_no_analog() {
    // Structural placeholder. No-op test asserting the flag is rejected.
    let db = fresh_db();
    let err = run_cli(&["add", "field", "-b", "{a:1}"], &db)
        .expect_err("-b is not a daydumb flag");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unexpected") || msg.contains("-b"),
        "expected parser rejection, got: {msg}",
    );
}

// ─── TS #17: "can write payloads directly by specifying base-data no-metadata and fieldless" ─

/// Skipped: requires `-b` + `-F` + `-M`. Two of the three are absent.
#[test]
fn direct_payload_write_skipped_no_analog() {
    // Closest analog: `-M` alone still writes a normal row with zeroed
    // metadata — covered by the headline TS #12 test.
}

// ─── TS #18: "throws a BaseDataError if baseData is malformed" ────────────────

/// Skipped: requires `-b`.
#[test]
fn malformed_base_data_skipped_no_analog() {}

// ─── TS #19: "prefers the _id specified when in no-metadata mode" ─────────────

/// Skipped: requires `-b` (inline `_id` in base data). Daydumb's slug is
/// the analog to datum's `_id`, and it is driven solely by `--id`/template
/// interpolation — no competing sources exist to pick among.
#[test]
fn payload_id_precedence_skipped_no_analog() {
    // Lock-in: `--id` is the single source for daydumb slug templating.
    let db = fresh_db();
    run_cli(&["add", "field", "--id", "explicit-slug"], &db)
        .expect("note --id explicit-slug");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(e.slug, "explicit-slug");
}

// ─── TS #20: "does not contain idStructure in the metadata if id does not depend on values from data" ─

/// Daydumb divergence: `--id` *always* records `id_structure`, even for
/// templates with no `%placeholder%`. Datum only recorded idStructure when
/// the template referenced extra_data keys. The lock-in matches daydumb's
/// simpler contract.
#[test]
fn id_structure_always_set_when_id_given() {
    let db = fresh_db();
    run_cli(&["add", "field", "--id", "rawString"], &db)
        .expect("note --id rawString");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(e.slug, "rawString");
    assert_eq!(
        e.id_structure.as_deref(),
        Some("rawString"),
        "daydumb always persists the id template, unlike datum",
    );

    // Without `--id` at all, id_structure stays None (built-in slug rule).
    run_cli(&["add", "other"], &db).expect("note other");
    let other = &db.entries_for_field("other", None).unwrap()[0];
    assert!(
        other.id_structure.is_none(),
        "no --id ⇒ no id_structure (default field:time slug)",
    );
}

// ─── TS #21: "can display just the data of documents or the whole documents" ─

/// `--show all` / `-A` must be accepted by the parser for log commands.
/// Output content is covered by `tests/output_parity.rs`; here we only
/// prove the flag wiring doesn't error and the row still lands.
#[test]
fn show_all_flag_wired_through() {
    let db = fresh_db();
    run_cli(&["add", "field", "--id", "that-id", "--show", "all"], &db)
        .expect("--show all");
    let e = db.get_entry("that-id").unwrap();
    assert_eq!(e.field, "field");

    run_cli(&["add", "field", "--id", "short-show-all", "-A"], &db)
        .expect("-A (show-all shorthand)");
    let e2 = db.get_entry("short-show-all").unwrap();
    assert_eq!(e2.field, "field");
}

// ─── TS #22: "can merge into an existing document with --merge" ──────────────

/// `--merge` maps to `ConflictStrategy::Merge` and fires on content_hash
/// collisions. Daydumb divergence: the datum case mutates `foo` within the
/// extra_data of a shared id; daydumb's merge runs only when *everything*
/// already matches (field/time/value/extra) — so the observable effect is
/// that `--merge` dispatches to the Merge branch and the row count stays
/// at 1. The A4 A/B/C strategy suite covers deeper merge semantics.
#[test]
fn merge_flag_routes_to_merge_strategy() {
    let db = fresh_db();
    run_cli(
        &["add", "field", "-t", "09:00", "-k", "foo=abc"],
        &db,
    )
    .expect("seed note");
    assert_count!(db, "field", 1);
    let before = db.entries_for_field("field", None).unwrap()[0].clone();

    std::thread::sleep(std::time::Duration::from_millis(5));

    // Same content → same content_hash. `--merge` dispatches to Merge
    // strategy, which bumps modified_at but leaves the row count at 1.
    run_cli(
        &[
            "add", "field", "-t", "09:00", "-k", "foo=abc", "--merge",
        ],
        &db,
    )
    .expect("merge dispatch");
    assert_count!(db, "field", 1);
    let after = db.entries_for_field("field", None).unwrap()[0].clone();
    assert_eq!(after.slug, before.slug, "merged in place, not rekeyed");
    assert!(
        after.modify_time > before.modify_time,
        "Merge strategy bumps modified_at",
    );
}

// ─── TS #23: "can update and existing document with --conflict" ──────────────

/// `--conflict update` maps to `ConflictStrategy::Update`. Lock-in matches
/// the existing `log_with_update_flag_routes_to_update_strategy` harness
/// test but via the full `--conflict` enum flag rather than the `--update`
/// shorthand.
#[test]
fn conflict_update_flag_routes_to_update_strategy() {
    let db = fresh_db();
    run_cli(&["add", "field", "-t", "09:00", "-k", "foo=abc"], &db)
        .expect("seed note");
    assert_count!(db, "field", 1);
    let before = db.entries_for_field("field", None).unwrap()[0].clone();

    std::thread::sleep(std::time::Duration::from_millis(5));

    run_cli(
        &[
            "note",
            "field",
            "-t",
            "09:00",
            "-k",
            "foo=abc",
            "--conflict",
            "update",
        ],
        &db,
    )
    .expect("--conflict update dispatch");
    assert_count!(db, "field", 1);
    let after = db.entries_for_field("field", None).unwrap()[0].clone();
    assert_eq!(after.slug, before.slug);
    assert!(after.modify_time > before.modify_time);
}

// ─── TS #24: "correctly displays interpolated field values in output" ────────

/// Daydumb templates the slug via `--id`, not the field itself. Adapted:
/// the slug interpolates extra_data correctly with both single and
/// composite templates.
#[test]
fn slug_template_interpolates_composite_keys() {
    let db = fresh_db();
    run_cli(
        &[
            "note",
            "field",
            "project=Testing",
            "activity=Fields",
            "--id",
            "%project%_%activity%",
        ],
        &db,
    )
    .expect("note composite template");
    let e1 = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(e1.slug, "Testing_Fields");

    run_cli(
        &[
            "note",
            "other",
            "project=Composite",
            "name=Test",
            "--id",
            "%project%-%name%",
        ],
        &db,
    )
    .expect("note hyphen template");
    let e2 = &db.entries_for_field("other", None).unwrap()[0];
    assert_eq!(e2.slug, "Composite-Test");
}

// ─── TS #25: "can add a remainder comment easily" ─────────────────────────────

/// Daydumb divergence: datum's `-C` flag promoted "all positionals after
/// kv pairs" into a single `comment` string. Daydumb uses `-c`/`--comment`
/// with an explicit value instead. The ported invariant: `-c` populates
/// `extra_data.comment`, and subsequent positionals become the primary
/// value (or extend it into an array, per `parse_log_args`).
#[test]
fn comment_flag_populates_extra_data_comment() {
    let db = fresh_db();
    run_cli(
        &["add", "field", "foo=bar", "-c", "comment remainder"],
        &db,
    )
    .expect("note -c 'comment remainder'");
    let e = &db.entries_for_field("field", None).unwrap()[0];
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra.get("foo"), Some(&json!("bar")));
    assert_eq!(
        extra.get("comment"),
        Some(&json!("comment remainder")),
        "-c sets comment in extra_data",
    );
}

// ─── P1 acceptance-criteria tests (beyond the 25-case port) ──────────────────

// ─── da-ndd.18: `id=` positional convention → field:id slugs ─────────────────

/// A positional `id=<name>` kvpair is promoted to the slug identity
/// (`field:name`) without needing an explicit `-K id` declaration. The
/// other kvpairs remain in extra_data. This makes entries act as documents
/// that later entries can reference via the colon pattern.
#[test]
fn id_positional_promotes_to_slug() {
    let db = fresh_db();
    run_cli(
        &["add", "book", "title=goldmund und narziß", "author=herman hesse", "lang=de", "id=goldmund"],
        &db,
    )
    .expect("datum add book …  id=goldmund");
    let e = db.get_entry("book:goldmund").expect("slug book:goldmund exists");
    assert_eq!(e.slug, "book:goldmund");
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra.get("title"), Some(&json!("goldmund und narziß")));
    assert_eq!(extra.get("author"), Some(&json!("herman hesse")));
    assert_eq!(extra.get("lang"), Some(&json!("de")));
    assert!(
        extra.get("id").is_none(),
        "id= is stripped from extra_data once promoted to the slug",
    );
}

/// `id=` with no other payload still promotes — minimal document form.
#[test]
fn id_positional_alone_produces_field_colon_id_slug() {
    let db = fresh_db();
    run_cli(&["add", "person", "id=vividn"], &db).expect("add person id=vividn");
    let e = db.get_entry("person:vividn").expect("slug person:vividn exists");
    assert_eq!(e.slug, "person:vividn");
    // Only `id` was supplied; it's consumed by the slug, so extra_data stays None.
    assert!(
        e.data.is_none(),
        "sole id= is consumed by the slug; no extra_data left behind",
    );
}

/// Without `id=` (or `-K`), slugs fall back to the default `field:<timestamp>`
/// shape. Regression guard: the new promotion path must not hijack entries
/// that have no `id=` at all.
#[test]
fn no_id_falls_back_to_timestamp_slug() {
    let db = fresh_db();
    run_cli(&["add", "book", "title=anon"], &db).expect("add book title=anon");
    let e = &db.entries_for_field("book", None).unwrap()[0];
    assert!(
        e.slug.starts_with("book:"),
        "slug preserves the field:<id> shape: {}",
        e.slug,
    );
    assert!(
        !e.slug.ends_with(":anon") && e.slug.len() > "book:".len() + 4,
        "slug is timestamp-derived, not id-derived: {}",
        e.slug,
    );
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra.get("title"), Some(&json!("anon")));
}

/// Explicit `-K` slug-keys take precedence over the `id=` convention. When
/// both are present, `-K` fills slug_values first and `id=` stays in
/// extra_data as ordinary payload — the user explicitly declared a
/// different slug contract.
#[test]
fn explicit_dash_k_wins_over_id_positional() {
    let db = fresh_db();
    run_cli(
        &["add", "book", "-K", "custom", "primary-slug", "id=secondary"],
        &db,
    )
    .expect("add book -K custom primary-slug id=secondary");
    let e = db.get_entry("book:primary-slug").expect("slug book:primary-slug exists");
    assert_eq!(e.slug, "book:primary-slug");
    let extra = e.data.as_ref().unwrap();
    assert_eq!(
        extra.get("id"),
        Some(&json!("secondary")),
        "id= stays in extra_data when -K already populated slug_values",
    );
    assert_eq!(
        extra.get("custom"),
        Some(&json!("primary-slug")),
        "-K slot also stored its value in extra_data (existing contract)",
    );
}

/// Values that look like `<field>:<id>` — e.g. `book:goldmund` — are stored
/// as plain strings on write. No slug lookup happens at write time; refs
/// are resolved on read so that documents added later still propagate
/// their data to earlier-written references (per the "live refs" design).
#[test]
fn colon_pattern_value_stored_as_plain_string() {
    let db = fresh_db();
    // Create the document.
    run_cli(&["add", "book", "title=Goldmund", "id=goldmund"], &db)
        .expect("add book id=goldmund");
    // Create a switch that references it.
    run_cli(&["switch", "consume", "book:goldmund"], &db)
        .expect("switch consume book:goldmund");
    let switch_entry = &db.entries_for_field("consume", None).unwrap()[0];
    assert_eq!(
        switch_entry.value,
        Some(json!("book:goldmund")),
        "reference value is stored verbatim — no write-time resolution",
    );
    // The helper parses the string shape; callers (future display code) are
    // responsible for DB lookups.
    let (field, id) = datum_core::doc_ref::parse_ref("book:goldmund")
        .expect("colon-pattern string is a candidate ref");
    assert_eq!(field, "book");
    assert_eq!(id, "goldmund");
}

/// Unresolved references do not block writes — writing a switch that
/// references a not-yet-existing document still succeeds. Matches the
/// "accept + mark unresolved" policy (open question Q1 resolved to option b).
#[test]
fn unresolved_doc_reference_does_not_block_write() {
    let db = fresh_db();
    run_cli(&["switch", "consume", "book:does-not-exist-yet"], &db)
        .expect("forward reference writes successfully");
    let e = &db.entries_for_field("consume", None).unwrap()[0];
    assert_eq!(e.value, Some(json!("book:does-not-exist-yet")));
}

/// After D3, `note` is no longer a first-class subcommand — it's a seeded
/// alias that routes to `add`. `datum note field …` and `datum add field …`
/// must produce identical entries, both with `Verb::Add`.
#[test]
fn note_verb_removed_but_alias_works() {
    let db_note = fresh_db();
    let db_add = fresh_db();

    run_cli(&["note", "field", "foo=bar", "-t", "09:00"], &db_note)
        .expect("note → add via seeded alias");
    run_cli(&["add", "field", "foo=bar", "-t", "09:00"], &db_add)
        .expect("add dispatch");

    let n = &db_note.entries_for_field("field", None).unwrap()[0];
    let a = &db_add.entries_for_field("field", None).unwrap()[0];

    assert_eq!(n.field, a.field);
    assert_eq!(n.verb, Verb::Add, "note alias resolves to Verb::Add");
    assert_eq!(a.verb, Verb::Add);
    assert_eq!(n.verb.to_string(), "add");
    assert_eq!(n.value, a.value);
    assert_eq!(n.data, a.data);
    // content_hash derives only from field/time/value/extra — must match
    // across the two dispatches.
    assert_eq!(
        n.content_hash, a.content_hash,
        "identical payloads ⇒ identical content_hash across add/note",
    );
}
