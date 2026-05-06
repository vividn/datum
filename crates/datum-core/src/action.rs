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
pub enum Action {
    /// `datum occur <field>` — single point-in-time event with no state.
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
    /// `datum add <field>` (alias `note`) — shape-inferring verb that
    /// derives the entry's shape from temporal hints in `extra_data`.
    Add,
}

impl Default for Action {
    fn default() -> Self {
        Action::Occur
    }
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
            "add" | "note" => Ok(Action::Add),
            _ => Err(format!("Unknown action: '{s}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in ["occur", "start", "stop", "switch", "block", "add"] {
            let a: Action = s.parse().unwrap();
            assert_eq!(a.to_string(), s);
        }
    }

    #[test]
    fn cli_aliases() {
        assert_eq!("end".parse::<Action>().unwrap(), Action::Stop);
        assert_eq!("note".parse::<Action>().unwrap(), Action::Add);
    }
}
