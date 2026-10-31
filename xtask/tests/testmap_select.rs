//! `testmap select` on a change to a file a test reads as data, which coverage
//! cannot see: the test runs, or the whole suite does when the map lacks it.

use std::path::Path;
use std::process::Command;

const KEY: &str = "commands::install::tests::the_release_key_is_quoted_where_it_is_published";
const DOCS: [&str; 3] = [
    "readme_anti_drift::every_generated_block_has_markers_in_its_file",
    "readme_anti_drift::readme_autogen_sections_in_sync",
    "readme_snippets::markdown_bash_snippets_run_against_datum_binary",
];

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// What `select` writes for a commit changing `path`, in a repo whose map names
/// `tests`: the workspace cohort's list, or None for the whole suite.
fn selected(path: &str, tests: &[&str]) -> Option<Vec<String>> {
    let case = format!("{}-{}", path.replace('/', "-"), tests.len());
    let dir = std::env::temp_dir().join(format!("testmap-select-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(dir.join("crates")).unwrap();
    std::fs::write(&file, "one\n").unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["update-ref", "refs/remotes/origin/staging", &base]);
    std::fs::write(&file, "two\n").unwrap();
    git(&dir, &["commit", "-qam", "the change"]);

    let tests: Vec<_> = tests.iter().map(|t| ["workspace", "b", t]).collect();
    let map = serde_json::json!({
        "commit": base, "tests": tests, "sets": [], "files": {}, "always": [],
    });
    let (map_path, out) = (dir.join(".git/map.json"), dir.join(".git/out"));
    std::fs::write(&map_path, map.to_string()).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["testmap", "select"])
        .args([map_path.as_path(), Path::new("origin/staging"), &out])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    let list = (!out.join("FULL").exists()).then(|| {
        let list = std::fs::read_to_string(out.join("workspace.tests")).unwrap();
        list.lines().map(str::to_string).collect()
    });
    let _ = std::fs::remove_dir_all(&dir);
    list
}

#[test]
fn a_change_to_a_file_a_test_reads_selects_that_test() {
    let mut key_and_docs = [DOCS.as_slice(), &[KEY]].concat();
    key_and_docs.sort();
    let map = [DOCS.as_slice(), &[KEY, "reads_nothing"]].concat();
    for (path, want) in [
        ("README.md", key_and_docs.as_slice()),
        ("web/static/agents.md", &key_and_docs),
        ("web/static/install.sh", &[KEY]),
        ("docs/x.md", &DOCS),
        ("web/static/x.txt", &[]),
    ] {
        assert_eq!(selected(path, &map).expect(path), want, "{path}");
    }
}

#[test]
fn a_map_lacking_the_test_that_reads_a_file_runs_the_whole_suite() {
    for path in ["README.md", "web/static/agents.md", "web/static/install.sh"] {
        assert_eq!(selected(path, &DOCS), None, "{path}");
    }
    assert_eq!(selected("docs/x.md", &[KEY]), None);
}
