//! `--show json` (da-yc9w): the action echo as data.
//!
//! A write command under [`Show::Json`](super::Show::Json) prints no lines.
//! Every action it would have echoed is collected here, and the outermost
//! dispatch scope prints them as ONE document:
//!
//! ```json
//! {"docs":[{"action":"CREATE","hid":"35mtf","id":"coffee:2026-09-14T16:50:09.613Z",
//!           "header":"CREATE 35mtf coffee:2026-09-14T16:50:09.613Z",
//!           "info":"18:50:09+2  coffee ∅●","info_time":"18:50:09+2",
//!           "data":["beans: \"ethiopian\"","cups: 2"],
//!           "data_inline":"beans: \"ethiopian\", cups: 2",
//!           "field":"coffee","state":null,"action_color":"#26a269",
//!           "field_color":"#24eb05","state_color":null,
//!           "info_segments":[{"text":"18:50:09","color":null},
//!                            {"text":"+2","color":"#888888"},
//!                            {"text":"  ","color":null},
//!                            {"text":"coffee","color":"#24eb05"},
//!                            {"text":" ","color":null},
//!                            {"text":"∅","color":"#888888"},
//!                            {"text":"●","color":"#24eb05"}]}]}
//! ```
//!
//! The strings are [`ActionLines`]' own, ANSI stripped — there is no second
//! formatter here, so the printed and the JSON forms cannot drift. A command
//! that echoes several entries emits one object per entry, in printed order,
//! which is what lets the web bar draw a section per document.
//!
//! The colours are the renderer's own too (da-yu9s): `field_color` and
//! `state_color` are `colors::field_hex` / `colors::state_hex`, the same
//! derivation `fields --json` publishes and the terminal paints with, and
//! `info_segments` is the painted info line split at its own escapes. So a
//! caller never needs a second palette, and never needs to look for the
//! field inside `info` — the document says where it is.
//!
//! Buffering is thread-local, so the in-process test harnesses (which drive
//! many runs on one thread) and the single-threaded WASI build behave alike.

use std::cell::RefCell;

use serde_json::{json, Value as JsonValue};

use super::ActionLines;

thread_local! {
    /// Docs collected in the current dispatch scope, and the scope depth.
    /// Depth is what makes a re-dispatching command (a kit command, a
    /// multi-side run) emit one document rather than one per nested call.
    static ECHO: RefCell<(usize, Vec<JsonValue>)> = const { RefCell::new((0, Vec::new())) };
}

/// One document for `entry`'s action, as the caller must address it: the
/// three header pieces separately, then the lines verbatim, then the
/// colours the terminal would have painted them with (da-yu9s).
fn document(lines: &ActionLines) -> JsonValue {
    json!({
        "action": lines.action,
        "hid": lines.hid,
        "id": lines.id,
        "header": super::strip_ansi(&lines.header),
        "info": super::strip_ansi(&lines.info),
        "info_time": lines.info_time,
        "data": lines.data_items,
        "data_inline": lines.data_inline,
        "action_color": lines.action_color,
        "field": lines.field,
        "state": lines.state,
        "field_color": lines.field_color,
        "state_color": lines.state_color,
        "info_segments": lines
            .info_segments
            .iter()
            .map(|s| json!({ "text": s.text, "color": s.color }))
            .collect::<Vec<_>>(),
    })
}

/// Record one action echo. Called only under `Show::Json`.
pub(super) fn push(lines: &ActionLines) {
    ECHO.with(|slot| slot.borrow_mut().1.push(document(lines)));
}

/// Run `f` as a dispatch scope, printing the collected document when the
/// OUTERMOST scope returns. Nested scopes just keep collecting.
///
/// It prints on an error too: a batch command that wrote and then failed
/// on a later token (`del <good>,<bad>`) echoed writes that happened, and
/// the default output prints them before the error (da-0uvyr). The error
/// itself stays on stderr with its own exit code.
pub fn scope<T>(f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    let outermost = ECHO.with(|slot| {
        let mut s = slot.borrow_mut();
        s.0 += 1;
        if s.0 == 1 {
            s.1.clear();
            true
        } else {
            false
        }
    });

    let result = f();

    let docs = ECHO.with(|slot| {
        let mut s = slot.borrow_mut();
        s.0 -= 1;
        if outermost {
            std::mem::take(&mut s.1)
        } else {
            Vec::new()
        }
    });

    if !docs.is_empty() {
        println!("{}", json!({ "docs": docs }));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(hid: &str) -> ActionLines {
        ActionLines {
            header: format!("CREATE {hid} coffee:2026-09-14T16:50:09.613Z"),
            info: "18:50:09+2  coffee".into(),
            info_time: "18:50:09+2".into(),
            data: vec![],
            action: "CREATE".into(),
            hid: hid.into(),
            id: "coffee:2026-09-14T16:50:09.613Z".into(),
            data_items: vec![],
            data_inline: String::new(),
            field: "coffee".into(),
            state: None,
            action_color: "#26a269".into(),
            field_color: "#000000".into(),
            state_color: None,
            info_segments: vec![],
        }
    }

    #[test]
    fn nested_scopes_collect_into_one_document() {
        let collected = scope(|| {
            push(&lines("aaaaa"));
            scope(|| {
                push(&lines("bbbbb"));
                Ok(())
            })?;
            Ok(ECHO.with(|s| s.borrow().1.len()))
        })
        .unwrap();
        assert_eq!(collected, 2, "the inner scope must not drain the buffer");
    }

    #[test]
    fn a_scope_starts_empty() {
        let _ = scope(|| {
            push(&lines("aaaaa"));
            Ok(())
        });
        let seen = scope(|| Ok(ECHO.with(|s| s.borrow().1.len()))).unwrap();
        assert_eq!(seen, 0, "a fresh outermost scope clears the previous run");
    }

    #[test]
    fn document_carries_the_addressable_pieces() {
        let doc = document(&lines("35mtf"));
        assert_eq!(doc["action"], "CREATE");
        assert_eq!(doc["hid"], "35mtf");
        assert_eq!(doc["id"], "coffee:2026-09-14T16:50:09.613Z");
        assert_eq!(doc["data"], json!([]));
    }

    // ─── The colours as data (da-yu9s) ───────────────────────────────────

    mod colors {
        use super::super::document;
        use crate::render::action::RenderContext;
        use crate::render::colors::field_hex;
        use crate::render::{action_lines_at, Outcome};
        use chrono::{TimeZone, Utc};
        use datum_core::action::Action;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::{FieldSpec, SpecRegistry};
        use datum_core::test_util::{entry_builder, fresh_db};
        use datum_core::time::DatumTime;
        use serde_json::{json, Value as JsonValue};

        fn at(hour: u32) -> DatumTime {
            DatumTime {
                utc: Utc.with_ymd_and_hms(2026, 9, 15, hour, 30, 0).unwrap(),
                offset_minutes: 0,
                timezone: Some("UTC".to_string()),
                uncertainty: None,
            }
        }

        fn row(field: &str, verb: Verb, state: Option<JsonValue>) -> Entry {
            Entry::new(
                field.to_string(),
                verb,
                state,
                None,
                Some(at(9)),
                None,
                "test-device".to_string(),
            )
        }

        /// The document for `entry` under `ctx`, against a fixed "now".
        fn doc_for(
            entry: &Entry,
            registry: Option<&SpecRegistry>,
            ctx: &RenderContext,
        ) -> JsonValue {
            document(&action_lines_at(
                entry,
                Outcome::Create,
                registry,
                Some(ctx),
                &at(12),
            ))
        }

        fn joined(doc: &JsonValue) -> String {
            doc["info_segments"]
                .as_array()
                .expect("info_segments is an array")
                .iter()
                .map(|s| s["text"].as_str().expect("a segment's text is a string"))
                .collect()
        }

        /// The anti-drift rule: whatever the line says, the segments say the
        /// same bytes. A dropped `!` or `¢` marker shows up here.
        #[test]
        fn segments_reassemble_the_info_line() {
            let plain = RenderContext::default();
            let repeat = RenderContext {
                lead_state: Some(json!("happy")),
                is_state_repeat: true,
                ..RenderContext::default()
            };
            let timeless = Entry::new(
                "word".to_string(),
                Verb::Point,
                Some(json!("correr")),
                None,
                None,
                None,
                "test-device".to_string(),
            );
            let cases: Vec<(&str, Entry, &RenderContext)> = vec![
                ("occur coffee", row("coffee", Verb::Point, None), &plain),
                (
                    "switch mood happy",
                    row("mood", Verb::Switch, Some(json!("happy"))),
                    &plain,
                ),
                (
                    "start work",
                    row("work", Verb::Switch, Some(json!(true))),
                    &plain,
                ),
                (
                    "end work",
                    row("work", Verb::Switch, Some(json!(false))),
                    &plain,
                ),
                (
                    "structured state",
                    row(
                        "book",
                        Verb::Switch,
                        Some(json!({"id": "dune", "format": "audio"})),
                    ),
                    &plain,
                ),
                (
                    "a repeated state, with its ! marker",
                    row("mood", Verb::Switch, Some(json!("happy"))),
                    &repeat,
                ),
                ("a timeless note", timeless, &plain),
            ];
            for (what, entry, ctx) in cases {
                let doc = doc_for(&entry, None, ctx);
                assert_eq!(
                    joined(&doc),
                    doc["info"].as_str().unwrap(),
                    "{what}: the segments must rebuild the info line exactly"
                );
                // da-7xp82: the stamp leads the line, a space before the rest.
                let time = doc["info_time"].as_str().unwrap();
                assert!(!time.is_empty(), "{what}: no info_time");
                assert!(
                    doc["info"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{time} ")),
                    "{what}: info does not start with info_time {time:?}"
                );
            }
        }

        /// The field's own segment carries `field_color` and the state's
        /// carries `state_color`, in that order — which is why a surface
        /// reads them off the segments instead of searching `info` for the
        /// field's name: `switch mood mood` writes the word twice, and the
        /// two occurrences are painted differently.
        #[test]
        fn the_field_and_state_segments_carry_their_colours() {
            let mut spec = FieldSpec::new("mood", Action::Switch);
            spec.color = Some("#ff6600".to_string());
            let mut registry = SpecRegistry::new();
            registry.fields.insert("mood".to_string(), spec);

            let entry = row("mood", Verb::Switch, Some(json!("mood")));
            let doc = doc_for(&entry, Some(&registry), &RenderContext::default());
            assert_ne!(
                doc["field_color"], doc["state_color"],
                "the state hashes its own name; the field took the spec's color"
            );
            let named: Vec<&JsonValue> = doc["info_segments"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["text"] == "mood")
                .collect();
            assert_eq!(named.len(), 2, "the field, then the state: {doc}");
            assert_eq!(named[0]["color"], doc["field_color"]);
            assert_eq!(named[1]["color"], doc["state_color"]);
        }

        /// One derivation, two spellings: the echo's `field_color` is the
        /// colour `fields --json` publishes for that field in that db.
        #[test]
        fn field_color_is_the_fields_listing_colour() {
            let db = fresh_db();
            let entry = entry_builder("coffee").insert(&db);
            let registry = db.load_registry().unwrap();

            let doc = doc_for(&entry, Some(&registry), &RenderContext::default());
            let listing = crate::commands::fields::compute_fields(&db).unwrap();
            assert_eq!(listing[0]["field"], "coffee");
            assert_eq!(doc["field_color"], listing[0]["color"]);
            assert_eq!(doc["field_color"], field_hex("coffee", None));
            assert_eq!(doc["field"], "coffee");
            assert_eq!(doc["state"], JsonValue::Null, "an occur logs no state");
            assert_eq!(doc["state_color"], JsonValue::Null);
        }

        /// A spec override reaches both the document and the painted line.
        #[test]
        fn a_spec_colour_shows_up_in_field_color() {
            let mut spec = FieldSpec::new("mood", Action::Switch);
            spec.color = Some("#ff6600".to_string());
            let mut registry = SpecRegistry::new();
            registry.fields.insert("mood".to_string(), spec);

            let entry = row("mood", Verb::Switch, Some(json!("happy")));
            let doc = doc_for(&entry, Some(&registry), &RenderContext::default());
            assert_eq!(doc["field_color"], "#ff6600");
            assert!(
                doc["info_segments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s["text"] == "mood" && s["color"] == "#ff6600"),
                "the field's segment is painted with the spec's color: {doc}"
            );
        }
    }
}
