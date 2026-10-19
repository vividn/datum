//! Where a finding comes from: its domain and category (da-0c2ne).

use super::Domain;

/// Where a finding comes from, which names its domain and its category
/// (da-0c2ne): the integrity queue's timeline problems are `integrity_*`
/// under entries, and sync's — a notice, an entry's `conflicts` — `sync_*`
/// under sync. Each pair is made by one macro, so a sync finding cannot
/// print as an integrity one.
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    Integrity(datum_core::integrity::IntegrityKind),
    /// A sync notice, by its kind.
    Notice(&'a str),
    /// An entry's `conflicts`.
    Conflicts,
}

macro_rules! integrity {
    ($name:literal) => {
        (Domain::Entries, concat!("integrity_", $name))
    };
}

macro_rules! sync {
    ($name:literal) => {
        (Domain::Sync, concat!("sync_", $name))
    };
}

impl Source<'_> {
    pub fn section(self) -> (Domain, &'static str) {
        use datum_core::integrity::IntegrityKind as K;
        match self {
            Source::Integrity(K::DoubleOpen) => integrity!("double_open"),
            Source::Integrity(K::OrphanEnd) => integrity!("orphan_end"),
            Source::Integrity(K::Overlap) => integrity!("overlap"),
            // Computed live and named for itself (da-mkes, da-acft).
            Source::Integrity(K::SameStateRepeat) => (Domain::Entries, "repeated_state"),
            Source::Integrity(K::LastStateMismatch) => integrity!("last_state_mismatch"),
            Source::Notice("refused") => sync!("refused"),
            Source::Notice("member") => sync!("joined"),
            Source::Notice("unlock") => sync!("unlocked"),
            Source::Notice("wipe") => sync!("wiped"),
            Source::Notice(_) => sync!("new_member"),
            Source::Conflicts => sync!("conflict"),
        }
    }
}
