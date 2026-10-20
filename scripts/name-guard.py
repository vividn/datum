#!/usr/bin/env python3
"""Name guard (da-r6o39): no word listed in NAME_GUARD reaches a PR.

usage: name-guard.py BASE [PR_JSON]

NAME_GUARD (env) holds one entry a line, a Python regex searched for
case-insensitively, so a plain word matches as a substring and `\bword\b` as a
whole word only. An entry that does not compile fails, named by its number. It
is searched for in: every line the PR adds (`git diff BASE...HEAD`, a removed line
is allowed) and the path it is in, the message, author and committer of every
commit in BASE..HEAD, and the `title` and `body` of PR_JSON, the forge's PR
object. It never prints a match: only the entry's number and where it is, since
CI logs become public with the repo. An empty or unset NAME_GUARD fails, as a
guard with nothing to look for examined nothing.
"""
import json
import os
import re
import subprocess
import sys


def git(*args):
    return subprocess.run(["git", *args], check=True, stdout=subprocess.PIPE).stdout.decode("utf-8", "replace")


def main(argv, guard):
    entries = []
    for n, e in enumerate((e.strip() for e in guard.splitlines() if e.strip()), 1):
        try:
            entries.append(re.compile(e, re.IGNORECASE))
        except re.error:
            print(f"::error::NAME_GUARD entry #{n} is not a valid regex")
            return 1
    if not entries:
        print("::error::NAME_GUARD secret missing")
        return 1
    print(f"name guard: read {len(entries)} entries")
    found = []

    def check(text, where):
        hits = [f"entry #{n} {where}" for n, e in enumerate(entries, 1) if e.search(text)]
        found.extend(hits)
        return hits

    base = argv[1]
    path, line, header = "", 0, True
    for row in git("-c", "core.quotePath=false", "diff", "-U0", "--text", "--no-color", "--no-ext-diff",
                   "--no-renames", f"{base}...HEAD").splitlines():
        if row.startswith("diff --git "):
            header = True
        elif header and row.startswith("+++ "):
            path = row[6:] if row.startswith("+++ b/") else row[4:]
            if check(path, "in the path of an added file"):
                path = "<a path holding an entry>"
        elif row.startswith("@@"):
            header = False
            line = int(re.match(r"@@ -\S+ \+(\d+)", row).group(1))
        elif row.startswith("+"):
            check(row[1:], f"at {path}:{line}")
            line += 1

    log = git("log", "--format=%H%x00%an <%ae>%x00%cn <%ce>%x00%B%x1e", f"{base}..HEAD")
    for commit in filter(str.strip, log.split("\x1e")):
        sha, author, committer, message = commit.strip("\n").split("\x00", 3)
        for text, what in ((message, "message"), (author, "author"), (committer, "committer")):
            check(text, f"in {sha[:12]} {what}")

    if len(argv) > 2:
        with open(argv[2], encoding="utf-8") as f:
            pr = json.load(f)
        for key in ("title", "body"):
            check(pr.get(key) or "", f"in the PR {key}")

    for line in found:
        print(line)
    if found:
        print(f"::error::name guard: {len(found)} finding(s) of a NAME_GUARD entry")
        return 1
    print("name guard: no entry found")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv, os.environ.get("NAME_GUARD", "")))
