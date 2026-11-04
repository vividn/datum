//! Id-space utilities — canonical port of datum's `src/ids/`.
//!
//! Today this module owns the quickId resolver (see [`resolve`]). Other
//! id-layer concerns (min-human-id shortening, id-structure assembly) live
//! in their own modules in the original TS tree and will be ported in
//! follow-up beads.

pub mod resolve;
pub mod types;

pub use resolve::{
    contains_wildcard, parse_underscore_ordinal, quick_ordinals, resolve_quickid,
    resolve_quickid_all, resolve_quickid_single, QuickRef,
};
pub use types::{Hid, Slug};

/// A new ULID, 26 characters of Crockford base32: 48 bits of Unix
/// milliseconds over 80 random ones, so ids sort by when they were made.
pub fn new_ulid() -> String {
    let millis = chrono::Utc::now().timestamp_millis() as u64;
    encode_ulid(millis, rand::random())
}

fn encode_ulid(millis: u64, random: u128) -> String {
    const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bits = u128::from(millis) << 80 | random >> 48;
    (0..26)
        .rev()
        .map(|i| CROCKFORD[(bits >> (i * 5) & 31) as usize] as char)
        .collect()
}

/// How deep the `_N` shorthand counts. Two digits is where it stops being
/// something anyone reaches for — past `_99` the hid sitting in the next
/// column is the handle you'd actually type — so the display surfaces stop
/// offering a short form there.
///
/// Ungated, unlike [`quick_ordinals`], because the browser assigns the same
/// ordinals over the rows it holds rather than over a database (da-6ec2),
/// and one cap has to govern both.
pub const QUICK_ORDINAL_CAP: usize = 99;

#[cfg(test)]
mod ulid_tests {
    use super::{encode_ulid, new_ulid};

    #[test]
    fn a_new_ulid_has_the_shape_and_says_when() {
        let before = chrono::Utc::now().timestamp_millis();
        let (a, b) = (new_ulid(), new_ulid());
        let after = chrono::Utc::now().timestamp_millis();
        assert_ne!(a, b);
        assert_eq!(a.len(), 26);
        const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        assert!(a.chars().all(|c| CROCKFORD.contains(c)), "{a}");
        let millis = a[..10]
            .chars()
            .fold(0i64, |n, c| n * 32 + CROCKFORD.find(c).unwrap() as i64);
        assert!((before..=after).contains(&millis), "{a} says {millis}");
    }

    /// What the `ulid` crate printed for these parts, before datum wrote its own.
    #[test]
    fn the_encoding_is_the_crates() {
        assert_eq!(
            encode_ulid(1_469_918_176_385, 0x0123_4567_89ab_cdef_0123 << 48),
            "01ARYZ6S4104HMASW9NF6YY093"
        );
    }
}
