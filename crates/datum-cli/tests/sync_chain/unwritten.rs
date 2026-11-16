//! An op whose row this copy's write check refuses: refused alone, the rest
//! of its batch landing, and tried again by another build (da-xjfol).

use super::*;

/// A one-pixel-header png, which is all an attachment's type check reads.
const PNG: &str = "data:image/png;base64,iVBORw0KGgo=";

/// `a`'s attachment row copied under another name, which is not its bytes'
/// hash, as no datum writes it: the next ops `a` sends sign it as a
/// baseline, as a hand-forged op or an older build's row would arrive.
fn forge_attachment(a: &Copy) {
    let columns = a.query("SELECT group_concat(name) FROM pragma_table_info('entries')");
    a.exec(&format!(
        "CREATE TEMP TABLE t AS SELECT {columns} FROM entries WHERE field = 'attachment';
         UPDATE t SET hid = 'forged0000000', slug = 'attachment:{bad}',
             data = json_set(data, '$.name', '{bad}');
         INSERT INTO entries ({columns}) SELECT * FROM t;",
        bad = "0".repeat(64)
    ));
}

#[test]
fn a_row_this_copy_refuses_is_refused_alone_and_tried_again_by_another_build() {
    let (a, b) = chained();
    a.ok(&["occur", "before", "-y"]);
    a.ok_in(&["db", "attachment"], PNG);
    forge_attachment(&a);
    a.ok(&["occur", "after", "-y"]);

    // The batch lands but for the forged row, which is said and not lost
    // to an abort that would stall every later op of `a`.
    let (ok, text) = a.send(&b);
    assert!(!ok, "{text}");
    assert!(text.contains("its row:"), "{text}");
    assert!(
        text.contains("its name and type are not its bytes'"),
        "{text}"
    );
    assert!(b.has("before") && b.has("after"), "{text}");
    let attachments = "SELECT COUNT(*) FROM entries WHERE field = 'attachment'";
    assert_eq!(b.query(attachments), "1", "the good attachment alone");
    assert_eq!(
        b.query("SELECT COUNT(*) FROM ops WHERE hid = 'forged0000000' AND applied = 2"),
        "1"
    );
    let sync = b.ok(&["check", "sync"]);
    assert!(sync.contains("sync_unwritten"), "{sync}");
    assert!(sync.contains("datum upgrade"), "{sync}");

    // Held, it is not sent again, and the link goes on.
    a.ok(&["occur", "later", "-y"]);
    let (ok, again) = a.send(&b);
    assert!(ok, "{again}");
    assert!(b.has("later"), "{again}");

    // The same build tries it no more; another tries it again, and this
    // one, refusing it too, says so again.
    let (_, quiet) = a.send(&b);
    assert!(!quiet.contains("its row:"), "{quiet}");
    b.exec("UPDATE _meta SET value = 'an older build' WHERE key = 'sync_unwritten_by'");
    let (ok, retried) = a.send(&b);
    assert!(!ok && retried.contains("its row:"), "{retried}");

    // Acknowledged, nothing is left to warn of.
    b.ok(&["sync", "chain", "ack", &a.id()[..18]]);
    assert!(!b.ok(&["check", "sync"]).contains("sync_unwritten"));
}

/// A row no write check names but the write still refuses — a following
/// state with no duration, which `derive_verb` refuses before the INSERT and
/// the schema's CHECK behind it — is the row's too: refused alone, and the
/// sender's later ops in the same receive apply (da-tk8g3).
#[test]
fn a_row_the_write_refuses_for_any_reason_of_its_own_is_refused_alone() {
    let (a, b) = chained();
    a.ok(&["switch", "mood", "happy", "-y"]);
    let columns = a.query("SELECT group_concat(name) FROM pragma_table_info('entries')");
    // As no datum writes it: past the CHECK, signed as a baseline.
    a.exec(&format!(
        "PRAGMA ignore_check_constraints = ON;
         CREATE TEMP TABLE t AS SELECT {columns} FROM entries WHERE field = 'mood';
         UPDATE t SET hid = 'forged0000000', field = 'forged', slug = 'forged:x',
             following_state = '\"sad\"', dur = NULL;
         INSERT INTO entries ({columns}) SELECT * FROM t;"
    ));
    a.ok(&["occur", "after", "-y"]);

    let (ok, text) = a.send(&b);
    assert!(!ok, "{text}");
    assert!(text.contains("its row:"), "{text}");
    assert!(
        text.contains("following_state requires a duration"),
        "{text}"
    );
    assert!(b.has("mood") && b.has("after"), "{text}");
    assert!(!b.has("forged"), "{text}");
    assert_eq!(
        b.query("SELECT COUNT(*) FROM ops WHERE hid = 'forged0000000' AND applied = 2"),
        "1"
    );
    assert!(b.ok(&["check", "sync"]).contains("sync_unwritten"));
}
