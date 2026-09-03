//! The browser's colours must be the CLI's colours.
//!
//! `datum-layout-wasm` re-implements the derivation rules in this module
//! (and `hash_y` from `datum_core::field_spec`) because `datum-core` cannot
//! reach `wasm32-unknown-unknown` — it pulls rusqlite. Re-implementations
//! drift silently, so this test holds the two side by side on the inputs
//! that matter: an md5-derived field colour, a named state, the `IGNORED`
//! and boolean sentinels, a parallel state set, spec overrides, and the
//! deterministic cross-axis position.

use datum_core::action::Action;
use datum_core::field_spec::FieldSpec;
use datum_layout_wasm::colors as wasm;
use serde_json::json;

use super::colors::{field_rgb, md5_rgb, state_value_colors, state_value_rgb};
use crate::commands::field_visuals::rgb_to_hex;

/// The fields whose colours a browser pane is most likely to paint.
const NAMES: &[&str] = &[
    "mood", "coffee", "sleep", "project", "word", "happy", "deep",
];

#[test]
fn md5_colours_match_the_cli() {
    for name in NAMES {
        assert_eq!(
            wasm::md5_hex(name),
            rgb_to_hex(md5_rgb(name)),
            "md5 colour drifted for {name}"
        );
    }
}

#[test]
fn cross_axis_position_matches_the_cli() {
    for name in NAMES {
        let resolved = FieldSpec::new(name.to_string(), Action::Occur)
            .presentation
            .resolve_position(name);
        assert!(
            (wasm::hash_y(name) - resolved.y).abs() < 1e-12,
            "hash_y drifted for {name}: {} vs {}",
            wasm::hash_y(name),
            resolved.y
        );
        assert_eq!(wasm::DEFAULT_FIELD_HEIGHT, resolved.height);
    }
}

#[test]
fn state_value_colours_match_the_cli() {
    let values = [
        json!("happy"),
        json!("IGNORED"),
        json!(true),
        json!(false),
        json!(null),
        json!(42),
        json!(["vertical", "sit"]),
        json!({ "id": "proj-1", "title": "ignored for colour" }),
        json!({ "no_id": 1 }),
    ];
    for value in &values {
        let theirs = state_value_rgb(value, "mood", None).map(rgb_to_hex);
        assert_eq!(
            wasm::state_value_hex(value, "mood", None),
            theirs,
            "state colour drifted for {value}"
        );

        let theirs: Vec<String> = state_value_colors(value, "mood", None)
            .into_iter()
            .map(rgb_to_hex)
            .collect();
        assert_eq!(
            wasm::state_value_colors(value, "mood", None),
            theirs,
            "parallel-state colours drifted for {value}"
        );
    }
}

#[test]
fn spec_overrides_match_the_cli() {
    let mut spec = FieldSpec::new("mood".to_string(), Action::Occur);
    spec.color = Some("#ff6600".to_string());
    spec.state_colors
        .insert("happy".to_string(), "00ff00".to_string());

    let style = wasm::FieldStyle {
        color: spec.color.clone(),
        state_colors: spec
            .state_colors
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ..wasm::FieldStyle::default()
    };

    assert_eq!(
        wasm::field_hex("mood", Some(&style)),
        rgb_to_hex(field_rgb("mood", Some(&spec)))
    );
    assert_eq!(
        wasm::state_value_hex(&json!("happy"), "mood", Some(&style)),
        state_value_rgb(&json!("happy"), "mood", Some(&spec)).map(rgb_to_hex)
    );
    // A state with no override still hashes its own name, not the field's.
    assert_eq!(
        wasm::state_value_hex(&json!("sad"), "mood", Some(&style)),
        state_value_rgb(&json!("sad"), "mood", Some(&spec)).map(rgb_to_hex)
    );
}
