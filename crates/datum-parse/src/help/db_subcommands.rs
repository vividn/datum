//! `datum db`'s subcommands and their usage, out of `commands.rs` for its
//! size.

use super::commands::sub;
use super::spec::SubcommandSpec;

pub(super) const DB_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub(
        "purge",
        &[
            "datum db purge [--leave [--force] | --rejoin] [--yes | -y]",
            "datum db purge --chain [--yes | -y] [--admin-key <FILE>]",
        ],
    ),
    sub(
        "backup",
        &[
            "datum db backup <PATH | -> [--overwrite]",
            "datum db backup [--to DIR] (--auto | --every DUR) [--slots N] [--all] [--dry-run [--span DUR]]",
            "datum db backup [--to DIR] --no-auto [--all]",
            "datum db backup --list DIR",
            "datum db backup status",
        ],
    ),
    sub(
        "restore",
        &["datum db restore <PATH | -> [--merge | --hard [--leave]] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]"],
    ),
    sub("migrate", &["datum db migrate [run | status | edit]"]),
    sub(
        "v2import",
        &["datum db v2import [FILE | - | --from FILE] [--dry-run]"],
    ),
    sub("config", &["datum db config [auto_setup [true | false] | timezone_follow [ask | auto]]"]),
    sub("attachment", &["datum db attachment [<HASH>]"]),
    sub("rename", &["datum db rename <NEW>"]),
    sub(
        "ops",
        &["datum db ops export", "datum db ops import [FILE | -]"],
    ),
];
