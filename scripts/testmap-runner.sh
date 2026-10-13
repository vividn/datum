#!/bin/sh
# nextest's target runner for the test map's instrumented run (da-fnt3v,
# scripts/testmap-build.sh). nextest runs one process per test and hands
# this its binary id and name in the environment, so each test's counters go
# to a directory of its own: every process under it, the `datum` binaries a
# datum-cli test spawns included, writes there, and the directory is that
# test's coverage. `%p-%m` keeps two processes apart and merges a reused pid.
if [ "$NEXTEST_TEST_PHASE" = run ]; then
  dir="$TESTMAP_PROF/$(printf '%s\t%s\t%s' "$TESTMAP_COHORT" "$NEXTEST_BINARY_ID" "$NEXTEST_TEST_NAME" | sha1sum | cut -c1-20)"
  mkdir -p "$dir"
  printf '%s\t%s\t%s\n' "$TESTMAP_COHORT" "$NEXTEST_BINARY_ID" "$NEXTEST_TEST_NAME" > "$dir/test"
else
  dir="$TESTMAP_PROF/.list"
fi
LLVM_PROFILE_FILE="$dir/%p-%m.profraw" exec "$@"
