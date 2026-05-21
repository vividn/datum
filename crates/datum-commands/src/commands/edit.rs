//! `datum edit <id_or_slug>` — open an entry's user-editable fields in $EDITOR.
//!
//! da-4lov: only fields the user is allowed to mutate appear in the editor
//! tempfile. System-only state — `extra_metadata`, `hid`, `slug`,
//! `content_hash`, `create_time`, `modify_time`, `device_id`, `parser_version`,
//! `raw_input`, `alias_chain`, `user_asserted_last_state`, `deleted_at` —
//! is never written to the buffer.
//!
//! da-5mnz: previously, a user who hand-typed a system-only key into the
//! tempfile had it silently dropped (UserEditableEntry has no slot for it).
//! That hides the boundary instead of enforcing it. Now we scan the parsed
//! JSON top-level for any [`SYSTEM_OWNED_KEYS`] entry and fail loud with a
//! clear "system-owned" message — better signal than a silent no-op.

use anyhow::{Context, Result};
use chrono::Utc;
use datum_core::{
    db::Db,
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Top-level keys on `Entry` that are system-managed and may never be written
/// via `datum edit`. Injecting any of these into the editor tempfile is a
/// hard error — see the module docstring (da-5mnz).
const SYSTEM_OWNED_KEYS: &[&str] = &[
    "extra_metadata",
    "hid",
    "slug",
    "content_hash",
    "create_time",
    "modify_time",
    "device_id",
    "parser_version",
    "raw_input",
    "alias_chain",
    "user_asserted_last_state",
    "deleted_at",
];

use crate::{
    args::{EditArgs, OutputFlagsExt},
    render::show_update,
};

/// Strict view of the fields a user is allowed to mutate via `datum edit`.
///
/// Anything not on this struct is system-only and physically cannot be
/// touched: it never gets written to the editor tempfile, and the
/// deserializer has nowhere to land it on read-back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserEditableEntry {
    field: String,
    verb: Verb,
    #[serde(default)]
    state: Option<JsonValue>,
    #[serde(default)]
    dur: Option<datum_core::dur::Dur>,
    #[serde(default)]
    dur_precision: Option<String>,
    #[serde(default)]
    following_state: Option<JsonValue>,
    #[serde(default)]
    data: Option<JsonValue>,
    #[serde(default)]
    occur_time: Option<DatumTime>,
    #[serde(default)]
    occur_time_precision: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    id_structure: Option<String>,
    #[serde(default)]
    field_structure: Option<String>,
}

impl UserEditableEntry {
    fn from_entry(e: &Entry) -> Self {
        Self {
            field: e.field.clone(),
            verb: e.verb,
            state: e.state.clone(),
            dur: e.dur.clone(),
            dur_precision: e.dur_precision.clone(),
            following_state: e.following_state.clone(),
            data: e.data.clone(),
            occur_time: e.occur_time.clone(),
            occur_time_precision: e.occur_time_precision.clone(),
            lat: e.lat,
            lon: e.lon,
            location: e.location.clone(),
            id_structure: e.id_structure.clone(),
            field_structure: e.field_structure.clone(),
        }
    }

    /// Merge the user-edited fields back into the existing entry. Every
    /// field on `Entry` that isn't on this struct keeps its original value —
    /// extra_metadata, hid, slug, content_hash, create_time, device_id,
    /// parser_version, raw_input, alias_chain, user_asserted_last_state,
    /// deleted_at, etc.
    fn apply_to(self, e: &mut Entry) {
        e.field = self.field;
        e.verb = self.verb;
        e.state = self.state.clone();
        e.dur = self.dur;
        e.dur_precision = self.dur_precision;
        e.following_state = self.following_state;
        e.data = self.data;
        e.occur_time = self.occur_time.clone();
        e.occur_time_precision = self.occur_time_precision;
        e.lat = self.lat;
        e.lon = self.lon;
        e.location = self.location;
        e.id_structure = self.id_structure;
        e.field_structure = self.field_structure;
        // Keep the legacy compat shims in lockstep so the rest of the
        // codebase that still reads entry.value / entry.start_time sees
        // the new state until sub-bead 8 deletes the shims.
        e.value = self.state;
        e.start_time = self.occur_time;
    }
}

pub fn edit(args: EditArgs, db: &Db) -> Result<()> {
    let existing = crate::commands::helpers::resolve_prefix_or_tombstone(db, &args.id_or_slug)?;
    if existing.deleted_at.is_some() {
        anyhow::bail!(
            "Cannot edit tombstoned entry {} — restore it first with `datum undo`",
            args.id_or_slug,
        );
    }

    let view = UserEditableEntry::from_entry(&existing);
    let original_json = serde_json::to_string_pretty(&view)
        .context("failed to serialize editable view for editing")?;

    let tmp = tempfile::Builder::new()
        .prefix("datum-edit-")
        .suffix(".json")
        .tempfile()
        .context("failed to create tempfile for editor")?;
    std::fs::write(tmp.path(), &original_json).context("failed to write entry to tempfile")?;

    let editor = std::env::var("DATUM_EDITOR")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("--")
        .arg(tmp.path())
        .status()
        .with_context(|| format!("failed to invoke editor: {editor}"))?;
    if !status.success() {
        anyhow::bail!("editor exited with non-zero status: {status}");
    }

    let edited =
        std::fs::read_to_string(tmp.path()).context("failed to read tempfile after editing")?;

    if edited == original_json {
        println!("No changes — entry {} unmodified", existing.slug);
        return Ok(());
    }

    let edited_value: JsonValue = serde_json::from_str(&edited)
        .map_err(|e| anyhow::anyhow!("edited content is not valid entry JSON: {e}"))?;

    if let JsonValue::Object(map) = &edited_value {
        if let Some(sys_key) = SYSTEM_OWNED_KEYS.iter().find(|k| map.contains_key(**k)) {
            anyhow::bail!(
                "datum edit cannot write to `{sys_key}` — this field is system-owned. \
                 Remove it from the edited JSON to save your other changes."
            );
        }
    }

    let edited_view: UserEditableEntry = serde_json::from_value(edited_value)
        .map_err(|e| anyhow::anyhow!("edited content is not valid entry JSON: {e}"))?;

    // Editor reflowed whitespace but no semantic change. Treat as no-op so
    // we don't bump modify_time / rewrite content_hash for nothing.
    if edited_view == view {
        println!("No changes — entry {} unmodified", existing.slug);
        return Ok(());
    }

    let mut updated = existing.clone();
    edited_view.apply_to(&mut updated);
    updated.modify_time = Utc::now();
    updated.content_hash = updated.expected_content_hash();

    db.save_existing_entry(&updated)?;

    let fresh = db.get_entry(&updated.slug)?;
    let registry = crate::commands::helpers::load_registry_opt(db);
    show_update(&fresh, registry.as_ref(), &args.output()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    //! `datum edit` tests.
    //!
    //! The edit command spawns a subprocess under `sh -c` driven by the
    //! DATUM_EDITOR env var. Env vars are process-global, so these tests
    //! serialize against a shared mutex to avoid stepping on each other.

    use super::*;
    use std::sync::Mutex;

    static EDIT_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Run a closure with DATUM_EDITOR set to `cmd`, holding the env lock.
    fn with_datum_editor<F: FnOnce()>(cmd: &str, f: F) {
        let _guard = EDIT_ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DATUM_EDITOR").ok();
        std::env::set_var("DATUM_EDITOR", cmd);
        f();
        match prev {
            Some(v) => std::env::set_var("DATUM_EDITOR", v),
            None => std::env::remove_var("DATUM_EDITOR"),
        }
    }

    /// Like `with_datum_editor` but returns the closure's result.
    fn with_datum_editor_result<T, F: FnOnce() -> T>(cmd: &str, f: F) -> T {
        let _guard = EDIT_ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DATUM_EDITOR").ok();
        std::env::set_var("DATUM_EDITOR", cmd);
        let out = f();
        match prev {
            Some(v) => std::env::set_var("DATUM_EDITOR", v),
            None => std::env::remove_var("DATUM_EDITOR"),
        }
        out
    }

    /// Build a minimal JSON payload for the editor tempfile that targets the
    /// `UserEditableEntry` shape. `occur_time` is taken verbatim from `e` so
    /// the round-trip preserves it (the deserializer lifts it back unchanged).
    fn editor_payload(e: &Entry, mutate: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut doc = serde_json::json!({
            "field": e.field,
            "verb": e.verb.to_string(),
            "state": e.state,
            "dur": e.dur,
            "following_state": e.following_state,
            "data": e.data,
            "occur_time": serde_json::to_value(&e.occur_time).unwrap(),
            "occur_time_precision": e.occur_time_precision,
            "lat": e.lat,
            "lon": e.lon,
            "location": e.location,
            "id_structure": e.id_structure,
            "field_structure": e.field_structure,
        });
        mutate(&mut doc);
        serde_json::to_string_pretty(&doc).unwrap()
    }

    fn write_editor_payload(payload: &str) -> tempfile::NamedTempFile {
        let prep = tempfile::Builder::new()
            .prefix("datum-edit-prep-")
            .suffix(".json")
            .tempfile()
            .unwrap();
        std::fs::write(prep.path(), payload).unwrap();
        prep
    }

    /// Stamp `extra_metadata` directly on the row via SQL. The public
    /// `save_existing_entry` deliberately doesn't write the `extra_metadata`
    /// column (only the import / migration path does), so tests that need
    /// to seed system-managed provenance write the column directly.
    fn stamp_extra_metadata(db: &Db, slug: &str, value: &JsonValue) {
        db.conn()
            .execute(
                "UPDATE entries SET extra_metadata = ?1 WHERE slug = ?2",
                rusqlite::params![value.to_string(), slug],
            )
            .unwrap();
    }

    fn seed_entry(
        db: &Db,
        field: &str,
        verb: Verb,
        value: Option<JsonValue>,
        extra: Option<JsonValue>,
    ) -> Entry {
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new_legacy(field, verb, value, extra, Some(dt), None, "d1");
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn edit_modifies_entry_via_scripted_editor() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"note": "sunny"})),
        );

        let payload = editor_payload(&e, |doc| {
            doc["state"] = serde_json::json!("focused");
            doc["data"] = serde_json::json!({"note": "clearer"});
        });
        let prep = write_editor_payload(&payload);

        let editor_cmd = format!("cp {}", prep.path().display());
        with_datum_editor(&editor_cmd, || {
            edit(
                EditArgs {
                    id_or_slug: e.slug.to_string(),
                    output: Default::default(),
                },
                &db,
            )
            .unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.state, Some(serde_json::json!("focused")));
        assert_eq!(after.data, Some(serde_json::json!({"note": "clearer"})));
        // content_hash refreshed to match the new state.
        assert_eq!(after.content_hash, after.expected_content_hash());
        assert_ne!(after.content_hash, e.content_hash);
        assert!(after.modify_time >= e.modify_time);
    }

    #[test]
    fn edit_no_change_is_noop() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "coffee", Verb::Point, Some(serde_json::json!(1)), None);
        let before = db.get_entry(&e.slug).unwrap();

        // `true` ignores its args → tempfile is left untouched.
        with_datum_editor("true", || {
            edit(
                EditArgs {
                    id_or_slug: e.slug.to_string(),
                    output: Default::default(),
                },
                &db,
            )
            .unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
        assert_eq!(after.state, before.state);
    }

    #[test]
    fn edit_rejects_invalid_json() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        let before = db.get_entry(&e.slug).unwrap();

        let prep = tempfile::Builder::new()
            .prefix("datum-edit-prep-")
            .suffix(".txt")
            .tempfile()
            .unwrap();
        std::fs::write(prep.path(), b"{ not valid json").unwrap();

        let editor_cmd = format!("cp {}", prep.path().display());
        let result = with_datum_editor_result(&editor_cmd, || {
            edit(
                EditArgs {
                    id_or_slug: e.slug.to_string(),
                    output: Default::default(),
                },
                &db,
            )
        });
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not valid entry JSON"),
            "unexpected error: {err}"
        );

        // Row untouched.
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }

    /// da-5mnz: writing a system-only key into the editor tempfile is a
    /// hard error — the legitimate edits in the same payload do NOT apply,
    /// and the row is fully unchanged. Replaces the da-4lov silent-drop
    /// behavior with fail-loud feedback so the user sees the boundary.
    #[test]
    fn edit_rejects_extra_metadata_injection() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );

        // Stamp system-managed provenance directly on the row, mimicking what
        // `datum db import --from <legacy>` would deposit.
        let original_meta = serde_json::json!({"_import": {"source": "legacy", "v": 1}});
        stamp_extra_metadata(&db, e.slug.as_str(), &original_meta);
        let before = db.get_entry(&e.slug).unwrap();
        assert_eq!(before.extra_metadata, Some(original_meta.clone()));

        let payload = editor_payload(&before, |doc| {
            // A legitimate change…
            doc["state"] = serde_json::json!("focused");
            // …paired with a system-key injection. The whole save must fail.
            doc["extra_metadata"] = serde_json::json!({"_evil": "hacked"});
        });
        let prep = write_editor_payload(&payload);

        let editor_cmd = format!("cp {}", prep.path().display());
        let result = with_datum_editor_result(&editor_cmd, || {
            edit(
                EditArgs {
                    id_or_slug: e.slug.to_string(),
                    output: Default::default(),
                },
                &db,
            )
        });
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("extra_metadata") && err.contains("system-owned"),
            "expected system-owned rejection naming extra_metadata, got: {err}"
        );

        // Row fully unchanged: the legit `state` edit did NOT apply because
        // the save was rejected wholesale.
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.extra_metadata, Some(original_meta));
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }

    /// da-5mnz: every documented system-owned top-level key is rejected
    /// individually with a message that names it. Guards against a future
    /// refactor adding a new system column without updating the boundary.
    #[test]
    fn edit_rejects_each_system_owned_key() {
        let cases: &[(&str, JsonValue)] = &[
            ("extra_metadata", serde_json::json!({"_evil": "hacked"})),
            ("hid", serde_json::json!("ffffffffffff")),
            ("slug", serde_json::json!("attacker:owned")),
            ("content_hash", serde_json::json!("deadbeef")),
            ("create_time", serde_json::json!("1970-01-01T00:00:00Z")),
            ("modify_time", serde_json::json!("1970-01-01T00:00:00Z")),
            ("device_id", serde_json::json!("attacker-device")),
            ("parser_version", serde_json::json!(999)),
            ("raw_input", serde_json::json!("rm -rf /")),
            ("alias_chain", serde_json::json!(["evil"])),
            ("user_asserted_last_state", serde_json::json!("hacked")),
            ("deleted_at", serde_json::json!("2099-01-01T00:00:00Z")),
        ];

        for (key, attack) in cases {
            let db = Db::open_in_memory().unwrap();
            let e = seed_entry(
                &db,
                "mood",
                Verb::Switch,
                Some(serde_json::json!("happy")),
                None,
            );
            let before = db.get_entry(&e.slug).unwrap();

            let payload = editor_payload(&before, |doc| {
                doc[*key] = attack.clone();
            });
            let prep = write_editor_payload(&payload);

            let editor_cmd = format!("cp {}", prep.path().display());
            let result = with_datum_editor_result(&editor_cmd, || {
                edit(
                    EditArgs {
                        id_or_slug: e.slug.to_string(),
                        output: Default::default(),
                    },
                    &db,
                )
            });
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains(*key) && err.contains("system-owned"),
                "key {key}: expected system-owned rejection, got: {err}"
            );

            // Row untouched.
            let after = db.get_entry(&e.slug).unwrap();
            assert_eq!(
                after.content_hash, before.content_hash,
                "key {key}: row mutated"
            );
            assert_eq!(
                after.modify_time, before.modify_time,
                "key {key}: modify_time bumped"
            );
        }
    }

    /// da-4lov complement: even when the user makes no semantic change,
    /// `extra_metadata` survives the round-trip. The editor view never carries
    /// it, so a save-without-changes still hits the "no-op" path or, if the
    /// editor whitespace-reformats, the apply path that copies user-only
    /// fields into a clone of the existing entry — extra_metadata stays put
    /// either way.
    #[test]
    fn edit_no_change_preserves_extra_metadata() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        let original_meta = serde_json::json!({"_import": {"source": "legacy"}});
        stamp_extra_metadata(&db, e.slug.as_str(), &original_meta);
        let before = db.get_entry(&e.slug).unwrap();

        with_datum_editor("true", || {
            edit(
                EditArgs {
                    id_or_slug: e.slug.to_string(),
                    output: Default::default(),
                },
                &db,
            )
            .unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.extra_metadata, Some(original_meta));
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }
}
