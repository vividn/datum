//! Every relative link in `README.md` and `docs/**/*.md` resolves (da-5bkw2).
//!
//! A link `](target)` whose target is no URL (`scheme:`) and not
//! root-absolute (`/`) names a path from the linking file's directory, which
//! must exist; a `#anchor` on it must be the GitHub slug of a heading in the
//! file it names (the linking file itself where the path is empty).

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn markdown_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read docs/") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            markdown_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// The targets of a file's markdown links, `](target)`, a title dropped.
fn link_targets(text: &str) -> Vec<&str> {
    text.match_indices("](")
        .filter_map(|(i, _)| {
            let rest = &text[i + 2..];
            let end = rest.find([')', ' ', '\n'])?;
            Some(&rest[..end])
        })
        .collect()
}

/// GitHub's slug of a heading: lowercase, every character but `a-z0-9`,
/// space and `-` dropped, spaces to `-`.
fn slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn has_anchor(text: &str, anchor: &str) -> bool {
    text.lines()
        .filter(|l| l.starts_with('#'))
        .any(|l| slug(l.trim_start_matches('#').trim()) == anchor)
}

/// A link this test checks: no URL (`scheme:`) and not root-absolute.
fn relative(target: &str) -> bool {
    let is_url = target
        .split_once(':')
        .is_some_and(|(s, _)| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()));
    !(target.is_empty() || is_url || target.starts_with('/'))
}

/// What is wrong with one relative link from a file at `dir` holding `text`.
fn broken(dir: &Path, text: &str, target: &str) -> Option<String> {
    let (path, anchor) = target.split_once('#').unwrap_or((target, ""));
    let linked;
    let text = if path.is_empty() {
        text
    } else {
        let file = dir.join(path);
        if !file.exists() {
            return Some("no such file".into());
        }
        if anchor.is_empty() {
            return None;
        }
        linked = std::fs::read_to_string(&file).unwrap_or_default();
        &linked
    };
    (!anchor.is_empty() && !has_anchor(text, anchor)).then(|| format!("no heading #{anchor}"))
}

#[test]
fn every_relative_link_in_the_docs_resolves() {
    let root = workspace_root();
    let mut files = vec![root.join("README.md")];
    markdown_under(&root.join("docs"), &mut files);
    let (mut checked, mut failures) = (0, Vec::new());
    for file in &files {
        let text = std::fs::read_to_string(file).expect("read doc");
        let dir = file.parent().expect("a doc's directory");
        for target in link_targets(&text).into_iter().filter(|t| relative(t)) {
            checked += 1;
            if let Some(why) = broken(dir, &text, target) {
                let rel = file.strip_prefix(&root).unwrap_or(file);
                failures.push(format!("{}: ]({target}): {why}", rel.display()));
            }
        }
    }
    assert!(
        checked > 40,
        "only {checked} links read: the walk missed the docs"
    );
    assert!(
        failures.is_empty(),
        "broken links:\n{}",
        failures.join("\n")
    );
}

#[test]
fn an_anchor_no_heading_has_is_broken() {
    let dir = workspace_root().join("docs");
    let text = "# Sync, at last!\n\nsee [it](#sync-at-last) and [no](#sync-at-first)\n";
    assert_eq!(slug("Sync, at last!"), "sync-at-last");
    assert_eq!(broken(&dir, text, "#sync-at-last"), None);
    assert_eq!(
        broken(&dir, text, "#sync-at-first").as_deref(),
        Some("no heading #sync-at-first")
    );
    assert_eq!(
        broken(&dir, "", "README.md#no-such-heading-here").as_deref(),
        Some("no heading #no-such-heading-here")
    );
    assert_eq!(broken(&dir, "", "gone.md").as_deref(), Some("no such file"));
    assert!(!relative("https://example.com/x.md"));
    assert!(!relative("/sync/join"));
    assert!(relative("../docs/sync.md#a-session"));
}
