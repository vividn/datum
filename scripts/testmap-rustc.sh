#!/bin/sh
# RUSTC_WORKSPACE_WRAPPER for the test map's build (da-fnt3v): instruments the
# workspace's own crates and none of their dependencies, whose functions were
# nine in ten of the coverage export and hold no line a PR of ours changes.
exec "$@" -C instrument-coverage
