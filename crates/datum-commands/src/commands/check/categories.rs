//! Every category `datum check` reports, by its checker (da-80t1i).

/// Each category a checker reports, by its checker (da-80t1i): a run
/// reporting one not here fails a debug build, and
/// `every_finding::a_named_checker_lists_every_finding` plants two of each.
pub const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "entries",
        &[
            "time_end_before_start",
            "reserved_data_key",
            "content_hash_mismatch",
            "unknown_field",
            "orphaned_extra_data_key",
            "mistyped_data_value",
            "kit_index_unbuilt",
        ],
    ),
    ("verbs", &["verb_drift", "verb_corruption"]),
    (
        "aliases",
        &[
            "alias_state_collision",
            "alias_legacy_double_dash",
            "spec_alias_names_stale",
            "alias_name_not_plain",
            "alias_shadowed_by_command",
            "kit_command_shadowed_by_field",
            "alias_orphan",
            "alias_cycle",
        ],
    ),
    ("slugs", &["slug_drift"]),
    (
        "integrity",
        &[
            "integrity_warning_unreadable",
            "integrity_double_open",
            "integrity_orphan_end",
            "integrity_overlap",
            "integrity_last_state_mismatch",
            "repeated_state",
            "sync_conflict",
        ],
    ),
    ("keys", &["key_drift"]),
    ("flags", &["flagged_entry"]),
    (
        "tz",
        &[
            "timezone_state_not_a_zone",
            "tz_offset_mismatch",
            "unknown_timezone",
            "tz_off_neighbours",
            "tz_off_declared",
        ],
    ),
    ("refs", &["dangling_state_ref", "dangling_data_ref"]),
    ("stats", &["stats_cache_drift"]),
    (
        "sync",
        &[
            "sync_orphan_ops",
            "sync_stale_row",
            "sync_unfoldable",
            "sync_no_op",
            "sync_refused",
            "sync_joined",
            "sync_unlocked",
            "sync_wiped",
            "sync_new_member",
            "sync_upgrade",
            "sync_unsent",
        ],
    ),
    (
        "spool",
        &["spool_stuck", "spool_waiting", "spool_set_aside"],
    ),
    (
        "install",
        &[
            "install_completion_missing",
            "install_update",
            "install_update_check",
            "install_daemon_missing",
            "install_daemon_binary",
            "install_daemon_stale",
            "backup_daemon_missing",
            "backup_daemon_failed",
        ],
    ),
];
