//! Database consistency checks.
//!
//! Runs a battery of SQL- and Rust-level sanity queries and produces a
//! grouped report. Errors set the process exit code to 1; warnings do not.

use anyhow::Result;
use datum_core::db::Db;
use serde::Serialize;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueExample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<IssueExample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub summary: Summary,
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.summary.errors > 0
    }
}

/// Run every consistency check and return a structured report.
pub fn collect_issues(db: &Db) -> Result<Report> {
    let mut issues: Vec<Issue> = Vec::new();

    // 1. time_end < time_start (ERROR)
    let time_bad = db.find_time_end_before_start()?;
    if !time_bad.is_empty() {
        let first = &time_bad[0];
        issues.push(Issue {
            severity: Severity::Error,
            category: "time_end_before_start",
            message: format!(
                "{} entr{} have time_end earlier than time_start",
                time_bad.len(),
                if time_bad.len() == 1 { "y" } else { "ies" },
            ),
            count: time_bad.len(),
            example: Some(IssueExample {
                id: Some(first.id.clone()),
                field: Some(first.field.clone()),
                detail: Some(format!(
                    "start={}  end={}",
                    first.time_start_utc, first.time_end_utc
                )),
            }),
        });
    }

    // 2. content_hash mismatches (ERROR)
    let entries = db.all_entries()?;
    let mut mismatches: Vec<(String, String, String, String)> = Vec::new();
    for e in &entries {
        let expected = e.expected_content_hash();
        if expected != e.content_hash {
            mismatches.push((
                e.id.clone(),
                e.field.clone(),
                e.content_hash.clone(),
                expected,
            ));
        }
    }
    if !mismatches.is_empty() {
        let (id, field, stored, expected) = mismatches[0].clone();
        issues.push(Issue {
            severity: Severity::Error,
            category: "content_hash_mismatch",
            message: format!(
                "{} entr{} have a stored content_hash that disagrees with its fields",
                mismatches.len(),
                if mismatches.len() == 1 { "y" } else { "ies" },
            ),
            count: mismatches.len(),
            example: Some(IssueExample {
                id: Some(id),
                field: Some(field),
                detail: Some(format!(
                    "stored={}…  expected={}…",
                    &stored[..stored.len().min(12)],
                    &expected[..expected.len().min(12)],
                )),
            }),
        });
    }

    // Load registry once for the remaining checks.
    let registry = db.load_registry()?;

    // 3. unknown fields — have entries but no field_spec (WARNING)
    let fields = db.distinct_fields()?;
    let unknown: Vec<String> = fields
        .iter()
        .filter(|f| registry.field_spec(f).is_none())
        .cloned()
        .collect();
    if !unknown.is_empty() {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "unknown_field",
            message: format!(
                "{} field{} not registered in any spec",
                unknown.len(),
                if unknown.len() == 1 { "" } else { "s" },
            ),
            count: unknown.len(),
            example: Some(IssueExample {
                id: None,
                field: Some(unknown[0].clone()),
                detail: Some(unknown.join(", ")),
            }),
        });
    }

    // 4. orphaned extra_data keys — field has a spec but extra_data contains
    //    keys that aren't declared in the spec's subfields (WARNING)
    let mut orphan_detail: Option<(String, String, String)> = None; // (id, field, key)
    let mut orphan_count = 0usize;
    for e in &entries {
        let Some(spec) = registry.field_spec(&e.field) else {
            continue;
        };
        let Some(extra) = e.extra_data.as_ref() else {
            continue;
        };
        if let JsonValue::Object(map) = extra {
            for key in map.keys() {
                if !spec.subfields.contains_key(key) {
                    orphan_count += 1;
                    if orphan_detail.is_none() {
                        orphan_detail = Some((e.id.clone(), e.field.clone(), key.clone()));
                    }
                }
            }
        }
    }
    if orphan_count > 0 {
        let (id, field, key) = orphan_detail.unwrap();
        issues.push(Issue {
            severity: Severity::Warning,
            category: "orphaned_extra_data_key",
            message: format!(
                "{} extra_data key{} not declared in the owning field's spec",
                orphan_count,
                if orphan_count == 1 { "" } else { "s" },
            ),
            count: orphan_count,
            example: Some(IssueExample {
                id: Some(id),
                field: Some(field),
                detail: Some(format!("key={key}")),
            }),
        });
    }

    // 5. tombstones — rows with deleted_at set (WARNING)
    let tombstones = db.tombstone_ids()?;
    if !tombstones.is_empty() {
        issues.push(Issue {
            severity: Severity::Warning,
            category: "tombstone_without_live_row",
            message: format!(
                "{} tombstone{} present (deleted rows with no surviving live counterpart)",
                tombstones.len(),
                if tombstones.len() == 1 { "" } else { "s" },
            ),
            count: tombstones.len(),
            example: Some(IssueExample {
                id: Some(tombstones[0].clone()),
                field: None,
                detail: None,
            }),
        });
    }

    let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let warnings = issues.iter().filter(|i| i.severity == Severity::Warning).count();
    Ok(Report {
        issues,
        summary: Summary { errors, warnings, clean: errors == 0 && warnings == 0 },
    })
}

/// Human-readable report, grouped by category with per-row count and example.
pub fn render_human(report: &Report) -> String {
    if report.issues.is_empty() {
        return "✓ No consistency issues found.\n".to_string();
    }
    let mut out = String::new();
    for issue in &report.issues {
        let tag = match issue.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
        };
        out.push_str(&format!(
            "[{tag}] {category} (x{count}): {msg}\n",
            category = issue.category,
            count = issue.count,
            msg = issue.message,
        ));
        if let Some(ex) = &issue.example {
            if let Some(id) = &ex.id {
                out.push_str(&format!("    example id: {id}\n"));
            }
            if let Some(field) = &ex.field {
                out.push_str(&format!("    example field: {field}\n"));
            }
            if let Some(detail) = &ex.detail {
                out.push_str(&format!("    {detail}\n"));
            }
        }
    }
    out.push_str(&format!(
        "\nSummary: {} error{}, {} warning{}.\n",
        report.summary.errors,
        if report.summary.errors == 1 { "" } else { "s" },
        report.summary.warnings,
        if report.summary.warnings == 1 { "" } else { "s" },
    ));
    out
}

/// JSON report for machine consumption.
pub fn render_json(report: &Report) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::db::Db;
    use datum_core::entry::{Entry, Verb};
    use datum_core::field_spec::{FieldSpec, SubfieldSpec, SubfieldType};
    use datum_core::time::DatumTime;
    use std::collections::HashMap;

    fn t_at(iso: &str) -> DatumTime {
        let utc: chrono::DateTime<chrono::Utc> = iso.parse().unwrap();
        DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
    }

    #[test]
    fn check_clean_db_exits_zero() {
        let db = Db::open_in_memory().unwrap();
        let report = collect_issues(&db).unwrap();
        assert!(!report.has_errors(), "empty db must have no errors");
        assert!(report.issues.is_empty(), "empty db must produce no issues");
        assert!(report.summary.clean);
    }

    #[test]
    fn check_reports_time_end_before_start() {
        let db = Db::open_in_memory().unwrap();
        // Intentional: end precedes start.
        let start = t_at("2024-01-01T10:00:00+00:00");
        let end = t_at("2024-01-01T09:00:00+00:00");
        let entry = Entry::new(
            "work",
            Verb::Stop,
            None,
            None,
            None,
            Some(start),
            Some(end),
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(report.has_errors(), "time_end<time_start must be an error");
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "time_end_before_start")
            .expect("time_end_before_start issue must be reported");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.count, 1);
        let example = issue.example.as_ref().unwrap();
        assert_eq!(example.field.as_deref(), Some("work"));
    }

    #[test]
    fn check_reports_unknown_field_as_warning() {
        let db = Db::open_in_memory().unwrap();
        // Insert an entry for "mood" without registering a spec.
        let entry = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(!report.has_errors(), "unknown fields are warnings, not errors");
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "unknown_field")
            .expect("unknown_field warning must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
        assert_eq!(
            issue.example.as_ref().and_then(|e| e.field.as_deref()),
            Some("mood")
        );
    }

    #[test]
    fn check_json_output_shape() {
        let db = Db::open_in_memory().unwrap();
        // Populate one error-level and one warning-level issue.
        let bad = Entry::new(
            "work",
            Verb::Stop,
            None,
            None,
            None,
            Some(t_at("2024-01-01T10:00:00+00:00")),
            Some(t_at("2024-01-01T09:00:00+00:00")),
            "d1",
        );
        db.insert_entry(&bad).unwrap();
        // Unknown-field warning on another field.
        let unknown = Entry::new(
            "mystery",
            Verb::Occur,
            None,
            None,
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&unknown).unwrap();

        let report = collect_issues(&db).unwrap();
        let json = render_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Top-level shape
        assert!(parsed.is_object());
        assert!(parsed.get("issues").is_some(), "report must have 'issues'");
        assert!(parsed.get("summary").is_some(), "report must have 'summary'");

        // Summary shape
        let summary = parsed.get("summary").unwrap();
        assert!(summary.get("errors").unwrap().is_number());
        assert!(summary.get("warnings").unwrap().is_number());
        assert!(summary.get("clean").unwrap().is_boolean());
        assert!(summary.get("errors").unwrap().as_u64().unwrap() >= 1);
        assert!(summary.get("warnings").unwrap().as_u64().unwrap() >= 1);
        assert_eq!(summary.get("clean").unwrap().as_bool(), Some(false));

        // Issue shape
        let issues = parsed.get("issues").unwrap().as_array().unwrap();
        assert!(!issues.is_empty());
        for issue in issues {
            assert!(issue.get("severity").unwrap().is_string());
            assert!(issue.get("category").unwrap().is_string());
            assert!(issue.get("message").unwrap().is_string());
            assert!(issue.get("count").unwrap().is_number());
            let sev = issue.get("severity").unwrap().as_str().unwrap();
            assert!(sev == "error" || sev == "warning");
        }
    }

    #[test]
    fn check_reports_orphaned_extra_data_key_as_warning() {
        let db = Db::open_in_memory().unwrap();
        // Register a spec for "mood" that declares only `intensity` as a subfield.
        let mut subfields = HashMap::new();
        subfields.insert(
            "intensity".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::Number,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.subfields = subfields;
        db.save_field_spec(&spec).unwrap();

        // Entry carries an *undeclared* extra_data key "weather".
        let entry = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"weather": "sunny"})),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();

        let report = collect_issues(&db).unwrap();
        assert!(!report.has_errors(), "orphaned keys are warnings, not errors");
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == "orphaned_extra_data_key")
            .expect("orphaned_extra_data_key warning must be reported");
        assert_eq!(issue.severity, Severity::Warning);
        assert_eq!(issue.count, 1);
    }
}
