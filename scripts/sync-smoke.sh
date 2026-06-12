#!/usr/bin/env bash
# End-to-end smoke for the sync bootstrap: real datum-sync-server + two
# datum CLI "devices" syncing one entry, authenticated via a minted token
# (DATUM_SYNC_TOKEN). Proves the mint-token + env-override path stood up by
# da-... Not a CI test (spawns a server, binds a port); a hand-run verifier.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRV=target/debug/datum-sync-server
DM=target/debug/datum
SECRET=0123456789abcdef0123456789abcdef   # 32 bytes; test-only
PORT=8787
URL="http://localhost:$PORT"
TMP="$(mktemp -d)"
RELAY_DB="$TMP/relay.db"
PASS="correct horse battery staple"
trap 'kill "$SRVPID" 2>/dev/null; rm -rf "$TMP"' EXIT

say() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

say "start relay"
DATUM_SYNC_JWT_SECRET="$SECRET" DATUM_SYNC_DB="$RELAY_DB" DATUM_SYNC_PORT="$PORT" \
  "$SRV" >"$TMP/relay.log" 2>&1 &
SRVPID=$!
for _ in $(seq 1 50); do curl -fsS "$URL/health" >/dev/null 2>&1 && break; sleep 0.2; done
curl -fsS "$URL/health" >/dev/null && echo "relay up on $PORT" || { echo "relay FAILED"; cat "$TMP/relay.log"; exit 1; }

say "mint a token for 'vividn'"
MINT=$(DATUM_SYNC_JWT_SECRET="$SECRET" DATUM_SYNC_DB="$RELAY_DB" "$SRV" mint-token vividn)
echo "$MINT"
TOKEN=$(echo "$MINT" | sed -n 's/^export DATUM_SYNC_TOKEN=//p')
[ -n "$TOKEN" ] && echo "got token (${#TOKEN} chars)" || { echo "mint FAILED"; exit 1; }

say "stage 1: minted token is accepted by the real server (authenticated pull → 200, not 401)"
CODE=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $TOKEN" \
  "$URL/v0/pull?since=0&device=probe&cluster_id=vividn&generation=0")
echo "pull status with token: $CODE"; [ "$CODE" = "200" ] || { echo "stage 1 FAILED"; exit 1; }
CODE=$(curl -s -o /dev/null -w '%{http_code}' "$URL/v0/pull?since=0&device=probe&cluster_id=vividn&generation=0")
echo "pull status WITHOUT token: $CODE (expect 401)"; [ "$CODE" = "401" ] || { echo "stage 1b FAILED"; exit 1; }

say "stage 2: CLI consumes DATUM_SYNC_TOKEN (error moves past auth to 'master key')"
mkdir -p "$TMP/a" "$TMP/b"
A_ENV=(env DATUM_HOME="$TMP/a-home" DATUM_CONFIG_HOME="$TMP/a-cfg" DATUM_SYNC_TOKEN="$TOKEN")
NOAUTH=$("${A_ENV[@]:0:3}" "$DM" --db "$TMP/a/datum.db" sync --server "$URL" 2>&1 | tail -1)
WITHAUTH=$("${A_ENV[@]}" "$DM" --db "$TMP/a/datum.db" sync --server "$URL" 2>&1 | tail -1)
echo "no-token : $NOAUTH"
echo "token    : $WITHAUTH"
echo "$WITHAUTH" | grep -qi "master key" && echo "✓ token consumed (reached key step)" || echo "… (inspect above)"

say "stage 3: full two-device sync (passphrase prompts driven through a pty via script(1))"
# Device A: set server, init the E2E master key (passphrase asked twice),
# log an entry, push. `script -qec` gives the CLI a pty so rpassword's
# /dev/tty read succeeds; piped stdin feeds the passphrase. This is exactly
# the runbook a human runs interactively — the pty just stands in for typing.
export DATUM_HOME="$TMP/a-home" DATUM_CONFIG_HOME="$TMP/a-cfg" DATUM_SYNC_TOKEN="$TOKEN"
"$DM" --db "$TMP/a/datum.db" sync config set-server "$URL" --global >/dev/null 2>&1
printf '%s\n%s\n' "$PASS" "$PASS" | script -qec "$DM --db $TMP/a/datum.db sync key init" /dev/null >"$TMP/a-keyinit.log" 2>&1
echo "A key init: $(grep -av '^$' "$TMP/a-keyinit.log" | tail -1)"
"$DM" --db "$TMP/a/datum.db" occur coffee >/dev/null 2>&1
"$DM" --db "$TMP/a/datum.db" sync >"$TMP/a-sync.log" 2>&1; echo "A sync: $(tail -1 "$TMP/a-sync.log")"

# Device B: fresh home (distinct device id), unlock the same key with the
# same passphrase, pull.
export DATUM_HOME="$TMP/b-home" DATUM_CONFIG_HOME="$TMP/b-cfg" DATUM_SYNC_TOKEN="$TOKEN"
"$DM" --db "$TMP/b/datum.db" sync config set-server "$URL" --global >/dev/null 2>&1
printf '%s\n' "$PASS" | script -qec "$DM --db $TMP/b/datum.db sync key unlock" /dev/null >"$TMP/b-keyunlock.log" 2>&1
echo "B key unlock: $(grep -av '^$' "$TMP/b-keyunlock.log" | tail -1)"
"$DM" --db "$TMP/b/datum.db" sync >"$TMP/b-sync.log" 2>&1; echo "B sync: $(tail -1 "$TMP/b-sync.log")"
echo "--- B tail ---"; "$DM" --db "$TMP/b/datum.db" tail 2>&1 | tail -5
if "$DM" --db "$TMP/b/datum.db" tail 2>&1 | grep -q coffee; then
  echo; echo "✓✓ STAGE 3 PASS — A logged 'coffee', it synced through the relay to B"
else
  echo; echo "✗ stage 3 incomplete — see $TMP logs"; exit 1
fi