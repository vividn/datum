//! datum-cli's integration tests, linked as ONE binary (da-ztzfi).
//!
//! Every `tests/*.rs` is its own crate to cargo, and each one statically
//! linked the whole dependency tree: 85 of them were 5.7 GB of
//! `target/debug/deps`. `autotests = false` in Cargo.toml turns that off, so
//! the files stay where they are and are declared here instead.
//!
//! A file that writes the process env is NOT declared here: it is its own
//! `[[test]]` in Cargo.toml, as before, because under a threaded `cargo test`
//! its writes would reach the in-process tests here that never take
//! `env_lock`. `every_test_file_is_declared` holds both halves: every file is
//! declared one way or the other, and none declared here — nor the shared
//! `common/` it compiles — writes the env.

mod common;

mod add_cmd;
mod alias_cmd;
mod alias_entries;
mod alias_entry_template;
mod alias_install;
mod alias_list_json;
mod alias_plain_name;
mod alias_show_json;
mod alias_time_compose;
mod ambiguous_time;
mod batch_undo;
mod batch_update_cmd;
mod check_examined;
mod check_fix_shared_slug;
mod check_flag_field;
mod cli_harness;
mod client_templates;
mod closed_pipe_last;
mod color_flag;
mod colour_spelling;
mod comma_slugs;
mod comment_appends;
mod completion;
mod concurrent_states_cmd;
mod concurrent_writers;
mod conflicts_cmd;
mod count_cmd;
mod d2_parser;
mod d3_verb_alias;
mod data_args_coercion;
mod data_refs;
mod date_vocabulary;
mod dayview_cmd;
mod dayview_integrity;
mod db_cmd;
mod db_import;
mod db_import_regression;
mod debug_cmd;
mod definition_entries;
mod delete_cmd;
mod dot_glyphs;
mod dot_icons;
mod dot_skip_positional;
mod dur_flag_removed;
mod e2e_subprocess;
mod edit_cmd;
mod ensure_last_state;
mod env_table_coverage;
mod field_key_slot;
mod field_name_commas;
mod field_name_controls;
mod field_prefix;
mod fields_cmd;
mod first_log_spec;
mod float_roundtrip;
mod get_cmd;
mod get_projection;
mod help_commands_dispatch;
mod help_defined_names;
mod help_fits_columns;
mod history_cmd;
mod id_lists;
mod ids_cmd;
mod if_changed;
mod import_progress;
mod info_setup_cmd;
mod inline_alias;
mod install_cmd;
mod integrity_cmd;
mod key_operators;
mod kit_command_args;
mod kit_install;
mod kit_layouts;
mod kit_name_clash;
mod kit_reinstall;
mod last_state_and_moment;
mod mark_titles;
mod migrate_cmd;
mod multi_alias_e2e;
mod neighbour_zone;
mod no_timestamp;
mod note_comment_remainder;
mod now_cmd;
mod nowview_cmd;
mod null_token;
mod occur_cmd;
mod output_parity;
mod parity;
mod parity_repro;
mod range_shorthand_cmd;
mod readme_anti_drift;
mod readme_snippets;
mod record_refs;
mod redo_cmd;
mod render_state_group;
mod reparse_cmd;
mod repl_lines;
mod restore_hard;
mod restore_merge_collision;
mod retime_cmd;
mod show_env;
mod show_flag_family;
mod show_json;
mod spec_cmd;
mod spec_doc_keys;
mod spec_entries;
mod spec_grammar;
mod spec_json_order;
mod spec_keys;
mod spec_list_ops;
mod spec_lists;
mod spool;
mod stale_last_gate_removed;
mod start_cmd;
mod state_edit;
mod state_id_normalize;
mod state_keys;
mod state_oracle;
mod state_refs;
mod state_scoped_ref;
mod stateless_same_instant;
mod stats_cache;
mod stop_cmd;
mod stored_spec_compat;
mod subcommand_flag_as_name;
mod switch_cmd;
mod sync_chain;
mod sync_daemon;
mod sync_grant;
mod sync_net;
mod sync_ops;
mod tail_cmd;
mod template_spec;
mod temporal_keys;
mod timeless_id_redesign;
mod timeless_state_note;
mod track_cmd;
mod track_declaration;
mod trailing_commands;
mod trailing_verbs;
mod typed_keys;
mod undo_shared_slug;
mod undo_stack;
mod unit_removed;
mod untimed_field_is_tracked;
mod update_cmd;
mod update_refield_cmd;
mod version;
mod view_cmd;
mod web_command_names;

#[test]
fn every_test_file_is_declared() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main = std::fs::read_to_string(root.join("tests/main.rs")).unwrap();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let env_writes = [
        "set_var",
        "remove_var",
        "ScopedEnv",
        "env_lock",
        "color_scope",
        "with_color",
        "with_no_color",
    ];
    let mut wrong = Vec::new();
    for entry in std::fs::read_dir(root.join("tests")).unwrap() {
        let path = entry.unwrap().path();
        let Some(m) = path
            .file_stem()
            .filter(|_| path.extension() == Some("rs".as_ref()))
        else {
            continue;
        };
        let m = m.to_string_lossy();
        let here = main.contains(&format!("\nmod {m};\n"));
        let own = manifest.contains(&format!("path = \"tests/{m}.rs\""));
        let src = std::fs::read_to_string(&path).unwrap();
        let writes = env_writes.iter().find(|w| src.contains(*w));
        if here == own {
            wrong.push(format!(
                "{m}: declare it once, `mod {m};` here or a [[test]] of its own"
            ));
        } else if let (true, Some(w)) = (here, writes) {
            wrong.push(format!(
                "{m}: writes the env (`{w}`), so it needs a [[test]] of its own"
            ));
        }
    }
    for m in main
        .lines()
        .filter_map(|l| l.strip_prefix("mod ")?.strip_suffix(';'))
    {
        let Ok(src) = std::fs::read_to_string(root.join(format!("tests/{m}/mod.rs"))) else {
            continue;
        };
        if let Some(w) = env_writes.iter().find(|w| src.contains(*w)) {
            wrong.push(format!(
                "{m}/mod.rs: shared module writes the env (`{w}`), which reaches every test in main"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
