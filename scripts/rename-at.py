#!/usr/bin/env python3
"""The mechanical half of the column rename `occur_time` -> `at` (da-95avu).

Re-run it on a rebase: reset to the new base, run this, commit its output
alone, and cherry-pick the hand-written commits back on. Old migration bodies
stay as they were: they run against the old column names.
"""
import re
import subprocess

# Longest first: each name is replaced whole, wherever it sits in an identifier.
NAMES = [
    ("entries_field_occurtime", "entries_field_at_unique"),
    ("entries_live_field_occur", "entries_live_field_at"),
    ("entries_field_occur", "entries_field_at"),
    ("entries_occur_time", "entries_at"),
    ("occur_time_precision", "at_uncertainty"),
    ("set_occur_uncertainty", "set_at_uncertainty"),
    ("occur_uncertainty", "at_uncertainty"),
    ("occur_time", "at"),
]
PATTERN = re.compile("|".join(old for old, _ in NAMES))
NEW = dict(NAMES)

# What stays in the old name: the walk up to v75, its tests, stage 2's tests of
# the old spellings, stored fixtures made by an old binary, and this script.
KEEP = re.compile(
    r"^(crates/datum-core/src/migrations/"
    r"|crates/datum-core/tests/migration_roundtrip\.rs$"
    r"|crates/datum-cli/tests/at_surface\.rs$"
    r"|crates/datum-cli/tests/at_column\.rs$"
    r"|crates/datum-cli/tests/fixtures/"
    r"|crates/datum-sync-wire/tests/"
    r"|scripts/rename-at\.py$)"
)

files = subprocess.check_output(["git", "ls-files"], text=True).split("\n")
changed = 0
for path in filter(None, files):
    if KEEP.match(path):
        continue
    try:
        with open(path, encoding="utf-8", newline="") as f:
            text = f.read()
    except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
        continue
    new = PATTERN.sub(lambda m: NEW[m.group(0)], text)
    if new != text:
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(new)
        changed += 1
print(f"{changed} files changed")
