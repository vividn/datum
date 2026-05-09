//! Anchored layout — resolves a `(y, height, z_index)` position for every
//! field in a [`FieldSpec`] map, honoring `relative_to` chains via
//! topological sort. Anchors that are missing, malformed, or part of a
//! cycle fall back to absolute (hash/default) resolution and surface as
//! diagnostics on the returned [`PositionResolution`].

use std::collections::HashMap;

use crate::action::Action;

use super::spec::{FieldSpec, ResolvedPosition};

/// Outcome of [`resolve_positions`]: per-field resolved triples plus
/// diagnostics surfaced for the caller (renderer / install command) to log.
#[derive(Debug, Clone, Default)]
pub struct PositionResolution {
    /// Resolved (y, height, z_index) per field name. Includes every input
    /// field — anchored fields whose anchor was missing or cyclic fall back
    /// to absolute defaults so callers always get a position.
    pub positions: HashMap<String, ResolvedPosition>,
    /// `(dependent_field, missing_anchor_name)` pairs. The dependent fell
    /// back to absolute. Empty when every anchor resolved.
    pub missing_anchors: Vec<(String, String)>,
    /// Anchor cycles. Each inner vec is the cycle path closed at both ends,
    /// e.g. `["a", "b", "c", "a"]`. Cycle members all fall back to absolute.
    pub cycles: Vec<Vec<String>>,
    /// Anchor-triple invariant violations (some of relative_to / pos_ratio /
    /// size_ratio set, others not). The offending field is treated as
    /// absolute.
    pub malformed: Vec<String>,
}

/// Resolve positions for a set of fields, honoring `relative_to` anchors
/// via topological sort. Anchors not present in `specs` (or whose triple is
/// malformed, or whose chain forms a cycle) are reported as diagnostics
/// while the dependent falls back to absolute resolution.
///
/// Resolution rules:
/// - Absolute (no `relative_to`): `y`/`height`/`z_index` with hash/default
///   fallback (same as [`FieldSpec::resolve_position`]).
/// - Anchored: `effective_y = anchor.y + anchor.height * anchor_pos_ratio`,
///   `effective_height = anchor.height * anchor_size_ratio`,
///   `z_index = self.z_index.unwrap_or(effective_y)` (anchored fields
///   default to rendering above their anchor since `effective_y` differs).
/// - Out-of-`[0, 1]` ratios are allowed and not clamped — useful for stacked
///   layouts above/below an anchor.
pub fn resolve_positions(
    specs: &HashMap<String, FieldSpec>,
    fields: impl IntoIterator<Item = String>,
) -> PositionResolution {
    let requested: Vec<String> = fields.into_iter().collect();
    let mut report = PositionResolution::default();

    // Detect cycles up front over the *full* spec graph so we know which
    // fields to short-circuit. Visiting only requested-field roots would
    // miss cycles that lie entirely in chained anchors not directly asked
    // for.
    let cycles = detect_anchor_cycles(specs);
    let mut in_cycle: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in &cycles {
        // Skip the duplicated tail when collecting members.
        for name in c.iter().take(c.len().saturating_sub(1)) {
            in_cycle.insert(name.clone());
        }
    }
    report.cycles = cycles;

    // Detect malformed anchor triples once across the full spec set.
    let mut malformed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, spec) in specs {
        if spec.has_anchor() && spec.validate_anchor_triple().is_err() {
            malformed.insert(name.clone());
            report.malformed.push(name.clone());
        }
    }

    // Memoize so a chain visited from multiple roots only resolves once.
    let mut memo: HashMap<String, ResolvedPosition> = HashMap::new();
    let mut warned: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    // Cover both explicitly-requested fields and any spec mentioned via an
    // anchor chain — so dayview can hand us only the fields it has data for
    // and we still resolve their anchors transitively.
    let mut roots: Vec<String> = requested;
    roots.extend(specs.keys().cloned());
    roots.sort();
    roots.dedup();

    for root in roots {
        resolve_one(
            &root,
            specs,
            &in_cycle,
            &malformed,
            &mut memo,
            &mut warned,
            &mut report,
        );
    }

    report.positions = memo;
    report
}

/// Recursive walker for [`resolve_positions`]. Anchors are resolved
/// depth-first; results are memoized so each field resolves once. Fields in
/// a cycle, malformed, or whose anchor is missing fall back to the absolute
/// path via [`FieldSpec::resolve_position`] (or — when no spec exists —
/// the hash/default defaults synthesized via a transient default spec).
fn resolve_one(
    field: &str,
    specs: &HashMap<String, FieldSpec>,
    in_cycle: &std::collections::HashSet<String>,
    malformed: &std::collections::HashSet<String>,
    memo: &mut HashMap<String, ResolvedPosition>,
    warned: &mut std::collections::HashSet<(String, String)>,
    report: &mut PositionResolution,
) -> ResolvedPosition {
    if let Some(p) = memo.get(field) {
        return *p;
    }

    let spec = specs.get(field);

    // Path 1: no spec at all → hash/default fallback.
    let Some(spec) = spec else {
        let pos = FieldSpec::new(field, Action::Occur).resolve_position(field);
        memo.insert(field.to_string(), pos);
        return pos;
    };

    // Path 2: in a cycle or malformed → absolute fallback. Cycles are
    // already collected in `report.cycles`; malformed is in `report.malformed`.
    if in_cycle.contains(field) || malformed.contains(field) {
        let pos = spec.resolve_position(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Path 3: not anchored → absolute resolution. The triple invariant is
    // already enforced (we'd be in `malformed` otherwise); has_anchor()
    // covers the well-formed-anchored case below.
    if spec.relative_to.is_none() {
        let pos = spec.resolve_position(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Path 4: well-formed anchor. Look up the anchor; missing → warn + fall
    // back to absolute. Triple invariant guarantees both ratios are Some.
    let anchor_name = spec.relative_to.as_deref().unwrap();
    let pos_ratio = spec.anchor_pos_ratio.unwrap();
    let size_ratio = spec.anchor_size_ratio.unwrap();

    if !specs.contains_key(anchor_name) {
        // First time we've seen this missing anchor for this dependent —
        // record once.
        let key = (field.to_string(), anchor_name.to_string());
        if warned.insert(key) {
            report
                .missing_anchors
                .push((field.to_string(), anchor_name.to_string()));
        }
        let pos = spec.resolve_position(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Recurse for the anchor's resolved triple. Since cycles are pre-detected,
    // recursion terminates.
    let anchor_pos = resolve_one(
        anchor_name,
        specs,
        in_cycle,
        malformed,
        memo,
        warned,
        report,
    );

    let y = anchor_pos.y + anchor_pos.height * pos_ratio;
    let height = anchor_pos.height * size_ratio;
    let z_index = spec.z_index.unwrap_or(y);
    let resolved = ResolvedPosition { y, height, z_index };
    memo.insert(field.to_string(), resolved);
    resolved
}

/// Detect cycles in the (field → relative_to) graph. Returns a Vec of
/// cycle paths; each path is a Vec of field names with the start name
/// repeated at the end (e.g. `["a", "b", "a"]`). Empty when acyclic.
///
/// Implementation: DFS-based three-color scan (white = unvisited, gray =
/// on the current path, black = fully explored). When a gray node is
/// revisited we slice the current path from the first occurrence to record
/// the cycle.
pub fn detect_anchor_cycles(specs: &HashMap<String, FieldSpec>) -> Vec<Vec<String>> {
    use std::collections::HashSet;

    let mut color: HashMap<String, DfsColor> =
        specs.keys().map(|k| (k.clone(), DfsColor::White)).collect();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut seen_cycle_keys: HashSet<Vec<String>> = HashSet::new();

    // Iterate roots in sorted order so detected cycles come back in a
    // deterministic order (kit install error messages and tests).
    let mut roots: Vec<String> = specs.keys().cloned().collect();
    roots.sort();

    for root in roots {
        if matches!(color.get(&root).copied(), Some(DfsColor::White)) {
            let mut path: Vec<String> = Vec::new();
            dfs_anchor(
                &root,
                specs,
                &mut color,
                &mut path,
                &mut cycles,
                &mut seen_cycle_keys,
            );
        }
    }
    cycles
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum DfsColor {
    White,
    Gray,
    Black,
}

fn dfs_anchor(
    node: &str,
    specs: &HashMap<String, FieldSpec>,
    color: &mut HashMap<String, DfsColor>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
    seen_cycle_keys: &mut std::collections::HashSet<Vec<String>>,
) {
    color.insert(node.to_string(), DfsColor::Gray);
    path.push(node.to_string());

    if let Some(spec) = specs.get(node) {
        if let Some(anchor) = spec.relative_to.as_deref() {
            // Edge skipped if the anchor is missing — that's a missing-anchor
            // diagnostic, not a cycle.
            if let Some(c) = color.get(anchor).copied() {
                match c {
                    DfsColor::White => {
                        dfs_anchor(anchor, specs, color, path, cycles, seen_cycle_keys);
                    }
                    DfsColor::Gray => {
                        // Cycle: slice from the first occurrence of `anchor`
                        // in `path` and append `anchor` to close the loop.
                        if let Some(idx) = path.iter().position(|n| n == anchor) {
                            let mut cycle: Vec<String> = path[idx..].to_vec();
                            cycle.push(anchor.to_string());
                            // Canonicalize so the same cycle reached from
                            // different start points dedups.
                            let key = canonical_cycle_key(&cycle);
                            if seen_cycle_keys.insert(key) {
                                cycles.push(cycle);
                            }
                        }
                    }
                    DfsColor::Black => { /* fully explored — no cycle */ }
                }
            }
        }
    }

    path.pop();
    color.insert(node.to_string(), DfsColor::Black);
}

/// Canonical key for a cycle path so two rotations of the same cycle dedup.
/// Drops the closing duplicate, rotates the path so the lexicographically
/// smallest member is first, and returns the rotated names.
fn canonical_cycle_key(cycle: &[String]) -> Vec<String> {
    let body = if cycle.first() == cycle.last() && cycle.len() > 1 {
        &cycle[..cycle.len() - 1]
    } else {
        cycle
    };
    if body.is_empty() {
        return Vec::new();
    }
    let min_idx = body
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.as_str())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut out: Vec<String> = Vec::with_capacity(body.len());
    out.extend_from_slice(&body[min_idx..]);
    out.extend_from_slice(&body[..min_idx]);
    out
}

/// Format a cycle path as `A → B → C → A` for user-facing error messages.
/// Mirrors the format expected by the kit-install error message.
pub fn format_cycle(cycle: &[String]) -> String {
    cycle.join(" → ")
}

#[cfg(test)]
mod tests {
    use super::super::spec::{hash_y, DEFAULT_FIELD_HEIGHT};
    use super::*;

    fn anchored_spec(name: &str, anchor: &str, pos_ratio: f64, size_ratio: f64) -> FieldSpec {
        let mut spec = FieldSpec::new(name, Action::Occur);
        spec.relative_to = Some(anchor.into());
        spec.anchor_pos_ratio = Some(pos_ratio);
        spec.anchor_size_ratio = Some(size_ratio);
        spec
    }

    fn absolute_spec(name: &str, y: f64, h: f64) -> FieldSpec {
        let mut spec = FieldSpec::new(name, Action::Occur);
        spec.y = Some(y);
        spec.height = Some(h);
        spec
    }

    #[test]
    fn resolve_positions_chains_anchor_a_b_c() {
        // body absolute; contacts anchored to body; thumbnail anchored to
        // contacts. Resolution order doesn't matter — topological walk.
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.20, 0.40));
        specs.insert(
            "contacts".into(),
            anchored_spec("contacts", "body", 0.10, 0.05),
        );
        specs.insert(
            "thumbnail".into(),
            anchored_spec("thumbnail", "contacts", 0.5, 0.5),
        );

        let report = resolve_positions(&specs, ["thumbnail".to_string()]);
        assert!(report.cycles.is_empty());
        assert!(report.missing_anchors.is_empty());

        let body = report.positions.get("body").unwrap();
        assert_eq!(body.y, 0.20);
        assert_eq!(body.height, 0.40);

        let contacts = report.positions.get("contacts").unwrap();
        // y = 0.20 + 0.40 * 0.10 = 0.24; height = 0.40 * 0.05 = 0.02
        assert!((contacts.y - 0.24).abs() < 1e-9);
        assert!((contacts.height - 0.02).abs() < 1e-9);

        let thumb = report.positions.get("thumbnail").unwrap();
        // y = 0.24 + 0.02 * 0.5 = 0.25; height = 0.02 * 0.5 = 0.01
        assert!((thumb.y - 0.25).abs() < 1e-9);
        assert!((thumb.height - 0.01).abs() < 1e-9);
    }

    #[test]
    fn resolve_positions_warns_and_falls_back_on_missing_anchor() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert(
            "contacts".into(),
            anchored_spec("contacts", "body", 0.10, 0.05),
        );
        let report = resolve_positions(&specs, ["contacts".to_string()]);
        assert_eq!(report.missing_anchors.len(), 1);
        assert_eq!(report.missing_anchors[0].0, "contacts");
        assert_eq!(report.missing_anchors[0].1, "body");
        // Falls back to absolute → hash-based y for "contacts"
        let pos = report.positions.get("contacts").unwrap();
        assert_eq!(pos.y, hash_y("contacts"));
        assert_eq!(pos.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn detect_anchor_cycles_finds_two_node_cycle() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.0, 1.0));
        specs.insert("b".into(), anchored_spec("b", "a", 0.0, 1.0));
        let cycles = detect_anchor_cycles(&specs);
        assert_eq!(cycles.len(), 1, "expected one cycle, got {cycles:?}");
        let c = &cycles[0];
        // First and last names match — cycle closes on itself.
        assert_eq!(c.first(), c.last());
    }

    #[test]
    fn detect_anchor_cycles_finds_three_node_cycle() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.0, 1.0));
        specs.insert("b".into(), anchored_spec("b", "c", 0.0, 1.0));
        specs.insert("c".into(), anchored_spec("c", "a", 0.0, 1.0));
        let cycles = detect_anchor_cycles(&specs);
        assert_eq!(cycles.len(), 1);
        // Body should contain all three names.
        let body: std::collections::HashSet<&str> = cycles[0]
            .iter()
            .take(cycles[0].len() - 1)
            .map(String::as_str)
            .collect();
        assert!(body.contains("a"));
        assert!(body.contains("b"));
        assert!(body.contains("c"));
    }

    #[test]
    fn resolve_positions_falls_back_for_cycle_members() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.5, 0.5));
        specs.insert("b".into(), anchored_spec("b", "a", 0.5, 0.5));
        let report = resolve_positions(&specs, ["a".to_string(), "b".to_string()]);
        assert_eq!(report.cycles.len(), 1);
        // Both members fall back to absolute (hash y, default height).
        let a = report.positions.get("a").unwrap();
        let b = report.positions.get("b").unwrap();
        assert_eq!(a.y, hash_y("a"));
        assert_eq!(b.y, hash_y("b"));
        assert_eq!(a.height, DEFAULT_FIELD_HEIGHT);
        assert_eq!(b.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn resolve_positions_allows_out_of_unit_ratios() {
        // Negative pos_ratio → field bleeds *above* its anchor; size_ratio > 1.0
        // → field is taller than its anchor. Both are intentional layouts and
        // must not be clamped.
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.50, 0.20));
        specs.insert("halo".into(), anchored_spec("halo", "body", -0.5, 1.5));
        let report = resolve_positions(&specs, ["halo".to_string()]);
        assert!(report.cycles.is_empty());
        let halo = report.positions.get("halo").unwrap();
        // y = 0.50 + 0.20 * -0.5 = 0.40; height = 0.20 * 1.5 = 0.30
        assert!((halo.y - 0.40).abs() < 1e-9);
        assert!((halo.height - 0.30).abs() < 1e-9);
    }

    #[test]
    fn resolve_positions_marks_malformed_triple_and_falls_back() {
        // anchor_pos_ratio set, anchor_size_ratio missing → malformed.
        let mut spec = FieldSpec::new("bad", Action::Occur);
        spec.relative_to = Some("body".into());
        spec.anchor_pos_ratio = Some(0.1);
        // Intentionally omit anchor_size_ratio.

        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.20, 0.40));
        specs.insert("bad".into(), spec);
        let report = resolve_positions(&specs, ["bad".to_string()]);
        assert!(report.malformed.iter().any(|n| n == "bad"));
        // Fell back to absolute (no y set on the spec → hash).
        let bad = report.positions.get("bad").unwrap();
        assert_eq!(bad.y, hash_y("bad"));
    }
}
