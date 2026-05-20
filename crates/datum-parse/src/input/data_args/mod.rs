//! `dataArgs` — the core key/value/positional reconciliation from datum's
//! `src/input/dataArgs.ts`.
//!
//! Given a `DataArgs` bag (positional `data`, declared `keys`, comment(s),
//! remainder disposition, leniency) this module produces the final
//! [`DatumData`] object the command layer writes to an entry.
//!
//! # Module split
//!
//! Single fold loop, three orthogonal phases:
//!
//! - [`classify`] — token kind / path normalization / suffix predicates
//! - [`coerce`]   — `&str` → JSON [`Value`] (numbers, durations, units…)
//! - [`accumulate`] — write the coerced value into the growing payload at
//!   a dotted path; handle append fan-out and `.` defaults
//!
//! The orchestrator below ([`handle_data_args`]) walks the positional
//! queue, dispatches `key=value` / positional / remainder routes, and
//! delegates everything else to those modules.
//!
//! # Non-ports
//!
//! * `-b`/`--base-data` (JSON seed). Low demand, explicitly dropped by the
//!   parity audit; add back later once the typed API lands.
//! * Command-change mid-parse escape (`dur=5 start` → switch verb).
//!   Daydumb dispatches verb-first; conflicts with the field/verb swap.
//! * Chrono-node natural-language parsing inside `alter_datum_data`'s
//!   time/date inference paths. Values that would require NL parsing
//!   stay as strings; structured forms (ISO date, `±N[unit]`, `+/-N` day
//!   offsets, `HH:MM`, `HHMM`, ISO duration, colon/named-unit duration)
//!   round-trip through the existing [`crate::time`] parsers.
//!
//! See `docs/parity-audit/parsing.md` ("`dataArgs.ts`") for the spec.

mod accumulate;
mod classify;
mod coerce;

use chrono::NaiveDateTime;
use serde_json::{Map, Value};

use crate::error::{ParseError, Result};
use crate::input::consolidate_keys::consolidate_keys;
use crate::input::flag_registry::{FlagAction, FlagSpec};
use crate::input::flexible_positional::FlexibleTarget;

use accumulate::alter_datum_data;
use classify::{
    datum_path, find_default_for_path, inferred_value_for_default, is_dot_scalar, split_first_eq,
};

pub use coerce::{infer_type, infer_value, unit_annotated_value};

/// Canonical flag set for the data concern.
///
/// `-c`/`--comment` is repeatable per datum's argparse shape (the TS code
/// joins multiple comments into a `comment: [...]` array); each occurrence
/// pushes onto `comment`. `-k` and `-K` likewise repeat — `-K` also tracks
/// the bare key name in `id_parts` so callers can render the slug template.
///
/// Positional `data` tokens (`key=value`, bare values) are NOT in this
/// registry — they're consumed by the parser's positional path because
/// they don't have a flag form.
pub static FLAG_SPECS: &[FlagSpec<DataArgs>] = &[
    FlagSpec {
        short: Some('k'),
        long: Some("key"),
        action: FlagAction::Value(|a, v| {
            a.keys.push(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('K'),
        long: Some("slug-key"),
        action: FlagAction::Value(|a, v| {
            a.push_id_key(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('c'),
        long: Some("comment"),
        action: FlagAction::Value(|a, v| {
            a.comment.push(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('R'),
        long: Some("remainder"),
        action: FlagAction::Value(|a, v| {
            a.remainder = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('S'),
        long: Some("string-remainder"),
        action: FlagAction::Switch(|a| {
            a.string_remainder = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('C'),
        long: Some("comment-remainder"),
        action: FlagAction::Switch(|a| {
            a.comment_remainder = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('l'),
        long: Some("lenient"),
        action: FlagAction::Switch(|a| {
            a.lenient = true;
            Ok(())
        }),
    },
];

/// The untyped data payload produced by [`handle_data_args`] — the direct
/// analogue of datum's `DatumData` (a plain object).
pub type DatumData = Map<String, Value>;

/// Input to [`handle_data_args`]. Field names mirror the datum TS shape.
#[derive(Debug, Clone, Default)]
pub struct DataArgs {
    /// Positional data values, in user-supplied order. May contain
    /// interleaved `key=value` pairs.
    pub data: Vec<String>,
    /// `-k` / `-K` specs. Non-consolidated input is fine — the handler
    /// calls [`consolidate_keys`] internally.
    pub keys: Vec<String>,
    /// `-c`/`--comment`. Joined into the final doc's `comment` key in
    /// the order supplied (string → `comment: "x"`, many → `comment: [...]`).
    pub comment: Vec<String>,
    /// `-K` additions — when the user supplies `-K key`, the key name is
    /// pushed here in addition to being appended to `keys`. Callers that
    /// want the `%key%` slug token can read this list directly.
    pub id_parts: Vec<String>,
    /// `-R` / `--remainder` override for the remainder bucket key.
    pub remainder: Option<String>,
    /// `-S` / `--string-remainder`: join remainder as one space-separated
    /// string instead of an array.
    pub string_remainder: bool,
    /// `-C` / `--comment-remainder`: shorthand for `-SR comment`.
    pub comment_remainder: bool,
    /// `-l` / `--lenient`: when no `-R` is supplied, default the
    /// remainder bucket to `extraData` instead of raising.
    pub lenient: bool,
    /// Daydumb compat flag — keeps the strings `"null"` / `"undefined"`
    /// as `String` values rather than coercing them to `Value::Null` /
    /// stripped (datum's TS behaviour). daydumb stores literal `"null"`
    /// switch states verbatim and lets state compilation normalise on
    /// read; eager null coercion at parse-time would lose that signal.
    /// `dur=null` still resolves to point shape downstream because
    /// `extract_add_temporal_keys` already accepts the `"null"` string
    /// alongside `Value::Null`.
    pub preserve_null_strings: bool,
    /// Local-time anchor used to resolve relative-offset values on
    /// `*_time` / `*_date` keys (`wake_time=+30`, `nextDate=+3`,
    /// `bedTime=-1h`). When `None`, relative offsets fall through as
    /// raw strings — callers without a clean occur-time anchor (reparse,
    /// update payload) leave this unset to preserve existing behavior.
    pub base_local: Option<NaiveDateTime>,
}

impl FlexibleTarget for DataArgs {
    fn data(&mut self) -> &mut Vec<String> {
        &mut self.data
    }
    fn keys(&mut self) -> &mut Vec<String> {
        &mut self.keys
    }
}

impl DataArgs {
    /// Record a `-K key` flag: append to `keys` and to `id_parts`. Mirrors
    /// the custom `IdKeyAction` in the TS argparse definition.
    pub fn push_id_key(&mut self, spec: String) {
        let key_name = spec.split_once('=').map(|(n, _)| n).unwrap_or(&spec);
        self.id_parts.push(format!("%{key_name}"));
        self.keys.push(spec);
    }
}

/// Per-invocation behavioural switches consumed by the recursive
/// helpers in this module. Built once by [`handle_data_args`] from the
/// caller's [`DataArgs`].
#[derive(Clone, Copy, Default)]
pub(crate) struct Compat {
    /// See [`DataArgs::preserve_null_strings`].
    pub(crate) preserve_null_strings: bool,
    /// See [`DataArgs::base_local`].
    pub(crate) base_local: Option<NaiveDateTime>,
}

/// Resolve `DataArgs` into the final `DatumData` object.
///
/// See the TS `handleDataArgs` for the reference algorithm; the Rust
/// version differs only in that the excluded non-ports (`baseData`,
/// command-change escape) never surface.
pub fn handle_data_args(args: &DataArgs) -> Result<DatumData> {
    let compat = Compat {
        preserve_null_strings: args.preserve_null_strings,
        base_local: args.base_local,
    };

    // Clone the bits we mutate; the input bag is treated as read-only.
    let mut data_queue: Vec<String> = args.data.clone();
    let mut keys: Vec<String> = consolidate_keys(&args.keys);

    let remainder_key: Option<String> = args.remainder.clone().or_else(|| {
        if args.comment_remainder {
            Some("comment".to_string())
        } else if args.lenient {
            Some("extraData".to_string())
        } else {
            None
        }
    });
    let remainder_as_string = args.string_remainder || args.comment_remainder;

    let mut datum_data: DatumData = Map::new();
    let mut remainder_data: Vec<String> = Vec::new();

    // ── positional loop ────────────────────────────────────────────────
    while !data_queue.is_empty() {
        let arg = data_queue.remove(0);
        let (before_eq, after_eq) = split_first_eq(&arg);

        if let Some(value) = after_eq {
            // Explicit key=value in the positional stream.
            let path = datum_path(before_eq);
            let existing = accumulate::get_path(&datum_data, &path).cloned();
            // Find the first declared key matching this path so an
            // explicit `key=.` can fall back to its default.
            let default_value = if let Some(v) = existing {
                inferred_value_for_default(&v)
            } else {
                find_default_for_path(&keys, &path)
            };
            alter_datum_data(
                &mut datum_data,
                &path,
                Some(value),
                default_value.as_deref(),
                false, // append
                false, // isRequired — explicit key=... is never required
                compat,
            )?;
            continue;
        }

        // Positional slot — try to consume the next unfilled key.
        let data_value = before_eq;
        let mut consumed = false;
        while !keys.is_empty() {
            let spec = keys.remove(0);
            let (raw_path, default_value) = split_first_eq(&spec);
            let is_required = default_value.is_none();
            let path = datum_path(raw_path);

            if accumulate::get_path(&datum_data, &path).is_some() {
                // Already filled manually — skip to the next declared key.
                continue;
            }

            alter_datum_data(
                &mut datum_data,
                &path,
                Some(data_value),
                default_value,
                false,
                is_required,
                compat,
            )?;
            consumed = true;
            break;
        }

        if !consumed {
            remainder_data.push(data_value.to_string());
        }
    }

    // ── unfilled keys: apply defaults; unfilled required → error ───────
    for spec in keys.drain(..) {
        let (raw_path, default_value) = split_first_eq(&spec);
        let path = datum_path(raw_path);
        let is_required = default_value.is_none();

        match accumulate::get_path(&datum_data, &path) {
            Some(existing) if !is_dot_scalar(existing) => continue,
            _ => {}
        }
        alter_datum_data(
            &mut datum_data,
            &path,
            default_value,
            None,
            false,
            is_required,
            compat,
        )?;
    }

    // ── comments ───────────────────────────────────────────────────────
    for comment in &args.comment {
        alter_datum_data(
            &mut datum_data,
            "comment",
            Some(comment),
            None,
            true,
            false,
            compat,
        )?;
    }

    // ── remainder ──────────────────────────────────────────────────────
    if !remainder_data.is_empty() {
        let Some(ref rkey) = remainder_key else {
            return Err(ParseError::ExtraData(remainder_data));
        };

        if remainder_as_string {
            let joined = remainder_data.join(" ");
            alter_datum_data(
                &mut datum_data,
                rkey,
                Some(&joined),
                None,
                true,
                false,
                compat,
            )?;
        } else {
            for rem in &remainder_data {
                alter_datum_data(&mut datum_data, rkey, Some(rem), None, true, false, compat)?;
            }
        }
    }

    Ok(datum_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args() -> DataArgs {
        DataArgs::default()
    }

    fn run(a: DataArgs) -> DatumData {
        handle_data_args(&a).expect("handle_data_args")
    }

    // ── direct ports of TS handleDataArgs tests ─────────────────────────

    #[test]
    fn empty_input_yields_empty_map() {
        assert_eq!(run(args()), Map::new());
    }

    #[test]
    fn explicit_key_value_pairs() {
        let mut a = args();
        a.data = vec!["abc=def".into()];
        assert_eq!(run(a), *json!({"abc": "def"}).as_object().unwrap());

        let mut a = args();
        a.data = vec!["first=arg".into(), "second=another".into()];
        assert_eq!(
            run(a),
            *json!({"first": "arg", "second": "another"})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.data = vec!["blank=".into()];
        assert_eq!(run(a), Map::new());
    }

    #[test]
    fn extra_equals_in_value_are_preserved() {
        let mut a = args();
        a.data = vec!["equation=1+2=3".into()];
        assert_eq!(run(a), *json!({"equation": "1+2=3"}).as_object().unwrap());

        let mut a = args();
        a.data = vec!["eqSep=====".into()];
        assert_eq!(run(a), *json!({"eqSep": "===="}).as_object().unwrap());
    }

    #[test]
    fn extra_data_without_leniency_errors() {
        let mut a = args();
        a.data = vec!["keyless".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));

        let mut a = args();
        a.keys = vec!["key1".into()];
        a.data = vec!["hasKey".into(), "noKey".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));
    }

    #[test]
    fn lenient_collects_into_extra_data() {
        let mut a = args();
        a.lenient = true;
        a.data = vec!["keyless".into()];
        assert_eq!(
            run(a),
            *json!({"extraData": "keyless"}).as_object().unwrap()
        );

        let mut a = args();
        a.lenient = true;
        a.data = vec![
            "extra=Data".into(),
            "can".into(),
            "be=interspersed".into(),
            "with".into(),
            "keyed=data".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "extra": "Data",
                "be": "interspersed",
                "keyed": "data",
                "extraData": ["can", "with"],
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn required_keys_consumed_in_order() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["value".into()];
        assert_eq!(run(a), *json!({"abc": "value"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["first".into(), "second".into()];
        assert_eq!(
            run(a),
            *json!({"a": "first", "b": "second"}).as_object().unwrap()
        );
    }

    #[test]
    fn missing_required_key_errors() {
        let mut a = args();
        a.keys = vec!["a".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));

        let mut a = args();
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["onlyOne".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));

        let mut a = args();
        a.lenient = true;
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["lenientDoesNotHelp".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn explicit_key_assignment_satisfies_required() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["abc=def".into()];
        assert_eq!(run(a), *json!({"abc": "def"}).as_object().unwrap());
    }

    #[test]
    fn manual_assignment_overwrites_positional() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["def".into(), "abc=ghi".into()];
        assert_eq!(run(a), *json!({"abc": "ghi"}).as_object().unwrap());
    }

    #[test]
    fn future_positional_skips_manually_specified_key() {
        let mut a = args();
        a.keys = vec!["abc".into(), "def".into(), "ghi".into()];
        a.data = vec!["def=123".into(), "xyz".into(), "foobar".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "xyz", "def": 123, "ghi": "foobar"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn optional_keys_with_and_without_defaults() {
        let mut a = args();
        a.keys = vec!["abc=".into()];
        a.data = vec!["cde".into()];
        assert_eq!(run(a), *json!({"abc": "cde"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["optional=".into()];
        assert_eq!(run(a), Map::new());

        let mut a = args();
        a.keys = vec!["withDefault=3".into()];
        assert_eq!(run(a), *json!({"withDefault": 3}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["withBlankDefault=".into()];
        assert_eq!(run(a), Map::new());

        let mut a = args();
        a.keys = vec!["withEmptyStringDefault=''".into()];
        assert_eq!(
            run(a),
            *json!({"withEmptyStringDefault": ""}).as_object().unwrap()
        );

        let mut a = args();
        a.keys = vec!["withDefault=3".into()];
        a.data = vec!["replacement".into()];
        assert_eq!(
            run(a),
            *json!({"withDefault": "replacement"}).as_object().unwrap()
        );
    }

    #[test]
    fn explicit_assignment_replaces_default() {
        let mut a = args();
        a.lenient = true;
        a.keys = vec!["abc=".into()];
        a.data = vec!["abc=cde".into(), "ghi".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "cde", "extraData": "ghi"})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.keys = vec!["abc=123".into()];
        a.data = vec!["replacesAbc".into(), "abc=replacesAgain".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "replacesAgain"}).as_object().unwrap()
        );
    }

    #[test]
    fn saves_comments_into_comment_key() {
        let mut a = args();
        a.comment = vec!["this is a comment.".into()];
        assert_eq!(
            run(a),
            *json!({"comment": "this is a comment."})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.comment = vec!["one".into(), "two".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["one", "two"]}).as_object().unwrap()
        );
    }

    #[test]
    fn concatenates_arg_comment_with_data_comment() {
        let mut a = args();
        a.comment = vec!["argComment".into()];
        a.data = vec!["comment=dataComment".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["dataComment", "argComment"]})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn remainder_collects_extra_data() {
        let mut a = args();
        a.remainder = Some("rem".into());
        a.data = vec!["oneArgHasNoArray".into()];
        assert_eq!(
            run(a),
            *json!({"rem": "oneArgHasNoArray"}).as_object().unwrap()
        );

        let mut a = args();
        a.remainder = Some("rem".into());
        a.data = vec!["abc".into(), "otherKey=def".into(), "hij".into()];
        assert_eq!(
            run(a),
            *json!({"rem": ["abc", "hij"], "otherKey": "def"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn string_remainder_joins_with_spaces() {
        let mut a = args();
        a.string_remainder = true;
        a.remainder = Some("rem".into());
        a.keys = vec!["firstKey=".into()];
        a.data = vec![
            "firstArg".into(),
            "Additional".into(),
            "arguments".into(),
            "get".into(),
            "made".into(),
            "into".into(),
            "a".into(),
            "single".into(),
            "string.".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "firstKey": "firstArg",
                "rem": "Additional arguments get made into a single string.",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn comment_remainder_joins_into_comment() {
        let mut a = args();
        a.comment_remainder = true;
        a.keys = vec!["optional1=".into()];
        a.data = vec![
            "the".into(),
            "non".into(),
            "filled".into(),
            "keys".into(),
            "will".into(),
            "key=value".into(),
            "be".into(),
            "a".into(),
            "comment.".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "optional1": "the",
                "key": "value",
                "comment": "non filled keys will be a comment.",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn comment_remainder_appends_to_existing_comments() {
        let mut a = args();
        a.comment_remainder = true;
        a.comment = vec!["explicit comment".into()];
        a.data = vec!["comment".into(), "remainder".into(), "string".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["explicit comment", "comment remainder string"]})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn interspersed_required_and_optional_keys() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=".into(),
            "req2".into(),
            "opt2=defaultValue".into(),
        ];
        a.data = vec![
            "value1".into(),
            "value2".into(),
            "value3".into(),
            "value4".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "req1": "value1",
                "opt1": "value2",
                "req2": "value3",
                "opt2": "value4",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn optional_later_provides_default_for_required() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec!["onlyOneValue".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "onlyOneValue",
                "withDefaultFromOptional": "noError",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn first_seen_position_preserved_even_when_later_spec_changes_form() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec!["one".into(), "two".into(), "three".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "one",
                "withDefaultFromOptional": "two",
                "opt1": "three",
            })
            .as_object()
            .unwrap()
        );

        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec!["one".into(), "two".into(), "three".into(), "four".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));
    }

    #[test]
    fn dot_default_on_optional_key() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=dotDefault".into(),
            "opt2=overwrittenDefault".into(),
            "opt3=notGivenDefault".into(),
        ];
        a.data = vec!["oneValue".into(), ".".into(), "threeValue".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "oneValue",
                "opt1": "dotDefault",
                "opt2": "threeValue",
                "opt3": "notGivenDefault",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn explicit_dot_assignment_uses_default() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=overwrittenDefault".into(),
            "opt2=dotDefault".into(),
            "opt3=notGivenDefault".into(),
        ];
        a.data = vec!["opt2=.".into(), "twoValue".into(), "threeValue".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "twoValue",
                "opt1": "threeValue",
                "opt2": "dotDefault",
                "opt3": "notGivenDefault",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn key_equals_deletes_previously_set_value() {
        let mut a = args();
        a.data = vec!["key=value".into(), "key2=value2".into(), "key=".into()];
        assert_eq!(run(a), *json!({"key2": "value2"}).as_object().unwrap());
    }

    #[test]
    fn dot_uses_existing_value_when_reassigned() {
        let mut a = args();
        a.data = vec!["key=value".into(), "key=.".into()];
        assert_eq!(run(a), *json!({"key": "value"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["req1".into()];
        a.data = vec!["someValue".into(), "req1=.".into()];
        assert_eq!(run(a), *json!({"req1": "someValue"}).as_object().unwrap());
    }

    #[test]
    fn pathlike_keys() {
        let mut a = args();
        a.keys = vec![
            "a.b".into(),
            "a.c".into(),
            "a.d.e=".into(),
            "another.key=5".into(),
        ];
        a.data = vec![
            "data".into(),
            "moreData".into(),
            "7".into(),
            "key.nested=abc".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "a": {
                    "b": "data",
                    "c": "moreData",
                    "d": {"e": 7},
                },
                "key": {"nested": "abc"},
                "another": {"key": 5},
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn top_level_value_gets_overwritten_when_path_nests() {
        let mut a = args();
        a.data = vec!["topKey=value".into(), "topKey.nestedKey=abc".into()];
        assert_eq!(
            run(a),
            *json!({"topKey": {"nestedKey": "abc"}}).as_object().unwrap()
        );
    }

    #[test]
    fn state_shorthand_routes_to_state_id() {
        let mut a = args();
        a.data = vec!["state=active".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "active"}}).as_object().unwrap()
        );

        let mut a = args();
        a.keys = vec!["state".into()];
        a.data = vec!["inactive".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "inactive"}}).as_object().unwrap()
        );
    }

    #[test]
    fn leading_dot_routes_to_state_subpath() {
        let mut a = args();
        a.data = vec![".key=active".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"key": "active"}}).as_object().unwrap()
        );
    }

    #[test]
    fn state_and_dot_compose_into_complex_state() {
        let mut a = args();
        a.keys = vec!["state=".into()];
        a.data = vec!["active".into(), ".type=common".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "active", "type": "common"}})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn dot_cannot_satisfy_required_key() {
        let mut a = args();
        a.keys = vec!["req1".into(), "req2".into()];
        a.data = vec!["value1".into(), ".".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn optional_can_be_upgraded_to_required() {
        let mut a = args();
        a.keys = vec![
            "opt1=".into(),
            "changedToReq=default".into(),
            "changedToReq".into(),
        ];
        a.data = vec!["value1".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn duration_key_infers_iso_duration() {
        let mut a = args();
        a.data = vec!["taskDuration=3h".into()];
        assert_eq!(
            run(a),
            *json!({"taskDuration": "PT3H"}).as_object().unwrap()
        );
    }

    #[test]
    fn duration_default_value_uses_key_inference() {
        let mut a = args();
        a.keys = vec!["duration=5".into()];
        assert_eq!(run(a), *json!({"duration": "PT5M"}).as_object().unwrap());
    }

    #[test]
    fn input_args_are_not_mutated() {
        let mut a = args();
        a.keys = vec!["req1".into(), "opt1=".into()];
        a.remainder = Some("rem".into());
        a.comment = vec!["c".into()];
        a.data = vec!["v1".into(), "v2".into(), "v3".into()];
        let before = a.clone();
        let _ = handle_data_args(&a).unwrap();
        assert_eq!(before.keys, a.keys);
        assert_eq!(before.data, a.data);
        assert_eq!(before.comment, a.comment);
        assert_eq!(before.remainder, a.remainder);
    }

    // ── -K flag (id key) ────────────────────────────────────────────────

    #[test]
    fn push_id_key_mirrors_ts_id_key_action() {
        let mut a = DataArgs::default();
        a.push_id_key("field1".to_string());
        a.push_id_key("field2=default".to_string());
        assert_eq!(a.keys, vec!["field1", "field2=default"]);
        assert_eq!(a.id_parts, vec!["%field1", "%field2"]);
    }

    #[test]
    fn dotted_keys_fold_into_state_subtree() {
        // da-ju71.4: leading-dot keys route into `state.*` and the bare
        // `state` key into `state.id`. This is the parser's default — the
        // old `flat_paths=true` daydumb-compat branch is gone (pre-v1
        // hard cut).
        let mut a = args();
        a.preserve_null_strings = true;
        a.data = vec![".subtask=mop".into(), "wake_time=8am".into()];
        let out = run(a);
        assert_eq!(out["state"]["subtask"], json!("mop"));
        assert!(out.get(".subtask").is_none());
        assert_eq!(out["wake_time"], json!("08:00:00"));
    }

    // ── suffix coercion with relative offsets (da-l9ie) ──────────────────

    fn base_at(year: i32, month: u32, day: u32, hour: u32, min: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, min, 0)
            .unwrap()
    }

    #[test]
    fn relative_date_offset_resolves_against_base_local() {
        let mut a = args();
        a.base_local = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec![
            "nextDate=+3".into(),
            "prev_date=-7".into(),
            "today_date=0".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "nextDate": "2026-05-23",
                "prev_date": "2026-05-13",
                "today_date": "2026-05-20",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn relative_date_offset_without_base_local_passes_through() {
        // No base anchor → relative day offsets cannot resolve, fall through
        // to raw-string passthrough rather than silently picking "now".
        let mut a = args();
        a.data = vec!["nextDate=+3".into()];
        assert_eq!(run(a), *json!({"nextDate": "+3"}).as_object().unwrap());
    }

    #[test]
    fn absolute_date_does_not_need_base_local() {
        let mut a = args();
        a.data = vec!["nextDate=2026-06-01".into()];
        assert_eq!(
            run(a),
            *json!({"nextDate": "2026-06-01"}).as_object().unwrap()
        );
    }

    #[test]
    fn relative_time_minute_offset_resolves_against_base_local() {
        let mut a = args();
        a.base_local = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["wake_time=+30".into()];
        assert_eq!(
            run(a),
            *json!({"wake_time": "10:30:00"}).as_object().unwrap()
        );

        let mut a = args();
        a.base_local = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["snooze_time=-15".into()];
        assert_eq!(
            run(a),
            *json!({"snooze_time": "09:45:00"}).as_object().unwrap()
        );
    }

    #[test]
    fn relative_time_duration_offset_resolves_against_base_local() {
        let mut a = args();
        a.base_local = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["wake_time=-1h".into(), "bedTime=+90m".into()];
        assert_eq!(
            run(a),
            *json!({
                "wake_time": "09:00:00",
                "bedTime": "11:30:00",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn duration_signed_token_passthrough_independent_of_base() {
        // Durations don't anchor — `+30m` / `-15m` on a `*_dur` key parses
        // as a signed ISO duration with no need for `base_local`.
        let mut a = args();
        a.data = vec!["nap_dur=+30m".into(), "pause_duration=-15m".into()];
        assert_eq!(
            run(a),
            *json!({
                "nap_dur": "PT30M",
                "pause_duration": "-PT15M",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn iso_instant_on_time_key_passes_through() {
        // Full-instant timestamps on `*_time` keys (e.g. chores-kit
        // `next_time=2026-05-03T00:00:00Z` due-dates) must not be
        // truncated to time-of-day — they carry calendar information
        // that callers depend on.
        let mut a = args();
        a.data = vec!["next_time=2026-05-03T00:00:00Z".into()];
        assert_eq!(
            run(a),
            *json!({"next_time": "2026-05-03T00:00:00Z"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn preserve_null_strings_keeps_literal_null() {
        // Daydumb compat: `null` / `undefined` stay as strings on the
        // primary-value path so the switch verb can store the literal.
        let mut a = args();
        a.preserve_null_strings = true;
        a.remainder = Some("__primary_value".into());
        a.data = vec!["null".into()];
        let out = run(a);
        assert_eq!(out["__primary_value"], json!("null"));
    }
}
