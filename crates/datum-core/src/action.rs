//! Command intent — what verb the user typed at the CLI.
//!
//! Distinct from [`crate::entry::Verb`], which classifies the *stored*
//! shape of an entry (Point/Switch/Block/BlockSwitch/PointSwitch — derived
//! from `state`/`dur`/`following_state` by [`crate::verb::derive_verb`]).
//! `Action` lives at the parse boundary: command handlers (`datum start`,
//! `datum stop`, `datum switch`, …) translate user intent into the entry's
//! `state`/`dur`/`following_state` fields, then `derive_verb` picks the
//! storage [`Verb`](crate::entry::Verb).
//!
//! Persisted on [`FieldSpec`](crate::field_spec::FieldSpec) as
//! `default_action` (the verb to apply when a bare-field invocation hits a
//! configured field). Migration `da-8y2q.2` rewrote the legacy `verb`
//! string on every stored field-spec.

use serde::{Deserialize, Serialize};

/// Command intent surfaced from the CLI / programmatic API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum Action {
    /// `datum occur <field>` — single point-in-time event with no state.
    #[default]
    Occur,
    /// `datum start <field>` — begin a state (canonicalizes a missing
    /// `state` to `true`).
    Start,
    /// `datum stop <field>` — end a state (canonicalizes a missing
    /// `state` to `false`).
    Stop,
    /// `datum switch <field> <state>` — record a state transition.
    Switch,
    /// `datum block <field> --dur <d>` — record a duration block.
    Block,
    /// `datum add <field>` — timeless, shape-inferring verb that derives
    /// the entry's shape from temporal hints in `extra_data`. da-r17h:
    /// `add` REQUIRES an explicit id (a keyed record identified by *what*);
    /// see [`Note`](Action::Note) for the auto-id sibling.
    Add,
    /// `datum note <field>` — timeless, **auto create_time id** (slug
    /// `field:c<create_time>`). The easy timeless jot, identified by *when*
    /// ("record this moment, won't look it up"). da-r17h split it from
    /// [`Add`](Action::Add): both are timeless, but `note` auto-ids on its
    /// create_time while `add` demands an explicit id.
    Note,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Occur => write!(f, "occur"),
            Action::Start => write!(f, "start"),
            Action::Stop => write!(f, "stop"),
            Action::Switch => write!(f, "switch"),
            Action::Block => write!(f, "block"),
            Action::Add => write!(f, "add"),
            Action::Note => write!(f, "note"),
        }
    }
}

impl std::str::FromStr for Action {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "occur" => Ok(Action::Occur),
            "start" => Ok(Action::Start),
            "stop" | "end" => Ok(Action::Stop),
            "switch" => Ok(Action::Switch),
            "block" => Ok(Action::Block),
            "add" => Ok(Action::Add),
            "note" => Ok(Action::Note),
            _ => Err(format!("Unknown action: '{s}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in ["occur", "start", "stop", "switch", "block", "add", "note"] {
            let a: Action = s.parse().unwrap();
            assert_eq!(a.to_string(), s);
        }
    }

    #[test]
    fn cli_aliases() {
        assert_eq!("end".parse::<Action>().unwrap(), Action::Stop);
    }

    #[test]
    fn note_is_distinct_from_add() {
        // da-r17h: `note` is its own action (auto create_time id), no
        // longer an alias for `add` (which requires an explicit id).
        assert_eq!("note".parse::<Action>().unwrap(), Action::Note);
        assert_eq!("add".parse::<Action>().unwrap(), Action::Add);
        assert_ne!(Action::Note, Action::Add);
    }
}
