//! The field specs both runtimes load: `specs/my_specs.toml`.
//!
//! v2 reads the file as `$DATUM_SPECS` (`runtime`), daydumb installs it as a
//! kit (`tests/text_goldens.rs`), and the color tests read their spec colors
//! from it here. A field the file does not name takes each runtime's default.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The file's text, as daydumb's `kit install -` reads it.
pub const MY_SPECS: &str = include_str!("../specs/my_specs.toml");

/// The file's path, for v2's `DATUM_SPECS`.
pub fn my_specs_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("my_specs.toml")
}

/// One `[[field]]`: the keys the color tests read.
#[derive(Debug, Deserialize)]
pub struct FieldSpec {
    /// The field's name.
    pub name: String,
    /// `#RRGGBB`, or `None` for md5(field).
    pub color: Option<String>,
    /// Per-state overrides.
    #[serde(default)]
    pub states: BTreeMap<String, StateSpec>,
}

/// A state's override.
#[derive(Debug, Deserialize)]
pub struct StateSpec {
    /// `#RRGGBB`, or `None` for md5(state).
    pub color: Option<String>,
}

#[derive(Deserialize)]
struct Kit {
    field: Vec<FieldSpec>,
}

fn fields() -> &'static [FieldSpec] {
    static FIELDS: OnceLock<Vec<FieldSpec>> = OnceLock::new();
    FIELDS.get_or_init(|| {
        toml::from_str::<Kit>(MY_SPECS)
            .expect("specs/my_specs.toml parses")
            .field
    })
}

/// The spec of `field`, or `None` when the file does not name it.
pub fn field_spec(field: &str) -> Option<&'static FieldSpec> {
    fields().iter().find(|f| f.name == field)
}

/// The override of `state` on `field`, or `None`.
pub fn state_spec(field: &str, state: &str) -> Option<&'static StateSpec> {
    field_spec(field).and_then(|f| f.states.get(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state color the file sets, not md5("run").
    #[test]
    fn body_run_is_the_files_color() {
        let s = state_spec("body", "run").expect("body.run is in my_specs.toml");
        assert_eq!(s.color.as_deref(), Some("#ff6e19"));
    }

    #[test]
    fn a_field_the_file_does_not_name_has_no_spec() {
        assert!(field_spec("does_not_exist").is_none());
    }

    /// daydumb's own kit parser takes the file: it refuses a key it does not
    /// read, which bare TOML parsing never would (da-ma5pr).
    #[test]
    fn daydumb_installs_it_as_a_kit() {
        let kit = datum_core::kit::KitToml::from_str(MY_SPECS)
            .unwrap_or_else(|e| panic!("my_specs.toml refused as a kit: {e}"));
        assert_eq!(kit.fields.len(), fields().len());
    }
}
