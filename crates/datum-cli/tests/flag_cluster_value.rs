//! da-dzxqb: in a cluster of short flags the first letter that takes a
//! value takes the rest of the token, as getopt does: `-yt20` is
//! `-y -t 20`.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};

fn at_of(db: &datum_core::db::Db, field: &str) -> chrono::DateTime<chrono::Utc> {
    logged_entries(db, field)[0].at.as_ref().unwrap().utc
}

#[test]
fn a_value_letter_in_a_cluster_takes_the_rest_of_the_token() {
    let db = fresh_db();
    for (field, cluster, spelled) in [("a", "-yt20", "20"), ("b", "-yt20:30", "20:30")] {
        run_cli(&["moment", field, cluster], &db).expect(cluster);
        let want = format!("{field}_spelled");
        run_cli(&["moment", &want, "-y", "-t", spelled], &db).expect("spelled out");
        assert_eq!(at_of(&db, field), at_of(&db, &want), "{cluster}");
    }
    // `-yt 20` is as it was.
    run_cli(&["moment", "c", "-yt", "20"], &db).expect("-yt 20");
    assert_eq!(at_of(&db, "c"), at_of(&db, "a_spelled"));
}

#[test]
fn a_cluster_with_a_letter_that_is_no_flag_is_refused_naming_it() {
    let db = fresh_db();
    for cluster in ["-yx20", "-yx"] {
        let err = run_cli(&["moment", "f", cluster], &db)
            .map(|_| String::new())
            .unwrap_or_else(|e| format!("{e:#}"));
        assert!(err.contains("-x"), "{cluster}: {err}");
    }
    assert!(logged_entries(&db, "f").is_empty());
}
