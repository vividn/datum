//! Entry identity: the hid, the default slug, and the content hash.
//!
//! All three are pure functions of an entry's own columns, so they live
//! here — in the crate that compiles to `wasm32-unknown-unknown` — rather
//! than in `datum-core` beside the database. `datum-core` wraps them back
//! into its `Hid` / `Slug` newtypes ([`datum_core::entry::new_hid`],
//! [`datum_core::entry::build_slug`]); the browser calls them through
//! `datum-parse-wasm` (da-7scm). One definition, two callers, no drift.
//!
//! Randomness is the one thing this module will not source for itself:
//! [`hid_from_bytes`] takes the bytes. On the host `rand` supplies them,
//! in the browser `crypto.getRandomValues` does, and the crate stays free
//! of a `getrandom` backend it would otherwise have to configure per
//! target.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::time::DatumTime;

/// Length of a hid — 12 base36 characters.
pub const HID_LEN: usize = 12;

/// Render `bytes` as a base36 hid, one character per byte.
///
/// The `[0-9a-z]` alphabet over [`HID_LEN`] characters gives ~36^12 ≈ 4.7
/// quintillion distinct values, so collisions under normal use are
/// astronomically unlikely; the DB still enforces uniqueness on the column
/// so an accidental collision simply forces a regeneration.
///
/// Each byte is taken modulo 36. A caller feeding uniform random bytes
/// gets a very slight bias toward the first four characters of the
/// alphabet (256 is not a multiple of 36) — irrelevant at this collision
/// margin, and the alternative (rejection sampling) would make the two
/// callers' streams diverge for the same bytes.
///
/// Fewer than [`HID_LEN`] bytes yields a correspondingly shorter hid;
/// extra bytes are ignored.
pub fn hid_from_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(HID_LEN)
        .map(|b| {
            let n = b % 36;
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + (n - 10)) as char
            }
        })
        .collect()
}

/// The `occur_time_uncertainty` value that marks a date-only entry
/// (da-fc0l): its instant was stripped to local midnight and carries no
/// time-of-day.
pub const DATE_ONLY_PRECISION: &str = "P1D";

/// An instant as a slug stamp: `YYYY-MM-DDTHH:MM:SS.sssZ`. Millisecond
/// precision keeps same-field same-second inserts on distinct slugs.
pub fn render_stamp(t: &DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// How `%occur_time%` renders in a slug.
///
/// da-4fyb: a date-only row (`P1D`) has no time-of-day, so it keys on the
/// bare *local* date — the stored instant is local midnight, and rendering
/// the UTC day would put an east-of-UTC row on the wrong calendar day.
/// Anything else keeps the full UTC stamp.
pub fn render_occur_token(t: &DatumTime, precision: Option<&str>) -> String {
    if precision == Some(DATE_ONLY_PRECISION) {
        t.local_time().format("%Y-%m-%d").to_string()
    } else {
        render_stamp(&t.utc)
    }
}

/// Default slug: what `%occur_time%` / `%create_time%` interpolate to for
/// an entry with no `id_structure` of its own.
///
/// A time-anchored entry slugs `<field>:<occur_time>`. A timeless entry
/// (no `occur_time`) slugs `<field>:c<create_time>` (da-r17h / da-xbjy) —
/// the `c` marker is a *prefix*, mirroring the display convention, and
/// `create_time` keeps many-per-field timeless entries apart (the old
/// `<field>:unset` made every timeless entry collide on the UNIQUE slug).
///
/// The CLI insert path reaches the same strings through
/// `datum_core::field_spec::interpolate`, which renders the two tokens
/// with the helpers above; a field carrying an `id_structure` template
/// overrides this default entirely.
pub fn build_slug(
    field: &str,
    time: Option<&DatumTime>,
    precision: Option<&str>,
    create_time: &DateTime<Utc>,
) -> String {
    match time {
        Some(t) => format!("{}:{}", field, render_occur_token(t, precision)),
        None => format!("{}:c{}", field, render_stamp(create_time)),
    }
}

/// The columns that constitute an entry's *content* for dedup, replay
/// detection and the sync tiebreaker.
///
/// vividn's ruling (da-4vyw): the hash covers **the fact asserted**, not the
/// annotations around it. In: `field`, `occur_time` (UTC instant only),
/// `dur`, `state`, `data`, `following_state`, and whether the row is
/// deleted. Out: `lat`/`lon`/`location`, `occur_time_offset`/`_tz` (the
/// instant is the fact, the offset is how it was rendered), the two
/// uncertainties, `user_asserted_last_state`, `extra_metadata`, and every
/// identity/provenance column.
///
/// **Uncertainty is out on purpose (da-b8rs), not by oversight.** Two
/// entries that differ only in how precisely the time or duration is known
/// describe the *same event*: `coffee ±5m` and `coffee ±1m` at the same
/// instant are one coffee. Hashing the uncertainty would split them into
/// two rows that never dedup and never converge. Where two devices really
/// do disagree, the LWW merge surfaces it as a conflict for the user to
/// settle — which is the outcome vividn asked for. Do not "fix" this by
/// adding uncertainty to `parts` below.
///
/// `following_state` closes a second hole as a side effect: `verb` is
/// derived from `(state, dur, following_state)`, so with all three hashed
/// a verb difference can no longer hide behind an equal hash.
///
/// Adding a field is one line here, one line in [`content_hash`]'s `parts`,
/// and one migration that rewrites every stored row — see
/// `datum_core::migrations::m32_to_33_content_hash_v33`.
pub struct HashInputs<'a> {
    pub field: &'a str,
    pub occur: &'a str,
    pub dur: &'a str,
    pub state: &'a str,
    pub data: &'a str,
    pub following_state: &'a str,
    /// **Deleted-ness, not the deletion instant.** `deleted_at` is a
    /// timestamp, but hashing its value would give two devices that delete
    /// the same entry independently two different hashes for the same
    /// outcome — and would change a tombstone's hash if the deletion time
    /// were ever rewritten. Only the null/non-null distinction is content.
    pub deleted: bool,
}

/// SHA-256 over the NUL-joined [`HashInputs`], lowercase hex.
pub fn content_hash(inputs: &HashInputs) -> String {
    let parts: [&str; 7] = [
        inputs.field,
        inputs.occur,
        inputs.dur,
        inputs.state,
        inputs.data,
        inputs.following_state,
        // A marker rather than the instant — see `HashInputs::deleted`.
        if inputs.deleted { "deleted" } else { "" },
    ];
    let mut h = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            h.update(b"\x00");
        }
        h.update(part.as_bytes());
    }
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn hid_is_base36_and_hid_len_long() {
        let hid = hid_from_bytes(&[0, 9, 10, 35, 36, 255, 128, 7, 200, 3, 44, 91]);
        assert_eq!(hid.len(), HID_LEN);
        assert!(hid
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        // 0→'0', 9→'9', 10→'a', 35→'z', 36→'0' (wraps).
        assert!(hid.starts_with("09az0"));
    }

    #[test]
    fn a_timed_entry_slugs_on_its_occur_time_to_the_millisecond() {
        let t = DatumTime::from_utc_and_tz(at("2026-03-16T09:15:04.250Z"), "UTC").unwrap();
        let slug = build_slug("coffee", Some(&t), None, &at("2026-03-16T11:00:00Z"));
        assert_eq!(slug, "coffee:2026-03-16T09:15:04.250Z");
    }

    /// da-4fyb: a date-only row keys on the bare *local* date. Berlin
    /// midnight is the previous UTC day, so rendering `utc` would file the
    /// entry under the wrong date.
    #[test]
    fn a_date_only_entry_slugs_on_its_local_date() {
        let t = DatumTime::from_utc_and_tz(at("2026-08-31T22:00:00Z"), "Europe/Berlin").unwrap();
        let slug = build_slug(
            "coffee",
            Some(&t),
            Some(DATE_ONLY_PRECISION),
            &at("2026-09-01T08:00:00Z"),
        );
        assert_eq!(slug, "coffee:2026-09-01");
    }

    /// A non-`P1D` precision is an `±N` uncertainty, not a date-only
    /// marker: the stamp stays full-resolution.
    #[test]
    fn another_precision_keeps_the_full_stamp() {
        let t = DatumTime::from_utc_and_tz(at("2026-03-16T09:15:00Z"), "UTC").unwrap();
        let slug = build_slug(
            "coffee",
            Some(&t),
            Some("PT5M"),
            &at("2026-03-16T11:00:00Z"),
        );
        assert_eq!(slug, "coffee:2026-03-16T09:15:00.000Z");
    }

    /// da-r17h / da-xbjy: the timeless marker is a `c` *prefix* on the
    /// create time, not a suffix and not the retired `:unset`.
    #[test]
    fn a_timeless_entry_slugs_on_its_create_time_behind_a_c_marker() {
        let slug = build_slug("word", None, None, &at("2026-03-16T11:00:00.125Z"));
        assert_eq!(slug, "word:c2026-03-16T11:00:00.125Z");
    }

    #[test]
    fn the_hash_separates_its_parts_so_a_shift_between_columns_changes_it() {
        let inputs = |field: &'static str, state: &'static str| {
            content_hash(&HashInputs {
                field,
                occur: "",
                dur: "",
                state,
                data: "",
                following_state: "",
                deleted: false,
            })
        };
        assert_ne!(inputs("ab", ""), inputs("a", "b"));
    }

    #[test]
    fn deleted_ness_is_a_marker_not_an_instant() {
        let one = |deleted| {
            content_hash(&HashInputs {
                field: "coffee",
                occur: "",
                dur: "",
                state: "",
                data: "",
                following_state: "",
                deleted,
            })
        };
        assert_ne!(one(true), one(false));
    }
}
