//! scripts/mq-check.sh against a fixture repo. With --dry-run (da-678rb): what a
//! can's gate would run for a selection, for the whole-suite answer, and for a PR
//! into an integration branch. Without it, on stubs: what a can's gate runs at and
//! logs (da-9az39).
//!
//! origin/staging is `base` then `moved`, and the maps are of `moved`, so a PR
//! forked at `base` is older than its map, as a can's branch often is.

use std::path::{Path, PathBuf};
use std::process::Command;

const PERMIT: &str = r#"[[branch]]
name = "integration/tutorial"
epic = "da-x"
review = "light"
web_tests_glob = "tests/tutorial*.test.mjs"
web_tests = ["tests/tree.test.mjs", "tests/tutorial.test.mjs"]
"#;

struct Repo {
    dir: PathBuf,
    base: String,
    moved: String,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Repo {
    /// The fixture, with `reaching` web test files besides a.test.mjs all
    /// reaching web/src/a.ts in the web map, checked out on `pr` at `base`.
    fn new(case: &str, reaching: usize) -> Repo {
        let dir = std::env::temp_dir().join(format!("mq-check-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut repo = Repo {
            dir,
            base: String::new(),
            moved: String::new(),
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for f in [
            "scripts/mq-check.sh",
            "scripts/integration-permit.py",
            "web/scripts/testmap.mjs",
        ] {
            let to = repo.dir.join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(root.join(f), to).unwrap();
        }
        repo.git(&["init", "-q", "-b", "main"]);
        repo.write(".forgejo/integration-branches.toml", PERMIT);
        for f in ["web/src/a.ts", "web/src/b.ts", "web/static/x.txt"] {
            repo.write(f, "one\n");
        }
        repo.write("crates/a/src/lib.rs", "pub fn a() -> u8 {\n    1\n}\n");
        repo.write("crates/a/tests/t.rs", "#[test]\nfn old() {}\n");
        let mut reached = vec!["tests/a.test.mjs".to_string()];
        reached.extend((0..reaching).map(|i| format!("tests/many{i}.test.mjs")));
        let others = ["tree", "tutorial", "b"].map(|t| format!("tests/{t}.test.mjs"));
        for t in reached.iter().chain(&others) {
            repo.write(&format!("web/{t}"), "// a test\n");
        }
        repo.base = repo.commit("base");
        repo.write("web/src/b.ts", "two\n");
        repo.moved = repo.commit("moved");
        repo.git(&["update-ref", "refs/remotes/origin/staging", &repo.moved]);

        let files: serde_json::Map<String, serde_json::Value> = reached
            .iter()
            .map(|t| (t.clone(), serde_json::json!(["src/a.ts"])))
            .chain(
                others
                    .iter()
                    .map(|t| (t.clone(), serde_json::json!(["src/b.ts"]))),
            )
            .collect();
        repo.git(&["checkout", "-q", "-b", "maps"]);
        repo.write(
            "testmap-web.json",
            &serde_json::json!({"commit": repo.moved, "files": files, "always": []}).to_string(),
        );
        repo.write(
            "testmap.json",
            &serde_json::json!({
                "commit": repo.moved,
                "tests": [["workspace", "a", "reaches_a"]],
                "sets": [[0]],
                "files": {"crates/a/src/lib.rs": [[1, 3, 0]]},
                "always": [],
            })
            .to_string(),
        );
        let maps = repo.commit("maps");
        repo.git(&["update-ref", "refs/remotes/origin/testmap", &maps]);
        repo.git(&["checkout", "-q", "-b", "pr", &repo.base]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self
            .cmd("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn cmd(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        cmd
    }

    fn write(&self, path: &str, text: &str) {
        let to = self.dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, text).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// The gate's plan, as `role` would run it: (exit ok, stdout + stderr).
    fn plan(&self, role: &str, flags: &[&str]) -> (bool, String) {
        // The gate reads cargo-nextest's presence, not what it does.
        let stubs = self.dir.join(".git/stubs");
        std::fs::create_dir_all(&stubs).unwrap();
        let nextest = stubs.join("cargo-nextest");
        std::fs::write(&nextest, "#!/bin/sh\n").unwrap();
        self.cmd("chmod").arg("+x").arg(&nextest).status().unwrap();
        let path = format!(
            "{}:{}",
            stubs.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = self
            .cmd("bash")
            .args(["scripts/mq-check.sh", "--dry-run"])
            .args(flags)
            .env("PATH", path)
            .env("CT_ROLE", role)
            .env("MQ_XTASK", env!("CARGO_BIN_EXE_xtask"))
            .env_remove("GT_ROLE")
            .env_remove("MQ_BASE")
            .env_remove("MQ_FULL")
            .env_remove("CI")
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    }

    fn can_plan(&self) -> String {
        let (ok, text) = self.plan("daydumb/cans/x", &[]);
        assert!(ok, "{text}");
        text
    }
}

/// The test files the plan lists, its `  tests/…` lines.
fn listed(plan: &str) -> Vec<&str> {
    plan.lines()
        .filter_map(|l| l.strip_prefix("  tests/"))
        .filter(|l| !l.contains('←'))
        .collect()
}

#[test]
fn a_selection_runs_what_the_map_picks_though_the_map_is_newer_than_the_fork() {
    let repo = Repo::new("selection", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.write("crates/a/src/lib.rs", "pub fn a() -> u8 {\n    2\n}\n");
    repo.commit("the change");
    let plan = repo.can_plan();
    assert!(plan.contains("the test map's selection, as CI"), "{plan}");
    assert_eq!(listed(&plan), ["a.test.mjs"], "{plan}");
    assert!(
        plan.contains("Rust tests (workspace): cargo nextest run --workspace --no-fail-fast --no-tests=pass -- --exact reaches_a"),
        "{plan}"
    );
    assert!(plan.contains("(e2e-full): no test reaches"), "{plan}");
}

#[test]
fn the_whole_suite_answer_runs_the_prs_own_tests_and_says_why() {
    let repo = Repo::new("whole", 0);
    repo.write("web/static/x.txt", "changed\n");
    repo.write("web/tests/b.test.mjs", "// edited\n");
    repo.write("Cargo.lock", "# changed\n");
    repo.write(
        "crates/a/tests/t.rs",
        "#[test]\nfn old() {}\n#[test]\nfn mine() {}\n",
    );
    repo.commit("the change");
    let plan = repo.can_plan();
    assert!(
        plan.contains("web/static/x.txt: the web suite reads it"),
        "{plan}"
    );
    assert!(plan.contains("CI runs the rest"), "{plan}");
    assert_eq!(listed(&plan), ["b.test.mjs"], "{plan}");
    assert!(plan.contains("Cargo.lock: a manifest"), "{plan}");
    assert!(plan.contains("--no-tests=pass mine"), "{plan}");
    assert!(!plan.contains("--no-tests=pass old"), "{plan}");
}

#[test]
fn no_own_test_on_the_whole_suite_answer_runs_nothing_and_passes() {
    let repo = Repo::new("whole-none", 0);
    repo.write("web/static/x.txt", "changed\n");
    repo.commit("the change");
    let plan = repo.can_plan();
    assert!(plan.contains("web tests: 0 files"), "{plan}");
}

#[test]
fn a_selection_over_the_threshold_is_the_whole_suite_answer() {
    let script = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/mq-check.sh"),
    )
    .unwrap();
    let max: usize = script
        .lines()
        .find_map(|l| l.strip_prefix("MAX_LOCAL_WEB_FILES="))
        .unwrap()
        .parse()
        .unwrap();

    let at = Repo::new("at-threshold", max - 1);
    at.write("web/src/a.ts", "changed\n");
    at.commit("the change");
    let plan = at.can_plan();
    assert_eq!(listed(&plan).len(), max, "{plan}");

    let over = Repo::new("over-threshold", max);
    over.write("web/src/a.ts", "changed\n");
    over.write("web/tests/b.test.mjs", "// edited\n");
    over.commit("the change");
    let plan = over.can_plan();
    assert!(plan.contains("more than MAX_LOCAL_WEB_FILES"), "{plan}");
    assert_eq!(listed(&plan), ["b.test.mjs"], "{plan}");
}

/// `pr` on `integration/tutorial`, which left staging at `base`, so the maps'
/// commit is no ancestor of it.
fn on_tutorial(case: &str) -> Repo {
    let repo = Repo::new(case, 0);
    repo.write("web/tutorial.txt", "the branch's own\n");
    let tip = repo.commit("on the branch");
    repo.git(&[
        "update-ref",
        "refs/remotes/origin/integration/tutorial",
        &tip,
    ]);
    repo
}

#[test]
fn a_pr_into_an_integration_branch_runs_the_permits_list() {
    let repo = on_tutorial("tutorial");
    repo.write("web/src/a.ts", "changed\n");
    repo.write("web/tests/b.test.mjs", "// edited\n");
    repo.commit("the change");
    let plan = repo.can_plan();
    assert!(plan.contains("a PR into integration/tutorial"), "{plan}");
    assert!(plan.contains("the permit's list"), "{plan}");
    assert!(!plan.contains("not the selection"), "{plan}");
    assert_eq!(
        listed(&plan),
        ["b.test.mjs", "tree.test.mjs", "tutorial.test.mjs"],
        "{plan}"
    );
}

#[test]
fn an_integration_branchs_red_list_is_red_here_too() {
    let repo = on_tutorial("tutorial-glob");
    repo.write("web/tests/tutorial-new.test.mjs", "// new\n");
    repo.commit("the change");
    let (ok, text) = repo.plan("daydumb/cans/x", &[]);
    assert!(!ok && text.contains("is not in its web_tests"), "{text}");
}

#[test]
fn an_integration_branchs_own_tip_is_its_final_pr_into_staging() {
    let repo = on_tutorial("tutorial-tip");
    let plan = repo.can_plan();
    assert!(!plan.contains("a PR into integration"), "{plan}");
    assert!(plan.contains("not the selection"), "{plan}");
}

#[test]
fn the_flag_and_the_refinery_run_the_whole_web_suite() {
    let repo = Repo::new("full", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.commit("the change");
    for (role, flags) in [
        ("daydumb/cans/x", &["--full"][..]),
        ("daydumb/alchemiter", &[][..]),
    ] {
        let (ok, plan) = repo.plan(role, flags);
        assert!(ok, "{plan}");
        assert!(
            plan.contains("running the web check and suite"),
            "{role}: {plan}"
        );
        assert!(!plan.contains("web tests:"), "{role}: {plan}");
    }
}

/// A can's real gate, with `pnpm` and `cargo` a stub that says what it was given
/// and then runs `then`: its child, for the test to signal its bash, and the path
/// of what the stub said.
fn gate(repo: &Repo, then: &str) -> (std::process::Child, PathBuf) {
    let stubs = repo.dir.join(".git/stubs");
    std::fs::create_dir_all(&stubs).unwrap();
    let said = repo.dir.join(".git/said");
    let stub = format!(
        "#!/bin/sh\necho \"files=$DATUM_TEST_CONCURRENCY nice=$(nice)\" >>{}\n{then}\n",
        said.display()
    );
    for name in ["pnpm", "cargo"] {
        let to = stubs.join(name);
        std::fs::write(&to, &stub).unwrap();
        repo.cmd("chmod").arg("+x").arg(&to).status().unwrap();
    }
    // Seen on PATH, or the gate's fallback puts the real pnpm ahead of these. The
    // selection it fails is the dry runs' to test: the gate reads none and goes on.
    let node = stubs.join("node");
    std::fs::write(&node, "#!/bin/sh\nexit 1\n").unwrap();
    repo.cmd("chmod").arg("+x").arg(&node).status().unwrap();
    let path = format!(
        "{}:{}",
        stubs.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = std::fs::File::create(repo.dir.join(".git/gate.out")).unwrap();
    let child = repo
        .cmd("bash")
        .arg("scripts/mq-check.sh")
        .env("PATH", path)
        .env("TMPDIR", repo.dir.join(".git"))
        .env("CT_ROLE", "daydumb/cans/x")
        .env("DATUM_TEST_CONCURRENCY", "3")
        .env_remove("GT_ROLE")
        .env_remove("MQ_BASE")
        .env_remove("MQ_FULL")
        .env_remove("CI")
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    (child, said)
}

fn gate_log(repo: &Repo) -> String {
    std::fs::read_to_string(repo.dir.join(".git/datum-gates.log")).unwrap_or_default()
}

#[test]
fn a_cans_gate_runs_six_files_at_nice_10_whatever_it_pinned_and_logs_what_it_cost() {
    let repo = Repo::new("gate-line", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.commit("the change");
    let (mut child, said) = gate(&repo, "exit 0");
    assert!(child.wait().unwrap().success());
    let said = std::fs::read_to_string(said).unwrap();
    assert!(said.lines().last().unwrap() == "files=6 nice=10", "{said}");
    let log = gate_log(&repo);
    assert!(log.contains(" daydumb/cans/x ran="), "{log}");
    assert!(log.contains("s exit=0 load="), "{log}");
}

#[test]
fn a_failed_gate_logs_its_status_and_exits_with_it() {
    let repo = Repo::new("gate-red", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.commit("the change");
    let (mut child, _) = gate(&repo, "exit 7");
    assert_eq!(child.wait().unwrap().code(), Some(7));
    let log = gate_log(&repo);
    assert!(log.contains("s exit=7 load="), "{log}");
}

#[test]
fn a_killed_gate_does_not_log_as_passed() {
    let repo = Repo::new("gate-killed", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.commit("the change");
    // Its child ends with 0, the status a killed bash would otherwise log.
    let (mut child, said) = gate(&repo, "sleep 1; exit 0");
    while !said.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            let out = std::fs::read_to_string(repo.dir.join(".git/gate.out")).unwrap();
            panic!("the gate ended with {status} before its stub ran:\n{out}");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    repo.cmd("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(143));
    let log = gate_log(&repo);
    assert!(log.contains("s exit=143 load="), "{log}");
}

#[test]
fn a_dry_run_logs_nothing() {
    let repo = Repo::new("gate-none", 0);
    repo.write("web/src/a.ts", "changed\n");
    repo.commit("the change");
    repo.can_plan();
    assert_eq!(gate_log(&repo), "");
}
