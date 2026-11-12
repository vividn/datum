//! `datum script`'s page (da-ygfeh), apart from `commands.rs` for its size.

use super::spec::{Category, CommandSpec};

pub(super) const SCRIPT: CommandSpec = CommandSpec {
        name: "script",
        aliases: &[],
        category: Category::Mutate,
        summary: "run a script of command lines, stopping at the first that fails",
        intro: "",
        long_description: None,
        usage: &["datum script [FILE] [--atomic | --allow-partial] [--dry-run] [--now TIME] [--tz ZONE] [--db DB]"],
        args: &[],
        flags: &[],
        shared_flags: None,
        examples: &[],
        summary_col: 0,
        body_override: Some(SCRIPT_BODY),
        display_input_name: false,
        subcommands: &[],
    };

const SCRIPT_BODY: &str = "USAGE:\n  datum script [FILE] [--atomic | --allow-partial] [--dry-run] [--now TIME] [--tz ZONE] [--db DB]\n  datum < FILE\n\nRuns FILE's lines in order, each as `datum <line>` would. With no FILE, or\n`-`, the lines come from stdin, and `datum < FILE` is `datum script FILE`.\nA blank line, or one starting with #, is skipped, and `exit` or `quit` ends\nthe script.\n\n  datum script day.txt --now '2026-11-10 21:00'\n\nA line that fails stops the script there: its error says its line number,\nthe lines after it do not run, and datum exits 1. A line means now unless it\nsays -t, as it does typed: for history, give each line its time.\n\n  --atomic         every line lands, or none does: one write, and one `undo`\n                   takes the whole script back. A line it cannot carry, a db,\n                   sync, config or kit write or a -L claim, is refused\n  --allow-partial  go on past a line that fails, saying each; datum still\n                   exits 1\n  --dry-run        run every line on a copy, and write nothing\n  --now TIME       every now of the script is TIME, read as -t reads it, so\n                   the lines' bare times are one instant\n  --tz ZONE        the zone every line reads its times in, as TZ would say\n  --db DB          the database every line runs against\n";
