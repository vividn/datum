//! scripts/name-guard.py (da-r6o39) against a fixture repo: a NAME_GUARD
//! entry the PR adds — in a line, a commit, its title or body — fails, and
//! one it only removes passes. The entry is a made-up word.

use std::path::{Path, PathBuf};
use std::process::Command;

const WORD: &str = "zorblax";
const GUARD: &str = "quuxle\nZorblax\n";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    /// A repo whose `base` branch holds `old.txt` with the word on line 2,
    /// checked out on `pr`, one commit past it.
    fn new(case: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("name-guard-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repo(dir);
        repo.git(&["init", "-q", "-b", "base"]);
        repo.write("old.txt", &format!("one\n{WORD} was here\nthree\n"));
        repo.commit("ann@example.com", "cy@example.com", "base");
        repo.git(&["checkout", "-q", "-b", "pr"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let out = self.cmd("git").args(args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn cmd(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.0)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        cmd
    }

    fn write(&self, path: &str, text: &str) {
        std::fs::write(self.0.join(path), text).unwrap();
    }

    /// Commits everything as Ann <`author`>, committed by Cy <`committer`>.
    fn commit(&self, author: &str, committer: &str, message: &str) {
        self.git(&["add", "-A"]);
        let ident = format!("Ann <{author}>");
        let mut cmd = self.cmd("git");
        cmd.args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
            "--author",
            &ident,
        ])
        .env("GIT_COMMITTER_NAME", "Cy")
        .env("GIT_COMMITTER_EMAIL", committer);
        assert!(cmd.status().unwrap().success());
    }

    /// Runs the guard against `base`, with `pr` as the forge's PR object.
    fn guard(&self, guard: &str, pr: Option<&str>) -> (bool, String) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/name-guard.py");
        let mut cmd = self.cmd("python3");
        cmd.arg(script).arg("base").env("NAME_GUARD", guard);
        if let Some(pr) = pr {
            self.write(".git/pr.json", pr);
            cmd.arg(self.0.join(".git/pr.json"));
        }
        let out = cmd.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !text.to_lowercase().contains(WORD),
            "the guard printed its entry:\n{text}"
        );
        (out.status.success(), text)
    }
}

fn fails_with(repo: &Repo, pr: Option<&str>, expect: &str) {
    let (ok, text) = repo.guard(GUARD, pr);
    assert!(!ok, "passed, expected «{expect}»:\n{text}");
    assert!(
        text.contains(expect) && text.contains("read 2 entries"),
        "expected «{expect}»:\n{text}"
    );
}

#[test]
fn an_added_line_fails() {
    let repo = Repo::new("added");
    repo.write(
        "new.txt",
        &format!("a\nb\nsaid {} hi\n", WORD.to_uppercase()),
    );
    repo.commit("ann@example.com", "cy@example.com", "adds a file");
    fails_with(&repo, None, "entry #2 at new.txt:3");
}

#[test]
fn a_commit_message_fails() {
    let repo = Repo::new("message");
    repo.commit(
        "ann@example.com",
        "cy@example.com",
        &format!("thanks to {WORD}"),
    );
    fails_with(&repo, None, "message");
}

#[test]
fn an_author_email_fails() {
    let repo = Repo::new("author");
    repo.commit(&format!("{WORD}@example.com"), "cy@example.com", "plain");
    fails_with(&repo, None, "author");
}

#[test]
fn a_committer_email_fails() {
    let repo = Repo::new("committer");
    repo.commit("ann@example.com", &format!("{WORD}@example.com"), "plain");
    fails_with(&repo, None, "committer");
}

#[test]
fn a_pr_title_or_body_fails() {
    let repo = Repo::new("title");
    fails_with(
        &repo,
        Some(&format!(r#"{{"title":"for {WORD}","body":null}}"#)),
        "entry #2 in the PR title",
    );
    fails_with(
        &repo,
        Some(&format!(r#"{{"title":"t","body":"by {WORD}"}}"#)),
        "entry #2 in the PR body",
    );
}

#[test]
fn a_path_fails_without_printing_it() {
    let repo = Repo::new("path");
    repo.write(&format!("{WORD}.txt"), &format!("{WORD}\n"));
    repo.commit("ann@example.com", "cy@example.com", "plain");
    fails_with(&repo, None, "entry #2 in the path of an added file");
}

#[test]
fn a_removed_line_and_a_clean_pr_pass() {
    let repo = Repo::new("removed");
    repo.write("old.txt", "one\nthree\n");
    repo.commit("ann@example.com", "cy@example.com", "takes a word out");
    let (ok, text) = repo.guard(GUARD, Some(r#"{"title":"t","body":"b"}"#));
    assert!(ok, "{text}");
    assert!(
        text.contains("read 2 entries") && text.contains("no entry found"),
        "{text}"
    );
}

#[test]
fn an_empty_guard_fails() {
    let repo = Repo::new("empty");
    for guard in ["", "\n  \n"] {
        let (ok, text) = repo.guard(guard, None);
        assert!(!ok && text.contains("NAME_GUARD secret missing"), "{text}");
    }
}

#[test]
fn an_entry_is_a_regex_so_a_word_can_be_whole() {
    let repo = Repo::new("regex");
    let guard = "quuxle\n\\bzorb\\b\n";
    for (title, caught) in [("said zorb hi", true), ("ZORB's", true), (WORD, false)] {
        let (ok, text) = repo.guard(guard, Some(&format!(r#"{{"title":"{title}","body":""}}"#)));
        assert_eq!(ok, !caught, "«{title}»:\n{text}");
        assert_eq!(
            text.contains("entry #2 in the PR title"),
            caught,
            "«{title}»:\n{text}"
        );
    }
}

#[test]
fn an_entry_that_does_not_compile_fails_by_its_number() {
    let repo = Repo::new("badregex");
    let (ok, text) = repo.guard("quuxle\n(zorb\n", None);
    assert!(
        !ok && text.contains("entry #2 is not a valid regex"),
        "{text}"
    );
    assert!(
        !text.contains("zorb"),
        "the guard printed its entry:\n{text}"
    );
}
