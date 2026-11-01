//! Property tests for the color algorithm port, over a seeded loop.
//!
//! These tests don't require the `parity-runtime` feature — they verify
//! invariants that any RFC-1321 md5 implementation guarantees, plus the
//! branch coverage of `state_color` against known-good reference outputs.
//!
//! ≥200 cases per property satisfies the bead's CI fuzz floor. The seed is
//! fixed, so a failure names an input that fails on every run.

use datum_parity::colors::{
    contrast_text_color, field_color, md5_color, parse_hex_rgb, state_color,
};

const CASES: usize = 500;
const SEED: u64 = 0x6461_7964_756d_6200;

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// Any string: half of them ASCII, the others from all of Unicode, and
    /// up to 80 characters so that some pass md5's 64-byte block.
    fn any_string(&mut self) -> String {
        let ascii = self.below(2) == 0;
        let limit = if ascii { 0x80 } else { 0x11_0000 };
        (0..self.below(81))
            .filter_map(|_| char::from_u32(self.below(limit) as u32))
            .collect()
    }

    /// `[a-z][a-z0-9_]{0,15}`
    fn name(&mut self) -> String {
        const REST: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_";
        let first = char::from(b'a' + self.below(26) as u8);
        let rest: String = (0..self.below(16))
            .map(|_| char::from(REST[self.below(REST.len() as u64) as usize]))
            .collect();
        format!("{first}{rest}")
    }

    /// `#[0-9a-f]{6}`
    fn hex(&mut self) -> String {
        format!("#{:06x}", self.below(0x100_0000))
    }

    fn maybe(&mut self, of: impl FnOnce(&mut Self) -> String) -> Option<String> {
        (self.below(2) == 0).then(|| of(self))
    }
}

/// `property` holds for `CASES` inputs of `input`; a failure names the input.
fn holds<T: std::fmt::Debug>(input: impl Fn(&mut Rng) -> T, property: impl Fn(&T) -> bool) {
    let mut rng = Rng(SEED);
    for case in 0..CASES {
        let input = input(&mut rng);
        assert!(property(&input), "case {case} fails on {input:?}");
    }
}

/// A field, its spec color and its state's, each there or not.
fn field_and_specs(rng: &mut Rng) -> (Option<String>, Option<String>, Option<String>) {
    (
        rng.maybe(Rng::name),
        rng.maybe(Rng::hex),
        rng.maybe(Rng::hex),
    )
}

/// `md5_color` always produces a 7-character `#` + 6-hex string.
#[test]
fn md5_color_always_valid_hex() {
    holds(Rng::any_string, |input| {
        let c = md5_color(input);
        c.len() == 7 && c.starts_with('#') && parse_hex_rgb(&c).is_some()
    });
}

/// Determinism: same input → same output, every time.
#[test]
fn md5_color_is_deterministic() {
    holds(Rng::any_string, |input| {
        md5_color(input) == md5_color(input)
    });
}

/// `field_color` with `None` always returns white, regardless of
/// whether a spec color was passed (consistent with datum's
/// `getFieldColor(undefined) = "#ffffff"` short-circuit).
#[test]
fn field_color_none_is_white() {
    holds(
        |rng| rng.maybe(Rng::hex),
        |spec| field_color(None, spec.as_deref()) == "#ffffff",
    );
}

/// Spec override always wins over md5 for non-None field.
#[test]
fn field_color_spec_overrides_md5() {
    holds(
        |rng| (rng.name(), rng.hex()),
        |(field, spec)| field_color(Some(field), Some(spec)) == *spec,
    );
}

/// `state_color(false)` is always `#000000`, regardless of field
/// or spec context (datum's `getStateColor` hard-codes this).
#[test]
fn state_color_false_is_black() {
    holds(field_and_specs, |(field, field_spec, state_spec)| {
        state_color(
            Some("false"),
            field.as_deref(),
            field_spec.as_deref(),
            state_spec.as_deref(),
        ) == "#000000"
    });
}

/// `state_color(null)` is always `#888888` (datum's IGNORED gray).
#[test]
fn state_color_null_is_gray() {
    holds(field_and_specs, |(field, field_spec, state_spec)| {
        state_color(
            Some("null"),
            field.as_deref(),
            field_spec.as_deref(),
            state_spec.as_deref(),
        ) == "#888888"
    });
}

/// `state_color(named)` with a state-spec override wins over md5
/// AND over the field-color path. This is the algorithm path that
/// da-438y broke.
#[test]
fn state_color_state_spec_wins() {
    holds(
        |rng| (rng.name(), rng.name(), rng.hex(), rng.hex()),
        |(field, state, field_spec, state_spec)| {
            state_color(Some(state), Some(field), Some(field_spec), Some(state_spec)) == *state_spec
        },
    );
}

/// `state_color(true)` resolves to the field color (spec OR md5 of
/// field name), NOT to md5("true"). Branch order matters here:
/// datum's `simplifyState` collapses `true` to the field color
/// before the named-state branch runs.
#[test]
fn state_color_true_falls_through_to_field() {
    holds(Rng::name, |field| {
        // md5_color keeps 24 bits, so a field can share true's color; the
        // inequality can't tell those apart.
        md5_color(field) == md5_color("true")
            || state_color(Some("true"), Some(field), None, None) == md5_color(field)
    });
}

/// The field that once shared true's color (#b326b5), which a loop over 24
/// bits would not come across again.
#[test]
fn state_color_true_falls_through_to_a_field_sharing_its_color() {
    assert_eq!(md5_color("j8_u_"), md5_color("true"));
    assert_eq!(
        state_color(Some("true"), Some("j8_u_"), None, None),
        md5_color("j8_u_")
    );
}

/// `contrast_text_color` always returns either pure black or pure
/// white, never any other color in the default-message branch.
#[test]
fn contrast_is_binary() {
    holds(Rng::hex, |hex| {
        let fg = contrast_text_color(hex);
        fg == "#000000" || fg == "#ffffff"
    });
}

/// `contrast_text_color` is monotonic in brightness: brighter
/// backgrounds get black foregrounds, darker ones get white. We
/// can't easily express this with random hex, but we can pin
/// either-way for the boundary cases.
#[test]
fn contrast_chooses_black_for_pure_white() {
    assert_eq!(contrast_text_color("#ffffff"), "#000000");
    assert_eq!(contrast_text_color("#000000"), "#ffffff");
}
