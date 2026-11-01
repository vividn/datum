#!/usr/bin/env python3
"""A new or changed dependency holds a PR for vividn (da-jks33).

usage: lockfile-check.py BASE HEAD

Reads Cargo.lock and web/pnpm-lock.yaml at both revisions through git, so
nothing of the PR runs. Prints one line for each third-party package HEAD adds
or changes: another version, or at one version another source, checksum or
resolution (a lockfile pointed at another tarball). A package only removed
prints nothing, nor does a workspace crate, which has no source. Exits 1, after
its lines, when Cargo.lock gains a `git` source, and at once when a lockfile
holds no package this reads: a new format must not read as all of them removed.
"""
import re
import subprocess
import sys
import tomllib


def show(rev, path):
    done = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
    return done.stdout if done.returncode == 0 else ""


def cargo(text):
    return {
        (p["name"], p["version"], p["source"], p.get("checksum", ""))
        for p in tomllib.loads(text).get("package", [])
        if "source" in p
    }


def pnpm(text):
    """The `packages:` section, read by its indentation: PyYAML is no dependency of CI's."""
    found, key, inside = {}, None, False
    for line in text.splitlines():
        if line and not line[0].isspace():
            inside = line.rstrip() == "packages:"
        elif inside and (m := re.match(r"  (\S.*?):( .*)?$", line)):
            key = m[1].strip("'\"")
            found[key] = ""
        elif inside and key and (m := re.match(r"    resolution: (.*)$", line)):
            found[key] = m[1]
    rows = set()
    for key, resolution in found.items():
        at = key.index("@", 1)  # a scope's own @ is first, a peer's or a URL's later
        name, version = key[:at], key[at + 1 :]
        rows.add((name, version, resolution))
    return rows


def named(old, new, label):
    gone, came = old - new, new - old
    for row in sorted(came):
        name, version = row[:2]
        was = sorted({g[1] for g in gone if g[0] == name})
        if version in was:
            yield f"{label}: {name} {version} changed its source or checksum"
        elif was:
            yield f"{label}: {name} {', '.join(was)} -> {version}"
        else:
            yield f"{label}: {name} {version} added"


def read(parse, rev, path):
    text = show(rev, path)
    rows = parse(text)
    if text and not rows:
        sys.exit(f"::error::{path} at {rev} holds no package this reads")
    return rows


def main(base, head):
    old, new = read(cargo, base, "Cargo.lock"), read(cargo, head, "Cargo.lock")
    for line in named(old, new, "Cargo.lock"):
        print(line)
    path = "web/pnpm-lock.yaml"
    for line in named(read(pnpm, base, path), read(pnpm, head, path), path):
        print(line)
    git = sorted(f"{r[0]} {r[1]} from {r[2]}" for r in new - old if r[2].startswith("git+"))
    for line in git:
        print(f"::error::Cargo.lock gains a git source: {line}", file=sys.stderr)
    return 1 if git else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
