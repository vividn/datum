//! `datum count` — count live entries, optionally grouped or time-bounded.

use anyhow::Result;
use datum_core::db::{Db, GroupColumn};

use crate::args::CountArgs;
use crate::commands::ids::parse_since;

/// Structured result of `count` — either a single total or a grouped
/// tally. Exposed so tests can assert on the data without parsing stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountResult {
    Total(usize),
    Grouped(Vec<(String, usize)>),
}

pub fn compute_count(args: &CountArgs, db: &Db) -> Result<CountResult> {
    let since_utc = args.since.as_deref().map(parse_since).transpose()?;
    let field = args.field.as_deref();

    if let Some(raw) = &args.group_by {
        let col: GroupColumn = raw
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;
        let rows = db.count_entries_grouped(field, col, since_utc)?;
        return Ok(CountResult::Grouped(rows));
    }

    let n = db.count_entries_filtered(field, since_utc)?;
    Ok(CountResult::Total(n))
}

pub fn count(args: CountArgs, db: &Db) -> Result<()> {
    let result = compute_count(&args, db)?;
    match (&result, args.json) {
        (CountResult::Total(n), false) => println!("{n}"),
        (CountResult::Total(n), true) => {
            println!("{}", serde_json::json!({ "count": n }));
        }
        (CountResult::Grouped(rows), false) => {
            for (k, n) in rows {
                println!("{n}\t{k}");
            }
        }
        (CountResult::Grouped(rows), true) => {
            let arr: Vec<_> = rows
                .iter()
                .map(|(k, n)| serde_json::json!({ "key": k, "count": n }))
                .collect();
            println!("{}", serde_json::Value::Array(arr));
        }
    }
    Ok(())
}
