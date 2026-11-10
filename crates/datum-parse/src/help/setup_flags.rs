//! `datum setup`'s flags, carved from `commands.rs` for its size cap.

use super::spec::FlagSpec;

/// A flag in one line: a value when `hint` names one. For a table written
/// out only for `help --json`, its page being a `body_override` (`SPEC_FLAGS`).
const fn opt(
    short: Option<char>,
    long: &'static str,
    hint: Option<&'static str>,
    summary: &'static str,
) -> FlagSpec {
    FlagSpec {
        short,
        long,
        takes_value: hint.is_some(),
        value_hint: hint,
        value_optional: false,
        summary,
        long_description: None,
        left_override: None,
    }
}

/// `setup`'s flags, as its page (`SPEC_BODY`) says them, in the order a
/// hand reaches for them: datum.day's › list on the setup tab offers them
/// (da-zx826). `every_documented_flag_is_the_parsers` holds them to it.
pub(super) const SPEC_FLAGS: &[FlagSpec] = &[
    opt(
        None,
        "kind",
        Some("K"),
        "what a press of the field means: moment, on, change, record, allday",
    ),
    opt(None, "add-state", Some("S"), "append S if it is not listed"),
    opt(
        None,
        "remove-state",
        Some("S"),
        "drop S, with its color, symbol, description, keys and assigns",
    ),
    opt(
        None,
        "archive-state",
        Some("S"),
        "drop S from the list and keep all of it, so its past rows draw as they did",
    ),
    opt(
        None,
        "states",
        Some("A,B"),
        "exactly these, in this order (`''` clears)",
    ),
    opt(
        None,
        "state-description",
        Some("S=TEXT"),
        "what a state means (`S=` clears it)",
    ),
    opt(
        None,
        "color",
        Some("C"),
        "the field's color: hex, an ANSI-256 number or a hue name (`''` clears it)",
    ),
    opt(
        None,
        "state-color",
        Some("S=C"),
        "a state's own color (`S=` clears it)",
    ),
    opt(
        None,
        "symbol",
        Some("CHAR"),
        "the field's symbol (`''` clears it)",
    ),
    opt(
        None,
        "state-symbol",
        Some("S=CHAR"),
        "a state's own symbol, over the field's (`S=` clears it)",
    ),
    opt(
        None,
        "symbol-by",
        Some("KEY"),
        "pick the symbol by a data key's value",
    ),
    opt(
        None,
        "symbol-map",
        Some("V=CHAR"),
        "the symbol for one value of `--symbol-by`'s key (`V=` removes it)",
    ),
    opt(
        None,
        "symbol-scale",
        Some("N"),
        "a dot N times the standard size (`1` clears it)",
    ),
    opt(
        None,
        "symbol-aspect",
        Some("N"),
        "a dot N times as wide as it is tall (`1` clears it)",
    ),
    opt(
        None,
        "icon",
        Some("FILE"),
        "an .svg for the field's dots in the dayview (`''` clears it)",
    ),
    opt(
        Some('k'),
        "data-key",
        Some("K"),
        "declare a `-k` key: `K` required, `K=` optional, `K=D` default D",
    ),
    opt(
        Some('K'),
        "id-key",
        Some("K"),
        "declare a `-K` key, its value in the slug",
    ),
    opt(None, "remove-key", Some("K"), "take a declared key away"),
    opt(
        None,
        "assign",
        Some("K=V"),
        "write K=V onto every log (`K=` removes it)",
    ),
    opt(
        None,
        "id-assign",
        Some("K=V"),
        "write K=V onto every log and into its slug",
    ),
    opt(None, "state-key", Some("S=K"), "a key of state S's own"),
    opt(
        None,
        "state-assign",
        Some("S=K=V"),
        "an assign of state S's own",
    ),
    opt(
        None,
        "add-tag",
        Some("T"),
        "append tag T if it is not listed",
    ),
    opt(None, "remove-tag", Some("T"), "drop tag T"),
    opt(None, "tags", Some("A,B"), "exactly these tags"),
    opt(
        None,
        "track",
        None,
        "write the field's TRACK row with its spec",
    ),
    opt(None, "ignore", None, "write an IGNORED row with its spec"),
    opt(
        None,
        "hide",
        None,
        "keep the field out of the dayview and the nowview",
    ),
    opt(None, "draw", None, "draw a hidden field again"),
    opt(
        None,
        "never-false",
        None,
        "a change field always in one of its states",
    ),
    opt(None, "allow-false", None, "clear `--never-false`"),
    opt(
        None,
        "only-listed-keys",
        None,
        "refuse a data key the spec does not declare",
    ),
    opt(None, "any-keys", None, "clear `--only-listed-keys`"),
    opt(
        None,
        "only-listed-states",
        None,
        "refuse a state the spec does not list",
    ),
    opt(None, "any-states", None, "clear `--only-listed-states`"),
    opt(
        Some('R'),
        "remainder",
        Some("KEY"),
        "send leftover words to KEY (`''` clears it)",
    ),
    opt(
        Some('S'),
        "string-remainder",
        None,
        "with `-R KEY`, joined into one string",
    ),
    opt(
        Some('C'),
        "comment-remainder",
        None,
        "leftover words kept as one comment",
    ),
    opt(
        None,
        "conflict",
        Some("HOW"),
        "a log landing on an entry: update, replace, merge or refuse",
    ),
    opt(
        None,
        "key-conflict",
        Some("K=HOW"),
        "a data key's own `--conflict`",
    ),
    opt(
        None,
        "state-lookup-field",
        Some("F"),
        "a bare state names the entry `F:<state>`",
    ),
    opt(
        None,
        "auto-setup",
        Some("BOOL"),
        "on a template: whether a log may create a field it covers",
    ),
    opt(None, "height", Some("N"), "the field's lane height"),
    opt(None, "from-json", Some("JSON"), "the spec as JSON"),
    opt(None, "json", None, "print the spec as JSON"),
    opt(None, "delete", None, "delete the spec document"),
    opt(None, "tag", Some("T"), "list only the fields tagged T"),
    opt(None, "premade", None, "list the premade fields"),
];
