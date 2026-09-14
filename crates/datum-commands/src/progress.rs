//! A long command's count, on stderr, as it goes (da-h9vj).
//!
//! A ~50 MB `db v2import` used to run in silence. Stdout is not an option:
//! `db backup` / `db restore` pipe JSONL through it. On a terminal the count
//! redraws one line a few times a second; anywhere else — a log, the web's
//! WASI process, whose stderr is a regular file — it is a plain line every
//! second or so, and the final count of each phase always.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

pub struct Progress {
    what: &'static str,
    total: Option<usize>,
    tty: bool,
    last: Instant,
}

impl Progress {
    /// `what` is the phase (`"v2import: reading"`); `total` if it is known.
    pub fn new(what: &'static str, total: Option<usize>) -> Self {
        Self {
            what,
            total,
            tty: std::io::stderr().is_terminal(),
            last: Instant::now(),
        }
    }

    /// `done` so far — said only if long enough has passed since the last.
    pub fn tick(&mut self, done: usize) {
        let every = Duration::from_millis(if self.tty { 200 } else { 1000 });
        if done.is_multiple_of(64) && self.last.elapsed() >= every {
            self.say(done, "");
        }
    }

    /// The phase is over: its final count, always, and the line ended.
    pub fn finish(mut self, done: usize) {
        self.say(done, "\n");
    }

    fn say(&mut self, done: usize, end: &str) {
        self.last = Instant::now();
        let count = match self.total {
            Some(total) => format!("{done}/{total}"),
            None => done.to_string(),
        };
        // One write per reading, so a reader of the stream (the web's
        // worker) never sees half of one.
        let line = if self.tty {
            format!("\r{} {count}\x1b[K{end}", self.what)
        } else {
            format!("{} {count}\n", self.what)
        };
        let _ = std::io::stderr().write_all(line.as_bytes());
    }
}
