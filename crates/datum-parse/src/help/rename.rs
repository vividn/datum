//! `datum rename`'s page, apart from `commands.rs` for its size.

use super::spec::{Category, CommandSpec};

pub(super) const RENAME: CommandSpec = CommandSpec {
    name: "rename",
    aliases: &[],
    category: Category::Mutate,
    summary: "give a field another name, its entries and spec with it",
    intro: "",
    long_description: None,
    usage: &[
        "datum rename <OLD> <NEW> [--merge] [--dry-run]",
        "datum rename <FIELD> --state <OLD> <NEW> [--merge] [--dry-run]",
    ],
    args: &[],
    flags: &[],
    shared_flags: None,
    examples: &[],
    summary_col: 0,
    body_override: Some(RENAME_BODY),
    display_input_name: false,
    subcommands: &[],
};

const RENAME_BODY: &str = "USAGE:\n  datum rename <OLD> <NEW> [--merge] [--dry-run]\n  datum rename <FIELD> --state <OLD> <NEW> [--merge] [--dry-run]\n\nEvery entry of OLD becomes NEW's, in place: each keeps its id, and its\nslug follows its field (`mood:2026-10-19T12:48Z` becomes\n`feeling:2026-10-19T12:48Z`). OLD's spec moves to `spec:NEW`, and the\naliases it derives with it: `feeling` and `feelinge` where `mood` and\n`moode` were. Each row is an edit like any other, so it syncs, and\nthe rename is one write: one `undo` takes it all back.\n\n  datum rename mood feeling\n\nNEW must be new: a field that holds any entry, a deleted one too, or a\nspec is refused unless you say --merge. Nor does it\nrename a field template or a field its entries take from one\n(`issue__%id%`).\n\nAn alias you wrote is text, and one whose expansion names OLD is listed\nas «still names OLD» and left as it is: `datum alias set` it again.\n\n  --merge     NEW may exist: OLD's entries become NEW's, for a mistyped\n              name. NEW's spec stays and OLD's goes, saying what differs.\n              An entry landing on one NEW holds, the same in all but its\n              writing times, becomes one with it: the older stays and the\n              younger is deleted. One that differs stays too, in a row with\n              a new id (its old id deleted, saying the new one), merged with\n              NEW's: both are listed, and `datum conflicts list` shows what\n              they say differently, the side it came from reading `backup`.\n  --dry-run   say how many entries, which spec and which aliases would\n              move, what a merge would list, and move nothing\n\nAn OLD entry another copy logs before it has the rename stays OLD's.

With --state, OLD and NEW are states of FIELD, and the state is renamed
everywhere: each entry holding it, alone or in a list, gets NEW in place,
and so do the spec's list of states, its archived ones and what it keeps
by state (color, description, symbol, keys, assigned keys). The alias the
spec derives from OLD becomes NEW's; one you wrote naming OLD is listed and
left. One write, one `undo`.

  datum rename mood --state meh okay

NEW must be new to FIELD (a deleted entry's state counts) unless --merge,
where NEW's color, description and the rest stay, saying what differed.
`true` and `false` are on and off, not names, and NEW must read back as
the state it is: no number, list, flag or command word. A state that names
a document (`--state-lookup-field`) does not rename the document.
";
