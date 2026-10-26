#!/usr/bin/env python3
"""Integration permit (da-44xpb): is BRANCH listed in .forgejo/integration-branches.toml?

usage: integration-permit.py BRANCH [WEB_TESTS_OUT [WEB_TESTS_GLOB_OUT]]

Exits 0 when it is, writing the entry's `web_tests` to WEB_TESTS_OUT one a line
and its `web_tests_glob` (empty when it has none) to WEB_TESTS_GLOB_OUT, and 1
when it is not, or when the file is missing or does not read: then a PR into
the branch gets the full CI, and a push to it none (ci.yml).
"""
import sys
import tomllib

branch = sys.argv[1]
try:
    with open(".forgejo/integration-branches.toml", "rb") as f:
        entries = tomllib.load(f).get("branch", [])
except (OSError, tomllib.TOMLDecodeError) as e:
    print(f"{branch}: the permit file does not read ({e})")
    sys.exit(1)
entry = next((b for b in entries if b.get("name") == branch), None)
if entry is None:
    print(f"{branch} is not in the permit file")
    sys.exit(1)
if len(sys.argv) > 2:
    with open(sys.argv[2], "w") as f:
        f.writelines(f"{t}\n" for t in entry.get("web_tests", []))
if len(sys.argv) > 3:
    with open(sys.argv[3], "w") as f:
        f.write(entry.get("web_tests_glob", ""))
print(f"{branch} is in the permit file: epic {entry.get('epic')}, review {entry.get('review')}")
