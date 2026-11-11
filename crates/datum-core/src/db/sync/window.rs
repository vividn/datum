//! A filter's time window (da-nflkk): where it starts, and what passes it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::{Map, Value};

use super::chain::Filter;
use super::{belongs_after, belongs_to, fold, Op};
use crate::db::alias::ALIAS_FIELD;
use crate::db::spec::SPEC_FIELD;
use crate::error::Result;

/// The instant a window is judged at: now, or `DATUM_SYNC_TEST_NOW` (the
/// tests), an RFC 3339 time.
fn judged_now() -> DateTime<Utc> {
    std::env::var("DATUM_SYNC_TEST_NOW")
        .ok()
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map_or_else(Utc::now, |t| t.with_timezone(&Utc))
}

/// Where a window of `secs` starts now: back to the hour, so that two
/// copies judging a moment apart agree.
pub(super) fn window_from(secs: i64) -> DateTime<Utc> {
    let start = judged_now() - chrono::Duration::seconds(secs);
    chrono::DurationRound::duration_trunc(start, chrono::Duration::hours(1)).unwrap_or(start)
}

/// A filter as it judges entries now (da-nflkk). With a window: its start
/// ([`window_from`]), and per field the entry that set its state then,
/// which passes too.
pub(super) struct Judge<'a> {
    pub(super) filter: &'a Filter,
    pub(super) from: Option<DateTime<Utc>>,
    pub(super) governing: BTreeMap<String, String>,
}

impl<'a> Judge<'a> {
    pub(super) fn new(conn: &Connection, filter: &'a Filter) -> Result<Judge<'a>> {
        let mut judge = Judge {
            filter,
            from: None,
            governing: BTreeMap::new(),
        };
        let Some(window) = filter.window else {
            return Ok(judge);
        };
        let from = window_from(window);
        let fields: Vec<String> = match &filter.fields {
            Some(f) => f.iter().cloned().collect(),
            None => conn
                .prepare("SELECT DISTINCT field FROM entries")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?,
        };
        let fields: Vec<&str> = fields
            .iter()
            .map(String::as_str)
            .filter(|f| ![SPEC_FIELD, ALIAS_FIELD].contains(f))
            .collect();
        judge.governing = crate::db::state::governing_at(conn, &fields, from)?
            .into_iter()
            .filter_map(|(f, (_, hid))| Some((f, hid?)))
            .collect();
        judge.from = Some(from);
        Ok(judge)
    }

    /// Whether an entry row passes: a spec by the field it specifies, or
    /// with `specs` always, as is an alias; any other entry by its field,
    /// and within a window by its time — `at`, else when it was made — or
    /// for setting its field's state at the window's start.
    pub(super) fn row(&self, r: &Map<String, Value>) -> bool {
        let field = r.get("field").and_then(Value::as_str);
        match field {
            Some(SPEC_FIELD) => self.filter.specs || self.filter.admits(belongs_to(r).as_deref()),
            Some(ALIAS_FIELD) if self.filter.specs => true,
            _ if !self.filter.admits(field) => false,
            Some(ALIAS_FIELD) => true,
            _ => self.from.is_none_or(|from| {
                let hid = r.get("hid").and_then(Value::as_str);
                let time = r["at"]["utc"].as_str().or(r["create_time"].as_str());
                let time = time.and_then(|t| DateTime::parse_from_rfc3339(t).ok());
                time.is_none_or(|t| t >= from)
                    || hid.is_some_and(|h| self.governing.values().any(|g| g == h))
            }),
        }
    }

    /// Whether the entry `ops` fold to passes.
    pub(super) fn ops(&self, ops: &[Op]) -> bool {
        match self.filter.by_field() {
            true => self.filter.admits(belongs_after(ops).as_deref()),
            false => fold(ops, None).is_some_and(|r| self.row(&r)),
        }
    }
}
