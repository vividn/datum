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

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::{ParseError, Result};
use crate::input::consolidate_keys::consolidate_keys;
use crate::input::flag_registry::{FlagAction, FlagSpec};
use crate::input::flexible_positional::FlexibleTarget;
use crate::input::TimeArgs;
use crate::time::DatumTime;

use accumulate::alter_datum_data;
use classify::{
    datum_path, find_default_for_path, inferred_value_for_default, is_dot_scalar, split_first_eq,
};

pub use coerce::{bracket_state_error, infer_type, KeyType};

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
        short: Some('I'),
        long: Some("string-state"),
        action: FlagAction::Switch(|a| {
            a.string_state = true;
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
    /// the order supplied — always an array, one element per comment.
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
    /// `-I` / `--string-state`: join the remainder into the entry's
    /// `state` bucket as one space-separated string (the state-targeting
    /// analogue of `-S`, which targets a data key). Lets a switch take a
    /// multi-word state — e.g. `switch tv -I adventure time` → state
    /// "adventure time". (da-vo0y Feature 1.)
    pub string_state: bool,
    /// da-vo0y Feature 2: before the remainder joins, peek its LAST
    /// positional — a duration-parseable token routes to `dur` instead of
    /// joining (`movie terminator 90` → title "terminator", dur 90m); a
    /// literal `.` is the explicit skip-dur sentinel (consumed, no
    /// re-peek); `\.` escapes to a literal `.` that joins. Set by the
    /// command layer when a string-join remainder (`-I`/`-S`/`-C`) is
    /// active on a duration-bearing verb — never by a flag directly.
    pub peek_trailing_dur: bool,
    /// The entry's own time, which `*_time` / `*_date` values resolve
    /// against as `-t` would (`wake_time=+30`, `nextDate=+3`,
    /// `bedTime=8am`), in its zone. When `None`, a relative `*_date`
    /// falls through as the raw string and a `*_time` reads the wall clock.
    pub base: Option<DatumTime>,
    /// The time flags `base` was built from, on a log (da-johpu): an
    /// `occur_time=` is one more `-t` beside them, and a relative one
    /// beside a named day needs a clock as theirs does. `None` where the
    /// base is a clock of its own, an entry's time (`update`).
    pub time_flags: Option<TimeArgs>,
    /// The types a field's spec declares for its keys (da-b2f8o), by
    /// key name. A typed key parses as its type or refuses; one with no
    /// entry here is typed by its suffix, or guessed by [`infer_type`].
    pub types: BTreeMap<String, KeyType>,
    /// Every value stays the string it was typed as: no spec type, no
    /// suffix type, no guess. `rekey`'s values are new key NAMES, so
    /// `due_date=deadline` must not parse `deadline` as a date (da-xvtbd).
    /// The sentinels still apply: an empty or `undefined` value drops its
    /// pair, and `null` stays a JSON null, which `rekey` refuses. A quoted
    /// value loses its quotes, as anywhere: `a='"b c"'` names `b c`.
    pub literal: bool,
    /// v2's trailing verbs (`changeDatumCommand`): a command word in the
    /// `dur=` slot, or leading the words no slot or remainder takes,
    /// changes the line's command. Set by the log commands alone; see
    /// [`handle_log_data_args`].
    pub command_change: bool,
    /// The words after a bare `--` (da-gqy3b), which follow [`Self::data`]:
    /// positionals as typed, so no `key=value`, command or trailing
    /// duration is read from them. They fill slots, and under `-C` are the
    /// comment's words.
    pub verbatim: Vec<String>,
    /// Which reading of a wall time a fall-back night repeats each `*_time`
    /// key means, by key, `true` the later (da-job98). With a map, a key
    /// naming one it does not answer is refused,
    /// [`ParseError::RepeatedTime`](crate::ParseError::RepeatedTime), for
    /// the command to ask; with none the first is taken, as a stored line
    /// read again (`reparse`) took it.
    pub later: Option<BTreeMap<String, bool>>,
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
pub(crate) struct Compat<'a> {
    /// See [`DataArgs::base`].
    pub(crate) base: Option<&'a DatumTime>,
    /// See [`DataArgs::time_flags`].
    pub(crate) time_flags: Option<&'a TimeArgs>,
    /// See [`DataArgs::types`].
    pub(crate) types: Option<&'a BTreeMap<String, KeyType>>,
    /// See [`DataArgs::literal`].
    pub(crate) literal: bool,
    /// See [`DataArgs::later`].
    pub(crate) later: Option<&'a BTreeMap<String, bool>>,
}

/// Resolve `DataArgs` into the final `DatumData` object.
///
/// See the TS `handleDataArgs` for the reference algorithm; the Rust
/// version differs only in that the excluded non-port (`baseData`)
/// never surfaces.
pub fn handle_data_args(args: &DataArgs) -> Result<DatumData> {
    handle_log_data_args(args).map(|(data, _)| data)
}

/// The commands a trailing word changes a log line to, as v2's
/// `commandChanges` (`stop` being `end`'s other name).
pub const COMMAND_CHANGES: &[&str] = &["occur", "start", "end", "stop", "switch"];

/// [`handle_data_args`], and the command a trailing verb changed the line
/// to (`start sleep end` → `end`, `occur coffee start` → `start`): with
/// [`DataArgs::command_change`] set, a command word filling the `dur=` slot,
/// or leading the leftover words when no remainder key takes them, is the
/// line's command from there, and the words after it are read again as its
/// own (v2's `changeDatumCommand`). The data already read stays; a `switch`
/// clears the state for the words after it to fill.
pub fn handle_log_data_args(args: &DataArgs) -> Result<(DatumData, Option<String>)> {
    let compat = Compat {
        base: args.base.as_ref(),
        time_flags: args.time_flags.as_ref(),
        types: Some(&args.types),
        literal: args.literal,
        later: args.later.as_ref(),
    };

    // Clone the bits we mutate; the input bag is treated as read-only.
    let mut data_queue: Vec<String> = [&args.data[..], &args.verbatim[..]].concat();
    // How many words at the queue's end came after `--`, and of those the
    // remainder took (a tail of it: once a word misses every slot, the
    // rest do too).
    let mut verbatim = args.verbatim.len();
    let mut verbatim_left = 0;
    let mut keys: Vec<String> = consolidate_keys(&args.keys);

    let remainder_key: Option<String> = args.remainder.clone().or_else(|| {
        if args.comment_remainder {
            Some("comment".to_string())
        } else if args.string_state {
            // `-I` / `--string-state`: the remainder joins into the entry's
            // state. `state` normalizes to `state.id` downstream (the
            // canonical state slot), so a switch can take a multi-word state.
            Some("state".to_string())
        } else if args.string_remainder {
            // `-S` without `-R`/`-C` means "string the rest"; "the rest"
            // defaults to the extra-data bucket. Explicit `-R` overrides.
            Some("extra_data".to_string())
        } else {
            None
        }
    });
    // Review (da-vo0y): `-I` forces string-joining only when it actually
    // drove the routing (the remainder bucket IS the state). An explicit
    // `-R <key>` keeps its own shape — array unless `-S` says otherwise.
    let remainder_as_string = args.string_remainder
        || args.comment_remainder
        || (args.string_state && remainder_key.as_deref() == Some("state"));

    let mut datum_data: DatumData = Map::new();
    let mut remainder_data: Vec<String> = Vec::new();
    // The bare positional the state slot took, beside which a comma touching
    // one side only (`vertical, sit`) is refused (da-oe3so). Any other slot's
    // value may end in a comma: there `a,b` would be a list, not a state.
    let mut prev_slot: Option<String> = None;
    let mut command: Option<String> = None;
    let is_command = |w: &str| args.command_change && COMMAND_CHANGES.contains(&w);

    loop {
        // ── positional loop ────────────────────────────────────────────────
        while !data_queue.is_empty() {
            let as_typed = data_queue.len() <= verbatim;
            let arg = data_queue.remove(0);
            let (before_eq, after_eq) = match as_typed {
                true => (arg.as_str(), None),
                false => split_first_eq(&arg),
            };
            let prev = prev_slot.take();

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
            if let Some(prev) = prev
                .filter(|p| !keys.is_empty() && (p.ends_with(',') || data_value.starts_with(',')))
            {
                let (a, b) = (
                    prev.trim_end_matches(','),
                    data_value.trim_start_matches(','),
                );
                return Err(ParseError::InvalidArg(format!(
                    "`{prev} {data_value}`: a comma touches one side only. `{a},{b}` is one \
                 parallel state; a comma standing alone between two commands \
                 (`<command> , <command>`) logs both at one instant"
                )));
            }
            while !keys.is_empty() {
                let spec = keys.remove(0);
                let (raw_path, default_value) = split_first_eq(&spec);
                let is_required = default_value.is_none();
                let path = datum_path(raw_path);

                if accumulate::get_path(&datum_data, &path).is_some() {
                    // Already filled manually — skip to the next declared key.
                    continue;
                }
                if path == "dur" && !as_typed && is_command(data_value) {
                    change_command(&mut datum_data, &mut keys, data_value);
                    command = Some(data_value.to_string());
                    consumed = true;
                    break;
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
                prev_slot = (path == "state.id").then(|| data_value.to_string());
                break;
            }

            if !consumed {
                remainder_data.push(data_value.to_string());
                verbatim_left += usize::from(as_typed);
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

        // ── leftover words led by a command: the line is that command ──────
        if remainder_key.is_none()
            && remainder_data.len() > verbatim_left
            && is_command(&remainder_data[0])
        {
            let word = remainder_data.remove(0);
            change_command(&mut datum_data, &mut keys, &word);
            command = Some(word);
            data_queue = std::mem::take(&mut remainder_data);
            verbatim = std::mem::take(&mut verbatim_left);
            continue;
        }
        break;
    }

    // ── comments ───────────────────────────────────────────────────────
    for comment in &args.comment {
        push_comment(&mut datum_data, comment);
    }

    // ── trailing-dur peek (da-vo0y F2) ─────────────────────────────────
    // With a string-join remainder active on a duration-bearing verb, the
    // LAST positional of the join window is peeked before joining:
    //   * duration-parseable → routes to `dur` (the trailing-dur idiom);
    //     an explicit `dur=<v>` anywhere already won — the peek defers.
    //   * `.`  → explicit skip-dur sentinel; consumed, and the peek does
    //     NOT re-fire on the new last token (one `.`, one skip).
    //   * `\.` → escapes to a literal `.` that joins normally.
    //   * anything else → joins normally.
    if args.peek_trailing_dur && verbatim_left == 0 && !remainder_data.is_empty() {
        let last = remainder_data.last().expect("checked non-empty");
        if last == "." {
            remainder_data.pop();
        } else if last == r"\." {
            // Escaped literal dot. With other tokens present, unescape now
            // so it joins as `.` mid-string. As the SOLE token, leave it
            // escaped — unescaping here would make the joined value exactly
            // `.`, which `alter_datum_data`'s dot-default handling drops;
            // the value path unescapes a lone `\.` itself (#44 review).
            if remainder_data.len() > 1 {
                let i = remainder_data.len() - 1;
                remainder_data[i] = ".".to_string();
            }
        } else if datum_data.get("dur").is_none() && crate::time::parse_duration(last).is_ok() {
            let dur_tok = remainder_data.pop().expect("checked non-empty");
            alter_datum_data(
                &mut datum_data,
                "dur",
                Some(&dur_tok),
                None,
                false,
                false,
                compat,
            )?;
        }
    }

    // ── remainder ──────────────────────────────────────────────────────
    if !remainder_data.is_empty() {
        let Some(ref rkey) = remainder_key else {
            return Err(ParseError::ExtraData(remainder_data));
        };

        if rkey == "comment" && remainder_as_string {
            push_comment(&mut datum_data, &remainder_data.join(" "));
        } else if remainder_as_string {
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

    // da-ult68: `comment` is an array whatever wrote it, `comment=x` included.
    if let Some(c) = datum_data.get_mut("comment") {
        if !c.is_array() {
            *c = Value::Array(vec![c.take()]);
        }
    }

    Ok((datum_data, command))
}

/// What v2's `changeDatumCommand` does to the data and the slots: an occur
/// has no duration, and every other command takes one next; a switch
/// clears the state and takes a new one first. The rest of a command's
/// meaning (a start's or an end's state) is its own, from its verb.
fn change_command(data: &mut DatumData, keys: &mut Vec<String>, command: &str) {
    if command == "occur" {
        data.insert("dur".into(), Value::Null);
        return;
    }
    if data.get("dur").is_some_and(Value::is_null) {
        data.remove("dur");
    }
    keys.insert(0, "dur=".into());
    if command == "switch" {
        data.remove("state");
        keys.insert(0, "state=true".into());
    }
}

/// Append one comment, as typed, to `data.comment` (da-ult68): every
/// `-c` / `-C` adds an element and none replaces one.
fn push_comment(data: &mut DatumData, comment: &str) {
    let slot = data
        .entry("comment")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !slot.is_array() {
        *slot = Value::Array(vec![slot.take()]);
    }
    if let Value::Array(a) = slot {
        a.push(Value::String(comment.to_string()));
    }
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
    fn extra_data_remainder_collects() {
        let mut a = args();
        a.remainder = Some("extra_data".into());
        a.data = vec!["keyless".into()];
        assert_eq!(
            run(a),
            *json!({"extra_data": "keyless"}).as_object().unwrap()
        );

        let mut a = args();
        a.remainder = Some("extra_data".into());
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
                "extra_data": ["can", "with"],
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
        a.remainder = Some("extra_data".into());
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["remainderDoesNotHelp".into()];
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
    fn one_sided_comma_between_slots_is_refused() {
        for data in [["vertical", ",sit"], ["vertical,", "sit"]] {
            let mut a = args();
            a.keys = vec!["state".into(), "dur=".into()];
            a.data = data.iter().map(|s| s.to_string()).collect();
            let Err(ParseError::InvalidArg(msg)) = handle_data_args(&a) else {
                panic!("{data:?} must be refused");
            };
            assert!(
                msg.contains("`vertical,sit` is one parallel state"),
                "{msg}"
            );
        }
        // A list touching nothing but its own slot is a list (`,sit`).
        let mut a = args();
        a.keys = vec!["field".into(), "state".into()];
        a.data = vec!["body".into(), ",sit".into()];
        assert!(handle_data_args(&a).is_ok());
        // A data key's value may end in a comma: no state to misread.
        let mut a = args();
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["buy milk,".into(), "urgent".into()];
        assert_eq!(
            run(a),
            *json!({"a": "buy milk,", "b": "urgent"})
                .as_object()
                .unwrap()
        );
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
        a.remainder = Some("extra_data".into());
        a.keys = vec!["abc=".into()];
        a.data = vec!["abc=cde".into(), "ghi".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "cde", "extra_data": "ghi"})
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
            *json!({"comment": ["this is a comment."]})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.comment = vec!["one".into(), "two".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["one", "two"]}).as_object().unwrap()
        );

        // A comment is kept as typed — `-c 42` is not the number 42.
        let mut a = args();
        a.comment = vec!["42".into()];
        assert_eq!(run(a), *json!({"comment": ["42"]}).as_object().unwrap());
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
    fn string_remainder_alone_infers_extra_data() {
        // da-tcr8: `-S` without `-R`/`-C` should "string the rest" into
        // the extra_data bucket (joined), not error.
        let mut a = args();
        a.string_remainder = true;
        a.data = vec!["extra".into(), "extra".into(), "extra".into()];
        assert_eq!(
            run(a),
            *json!({"extra_data": "extra extra extra"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn string_remainder_with_explicit_key_overrides_inferred_bucket() {
        // `-SR custom_key` still routes to the explicit key (joined string).
        let mut a = args();
        a.string_remainder = true;
        a.remainder = Some("custom_key".into());
        a.data = vec!["extra".into(), "extra".into()];
        assert_eq!(
            run(a),
            *json!({"custom_key": "extra extra"}).as_object().unwrap()
        );
    }

    #[test]
    fn remainder_alone_collects_array_not_joined() {
        // `-R` without `-S` keeps single tokens as an array.
        let mut a = args();
        a.remainder = Some("extra_data".into());
        a.data = vec!["extra".into(), "extra".into()];
        assert_eq!(
            run(a),
            *json!({"extra_data": ["extra", "extra"]})
                .as_object()
                .unwrap()
        );
    }

    // ── -I / --string-state (da-vo0y Feature 1) ─────────────────────────

    #[test]
    fn string_state_joins_remainder_into_state() {
        // `-I` joins the remainder into the entry's state (canonical slot
        // `state.id`), space-joined like `-S`. Lets a switch take a
        // multi-word state: `switch tv -I adventure time` → "adventure time".
        let mut a = args();
        a.string_state = true;
        a.data = vec!["adventure".into(), "time".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "adventure time"}})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn string_state_single_token() {
        let mut a = args();
        a.string_state = true;
        a.data = vec!["blueberry".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "blueberry"}}).as_object().unwrap()
        );
    }

    #[test]
    fn string_state_fills_declared_key_then_joins_rest_to_state() {
        // A declared data key consumes its positional first; the remaining
        // positionals join into state.
        let mut a = args();
        a.string_state = true;
        a.keys = vec!["firstKey=".into()];
        a.data = vec!["fk".into(), "rest".into(), "of".into(), "state".into()];
        assert_eq!(
            run(a),
            *json!({"firstKey": "fk", "state": {"id": "rest of state"}})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn string_state_no_positionals_yields_empty() {
        // `-I` with nothing to claim leaves state unset (the command layer
        // supplies the action-default state downstream).
        let mut a = args();
        a.string_state = true;
        assert_eq!(run(a), Map::new());
    }

    #[test]
    fn string_state_with_explicit_remainder_keeps_array_shape() {
        // Review fix (da-vo0y): `-I` forces joining only when it actually
        // drove the routing to `state`. With an explicit `-R custom`, the
        // remainder keeps -R's own array shape. (The command layer rejects
        // this combination for switch; the library keeps clean semantics.)
        let mut a = args();
        a.string_state = true;
        a.remainder = Some("rem".into());
        a.data = vec!["x".into(), "y".into()];
        assert_eq!(run(a), *json!({"rem": ["x", "y"]}).as_object().unwrap());
    }

    // ── trailing-dur peek (da-vo0y Feature 2) ───────────────────────────

    #[test]
    fn peek_takes_trailing_duration_into_dur() {
        // `switch tv -I adventure time 30m` → state + dur.
        let mut a = args();
        a.string_state = true;
        a.peek_trailing_dur = true;
        a.data = vec!["adventure".into(), "time".into(), "30m".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "adventure time"}, "dur": "PT30M"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn peek_dot_skips_dur_and_joins_rest() {
        // `movie terminator 2 .` → title "terminator 2", no dur — the `.`
        // opts out and the would-be-duration "2" joins instead.
        let mut a = args();
        a.string_remainder = true;
        a.remainder = Some("title".into());
        a.peek_trailing_dur = true;
        a.data = vec!["terminator".into(), "2".into(), ".".into()];
        assert_eq!(
            run(a),
            *json!({"title": "terminator 2"}).as_object().unwrap()
        );
    }

    #[test]
    fn peek_bare_int_parses_as_minutes() {
        // `movie terminator 90` → title "terminator", dur 90 minutes.
        let mut a = args();
        a.string_remainder = true;
        a.remainder = Some("title".into());
        a.peek_trailing_dur = true;
        a.data = vec!["terminator".into(), "90".into()];
        assert_eq!(
            run(a),
            *json!({"title": "terminator", "dur": "PT1H30M"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn no_peek_without_flag_duration_joins() {
        // Without the command layer arming the peek (non-dur-bearing verb),
        // a trailing duration-looking token joins like any other word.
        let mut a = args();
        a.string_state = true;
        a.data = vec!["adventure".into(), "time".into(), "30m".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "adventure time 30m"}})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn peek_defers_to_explicit_dur_key() {
        // `dur=5` anywhere wins; the trailing 10m joins instead of peeking.
        let mut a = args();
        a.string_state = true;
        a.peek_trailing_dur = true;
        a.data = vec!["dur=5".into(), "a".into(), "10m".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "a 10m"}, "dur": "PT5M"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn peek_escaped_dot_joins_as_literal() {
        let mut a = args();
        a.string_state = true;
        a.peek_trailing_dur = true;
        a.data = vec!["loading".into(), r"\.".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "loading ."}}).as_object().unwrap()
        );
    }

    #[test]
    fn peek_sole_escaped_dot_survives_as_literal_dot() {
        // #44 review: a sole `\.` must NOT be unescaped before the join —
        // the joined value would be exactly `.`, which the dot-default
        // handling drops. Left escaped, the value path unescapes it.
        let mut a = args();
        a.string_state = true;
        a.peek_trailing_dur = true;
        a.data = vec![r"\.".into()];
        assert_eq!(run(a), *json!({"state": {"id": "."}}).as_object().unwrap());
    }

    #[test]
    fn peek_only_token_is_duration() {
        // `snack 30m` → the join has nothing left to claim; 30m is the dur
        // (state falls to the action default downstream).
        let mut a = args();
        a.string_state = true;
        a.peek_trailing_dur = true;
        a.data = vec!["30m".into()];
        assert_eq!(run(a), *json!({"dur": "PT30M"}).as_object().unwrap());
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
                "comment": ["non filled keys will be a comment."],
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
        a.data = vec![".subtask=mop".into(), "wake_time=8am".into()];
        let out = run(a);
        assert_eq!(out["state"]["subtask"], json!("mop"));
        assert!(out.get(".subtask").is_none());
        let wake = DatumTime::from_v2_json(&out["wake_time"]).expect("an instant");
        assert_eq!(wake.local_time().time().to_string(), "08:00:00");
    }

    // ── suffix coercion with relative offsets (da-l9ie) ──────────────────

    /// An entry time in UTC, so its wall clock is the instant.
    fn base_at(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DatumTime {
        let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, min, 0)
            .unwrap();
        DatumTime::from_utc_and_tz(naive.and_utc(), "UTC").unwrap()
    }

    /// A `*_time` value as v2 stores it, for an instant in UTC.
    fn instant(utc: &str) -> Value {
        json!({"utc": format!("{utc}.000Z"), "o": 0, "tz": "UTC"})
    }

    #[test]
    fn relative_date_offset_resolves_against_base_local() {
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
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

    /// da-6d09r: unsigned, `due_date=2026` was 2026 days after the entry.
    #[test]
    fn unsigned_integer_on_a_date_key_is_refused_with_the_way_to_write_it() {
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        for (typed, said) in [
            (
                "due_date=5",
                "due_date: \"5\" is not a date: use '+5' or '-5' for relative days, \
                 or a date like 2026-10-05",
            ),
            (
                "due_date=2026",
                "due_date: \"2026\" is not a date: use a date like 2026-10-05",
            ),
        ] {
            a.data = vec![typed.into()];
            assert_eq!(handle_data_args(&a).unwrap_err().to_string(), said);
        }
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
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["wake_time=+30".into()];
        assert_eq!(run(a)["wake_time"], instant("2026-05-20T10:30:00"));

        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["snooze_time=-15".into()];
        assert_eq!(run(a)["snooze_time"], instant("2026-05-20T09:45:00"));
    }

    #[test]
    fn relative_time_duration_offset_resolves_against_base_local() {
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["wake_time=-1h".into(), "bedTime=+90m".into()];
        assert_eq!(
            run(a),
            *json!({
                "wake_time": instant("2026-05-20T09:00:00"),
                "bedTime": instant("2026-05-20T11:30:00"),
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
    fn iso_instant_on_time_key_keeps_its_instant() {
        // An ISO instant is that instant, in the entry's zone, as `-t`
        // keeps it (chores-kit `next_time=2026-05-03T00:00:00Z` due-dates).
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["next_time=2026-05-03T00:00:00Z".into()];
        assert_eq!(run(a)["next_time"], instant("2026-05-03T00:00:00"));
    }

    /// da-x15of: a relative `*_time` that crosses midnight keeps its day —
    /// a clock time stored `12:00:00` and lost it at log time.
    #[test]
    fn relative_time_across_midnight_keeps_its_day() {
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 20, 0));
        a.data = vec!["next_time=+16h".into()];
        assert_eq!(run(a)["next_time"], instant("2026-05-21T12:00:00"));
    }

    /// da-x15of: the shape v2's `toDatumTime` writes — `o` in hours, `tz`
    /// the zone's name — as v2 at the pin stored these values for an
    /// entry at 2024-01-15 09:00 in Europe/Berlin (a clock time against
    /// the entry's day here, where v2 read the wall clock). An unparsed
    /// value is refused, as v2 refused it.
    #[test]
    fn a_time_value_is_v2s_instant_in_the_entrys_zone() {
        let berlin = |utc: &str| json!({"utc": utc, "o": 1, "tz": "Europe/Berlin"});
        let mut a = args();
        a.base = Some(
            DatumTime::from_utc_and_tz("2024-01-15T08:00:00Z".parse().unwrap(), "Europe/Berlin")
                .unwrap(),
        );
        a.data = vec![
            "wake_time=8am".into(),
            "c_time=2024-01-15T07:15".into(),
            "next_time=+3h".into(),
        ];
        let out = run(a.clone());
        assert_eq!(out["wake_time"], berlin("2024-01-15T07:00:00.000Z"));
        assert_eq!(out["c_time"], berlin("2024-01-15T06:15:00.000Z"));
        assert_eq!(out["next_time"], berlin("2024-01-15T11:00:00.000Z"));

        a.data = vec!["e_time=garbage".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::WrongType { ty: "time", .. })
        ));
    }

    // ── day words / weekdays on temporal keys (da-0gs4) ─────────────────

    #[test]
    fn day_words_on_temporal_keys_resolve_against_base_local() {
        // 2026-05-20 is a Wednesday. A `*_date` key yields the date; a
        // `*_time` key yields that day's midnight, matching the full-
        // datetime shape plugin due-dates already use.
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec![
            "question_date=yesterday".into(),
            "next_time=tomorrow".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "question_date": "2026-05-19",
                "next_time": instant("2026-05-21T00:00:00"),
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn weekday_on_temporal_key_takes_the_nearest_side() {
        // Data args face `Nearest`, so from Wednesday 2026-05-20 the
        // unmarked `friday` is the coming one (2 days on) rather than
        // the past one (5 days back). `-` forces the past.
        let mut a = args();
        a.base = Some(base_at(2026, 5, 20, 10, 0));
        a.data = vec!["finish_time=friday".into(), "start_date=friday-".into()];
        assert_eq!(
            run(a),
            *json!({
                "finish_time": instant("2026-05-22T00:00:00"),
                "start_date": "2026-05-15",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn weekday_without_base_local_falls_back_to_the_wall_clock() {
        // With no entry anchor the wall clock stands in — the same
        // fallback the year-less compact form (`apr29`) already used.
        use chrono::{Datelike, Weekday};
        let mut a = args();
        a.data = vec!["finish_time=friday".into()];
        let out = run(a);
        let day = DatumTime::from_v2_json(&out["finish_time"])
            .expect("resolved to an instant")
            .local_time()
            .date_naive();
        assert_eq!(day.weekday(), Weekday::Fri);
        let today = crate::time::DatumTime::now().local_time().date_naive();
        assert!(
            (day - today).num_days().abs() <= 3,
            "nearest Friday is within half a week (today={today} got={day})"
        );
    }

    #[test]
    fn null_is_json_null_and_quoted_null_is_the_string() {
        // da-revva: `null` is a JSON null (v2 parity), `undefined` drops
        // the key, and the string is reachable through the JSON quotes.
        let mut a = args();
        a.data = vec!["k=null".into(), "u=undefined".into(), "s=\"null\"".into()];
        let out = run(a);
        assert_eq!(out["k"], Value::Null);
        assert!(out.get("u").is_none());
        assert_eq!(out["s"], json!("null"));
    }

    #[test]
    fn empty_key_token_is_raw_data() {
        // da-c312: no key before the first `=` means no split — the whole
        // token, `=` included, fills the next positional key as a string.
        for tok in ["=2", "==2", "="] {
            let mut a = args();
            a.keys = vec!["abc".into()];
            a.data = vec![tok.into()];
            assert_eq!(run(a), *json!({"abc": tok}).as_object().unwrap());
        }

        let mut a = args();
        a.data = vec!["=2".into()];
        assert!(handle_data_args(&a).is_err(), "no free key: rejected");

        // An empty VALUE still splits (and drops the key).
        let mut a = args();
        a.data = vec!["cups=".into()];
        assert_eq!(run(a), Map::new());
    }

    // ── trailing verbs (v2's changeDatumCommand, da-z5vek) ──────────────

    fn changed(data: &[&str], keys: &[&str]) -> (Value, Option<String>) {
        let a = DataArgs {
            data: data.iter().map(|s| s.to_string()).collect(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
            command_change: true,
            ..args()
        };
        let (d, c) = handle_log_data_args(&a).expect("handle_log_data_args");
        (Value::Object(d), c)
    }

    #[test]
    fn a_command_in_the_dur_slot_changes_the_command() {
        // `start sleep end 30m`: `end` is the command, 30m its duration.
        assert_eq!(
            changed(&["end", "30m"], &["dur="]),
            (json!({"dur": "PT30M"}), Some("end".into()))
        );
        // `mood happy end`: the state read stays, for the end to assert.
        assert_eq!(
            changed(&["happy", "end"], &["state=true", "dur="]),
            (json!({"state": {"id": "happy"}}), Some("end".into()))
        );
    }

    #[test]
    fn leftover_words_led_by_a_command_are_read_again_as_its_own() {
        // `occur coffee start 5m`: an occur has no slot for either word.
        assert_eq!(
            changed(&["start", "5m"], &[]),
            (json!({"dur": "PT5M"}), Some("start".into()))
        );
        // A switch takes its state from the words after it.
        assert_eq!(
            changed(&["switch", "tired"], &[]),
            (json!({"state": {"id": "tired"}}), Some("switch".into()))
        );
        // An occur has no duration.
        assert_eq!(
            changed(&["end", "occur"], &["dur="]),
            (json!({"dur": null}), Some("occur".into()))
        );
    }

    #[test]
    fn a_command_word_with_somewhere_to_go_is_data() {
        // A remainder takes it: `note jot the end of it`.
        let a = DataArgs {
            data: vec!["the".into(), "end".into()],
            comment_remainder: true,
            command_change: true,
            ..args()
        };
        let (d, c) = handle_log_data_args(&a).unwrap();
        assert_eq!(
            (Value::Object(d), c),
            (json!({"comment": ["the end"]}), None)
        );
        // A slot that is not `dur=` takes it.
        assert_eq!(changed(&["end"], &["what"]), (json!({"what": "end"}), None));
        // Without the flag (`update`, kit commands) nothing changes.
        assert!(matches!(
            handle_data_args(&DataArgs {
                data: vec!["end".into()],
                ..args()
            }),
            Err(ParseError::ExtraData(_))
        ));
    }
}
