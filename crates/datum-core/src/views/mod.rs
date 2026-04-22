//! Named views over the entry store, backed by SQL.
//!
//! Each [`View`] owns the SQL it needs and produces [`ViewRow`]s on demand.
//! The map/reduce split that the original datum inherited from CouchDB is
//! gone — a view simply answers a query against live entries, using
//! parameters ([`ViewParams`]) that mirror the datum CLI surface
//! (`--key`/`--startkey`/`--endkey`/`--group`/`--group-level`/`--reduce`/`--limit`).
//! Grouping and aggregation happen inside each view, typically via
//! `GROUP BY`.
//!
//! Call sites look views up by name via [`Registry`]; [`builtins`] wires in
//! every view shipped by datum-core.

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
/// Wire this into call sites (CLI, web, etc.) so `datum map <name>` sees
/// every built-in view without each call site having to enumerate them.
pub fn builtins() -> Registry {
    let mut reg = Registry::new();
    reg.insert(TimingView.name(), Box::new(TimingView));
    reg.insert(StateChangeView.name(), Box::new(StateChangeView));
    reg.insert(DurationSumView.name(), Box::new(DurationSumView));
    reg.insert(SubHumanIdView.name(), Box::new(SubHumanIdView));
    reg
}

/// One result row. For unreduced views, `id` carries the source entry's
/// slug; reduced rows (one per group) set `id = None`.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewRow {
    pub key: JsonValue,
    pub value: JsonValue,
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

/// A named SQL-backed query over the entry store.
pub trait View: Send + Sync {
    fn name(&self) -> &str;

    /// Produce the rows this view emits for the given parameters. The view
    /// is responsible for applying all of `params` — key filter, reduce,
    /// grouping, limit — typically by encoding them in SQL. Rows must be
    /// returned in the same order the caller would expect from a
    /// CouchDB-style view: sorted by `key` ascending.
    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>>;
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

    /// Run a view by name.
    pub fn query(
        &self,
        name: &str,
        db: &Db,
        params: &ViewParams,
    ) -> Result<Vec<ViewRow>> {
        let view = self
            .get(name)
            .ok_or_else(|| DatumError::NotFound(format!("view: {name}")))?;
        view.query(db, params)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Shared helpers used by views ───────────────────────────────────────────

/// Key match: equal, OR (CouchDB convention) `row_key` is an array that
/// starts with the elements of `query_key` (prefix match on array keys).
pub fn key_matches(row_key: &JsonValue, query_key: &JsonValue) -> bool {
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

/// Retain only rows matching `params.key` (prefix-aware) or falling within
/// `[startkey, endkey]` (inclusive). When no key params are set, no rows
/// are dropped. Views push down what they can into SQL and call this to
/// apply the residual filter (e.g. the date component of an array key).
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

/// Apply `params.limit` in place.
pub fn apply_limit(mut rows: Vec<ViewRow>, params: &ViewParams) -> Vec<ViewRow> {
    if let Some(n) = params.limit {
        rows.truncate(n);
    }
    rows
}

/// Group key used when reducing: full key if `group=true`, the first
/// `group_level` array elements for array keys, else `Null` (one bucket).
pub fn group_key(key: &JsonValue, params: &ViewParams) -> JsonValue {
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

/// Extract a single field name for SQL pushdown when the caller's key
/// params narrow the query to one field.
///
/// Views keyed as `[field, ...]` can push an exact-field filter into SQL
/// whenever either (a) `key`'s head is a string, (b) `key` is a bare
/// string, or (c) `startkey` and `endkey` agree on their head element.
pub fn field_filter_from_params(params: &ViewParams) -> Option<String> {
    fn head(v: &JsonValue) -> Option<&str> {
        v.as_array()
            .and_then(|a| a.first())
            .and_then(|f| f.as_str())
    }
    if let Some(k) = &params.key {
        if let Some(s) = head(k) {
            return Some(s.to_string());
        }
        if let Some(s) = k.as_str() {
            return Some(s.to_string());
        }
    }
    if let (Some(s), Some(e)) = (params.startkey.as_ref(), params.endkey.as_ref()) {
        if let (Some(a), Some(b)) = (head(s), head(e)) {
            if a == b {
                return Some(a.to_string());
            }
        }
    }
    None
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

    #[test]
    fn registry_lookup_and_unknown_errors() {
        let db = Db::open_in_memory().unwrap();
        let reg = Registry::new();
        let err = reg.query("nope", &db, &ViewParams::default());
        assert!(matches!(err, Err(DatumError::NotFound(_))));

        let reg = builtins();
        let names: Vec<&str> = reg.names().collect();
        assert!(names.contains(&"datum_timing"));
        assert!(names.contains(&"state_change"));
        assert!(names.contains(&"duration_sum"));
        assert!(names.contains(&"datum_sub_human_id"));
    }

    #[test]
    fn parse_key_arg_parses_json_and_falls_back_to_string() {
        use serde_json::json;
        assert_eq!(parse_key_arg("42"), json!(42));
        assert_eq!(parse_key_arg("\"hello\""), json!("hello"));
        assert_eq!(parse_key_arg("[1,2,3]"), json!([1, 2, 3]));
        assert_eq!(parse_key_arg("null"), JsonValue::Null);
        assert_eq!(parse_key_arg("true"), json!(true));
        assert_eq!(parse_key_arg("2026-04-15"), json!("2026-04-15"));
    }

    #[test]
    fn cmp_json_respects_couchdb_collation() {
        use serde_json::json;
        use std::cmp::Ordering::*;
        assert_eq!(cmp_json(&JsonValue::Null, &json!(false)), Less);
        assert_eq!(cmp_json(&json!(false), &json!(1)), Less);
        assert_eq!(cmp_json(&json!(1), &json!("a")), Less);
        assert_eq!(cmp_json(&json!("a"), &json!([1])), Less);
        assert_eq!(cmp_json(&json!([1, 2]), &json!([1, 3])), Less);
        assert_eq!(cmp_json(&json!([1, 2]), &json!([1, 2, 3])), Less);
    }

    #[test]
    fn key_matches_array_prefix() {
        use serde_json::json;
        assert!(key_matches(&json!(["workout", "2026-04-20"]), &json!(["workout"])));
        assert!(key_matches(&json!(["workout"]), &json!(["workout"])));
        assert!(!key_matches(&json!(["mood"]), &json!(["workout"])));
        assert!(!key_matches(&json!(["workout"]), &json!(["workout", "2026-04-20"])));
    }
}
