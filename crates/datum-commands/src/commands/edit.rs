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
    dur: Option<String>,
    #[serde(default)]
    following_state: Option<JsonValue>,
    #[serde(default)]
    data: Option<JsonValue>,
    #[serde(default)]
    unit: Option<String>,
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
            verb: e.verb.clone(),
            state: e.state.clone(),
            dur: e.dur.clone(),
            following_state: e.following_state.clone(),
            data: e.data.clone(),
            unit: e.unit.clone(),
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
        e.following_state = self.following_state;
        e.data = self.data;
        e.unit = self.unit;
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
    let existing = crate::cmd::resolve_prefix_or_tombstone(db, &args.id_or_slug)?;
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
    std::fs::write(tmp.path(), &original_json)
        .context("failed to write entry to tempfile")?;

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

    let edited = std::fs::read_to_string(tmp.path())
        .context("failed to read tempfile after editing")?;

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
    let registry = crate::cmd::load_registry_opt(db);
    show_update(&fresh, registry.as_ref(), &args.output());
    Ok(())
}
