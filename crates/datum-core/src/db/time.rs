//! Bulk retime paths.
//!
//! Post-pivot the entry's time-shape is just `occur_time` + `dur` — no
//! end_time to slide. These methods retime `occur_time` and leave `dur`
//! untouched.
//!
//! da-5ns.4: when the row's `id_structure` references `%occur_time%`, a
//! retime cascades into the slug. Each row goes through
//! [`Db::save_existing_entry_with_auto_reinterp`] so a templated slug is
//! re-derived against the new occur_time and the row is rekeyed in place.
//! Rows without a templated slug (or whose template doesn't reference
//! occur_time) stay put — the helper's slug-changed check is the gate.

use chrono::Duration;

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::DEFAULT_ID_DELIMITER;

use super::row_mapper::row_to_entry;
use super::types::RetimeFilter;
use super::Db;

impl Db {
    /// Retime matching rows to a fresh absolute `occur_time` + precision.
    ///
    /// The `precision_unit` is rendered into a single ISO-8601 duration
    /// string (e.g. `PT1S`, `PT1M`, `P1D`) and stored in
    /// `occur_time_precision` — the v=20 schema collapsed the
    /// (precision_seconds, precision_unit) pair into one column.
    pub fn set_start_time_with_precision(
        &self,
        filter: &RetimeFilter,
        new_start: &crate::time::DatumTime,
        precision_seconds: u32,
        precision_unit: &str,
    ) -> Result<usize> {
        let entries = select_retime_targets(self, filter)?;
        let precision_iso = precision_to_iso(precision_seconds, precision_unit);
        let mut count = 0usize;
        for mut e in entries {
            let old_slug = e.slug.clone();
            e.occur_time = Some(new_start.clone());
            e.occur_time_precision = precision_iso.clone();
            self.save_existing_entry_with_auto_reinterp_kind(
                &old_slug,
                e,
                DEFAULT_ID_DELIMITER,
                crate::history::ChangeKind::Retime,
            )?;
            count += 1;
        }
        Ok(count)
    }

    pub fn update_times(&self, filter: &RetimeFilter, offset: Duration) -> Result<usize> {
        let entries = select_retime_targets(self, filter)?;
        let mut count = 0usize;
        for e in entries {
            let old_slug = e.slug.clone();
            let mut shifted = e;
            if let Some(t) = shifted.occur_time.as_mut() {
                t.utc += offset;
            }
            self.save_existing_entry_with_auto_reinterp_kind(
                &old_slug,
                shifted,
                DEFAULT_ID_DELIMITER,
                crate::history::ChangeKind::Retime,
            )?;
            count += 1;
        }
        Ok(count)
    }
}

/// Select live rows matching the retime filter. Shared between the
/// absolute (`set_start_time_with_precision`) and offset (`update_times`)
/// paths so the SQL surface stays in one place.
fn select_retime_targets(db: &Db, filter: &RetimeFilter) -> Result<Vec<Entry>> {
    let mut sql = String::from("SELECT * FROM entries WHERE deleted_at IS NULL");
    let mut bindings: Vec<String> = Vec::new();

    if let Some(needle) = filter.slug_or_hid {
        sql.push_str(" AND (slug = ? OR hid = ?)");
        bindings.push(needle.to_string());
        bindings.push(needle.to_string());
    }
    if let Some(field) = filter.field {
        sql.push_str(" AND field = ?");
        bindings.push(field.to_string());
    }
    if let Some(after) = filter.after_utc {
        sql.push_str(" AND occur_time >= ?");
        bindings.push(after.to_rfc3339());
    }
    if let Some(before) = filter.before_utc {
        sql.push_str(" AND occur_time <= ?");
        bindings.push(before.to_rfc3339());
    }

    let mut stmt = db.conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), row_to_entry)?;
    rows.map(|r| r.map_err(DatumError::Db))
        .collect::<Result<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::ids::Slug;
    use crate::time::DatumTime;
    use chrono::{DateTime, Utc};

    fn seed_dt(rfc: &str) -> DatumTime {
        let utc: DateTime<Utc> = rfc.parse().unwrap();
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
    }

    /// da-5ns.4: when `id_structure` references `%occur_time%`, retiming
    /// the entry must rekey the row in place (hid preserved, slug
    /// updated to track the new timestamp).
    #[test]
    fn retime_rekeys_when_id_structure_references_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new(
            "nap",
            Verb::Point,
            None,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        e.id_structure = Some("%field%:%occur_time%".to_string());
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();
        // Stamp the templated slug post-insert so we start from the
        // canonical form (the default slug builder doesn't honor
        // id_structure, so we rekey explicitly here).
        let stamped_slug = Slug("nap:2024-01-01T10:00:00.000Z".to_string());
        let mut stamped = e.clone();
        stamped.slug = stamped_slug.clone();
        db.save_existing_entry_with_rekey(&e.slug, &stamped).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(stamped_slug.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(by_hid.slug.as_str(), "nap:2024-01-01T10:30:00.000Z");
        assert_eq!(by_hid.hid, hid);
        assert!(db.get_entry(stamped_slug.as_str()).is_err());
    }

    /// `set_start_time_with_precision` (the `--time` retime path) must
    /// also trigger slug re-interpolation against the templated id.
    #[test]
    fn set_start_time_rekeys_when_template_references_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        e.id_structure = Some("%field%:%occur_time%".to_string());
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();
        let stamped_slug = Slug("coffee:2024-01-01T10:00:00.000Z".to_string());
        let mut stamped = e.clone();
        stamped.slug = stamped_slug.clone();
        db.save_existing_entry_with_rekey(&e.slug, &stamped).unwrap();

        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let filter = RetimeFilter {
            slug_or_hid: Some(stamped_slug.as_str()),
            ..Default::default()
        };
        db.set_start_time_with_precision(&filter, &new_start, 1, "minute")
            .unwrap();

        let by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(by_hid.slug.as_str(), "coffee:2026-05-01T10:00:00.000Z");
        assert_eq!(by_hid.hid, hid);
    }

    /// Retime on a row whose `id_structure` does NOT reference
    /// occur_time must leave the slug untouched (the slug-changed gate
    /// in [`Db::save_existing_entry_with_auto_reinterp_kind`] suppresses
    /// the rekey).
    #[test]
    fn retime_leaves_slug_alone_when_template_does_not_reference_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new(
            "nap",
            Verb::Point,
            None,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        let slug = e.slug.clone();
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(&hid),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let after = db.get_entry(slug.as_str()).unwrap();
        assert_eq!(after.hid, hid);
    }
}

/// Map a precision-unit name ("second", "minute", "hour", "day",
/// "month", "year", "exact") to a single ISO-8601 duration string.
/// Sub-bead 3 will replace this with a proper PrecisionUnit→ISO helper.
/// Encode a (seconds, unit) precision pair as a single ISO-8601 duration
/// string for the v=20 `occur_time_precision` column. `unit` selects the
/// component (PT…M / PT…H / P…D etc.) so a `(300, "minute")` precision
/// renders as `"PT5M"` rather than collapsing to the bare unit. `exact`
/// or unrecognized units return `None` (column stored as NULL).
fn precision_to_iso(seconds: u32, unit: &str) -> Option<String> {
    if seconds == 0 || matches!(unit, "exact" | "") {
        return None;
    }
    match unit {
        "second" => Some(format!("PT{seconds}S")),
        "minute" => {
            let n = seconds / 60;
            Some(format!("PT{}M", n.max(1)))
        }
        "hour" => {
            let n = seconds / 3600;
            Some(format!("PT{}H", n.max(1)))
        }
        "date" | "day" => {
            let n = seconds / 86400;
            Some(format!("P{}D", n.max(1)))
        }
        "month" => Some("P1M".to_string()),
        "year" => Some("P1Y".to_string()),
        _ => None,
    }
}
