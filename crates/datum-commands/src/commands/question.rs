//! The one way a command asks its person something (da-jytnl).
//!
//! On a terminal it prompts. Anywhere else it reads a line of stdin as the
//! answer, and with none takes the question's default; with no default it is
//! [`Unanswered`] (exit 12), never a hang. Under `DATUM_NO_DEFAULT_ANSWER=1`
//! no default is taken either: the question goes on stderr as one
//! `DATUM_ASK=<json>` line, which datum.day shows as a dialog with no code of
//! its own (`asking` in `web/src/lib/dbio.ts`), and it runs the line again
//! with every answer so far on stdin.
//!
//! A command asks before its transaction opens, so no one waits on the write
//! lock while a person reads; which also means a command that asks only
//! sometimes, a log, pays nothing when it does not (`asks` in `datum-cli`).

use anyhow::Result;
use serde::Serialize;
use std::io::{BufRead, IsTerminal, Write};

/// How long a stdin that is no terminal may stay silent before it counts as
/// no answer: a pipe nothing writes to and nothing closes would wait forever.
#[cfg(not(target_family = "wasm"))]
const PIPE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Kind {
    YesNo {
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    Choose {
        choices: Vec<String>,
    },
    /// Answered by typing `name`, so reflexive `y`s won't do it: a pipe's
    /// line is read for one only as the page's answer.
    Name {
        name: String,
        /// What a terminal is told to type; the page's dialog says its own.
        #[serde(skip)]
        prompt: String,
    },
}

/// A question, as the page reads it.
#[derive(Debug, Clone, Serialize)]
pub struct Question {
    text: String,
    /// The word on the button that says yes.
    action: String,
    #[serde(flatten)]
    kind: Kind,
    /// `NAME=value` for the answer's run to carry in its env.
    #[serde(skip_serializing_if = "Option::is_none")]
    pin: Option<String>,
}

impl Question {
    /// Yes or no: `default` is what no answer means, `None` where neither is safe.
    pub fn yes_no(text: impl Into<String>, action: &str, default: Option<bool>) -> Self {
        Self::new(text, action, Kind::YesNo { default })
    }

    /// One of `choices`, with no default: a bare or unknown answer declines.
    pub fn choose(text: impl Into<String>, choices: Vec<String>) -> Self {
        Self::new(text, "", Kind::Choose { choices })
    }

    /// `name`, typed as `prompt` says, after `text`, which may be empty. A
    /// bare or different answer declines.
    pub fn name(text: impl Into<String>, action: &str, name: &str, prompt: String) -> Self {
        let name = name.to_string();
        Self::new(text, action, Kind::Name { name, prompt })
    }

    fn new(text: impl Into<String>, action: &str, kind: Kind) -> Self {
        Question {
            text: text.into(),
            action: action.to_string(),
            kind,
            pin: None,
        }
    }

    /// What the answer's run carries in its env, to say what it answered.
    pub fn pinned(mut self, pin: String) -> Self {
        self.pin = Some(pin);
        self
    }

    /// The answer, trimmed, read from the process's stdin.
    pub fn ask(&self) -> Result<String> {
        let stdin = std::io::stdin();
        let terminal = stdin.is_terminal();
        self.say(terminal);
        let line = match self.listens(terminal) {
            true if terminal => read_line(&mut stdin.lock()),
            true => read_pipe(),
            false => None,
        };
        self.settle(line, terminal)
    }

    /// [`Self::ask`] on an injected reader: a test's script, or `io::empty()`
    /// where stdin is the command's data and no one can answer on it.
    pub fn ask_on(&self, stdin: &mut dyn BufRead, terminal: bool) -> Result<String> {
        self.say(terminal);
        let line = self.listens(terminal).then(|| read_line(stdin)).flatten();
        self.settle(line, terminal)
    }

    /// The question, unanswered: said on stderr for the page, on a line of
    /// its own after a prompt it did not need.
    pub fn unanswered(&self) -> anyhow::Error {
        let json = serde_json::to_string(self).expect("a question serializes");
        if !page() {
            eprintln!();
        }
        eprint!("DATUM_ASK={json}");
        Unanswered.into()
    }

    fn listens(&self, terminal: bool) -> bool {
        terminal || page() || !matches!(self.kind, Kind::Name { .. })
    }

    /// The prompt, unless the page will be told or has already asked.
    fn say(&self, terminal: bool) {
        if !terminal && page() {
            return;
        }
        let text = &self.text;
        let prompt = match &self.kind {
            Kind::YesNo { default } => {
                let hint = match default {
                    Some(true) => "[Y/n]",
                    Some(false) => "[y/N]",
                    None => "[y/n]",
                };
                format!("{text} {hint} ")
            }
            Kind::Choose { choices } => {
                let mut said = format!("{text}\n");
                for (i, choice) in choices.iter().enumerate() {
                    said += &format!("  {}  {choice}\n", i + 1);
                }
                let numbers: Vec<String> = (1..=choices.len()).map(|n| n.to_string()).collect();
                format!("{said}[{}] ", numbers.join("/"))
            }
            Kind::Name { prompt, .. } => {
                let text = if text.is_empty() {
                    String::new()
                } else {
                    format!("{text}\n")
                };
                match terminal {
                    true => format!("{text}{prompt} "),
                    false => format!(
                        "{text}{prompt}\nno terminal to type it on: --yes confirms without a prompt"
                    ),
                }
            }
        };
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "{prompt}");
        let _ = stderr.flush();
    }

    /// What `line` comes to: `None` is no answer at all.
    fn settle(&self, line: Option<String>, terminal: bool) -> Result<String> {
        let default = match self.kind {
            Kind::YesNo { default } => default.map(|yes| if yes { "y" } else { "n" }),
            _ => None,
        };
        let line = match line {
            Some(line) => line,
            None if terminal => String::new(),
            None => match default {
                Some(default) if !page() => return Ok(default.to_string()),
                _ => return Err(self.unanswered()),
            },
        };
        Ok(match line.trim() {
            "" => default.unwrap_or_default().to_string(),
            answer => answer.to_string(),
        })
    }
}

/// Whether an answer to a [`Question::yes_no`] says yes.
pub fn is_yes(answer: &str) -> bool {
    matches!(answer.to_lowercase().as_str(), "y" | "yes")
}

/// Whether the host answers by running the line again (datum.day).
pub(crate) fn page() -> bool {
    std::env::var("DATUM_NO_DEFAULT_ANSWER").is_ok_and(|v| v == "1")
}

fn read_line(stdin: &mut dyn BufRead) -> Option<String> {
    let mut line = String::new();
    (stdin.read_line(&mut line).unwrap_or(0) > 0).then_some(line)
}

/// A line of stdin, or `None` at its end or after [`PIPE_WAIT`] of silence.
/// The reading thread dies with the process; a later question's waits behind
/// it on stdin's lock, and comes to no answer as this one did.
#[cfg(not(target_family = "wasm"))]
fn read_pipe() -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(read_line(&mut std::io::stdin().lock())));
    rx.recv_timeout(PIPE_WAIT).ok().flatten()
}

/// WASI has no threads, and the page's stdin always ends.
#[cfg(target_family = "wasm")]
fn read_pipe() -> Option<String> {
    read_line(&mut std::io::stdin().lock())
}

/// A question went unanswered: [`Question::unanswered`] said it on stderr, or
/// an answer no longer holds (`--if-unchanged`, da-2zqad) and the page counts
/// again; `datum` exits with its own code for this and says nothing more.
#[derive(Debug)]
pub struct Unanswered;

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no answer on stdin")
    }
}

impl std::error::Error for Unanswered {}

#[cfg(test)]
mod tests {
    use super::*;

    fn yes_no(default: Option<bool>) -> Question {
        Question::yes_no("Sure?", "go", default)
    }

    #[test]
    fn a_pipe_with_no_answer_takes_the_default_or_goes_unanswered() {
        let mut empty = std::io::empty();
        assert_eq!(yes_no(Some(true)).ask_on(&mut empty, false).unwrap(), "y");
        assert_eq!(yes_no(Some(false)).ask_on(&mut empty, false).unwrap(), "n");
        let unanswered = yes_no(None).ask_on(&mut empty, false).unwrap_err();
        assert!(unanswered.is::<Unanswered>(), "{unanswered:?}");
        let choose = Question::choose("Which?", vec!["a".into(), "b".into()]);
        let unanswered = choose.ask_on(&mut empty, false).unwrap_err();
        assert!(unanswered.is::<Unanswered>(), "{unanswered:?}");
    }

    #[test]
    fn a_line_is_the_answer_and_a_bare_one_the_default() {
        let ask = |line: &str, default| yes_no(default).ask_on(&mut line.as_bytes(), false);
        assert_eq!(ask("n\n", Some(true)).unwrap(), "n");
        assert_eq!(ask("\n", Some(true)).unwrap(), "y");
        assert_eq!(ask("\n", None).unwrap(), "");
    }

    /// `yes | datum db purge` is what the typed name is there to stop.
    #[test]
    fn a_pipe_never_answers_a_typed_name() {
        let question = Question::name("", "purge", "mine", "Type it:".into());
        let piped = question
            .ask_on(&mut "mine\n".as_bytes(), false)
            .unwrap_err();
        assert!(piped.is::<Unanswered>(), "{piped:?}");
        let typed = question.ask_on(&mut "mine\n".as_bytes(), true).unwrap();
        assert_eq!(typed, "mine");
    }

    #[test]
    fn the_page_reads_one_json_object() {
        let json = serde_json::to_value(yes_no(Some(true)).pinned("X=1".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"text": "Sure?", "action": "go", "kind": "yes_no", "default": true, "pin": "X=1"})
        );
    }
}
