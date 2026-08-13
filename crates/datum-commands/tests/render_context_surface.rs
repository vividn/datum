//! da-9p2u — cross-surface parity for [`RenderContext`].
//!
//! Verifies the canonical [`RenderContext::for_entry`] constructor produces
//! identical state-transition context whether the caller is the Action
//! (write-action header) or the Table (`datum tail` row) surface. The
//! interesting case is an entry sitting adjacent to an `IGNORED` transition:
//! the prior surface-specific build paths in `cmd.rs` and `render/mod.rs`
//! used to disagree on whether the IGNORED span counted as the lead state.
//! With the unified constructor they must agree by construction.

use datum_commands::render::{RenderContext, Surface};
use datum_core::{
    db::IGNORED_STATE_VALUE,
    entry::Verb,
    test_util::{entry_builder, fresh_db},
    time::DatumTime,
};
use serde_json::json;

fn t(secs: i64) -> DatumTime {
    use chrono::{TimeZone, Utc};
    let base = Utc.with_ymd_and_hms(2026, 4, 22, 9, 0, 0).unwrap();
    DatumTime {
        utc: base + chrono::Duration::seconds(secs),
        offset_minutes: 0,
        timezone: Some("UTC".to_string()),
    }
}

/// Action and Table surfaces must agree on lead / trail / ongoing /
/// first-occurrence for a row that bookends an IGNORED → tracked transition.
/// Both surfaces now route through `RenderContext::for_entry`, so the
/// per-surface arm is the same code path — the test guards against future
/// drift if anyone reintroduces a Table-only or Action-only ad-hoc build.
#[test]
fn action_and_table_agree_across_ignored_transition() {
    let db = fresh_db();

    // Layer the field history:
    //   T+0   switch focus=IGNORED   (no-tracking sentinel)
    //   T+60  switch focus=work      (tracked transition out of IGNORED)
    //   T+120 switch focus=break     (the row whose context we compare)
    entry_builder("focus")
        .verb(Verb::Switch)
        .state(json!(IGNORED_STATE_VALUE))
        .time_start(t(0))
        .insert(&db);
    entry_builder("focus")
        .verb(Verb::Switch)
        .state(json!("work"))
        .time_start(t(60))
        .insert(&db);
    let probe = entry_builder("focus")
        .verb(Verb::Switch)
        .state(json!("break"))
        .time_start(t(120))
        .insert(&db);

    let action_ctx =
        RenderContext::for_entry(&db, &probe, Surface::Action, None).expect("Action context build");
    let table_ctx =
        RenderContext::for_entry(&db, &probe, Surface::Table, None).expect("Table context build");

    assert_eq!(
        action_ctx.lead_state, table_ctx.lead_state,
        "lead_state must match across surfaces near an IGNORED transition: action={:?} table={:?}",
        action_ctx.lead_state, table_ctx.lead_state,
    );
    assert_eq!(
        action_ctx.trail_state, table_ctx.trail_state,
        "trail_state must match across surfaces"
    );
    assert_eq!(
        action_ctx.ongoing_state, table_ctx.ongoing_state,
        "ongoing_state must match across surfaces"
    );
    assert_eq!(
        action_ctx.is_first_occurrence, table_ctx.is_first_occurrence,
        "is_first_occurrence must match across surfaces"
    );

    // Sanity-check the actual lead — the prior tracked state is `work`, so
    // the LEAD slot should paint `work`'s color (not the IGNORED grey). If
    // a future regression collapses IGNORED→work to None, both contexts
    // would still match each other but the slot would render wrong.
    assert_eq!(
        action_ctx.lead_state,
        Some(json!("work")),
        "lead should resolve to the most recent tracked state, not IGNORED"
    );
}

/// `InfoLine` and `NowView` surfaces don't carry a transition context.
/// They must return the default context regardless of database state, so
/// renderers consuming them never fall back to surface-specific probes.
#[test]
fn info_line_and_now_view_return_default_context() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("calm"))
        .time_start(t(0))
        .insert(&db);
    let probe = entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("happy"))
        .time_start(t(60))
        .insert(&db);

    let info = RenderContext::for_entry(&db, &probe, Surface::InfoLine, None)
        .expect("InfoLine context build");
    let now = RenderContext::for_entry(&db, &probe, Surface::NowView, None)
        .expect("NowView context build");
    let default = RenderContext::default();

    for (name, ctx) in [("InfoLine", info), ("NowView", now)] {
        assert_eq!(ctx.lead_state, default.lead_state, "{name} lead_state");
        assert_eq!(ctx.trail_state, default.trail_state, "{name} trail_state");
        assert_eq!(
            ctx.ongoing_state, default.ongoing_state,
            "{name} ongoing_state"
        );
        assert_eq!(
            ctx.is_first_occurrence, default.is_first_occurrence,
            "{name} is_first_occurrence"
        );
    }
}
