//! `update`'s echo says what changed (da-27x6p): each part that changed as
//! `old → new`, the old dim and struck, the new in UPDATE's color, and
//! every part that did not change dim.
//!
//! ```text
//! UPDATE gtvoz coffee:2026-10-28T08:00:00.000Z
//! 09:00:00+1 → 09:30:00+1  coffee ●
//!   size: "large" → "small"
//!   + milk: "oat"
//!   − cups: 2
//!   shop: "corner"
//! ```
//!
//! The changed lines come first, then the unchanged. One list of [`Part`]s
//! is drawn for the terminal and carried as `changes` under `--show json`,
//! so the two cannot disagree.

use datum_core::{entry::Entry, field_spec::SpecRegistry, time::DatumTime};

use super::action::{info_parts, RenderContext};
use super::blocks::{data_inline, data_pairs, format_state_doc_line};
use super::colors::{ansi_segments, paint, strip_ansi, ColorOverride};
use super::{ActionLines, Outcome};

const DIM: &str = "\x1b[2m";
const STRUCK: &str = "\x1b[2;9m";

/// One part of an entry as it was and as it is now, each painted as the
/// entry's own echo paints it, `None` where that side has none.
struct Part {
    /// `time`, `field`, `state`, `dur` (the info line's), `state_line` (a
    /// structured state's line under it) or `data`.
    name: &'static str,
    /// The data key, for a `data` part.
    key: Option<String>,
    before: Option<String>,
    after: Option<String>,
}

/// A part's text, ANSI and a state group's edge slots dropped.
fn text(s: &Option<String>) -> Option<String> {
    s.as_deref().map(|s| strip_ansi(s).trim().to_string())
}

/// A part that changed, ANSI-free, for `--show json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub part: &'static str,
    pub key: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl Part {
    fn changed(&self) -> bool {
        text(&self.before) != text(&self.after)
    }

    /// The field badge and a state keep their own colors; a time, a
    /// duration or a data value is painted UPDATE's.
    fn painted_new(&self, s: &str) -> String {
        match self.name {
            "field" | "state" | "state_line" => s.to_string(),
            _ => paint(Outcome::Update.ansi_color(), &strip_ansi(s)),
        }
    }

    /// `old → new`, `old` or `new` for a changed part; the part dim if not.
    fn draw(&self) -> String {
        let was = |s: &str| paint(STRUCK, strip_ansi(s).trim_end());
        match (self.changed(), &self.before, &self.after) {
            (false, _, Some(a)) => paint(DIM, &strip_ansi(a)),
            (true, Some(b), Some(a)) => {
                // A state group's lead slot is a space of its own.
                let to = if strip_ansi(a).starts_with(' ') {
                    " →"
                } else {
                    " → "
                };
                format!("{}{to}{}", was(b), self.painted_new(a))
            }
            (_, _, Some(a)) => self.painted_new(a),
            (_, Some(b), None) => was(b),
            (_, None, None) => String::new(),
        }
    }

    /// The part as a line under the info line: a key changed, added (`+`),
    /// removed (`−`) or kept.
    fn draw_line(&self) -> String {
        let key = self
            .key
            .as_deref()
            .map(|k| format!("{k}: "))
            .unwrap_or_default();
        let was = |s: &str| paint(STRUCK, &format!("− {key}{}", strip_ansi(s)));
        let body = match (self.changed(), &self.before, &self.after) {
            (false, _, Some(a)) => paint(DIM, &format!("{key}{}", strip_ansi(a))),
            (true, Some(b), Some(a)) => {
                format!(
                    "{key}{} → {}",
                    paint(STRUCK, &strip_ansi(b)),
                    self.painted_new(a)
                )
            }
            (_, None, Some(a)) => self.painted_new(&format!("+ {key}{a}")),
            (_, Some(b), None) => was(b),
            (_, None, None) => String::new(),
        };
        format!("  {body}")
    }
}

/// The parts of `before` → `after`: the info line's four, then the lines
/// under it, the changed ones first.
fn parts(
    before: &Entry,
    after: &Entry,
    registry: Option<&SpecRegistry>,
    now: &DatumTime,
) -> (Vec<Part>, Vec<Part>) {
    let spec = |e: &Entry| registry.and_then(|r| r.fields.get(&e.field));
    let ctx = RenderContext::default();
    let [b, a] = [before, after].map(|e| info_parts(e, spec(e), &ctx, now));
    let info = ["time", "field", "state", "dur"]
        .into_iter()
        .zip(b.into_iter().zip(a))
        .map(|(name, (before, after))| Part {
            name,
            key: None,
            before,
            after,
        })
        .collect();

    let state_line = |e: &Entry| {
        e.state
            .as_ref()
            .and_then(|v| format_state_doc_line(v, &e.field, spec(e)))
    };
    let mut lines = vec![Part {
        name: "state_line",
        key: None,
        before: state_line(before),
        after: state_line(after),
    }];
    let [b, a] = [before, after].map(|e| data_pairs(e).unwrap_or_default());
    let value = |pairs: &[(String, String)], k: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    let keys = a
        .iter()
        .chain(b.iter().filter(|(k, _)| value(&a, k).is_none()));
    lines.extend(keys.map(|(k, _)| Part {
        name: "data",
        key: Some(k.clone()),
        before: value(&b, k),
        after: value(&a, k),
    }));
    lines.retain(|p| p.before.is_some() || p.after.is_some());
    lines.sort_by_key(|p| !p.changed());
    (info, lines)
}

fn draw(parts: &(Vec<Part>, Vec<Part>)) -> (String, Vec<String>) {
    let info = parts.0.iter().map(Part::draw).filter(|s| !s.is_empty());
    (
        info.collect::<Vec<_>>().join(" "),
        parts.1.iter().map(Part::draw_line).collect(),
    )
}

/// Turn `lines`, `after`'s UPDATE echo, into the echo of `before` →
/// `after`: its info line and data view drawn as the diff, `changes` set.
pub(crate) fn apply(
    lines: &mut ActionLines,
    before: &Entry,
    after: &Entry,
    registry: Option<&SpecRegistry>,
    now: &DatumTime,
) {
    let plain = parts(before, after, registry, now);
    let (info, data) = draw(&plain);
    // Painted again with the gate open, as `action_lines_at` does, so the
    // segments' texts are `info` stripped.
    let painted = {
        let _on = ColorOverride::set(true);
        draw(&parts(before, after, registry, now)).0
    };
    lines.info = info;
    // The stamp `info` starts with is the time part as drawn (da-7xp82):
    // `09:00:00+1 → 09:30:00+1` when it moved.
    lines.info_time = strip_ansi(&plain.0[0].draw());
    lines.info_segments = ansi_segments(&painted);
    lines.data_items = data
        .iter()
        .map(|l| strip_ansi(l.strip_prefix("  ").unwrap_or(l)))
        .collect();
    lines.data_inline = data_inline(&lines.data_items);
    lines.data = data;
    let changes = plain.0.iter().chain(&plain.1).filter(|p| p.changed());
    lines.changes = Some(
        changes
            .map(|p| Change {
                part: p.name,
                key: p.key.clone(),
                before: text(&p.before),
                after: text(&p.after),
            })
            .collect(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::entry::Verb;
    use serde_json::json;

    fn at(hm: &str) -> DatumTime {
        DatumTime {
            utc: format!("2024-01-01T{hm}:00Z").parse().unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        }
    }

    fn coffee(data: serde_json::Value) -> Entry {
        Entry::new(
            "coffee",
            Verb::Point,
            None,
            Some(data),
            Some(at("09:00")),
            None,
            "d1",
        )
    }

    /// The echo of `before` → `after`, color forced as `on` says.
    fn echo(before: &Entry, after: &Entry, on: bool) -> ActionLines {
        let _gate = ColorOverride::set(on);
        super::super::update_lines_at(before, after, None, &at("12:00"))
    }

    #[test]
    fn a_changed_value_is_old_to_new_and_comes_before_the_unchanged() {
        let before = coffee(json!({"shop": "corner", "size": "large"}));
        let mut after = before.clone();
        after.data = Some(json!({"shop": "corner", "size": "small"}));
        let lines = echo(&before, &after, false);
        assert_eq!(lines.info, "09:00:00+0  coffee ●");
        assert_eq!(lines.info_time, "09:00:00+0");
        assert_eq!(
            lines.data,
            ["  size: \"large\" → \"small\"", "  shop: \"corner\""]
        );
        assert_eq!(
            lines.data_items,
            ["size: \"large\" → \"small\"", "shop: \"corner\""]
        );
        assert_eq!(
            lines.changes.unwrap(),
            [Change {
                part: "data",
                key: Some("size".into()),
                before: Some("\"large\"".into()),
                after: Some("\"small\"".into()),
            }]
        );
    }

    #[test]
    fn a_key_added_is_plus_and_a_key_removed_is_minus() {
        let before = coffee(json!({"shop": "corner", "cups": 2}));
        let after = coffee(json!({"cups": 2, "milk": "oat"}));
        let lines = echo(&before, &after, false);
        assert_eq!(
            lines.data,
            ["  + milk: \"oat\"", "  − shop: \"corner\"", "  cups: 2"]
        );
        let changes = lines.changes.unwrap();
        assert_eq!(changes[0].before, None);
        assert_eq!(changes[1].after, None);
    }

    #[test]
    fn a_state_changed_is_old_to_new_on_the_info_line() {
        let before = Entry::new(
            "mood",
            Verb::Switch,
            Some(json!("calm")),
            None,
            Some(at("09:10")),
            None,
            "d1",
        );
        let mut after = before.clone();
        after.state = Some(json!("tired"));
        let lines = echo(&before, &after, false);
        assert_eq!(lines.info.trim_end(), "09:10:00+0  mood  calm → tired");
        let change = &lines.changes.unwrap()[0];
        assert_eq!(
            (change.part, change.before.as_deref()),
            ("state", Some("calm"))
        );
        assert_eq!(change.after.as_deref(), Some("tired"));
    }

    #[test]
    fn a_moved_time_is_old_to_new_on_the_info_line() {
        let before = coffee(json!({"shop": "corner"}));
        let mut after = before.clone();
        after.occur_time = Some(at("09:30"));
        let lines = echo(&before, &after, false);
        assert_eq!(lines.info, "09:00:00+0 → 09:30:00+0  coffee ●");
        assert_eq!(lines.info_time, "09:00:00+0 → 09:30:00+0");
        assert_eq!(lines.data, ["  shop: \"corner\""]);
    }

    /// On a terminal the old is dim and struck, the new UPDATE's cyan, and
    /// what did not change dim; the segments say the same as the line.
    #[test]
    fn color_strikes_the_old_paints_the_new_and_dims_the_rest() {
        let before = coffee(json!({"shop": "corner", "size": "large"}));
        let mut after = before.clone();
        after.data = Some(json!({"shop": "corner", "size": "small", "milk": "oat"}));
        after.occur_time = Some(at("09:30"));
        let lines = echo(&before, &after, true);
        let cyan = Outcome::Update.ansi_color();
        assert_eq!(
            lines.data,
            [
                format!("  {cyan}+ milk: \"oat\"\x1b[0m"),
                format!("  size: {STRUCK}\"large\"\x1b[0m → {cyan}\"small\"\x1b[0m"),
                format!("  {DIM}shop: \"corner\"\x1b[0m"),
            ]
        );
        assert!(lines.info.starts_with(&format!(
            "{STRUCK}09:00:00+0\x1b[0m → {cyan}09:30:00+0\x1b[0m {DIM}"
        )));
        let segments: String = lines
            .info_segments
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(segments, strip_ansi(&lines.info));

        let plain = echo(&before, &after, false);
        assert!(!plain.info.contains('\x1b') && !plain.data.concat().contains('\x1b'));
        assert_eq!(strip_ansi(&lines.info), plain.info);
        assert_eq!(
            lines.data.iter().map(|l| strip_ansi(l)).collect::<Vec<_>>(),
            plain.data
        );
    }
}
