//! Which build this is: the commit height and the commit (da-5172a), and
//! the version they make, `3.0.<height>-beta (<sha7>)` (da-cv118).
//!
//! Shared by `build.rs`, which hands it to the binary as `DATUM_BUILD`, and
//! by `tests/version.rs`, which runs it against repos of a known shape.
//! `web/scripts/build-id.mjs` computes the same for the page.

use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether the history is cut short: a shallow boundary commit whose own
/// object names a parent the clone left out. `--is-shallow-repository`
/// alone says true of a clone whose one boundary is the root commit, where
/// the count is whole.
fn truncated(dir: &Path) -> bool {
    let shallow = git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "shallow",
        ],
    );
    let boundary = shallow.and_then(|p| std::fs::read_to_string(p).ok());
    boundary.unwrap_or_default().lines().any(|sha| {
        git(dir, &["cat-file", "commit", sha])
            .is_none_or(|c| c.lines().any(|l| l.starts_with("parent ")))
    })
}

/// The commit `git archive` exported this tree from: `COMMIT` is
/// `export-subst` in `.gitattributes`, so git writes the sha over its
/// `$Format:%H$`, and a tree that was never archived keeps the placeholder.
/// It attests the export, not the tree's content now: archive X, edit a
/// file, build, and it still says X — staleness, not modification. And only
/// if the build script ran: `git archive` dates every file at its commit, so
/// a tree extracted where another tree was built (one shared target dir)
/// looks older than that build, and cargo replays the other tree's stamp;
/// `touch` the tree first. Its height is `unknown`, so the version says `0`.
fn exported(dir: &Path) -> Option<String> {
    let stamp = std::fs::read_to_string(dir.join("COMMIT")).ok()?;
    let sha = stamp.trim();
    (sha.len() >= 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sha[..7].to_string())
}

/// `(height, commit)`: `height` is `git rev-list --count` — every commit
/// reachable, the number the forge shows — and `commit` the sha7, with `+`
/// for a dirty tree. `ci_sha` is CI's `GITHUB_SHA`, which names the commit
/// the run was for even when the checkout is a detached merge.
///
/// Either is `unknown` rather than wrong: a shallow clone would count only
/// its own truncated history, and a tree with no git has nothing to count.
/// Such a tree's commit is the one `git archive` wrote into it, if it was
/// archived (`exported`); its height stays `unknown`.
pub fn build_id(dir: &Path, ci_sha: Option<&str>) -> (String, String) {
    let rev = ci_sha.unwrap_or("HEAD");
    let height = (!truncated(dir))
        .then(|| git(dir, &["rev-list", "--count", rev]))
        .flatten();
    let commit = git(dir, &["rev-parse", "--verify", rev])
        .map(|sha| {
            let dirty = ci_sha.is_none()
                && git(dir, &["--no-optional-locks", "status", "--porcelain"])
                    .is_some_and(|s| !s.is_empty());
            format!("{}{}", &sha[..7], if dirty { "+" } else { "" })
        })
        .or_else(|| exported(dir));
    let unknown = || "unknown".to_string();
    (
        height.unwrap_or_else(unknown),
        commit.unwrap_or_else(unknown),
    )
}

/// `{MAJOR}.{MINOR}.{height}[-{PRE}] ({commit})` from Cargo's version: valid
/// semver, the patch the commit height, so versions order as commits do. A
/// height git cannot give is `0`, since semver needs a number there.
/// `web/scripts/build-id.mjs` spells the same.
pub fn version(height: &str, commit: &str) -> String {
    let patch = if height == "unknown" { "0" } else { height };
    let pre = env!("CARGO_PKG_VERSION_PRE");
    format!(
        "{}.{}.{patch}{}{pre} ({commit})",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        if pre.is_empty() { "" } else { "-" },
    )
}
