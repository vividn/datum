//! `--show json` (da-yc9w): the action echo as data.
//!
//! A write command under [`Show::Json`](super::Show::Json) prints no lines.
//! Every action it would have echoed is collected here, and the outermost
//! dispatch scope prints them as ONE document:
//!
//! ```json
//! {"docs":[{"action":"CREATE","hid":"35mtf","id":"coffee:2026-09-14T16:50:09.613Z",
//!           "header":"CREATE 35mtf coffee:2026-09-14T16:50:09.613Z",
//!           "info":"18:50:09+2  coffee ∅●",
//!           "data":["beans: \"ethiopian\"","cups: 2"],
//!           "data_inline":"beans: \"ethiopian\", cups: 2"}]}
//! ```
//!
//! The strings are [`ActionLines`]' own, ANSI stripped — there is no second
//! formatter here, so the printed and the JSON forms cannot drift. A command
//! that echoes several entries emits one object per entry, in printed order,
//! which is what lets the web bar draw a section per document.
//!
//! Buffering is thread-local, so the in-process test harnesses (which drive
//! many runs on one thread) and the single-threaded WASI build behave alike.

use std::cell::RefCell;

use serde_json::{json, Value as JsonValue};

use super::ActionLines;

thread_local! {
    /// Docs collected in the current dispatch scope, and the scope depth.
    /// Depth is what makes a re-dispatching command (a kit command, a
    /// multi-side run) emit one document rather than one per nested call.
    static ECHO: RefCell<(usize, Vec<JsonValue>)> = const { RefCell::new((0, Vec::new())) };
}

/// One document for `entry`'s action, as the caller must address it: the
/// three header pieces separately, then the lines verbatim.
fn document(lines: &ActionLines) -> JsonValue {
    json!({
        "action": lines.action,
        "hid": lines.hid,
        "id": lines.id,
        "header": super::strip_ansi(&lines.header),
        "info": super::strip_ansi(&lines.info),
        "data": lines.data_items,
        "data_inline": lines.data_inline,
    })
}

/// Record one action echo. Called only under `Show::Json`.
pub(super) fn push(lines: &ActionLines) {
    ECHO.with(|slot| slot.borrow_mut().1.push(document(lines)));
}

/// Run `f` as a dispatch scope, printing the collected document when the
/// OUTERMOST scope returns `Ok`. Nested scopes just keep collecting.
///
/// An error prints nothing: `--show json` is about the echo, not about
/// error reporting, which stays on stderr with its own exit code.
pub fn scope<T>(f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    let outermost = ECHO.with(|slot| {
        let mut s = slot.borrow_mut();
        s.0 += 1;
        if s.0 == 1 {
            s.1.clear();
            true
        } else {
            false
        }
    });

    let result = f();

    let docs = ECHO.with(|slot| {
        let mut s = slot.borrow_mut();
        s.0 -= 1;
        if outermost {
            std::mem::take(&mut s.1)
        } else {
            Vec::new()
        }
    });

    if result.is_ok() && !docs.is_empty() {
        println!("{}", json!({ "docs": docs }));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(hid: &str) -> ActionLines {
        ActionLines {
            header: format!("CREATE {hid} coffee:2026-09-14T16:50:09.613Z"),
            info: "18:50:09+2  coffee".into(),
            data: vec![],
            action: "CREATE".into(),
            hid: hid.into(),
            id: "coffee:2026-09-14T16:50:09.613Z".into(),
            data_items: vec![],
            data_inline: String::new(),
        }
    }

    #[test]
    fn nested_scopes_collect_into_one_document() {
        let collected = scope(|| {
            push(&lines("aaaaa"));
            scope(|| {
                push(&lines("bbbbb"));
                Ok(())
            })?;
            Ok(ECHO.with(|s| s.borrow().1.len()))
        })
        .unwrap();
        assert_eq!(collected, 2, "the inner scope must not drain the buffer");
    }

    #[test]
    fn a_scope_starts_empty() {
        let _ = scope(|| {
            push(&lines("aaaaa"));
            Ok(())
        });
        let seen = scope(|| Ok(ECHO.with(|s| s.borrow().1.len()))).unwrap();
        assert_eq!(seen, 0, "a fresh outermost scope clears the previous run");
    }

    #[test]
    fn document_carries_the_addressable_pieces() {
        let doc = document(&lines("35mtf"));
        assert_eq!(doc["action"], "CREATE");
        assert_eq!(doc["hid"], "35mtf");
        assert_eq!(doc["id"], "coffee:2026-09-14T16:50:09.613Z");
        assert_eq!(doc["data"], json!([]));
    }
}
