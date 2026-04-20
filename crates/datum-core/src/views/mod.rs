//! Named SQL views framework — map/reduce over entries.
//!
//! Modeled on datum's CouchDB views: each named view emits `(key, value)` rows
//! (map phase), optionally folds them into aggregates (reduce phase), and the
//! framework applies `key`/`startkey`/`endkey`/`limit`/`group`/`group_level`
//! semantics over the result.
//!
//! A `View` is stateless and owns its query. The `Registry` looks views up by
//! name. Call sites (CLI / web / etc.) wire a `Db` + `ViewParams` in.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

use crate::db::Db;
use crate::error::{DatumError, Result};

pub mod duration_sum;
pub mod state_change;
pub mod sub_human_id;
pub mod timing;

pub use duration_sum::DurationSumView;
pub use state_change::StateChangeView;
pub use sub_human_id::SubHumanIdView;
pub use timing::TimingView;

/// Registry pre-populated with every view shipped by datum-core.
///
/// Wire this into call sites (CLI, web, etc.) so `datum map <name>` sees every
/// built-in view without each call site having to enumerate them.
pub fn builtins() -> Registry {
    let mut reg = Registry::new();
    reg.insert(TimingView.name(), Box::new(TimingView));
    reg.insert(StateChangeView.name(), Box::new(StateChangeView));
    reg.insert(DurationSumView.name(), Box::new(DurationSumView));
    reg.insert(SubHumanIdView.name(), Box::new(SubHumanIdView));
    reg
}

/// A row emitted by a view's map or reduce phase.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewRow {
    pub key: JsonValue,
    pub value: JsonValue,
    /// Source entry id for map rows; `None` for reduced rows.
    pub id: Option<String>,
}

/// Query-time parameters, mirroring datum's view-query surface.
#[derive(Debug, Default, Clone)]
pub struct ViewParams {
    pub key: Option<JsonValue>,
    pub startkey: Option<JsonValue>,
    pub endkey: Option<JsonValue>,
    pub group: bool,
    pub group_level: Option<usize>,
    pub limit: Option<usize>,
    pub reduce: bool,
}

/// A named view: a deterministic query over the entry store.
///
/// Views own their own SQL. Default implementations of `supports_reduce` and
/// `reduce` reject reduce requests; views that reduce override them.
pub trait View: Send + Sync {
    fn name(&self) -> &str;

    /// Emit this view's map rows. The framework applies key filtering and
    /// limit after the view emits.
    fn map(&self, db: &Db) -> Result<Vec<ViewRow>>;

    /// Whether this view supports the reduce path.
    fn supports_reduce(&self) -> bool {
        false
    }

    /// Reduce a single group of `values` sharing `key` to one value. The
    /// framework handles grouping (via `group` / `group_level`) and calls
    /// this once per group. Default impl errors — override for reducing views.
    fn reduce(&self, _key: &JsonValue, _values: &[JsonValue]) -> Result<JsonValue> {
        Err(DatumError::Other(format!(
            "view {} does not support reduce",
            self.name()
        )))
    }
}

/// Registry mapping view name → view implementation.
pub struct Registry {
    views: BTreeMap<String, Box<dyn View>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            views: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, name: impl Into<String>, view: Box<dyn View>) {
        self.views.insert(name.into(), view);
    }

    pub fn get(&self, name: &str) -> Option<&dyn View> {
        self.views.get(name).map(|v| v.as_ref())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.views.keys().map(|s| s.as_str())
    }

    /// Run a view end-to-end: map → key filter → optional reduce/group → limit.
    pub fn query(
        &self,
        name: &str,
        db: &Db,
        params: &ViewParams,
    ) -> Result<Vec<ViewRow>> {
        let view = self
            .get(name)
            .ok_or_else(|| DatumError::NotFound(format!("view: {name}")))?;

        let mut rows = view.map(db)?;
        rows = filter_by_key(rows, params);
        rows.sort_by(|a, b| cmp_json(&a.key, &b.key));

        if params.reduce {
            if !view.supports_reduce() {
                return Err(DatumError::Other(format!(
                    "view {} does not support reduce",
                    view.name()
                )));
            }
            rows = reduce_rows(view, rows, params)?;
        }

        if let Some(n) = params.limit {
            rows.truncate(n);
        }
        Ok(rows)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Keep rows whose key satisfies `key` (exact) or `startkey`..=`endkey` (range).
pub fn filter_by_key(rows: Vec<ViewRow>, params: &ViewParams) -> Vec<ViewRow> {
    rows.into_iter()
        .filter(|r| {
            if let Some(k) = &params.key {
                return key_matches(&r.key, k);
            }
            if let Some(start) = &params.startkey {
                if cmp_json(&r.key, start) == std::cmp::Ordering::Less {
                    return false;
                }
            }
            if let Some(end) = &params.endkey {
                if cmp_json(&r.key, end) == std::cmp::Ordering::Greater {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Key match: equal, OR (datum/CouchDB convention) `row_key` is an array
/// that starts with `query_key`'s elements (prefix match on array keys).
fn key_matches(row_key: &JsonValue, query_key: &JsonValue) -> bool {
    if row_key == query_key {
        return true;
    }
    if let (JsonValue::Array(r), JsonValue::Array(q)) = (row_key, query_key) {
        if q.len() <= r.len() {
            return r.iter().zip(q.iter()).all(|(a, b)| a == b);
        }
    }
    false
}

/// Order JSON values for sorting / range comparisons, using CouchDB-ish
/// collation: Null < Bool < Number < String < Array < Object.
pub fn cmp_json(a: &JsonValue, b: &JsonValue) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    use JsonValue::*;
    fn rank(v: &JsonValue) -> u8 {
        match v {
            Null => 0,
            Bool(_) => 1,
            Number(_) => 2,
            String(_) => 3,
            Array(_) => 4,
            Object(_) => 5,
        }
    }
    match (a, b) {
        (Null, Null) => Equal,
        (Bool(x), Bool(y)) => x.cmp(y),
        (Number(x), Number(y)) => {
            let xf = x.as_f64().unwrap_or(0.0);
            let yf = y.as_f64().unwrap_or(0.0);
            xf.partial_cmp(&yf).unwrap_or(Equal)
        }
        (String(x), String(y)) => x.cmp(y),
        (Array(x), Array(y)) => {
            for (a, b) in x.iter().zip(y.iter()) {
                match cmp_json(a, b) {
                    Equal => continue,
                    o => return o,
                }
            }
            x.len().cmp(&y.len())
        }
        _ => rank(a).cmp(&rank(b)),
    }
}

/// Key used for grouping: `group=true` → full key; `group_level=N` → first N
/// array elements for array keys (non-array keys are kept whole); no grouping
/// → `Null` (one bucket → single reduced row with `key=null`).
fn group_key(key: &JsonValue, params: &ViewParams) -> JsonValue {
    if !params.group && params.group_level.is_none() {
        return JsonValue::Null;
    }
    if let Some(level) = params.group_level {
        if let JsonValue::Array(arr) = key {
            let take = level.min(arr.len());
            return JsonValue::Array(arr.iter().take(take).cloned().collect());
        }
    }
    key.clone()
}

fn reduce_rows(
    view: &dyn View,
    rows: Vec<ViewRow>,
    params: &ViewParams,
) -> Result<Vec<ViewRow>> {
    // Preserve first-seen order of group keys for deterministic output.
    let mut groups: Vec<(JsonValue, Vec<JsonValue>)> = Vec::new();
    for r in rows {
        let gk = group_key(&r.key, params);
        if let Some((_, values)) = groups.iter_mut().find(|(k, _)| k == &gk) {
            values.push(r.value);
        } else {
            groups.push((gk, vec![r.value]));
        }
    }

    groups
        .into_iter()
        .map(|(key, values)| {
            let reduced = view.reduce(&key, &values)?;
            Ok(ViewRow {
                key,
                value: reduced,
                id: None,
            })
        })
        .collect()
}

/// Parse a `--key` / `--startkey` / `--endkey` string into a JsonValue.
///
/// Tries JSON first (so `[2024,4,15]`, `"str"`, `42`, `null`, `true` all work),
/// falls back to treating the raw input as a bare string.
pub fn parse_key_arg(s: &str) -> JsonValue {
    serde_json::from_str(s).unwrap_or_else(|_| JsonValue::String(s.to_string()))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// In-memory test view that emits preset rows and sums numeric values on reduce.
    struct FixedView {
        name: String,
        rows: Vec<ViewRow>,
        reducible: bool,
    }

    impl View for FixedView {
        fn name(&self) -> &str {
            &self.name
        }
        fn map(&self, _db: &Db) -> Result<Vec<ViewRow>> {
            Ok(self.rows.clone())
        }
        fn supports_reduce(&self) -> bool {
            self.reducible
        }
        fn reduce(&self, _key: &JsonValue, values: &[JsonValue]) -> Result<JsonValue> {
            let sum: f64 = values.iter().filter_map(|v| v.as_f64()).sum();
            Ok(json!(sum))
        }
    }

    fn row(key: JsonValue, value: JsonValue, id: &str) -> ViewRow {
        ViewRow {
            key,
            value,
            id: Some(id.to_string()),
        }
    }

    #[test]
    fn registry_lookup_returns_view() {
        let mut reg = Registry::new();
        reg.insert(
            "demoView",
            Box::new(FixedView {
                name: "demoView".into(),
                rows: vec![row(json!("a"), json!(1), "id1")],
                reducible: false,
            }),
        );
        assert!(reg.get("demoView").is_some());
        assert_eq!(reg.get("demoView").unwrap().name(), "demoView");
        assert!(reg.get("missing").is_none());
        let names: Vec<&str> = reg.names().collect();
        assert_eq!(names, vec!["demoView"]);
    }

    #[test]
    fn map_with_key_filter() {
        let db = Db::open_in_memory().unwrap();
        let mut reg = Registry::new();
        reg.insert(
            "daily",
            Box::new(FixedView {
                name: "daily".into(),
                rows: vec![
                    row(json!("2026-04-14"), json!(1), "a"),
                    row(json!("2026-04-15"), json!(2), "b"),
                    row(json!("2026-04-15"), json!(3), "c"),
                    row(json!("2026-04-16"), json!(4), "d"),
                ],
                reducible: false,
            }),
        );

        // --key filter: only rows whose key equals "2026-04-15"
        let params = ViewParams {
            key: Some(json!("2026-04-15")),
            ..Default::default()
        };
        let got = reg.query("daily", &db, &params).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].value, json!(2));
        assert_eq!(got[1].value, json!(3));

        // --startkey/--endkey range
        let params = ViewParams {
            startkey: Some(json!("2026-04-15")),
            endkey: Some(json!("2026-04-16")),
            ..Default::default()
        };
        let got = reg.query("daily", &db, &params).unwrap();
        assert_eq!(got.len(), 3);

        // --limit truncates after filter
        let params = ViewParams {
            limit: Some(2),
            ..Default::default()
        };
        let got = reg.query("daily", &db, &params).unwrap();
        assert_eq!(got.len(), 2);

        // Array-key prefix match
        let mut reg2 = Registry::new();
        reg2.insert(
            "byDate",
            Box::new(FixedView {
                name: "byDate".into(),
                rows: vec![
                    row(json!([2026, 4, 14]), json!("a"), "1"),
                    row(json!([2026, 4, 15]), json!("b"), "2"),
                    row(json!([2026, 4, 15, "evening"]), json!("c"), "3"),
                    row(json!([2026, 5, 1]), json!("d"), "4"),
                ],
                reducible: false,
            }),
        );
        let params = ViewParams {
            key: Some(json!([2026, 4])),
            ..Default::default()
        };
        let got = reg2.query("byDate", &db, &params).unwrap();
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn reduce_with_grouping() {
        let db = Db::open_in_memory().unwrap();
        let mut reg = Registry::new();
        reg.insert(
            "durations",
            Box::new(FixedView {
                name: "durations".into(),
                rows: vec![
                    row(json!([2026, 4, 15]), json!(30.0), "a"),
                    row(json!([2026, 4, 15]), json!(45.0), "b"),
                    row(json!([2026, 4, 16]), json!(10.0), "c"),
                    row(json!([2026, 4, 16]), json!(20.0), "d"),
                    row(json!([2026, 5, 1]), json!(5.0), "e"),
                ],
                reducible: true,
            }),
        );

        // reduce=true, group=true → one row per full key, summing values.
        let params = ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let got = reg.query("durations", &db, &params).unwrap();
        assert_eq!(got.len(), 3);
        // Sorted by key: [2026,4,15], [2026,4,16], [2026,5,1]
        assert_eq!(got[0].key, json!([2026, 4, 15]));
        assert_eq!(got[0].value, json!(75.0));
        assert_eq!(got[0].id, None);
        assert_eq!(got[1].key, json!([2026, 4, 16]));
        assert_eq!(got[1].value, json!(30.0));
        assert_eq!(got[2].key, json!([2026, 5, 1]));
        assert_eq!(got[2].value, json!(5.0));

        // group_level=2 → group by [year, month], collapsing day-level keys.
        let params = ViewParams {
            reduce: true,
            group_level: Some(2),
            ..Default::default()
        };
        let got = reg.query("durations", &db, &params).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].key, json!([2026, 4]));
        assert_eq!(got[0].value, json!(105.0));
        assert_eq!(got[1].key, json!([2026, 5]));
        assert_eq!(got[1].value, json!(5.0));

        // reduce=true with no group → single aggregate row with key=null.
        let params = ViewParams {
            reduce: true,
            ..Default::default()
        };
        let got = reg.query("durations", &db, &params).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].key, JsonValue::Null);
        assert_eq!(got[0].value, json!(110.0));
    }

    #[test]
    fn reduce_requires_opt_in() {
        let db = Db::open_in_memory().unwrap();
        let mut reg = Registry::new();
        reg.insert(
            "plain",
            Box::new(FixedView {
                name: "plain".into(),
                rows: vec![row(json!("a"), json!(1), "1")],
                reducible: false,
            }),
        );
        let params = ViewParams {
            reduce: true,
            ..Default::default()
        };
        let err = reg.query("plain", &db, &params);
        assert!(matches!(err, Err(DatumError::Other(_))));
    }

    #[test]
    fn unknown_view_errors() {
        let db = Db::open_in_memory().unwrap();
        let reg = Registry::new();
        let err = reg.query("nope", &db, &ViewParams::default());
        assert!(matches!(err, Err(DatumError::NotFound(_))));
    }

    #[test]
    fn parse_key_arg_parses_json_and_falls_back_to_string() {
        assert_eq!(parse_key_arg("42"), json!(42));
        assert_eq!(parse_key_arg("\"hello\""), json!("hello"));
        assert_eq!(parse_key_arg("[1,2,3]"), json!([1, 2, 3]));
        assert_eq!(parse_key_arg("null"), JsonValue::Null);
        assert_eq!(parse_key_arg("true"), json!(true));
        // Bare string (not valid JSON) falls back to JSON string.
        assert_eq!(parse_key_arg("2026-04-15"), json!("2026-04-15"));
    }
}
