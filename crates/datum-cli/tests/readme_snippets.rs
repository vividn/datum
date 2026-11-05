//! Markdown bash-snippet anti-drift test.
//!
//! Extracts every ```` ```bash ```` fenced block from the workspace `README.md`
//! and from `docs/grammar-tokens.md`, and runs each command against the built
//! `datum` binary, so the quickstart and the token reference cannot drift from
//! the actual CLI grammar.
//!
//! ## Annotation conventions
//!
//! - ```` ```bash ```` — run as-is, expect exit 0.
//! - ```` ```bash,no_run ```` — skipped (used for examples that need the
//!   network, the user's `$EDITOR`, or external tools like `cargo` / `man`).
//! - ```` ```bash,fails_with=N ```` — expect exit code `N` (any int).
//!
//! ## Block body grammar
//!
//! - Lines whose first non-whitespace char is `#` are comments and are
//!   skipped before tokenization.
//! - Blank lines are skipped.
//! - A trailing ` # comment` on a command line (`#` preceded by whitespace
//!   and outside quotes) is stripped.
//! - A line ending in `\` is joined with the next line — multi-line commands.
//!
//! ## Transcripts
//!
//! In a doc `TRANSCRIPT_DOCS` lists, a plain fence (no info string) after a
//! bash block is what that block printed, stdout then stderr of each command
//! in turn, and is checked (da-2i4fd). A block's only transcript, with
//! nothing but blank lines between them, is the WHOLE output; any other is
//! an excerpt, a run of lines found together in it, where a line ending in
//! `…` matches any line it starts. What a run mints — hids, timestamps,
//! clocks, dates — is masked on both sides, and the blocks run in UTC.
//! `DATUM_REPASTE=1` writes each whole transcript that drifted with what
//! was printed; an excerpt is chosen by hand, so it is re-pasted by hand.
//!
//! ## Isolation
//!
//! Each block runs against its own `tempfile::TempDir`, exported as
//! `DATUM_HOME` / `DATUM_DB` / `DATUM_CONFIG_HOME`, and as `HOME` /
//! `XDG_CONFIG_HOME` / `XDG_DATA_HOME` too, so a command that writes under
//! the home (`completions install`, an rc file) never writes the caller's
//! (da-95o6k). Commands inside one block share state; blocks do not bleed
//! into each other.

use std::path::PathBuf;
use std::process::Command;

const AUTOGEN_BEGIN: &str = "<!-- BEGIN AUTOGEN: COMMANDS -->";
const AUTOGEN_END: &str = "<!-- END AUTOGEN: COMMANDS -->";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// The Markdown files whose `bash` blocks must run. `docs/grammar-tokens.md`
/// (da-yc4n8) documents the punctuation of the grammar, and every example on
/// it is a transcript of a real run — this is what keeps it that way.
/// The docs pages that took the README's runnable examples on 2026-10-20 are
/// here for the same reason, and so is datum.day's guide for AI agents, whose
/// setup block an agent runs as written (it once set `mood` up with one key and
/// then gave it two values).
const SNIPPET_DOCS: &[&str] = &[
    "README.md",
    "docs/grammar-tokens.md",
    "docs/agents.md",
    "web/static/agents.md",
];

/// The docs whose plain fences under a bash block are its transcript.
const TRANSCRIPT_DOCS: &[&str] = &["docs/grammar-tokens.md"];

#[derive(Debug)]
struct Block {
    info: String,
    body: String,
    line_no: usize,
    transcripts: Vec<Transcript>,
}

#[derive(Debug)]
struct Transcript {
    body: String,
    line_no: usize,
    /// Nothing but blank lines between it and the block.
    direct: bool,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    RunOk,
    Skip,
    FailsWith(i32),
}

fn parse_info(info: &str) -> Mode {
    let mut parts = info.split(',').map(str::trim);
    let lang = parts.next().unwrap_or("");
    assert_eq!(
        lang, "bash",
        "extract_bash_blocks should only yield bash fences"
    );
    for opt in parts {
        if opt.is_empty() {
            continue;
        }
        if opt == "no_run" {
            return Mode::Skip;
        }
        if let Some(val) = opt.strip_prefix("fails_with=") {
            let code: i32 = val
                .parse()
                .unwrap_or_else(|e| panic!("bad fails_with value {val:?}: {e}"));
            return Mode::FailsWith(code);
        }
        panic!("unknown bash block option {opt:?} in info string {info:?}");
    }
    Mode::RunOk
}

fn extract_bash_blocks(readme: &str) -> Vec<Block> {
    let cleaned = strip_autogen_regions(readme);
    let mut blocks: Vec<Block> = Vec::new();
    let mut open: Option<(String, String, usize)> = None;
    // Prose since the last fence closed, and whether that fence was a bash
    // block or one of its transcripts, with no heading since.
    let (mut prose, mut under_bash) = (false, false);
    for (idx, line) in cleaned.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            match open.take() {
                None => {
                    let info = trimmed.trim_start_matches('`').trim().to_string();
                    open = Some((info, String::new(), idx + 1));
                }
                Some((info, body, line_no)) => {
                    let bash = info.split(',').next().map(str::trim) == Some("bash");
                    let plain = info.is_empty();
                    if bash {
                        blocks.push(Block {
                            info,
                            body,
                            line_no,
                            transcripts: Vec::new(),
                        });
                    } else if plain && under_bash {
                        let block = blocks.last_mut().expect("a bash block");
                        block.transcripts.push(Transcript {
                            body,
                            line_no,
                            direct: !prose && block.transcripts.is_empty(),
                        });
                    }
                    under_bash = bash || (plain && under_bash);
                    prose = false;
                }
            }
        } else if let Some((_, body, _)) = open.as_mut() {
            body.push_str(line);
            body.push('\n');
        } else if trimmed.starts_with('#') {
            under_bash = false;
        } else if !trimmed.is_empty() {
            prose = true;
        }
    }
    blocks
}

/// A transcript's lines with what a run mints masked: hids, instants, clocks
/// and dates, in this order, so the short hid's masks read `<clock>`. Both
/// sides pass through it, so a mask can only hide what it covers: a value of
/// twelve lowercase characters and digits reads as a hid. A short hid is
/// masked where the echo puts one, after the action word, and where `check`
/// lists an entry, before its time; a change to either format fails the test.
fn masked(text: &str) -> Vec<String> {
    use regex_lite::Regex;
    use std::sync::LazyLock;
    static MASKS: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| {
        [
            (r"\b[0-9a-z]{12}\b", "<hid>"),
            (r"\d{4}-\d\d-\d\dT[0-9:.]+(Z|[+-]\d\d:\d\d)?", "<when>"),
            (r"\d{4}-\d\d-\d\d", "<date>"),
            (r"\d\d:\d\d:\d\d", "<clock>"),
            (r"^([A-Z][A-Z/]+ )[0-9a-z]{4,11} ", "$1<id> "),
            (r"^(\s+)[0-9a-z]{4,11}(  <clock>)", "$1<id>$2"),
        ]
        .into_iter()
        .map(|(re, to)| (Regex::new(re).unwrap(), to))
        .collect()
    });
    text.trim_end()
        .lines()
        .map(|line| {
            let mut line = line.trim_end().to_string();
            for (re, to) in MASKS.iter() {
                line = re.replace_all(&line, *to).into_owned();
            }
            line
        })
        .collect()
}

/// Whether a transcript is what was printed: all of it, or a run of it.
fn transcribes(t: &Transcript, whole: bool, printed: &str) -> bool {
    let (want, got) = (masked(&t.body), masked(printed));
    let same = |w: &String, g: &String| match w.strip_suffix('…') {
        Some(head) => g.starts_with(head),
        None => w == g,
    };
    if whole {
        return want.len() == got.len() && want.iter().zip(&got).all(|(w, g)| same(w, g));
    }
    !want.is_empty()
        && got
            .windows(want.len())
            .any(|run| run.iter().zip(&want).all(|(g, w)| same(w, g)))
}

/// Drop EVERY autogenerated region from a doc before scanning (a doc holds
/// only its own, and a region it does not hold is left alone).
///
/// The generated blocks carry fences with no language tag, so they are
/// already skipped — but they also hold real `datum` command lines that
/// no one here ever intended to run, and da-ce7n3 turned one region into
/// three. Stripping them all keeps this scanner about the prose a person
/// wrote.
fn strip_autogen_regions(readme: &str) -> String {
    let mut out = readme.to_string();
    for (_file, block) in xtask::AUTOGEN_BLOCKS {
        out = strip_one_autogen_region(
            &out,
            &format!("<!-- BEGIN AUTOGEN: {block} -->"),
            &format!("<!-- END AUTOGEN: {block} -->"),
        );
    }
    out
}

fn strip_one_autogen_region(readme: &str, begin: &str, end: &str) -> String {
    let (b, e) = match (readme.find(begin), readme.find(end)) {
        (Some(b), Some(e)) if e > b => (b, e),
        _ => return readme.to_string(),
    };
    let mut s = String::with_capacity(readme.len());
    s.push_str(&readme[..b]);
    // Replace with blank lines so subsequent line numbers stay roughly
    // aligned with the on-disk README, which makes failure messages
    // easier to map back.
    let dropped = &readme[b..e + end.len()];
    for _ in dropped.bytes().filter(|c| *c == b'\n') {
        s.push('\n');
    }
    s.push_str(&readme[e + end.len()..]);
    s
}

/// Pre-process a block body into a list of logical command lines:
/// strip blank/comment lines, join `\`-continuations.
fn split_commands(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut acc = String::new();
    for raw in body.lines() {
        let trimmed_end = raw.trim_end();
        if let Some(stripped) = trimmed_end.strip_suffix('\\') {
            acc.push_str(stripped);
            acc.push(' ');
            continue;
        }
        acc.push_str(raw);
        let cmd = std::mem::take(&mut acc);
        let lstrip = cmd.trim_start();
        if lstrip.is_empty() || lstrip.starts_with('#') {
            continue;
        }
        out.push(cmd);
    }
    if !acc.trim().is_empty() {
        out.push(acc);
    }
    out
}

/// Tokenize a single command line into argv. Handles single/double quotes
/// and trailing inline `# ...` comments. No expansions, no escapes — these
/// are quickstart-grade snippets, not arbitrary shell.
fn tokenize(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut in_single = false;
    let mut in_double = false;
    for c in line.chars() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                cur.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else {
                cur.push(c);
            }
        } else if c.is_whitespace() {
            if in_token {
                out.push(std::mem::take(&mut cur));
                in_token = false;
            }
        } else if c == '#' && !in_token {
            // Inline comment — discard the rest of the line.
            break;
        } else if c == '\'' {
            in_single = true;
            in_token = true;
        } else if c == '"' {
            in_double = true;
            in_token = true;
        } else {
            cur.push(c);
            in_token = true;
        }
    }
    if in_token {
        out.push(cur);
    }
    out
}

struct TestEnv {
    root: PathBuf,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
    _tmp: tempfile::TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir for README snippet block");
        let home = tmp.path().join("data");
        let config_home = tmp.path().join("config");
        std::fs::create_dir_all(&home).expect("mkdir DATUM_HOME");
        std::fs::create_dir_all(&config_home).expect("mkdir DATUM_CONFIG_HOME");
        let db = home.join("readme.db");
        Self {
            root: tmp.path().to_path_buf(),
            home,
            config_home,
            db,
            _tmp: tmp,
        }
    }

    fn apply<'c>(&self, cmd: &'c mut Command) -> &'c mut Command {
        cmd.env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join(".config"))
            .env("XDG_DATA_HOME", self.root.join(".local/share"))
            .env_remove("ZDOTDIR")
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            // An error prints as a person sees it, without CI's backtrace.
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE")
    }
}

fn run_command(args: &[String], env: &TestEnv) -> std::process::Output {
    env.apply(&mut Command::new(env!("CARGO_BIN_EXE_datum")))
        .args(args)
        .output()
        .expect("spawn datum")
}

/// A block's command that writes under the home writes the block's temp
/// dir, not the home of whoever runs the test (da-95o6k).
#[test]
fn a_block_writes_nothing_in_the_callers_home() {
    let caller = tempfile::tempdir().expect("tempdir for the caller's home");
    let env = TestEnv::new();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    for var in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "ZDOTDIR"] {
        cmd.env(var, caller.path());
    }
    let out = env
        .apply(&mut cmd)
        .args(["completions", "install", "--shell", "fish"])
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "completions install failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    let written: Vec<_> = std::fs::read_dir(caller.path()).unwrap().collect();
    assert!(written.is_empty(), "wrote the caller's home: {written:?}");
    assert!(env
        .root
        .join(".config/fish/completions/datum.fish")
        .is_file());
}

#[test]
fn markdown_bash_snippets_run_against_datum_binary() {
    for rel in SNIPPET_DOCS {
        run_snippets_in(rel);
    }
}

fn run_snippets_in(rel: &str) {
    let doc_path = workspace_root().join(rel);
    let doc = std::fs::read_to_string(&doc_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", doc_path.display()));
    let blocks = extract_bash_blocks(&doc);
    assert!(
        !blocks.is_empty(),
        "expected at least one ```bash``` block in {}",
        doc_path.display(),
    );

    let mut ran_blocks = 0usize;
    let mut ran_commands = 0usize;
    let mut skipped = 0usize;
    let mut drifted = Vec::new();
    let mut repaste = Vec::new();

    for block in &blocks {
        let mode = parse_info(&block.info);
        if matches!(mode, Mode::Skip) {
            skipped += 1;
            continue;
        }
        let env = TestEnv::new();
        ran_blocks += 1;
        let mut printed = String::new();
        for cmd in split_commands(&block.body) {
            let argv = tokenize(&cmd);
            assert!(
                !argv.is_empty(),
                "tokenize produced empty argv from non-empty line {cmd:?} \
                 (block at {rel} line {})",
                block.line_no,
            );
            assert_eq!(
                argv[0], "datum",
                "bash snippet must invoke `datum` (got `{}` at \
                 {rel} line ~{}). Mark non-datum examples with \
                 ```bash,no_run.",
                argv[0], block.line_no,
            );
            let cli_args = &argv[1..];
            let out = run_command(cli_args, &env);
            let actual = out.status.code().unwrap_or(-1);
            let expected = match mode {
                Mode::RunOk => 0,
                Mode::FailsWith(n) => n,
                Mode::Skip => unreachable!(),
            };
            if actual != expected {
                panic!(
                    "bash snippet failed.\n\
                     block opens at {rel} line ~{}\n\
                     expected exit {expected}, got {actual}\n\
                     $ datum {}\n\
                     --- stdout ---\n{}\n\
                     --- stderr ---\n{}",
                    block.line_no,
                    cli_args.join(" "),
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr),
                );
            }
            printed.push_str(&String::from_utf8_lossy(&out.stdout));
            printed.push_str(&String::from_utf8_lossy(&out.stderr));
            ran_commands += 1;
        }
        if TRANSCRIPT_DOCS.contains(&rel) {
            let whole = |t: &Transcript| t.direct && block.transcripts.len() == 1;
            for t in block.transcripts.iter() {
                if transcribes(t, whole(t), &printed) {
                    continue;
                }
                if whole(t) && std::env::var_os("DATUM_REPASTE").is_some() {
                    repaste.push((t.line_no, t.body.lines().count(), printed.clone()));
                    continue;
                }
                drifted.push(format!(
                    "{rel} line {}: the transcript is not what the block at line {} printed{}:\n{printed}",
                    t.line_no,
                    block.line_no,
                    if whole(t) { "" } else { " (an excerpt: a run of these lines)" },
                ));
            }
        }
    }
    if !repaste.is_empty() {
        let mut lines: Vec<String> = doc.lines().map(String::from).collect();
        for (fence, len, printed) in repaste.into_iter().rev() {
            let pasted = printed.lines().map(|l| l.trim_end().to_string());
            lines.splice(fence..fence + len, pasted);
        }
        std::fs::write(&doc_path, lines.join("\n") + "\n").expect("re-paste");
    }
    assert!(
        drifted.is_empty(),
        "{} transcript(s) drifted; paste what was printed \
         (DATUM_REPASTE=1 re-pastes each whole one):\n\n{}",
        drifted.len(),
        drifted.join("\n"),
    );

    assert!(
        ran_commands > 0,
        "{rel} has bash blocks but none ran ({ran_blocks} blocks, {skipped} skipped)"
    );
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn tokenize_strips_inline_comments() {
        assert_eq!(
            tokenize("datum tail            # most recent 10 entries"),
            vec!["datum", "tail"],
        );
    }

    #[test]
    fn tokenize_keeps_quoted_strings_intact() {
        assert_eq!(
            tokenize(r#"datum note word "to run" pos=verb"#),
            vec!["datum", "note", "word", "to run", "pos=verb"],
        );
    }

    #[test]
    fn tokenize_ignores_hash_inside_token() {
        // Hash inside a token (no whitespace before it) is not a comment.
        assert_eq!(
            tokenize("datum --tag bug#42"),
            vec!["datum", "--tag", "bug#42"],
        );
    }

    #[test]
    fn split_commands_drops_comments_and_blanks() {
        let body = "\
# Track the current state
datum switch mood happy

# Look back
datum tail   # inline note kept on the line
";
        assert_eq!(
            split_commands(body),
            vec![
                "datum switch mood happy".to_string(),
                "datum tail   # inline note kept on the line".to_string(),
            ],
        );
    }

    #[test]
    fn split_commands_joins_backslash_continuations() {
        let body = "\
datum occur weight \\
    75.2kg \\
    src=morning
";
        let cmds = split_commands(body);
        assert_eq!(cmds.len(), 1);
        // Tokenization will collapse the runs of whitespace; we just need
        // the joined line to round-trip back to the expected argv.
        assert_eq!(
            tokenize(&cmds[0]),
            vec!["datum", "occur", "weight", "75.2kg", "src=morning"],
        );
    }

    #[test]
    fn parse_info_recognizes_modes() {
        assert!(matches!(parse_info("bash"), Mode::RunOk));
        assert!(matches!(parse_info("bash,no_run"), Mode::Skip));
        assert!(matches!(
            parse_info("bash,fails_with=2"),
            Mode::FailsWith(2)
        ));
        assert!(matches!(parse_info("bash, no_run"), Mode::Skip));
    }

    #[test]
    fn extract_bash_blocks_finds_quickstart() {
        let md = "\
# Title
```bash
datum switch mood happy
```

```rust
fn main() {}
```

```bash,no_run
cargo install --path crates/datum-cli
```
";
        let blocks = extract_bash_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].info, "bash");
        assert!(blocks[0].body.contains("datum switch"));
        assert_eq!(blocks[1].info, "bash,no_run");
    }

    #[test]
    fn a_transcript_is_the_whole_output_or_a_run_of_it() {
        let md = "```bash\ndatum moment a\n```\n\n```\nMOMENT x1y2z a:2026-01-01T00:00:00.000Z\n```\n\nprose\n\n```\nb\n```\n";
        let block = &extract_bash_blocks(md)[0];
        let (first, second) = (&block.transcripts[0], &block.transcripts[1]);
        assert!(first.direct && !second.direct);
        let printed = "MOMENT 9qq8w a:2026-11-05T20:01:08.065Z\nb\nc\n";
        assert!(
            !transcribes(first, true, printed),
            "whole, so `b` and `c` are missing"
        );
        assert!(
            transcribes(first, false, printed),
            "the hid and instant are masked"
        );
        assert!(transcribes(second, false, printed));
        assert!(!transcribes(second, false, "a\nc\n"));
    }

    #[test]
    fn extract_skips_autogen_region() {
        let md = format!(
            "intro\n{begin}\n```bash\necho do_not_run\n```\n{end}\nafter\n```bash\ndatum tail\n```\n",
            begin = AUTOGEN_BEGIN,
            end = AUTOGEN_END,
        );
        let blocks = extract_bash_blocks(&md);
        assert_eq!(blocks.len(), 1, "autogen region should be skipped");
        assert!(blocks[0].body.contains("datum tail"));
    }
}
