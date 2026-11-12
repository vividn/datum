//! `datum import` and `datum export`'s pages (da-jouhm), apart from
//! `commands.rs` for its size. `docs/import.md` is the spec in full.

use super::spec::{Category, CommandSpec};

pub(super) const IMPORT: CommandSpec = CommandSpec {
    name: "import",
    aliases: &[],
    category: Category::Admin,
    summary: "import entries from JSONL rows, every row or none",
    intro: "",
    long_description: None,
    usage: &["datum import [FILE | -] [--allow-partial] [--dry-run] [--tz ZONE] [--schema]"],
    args: &[],
    flags: &[],
    shared_flags: None,
    examples: &[],
    summary_col: 0,
    body_override: Some(IMPORT_BODY),
    display_input_name: false,
    subcommands: &[],
};

pub(super) const EXPORT: CommandSpec = CommandSpec {
    name: "export",
    aliases: &[],
    category: Category::Admin,
    summary: "print entries as the JSONL rows `import` reads",
    intro: "",
    long_description: None,
    usage: &["datum export --jsonl [FIELD…]"],
    args: &[],
    flags: &[],
    shared_flags: None,
    examples: &[],
    summary_col: 0,
    body_override: Some(EXPORT_BODY),
    display_input_name: false,
    subcommands: &[],
};

const IMPORT_BODY: &str = "USAGE:\n  datum import [FILE | -] [--allow-partial] [--dry-run] [--tz ZONE] [--schema]\n\nImports FILE, or stdin, one entry a line, each a JSON object:\n\n  {\"field\": \"coffee\", \"at\": \"2026-11-10T08:30:00+01:00\", \"dur\": \"MOMENT\"}\n  {\"field\": \"mood\", \"at\": \"2026-11-10 09:00\", \"state\": \"happy\"}\n  {\"field\": \"walk\", \"at\": \"2026-11-10 13:00\", \"state\": true, \"dur\": \"PT30M\"}\n\n`field` is required, and `at` for a timed entry: one with a state, a dur or\na following_state, or of a field whose kind is timed. A row without it is\nrefused, never put at now. `at` is a time as -t reads it, or the object\n`datum export --jsonl` prints; a time naming no zone is in --tz, else this\nmachine's. `create_time`, given, is the entry's own; else the import's. The\nother keys are those `export` prints; hid, slug, id_structure, content_hash,\nclient_id, parser_version, modify_time and alias_chain are derived, and\nignored if given. `datum import --schema` prints the rows' JSON Schema, and\ndocs/import.md says every key.\n\nEvery row lands, or none does: a row that is refused is named by its line,\nnothing is written, and datum exits 1. A row already there, at its field and\ntime, is skipped, so a second run writes nothing new; one there with other\ncontent is kept as it is, and named. One `undo` takes the import back.\n\n  --allow-partial  write the rows that read, naming each refused; datum\n                   still exits 1\n  --dry-run        say what it would import, and write nothing\n  --tz ZONE        the zone a time naming none is in\n  --schema         print the rows' JSON Schema, and import nothing\n";

const EXPORT_BODY: &str = "USAGE:\n  datum export --jsonl [FIELD…]\n\nPrints each live entry, or each of the FIELDs', as the JSON row `datum\nimport` reads, one a line, oldest first: so an export imports back to the\nsame entries, and its rows are examples of what an import takes. The keys\nimport derives are left out, and so is a key with no value. A meta field's\nentries (specs, aliases, layouts) are not exported.\n";
