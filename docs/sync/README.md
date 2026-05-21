# daydumb sync — front door

Sync in daydumb is end-to-end encrypted. Devices push opaque ciphertext
to a relay; other devices of the same user pull it back and decrypt
locally. The relay sees `(device_id, seq, ciphertext)` and nothing else.

This page is the umbrella. It explains how the four sync crates fit
together, then walks through standing up a relay, connecting a client,
and running the background agent. For the JSON wire format and the
threat model, see [`docs/sync-protocol.md`](../sync-protocol.md).

## The four crates

| Crate | Role | Deployment target |
|---|---|---|
| [`datum-sync-protocol`](../../crates/datum-sync-protocol/) | Shared wire types — `serde` + `thiserror` only. | WASM-safe. Pulled in by both client and server so the JSON shapes live in exactly one place. |
| [`datum-sync-client`](../../crates/datum-sync-client/) | The push/pull state machine, on-device crypto (AES-256-GCM, key-wrap), and HTTP. | WASM-safe core; native-only `db` and `http` features gated for the PWA. |
| [`datum-sync-server`](../../crates/datum-sync-server/) | The relay: HTTP routes, passkey auth, JWT issuance, opaque-blob storage. | Native daemon (axum + tokio + webauthn-rs + rusqlite). |
| [`datum-sync-agent`](../../crates/datum-sync-agent/) | Background daemon — wraps the client to push-on-write and pull on an interval. | Native daemon (tokio + `notify` filesystem watcher + pid-file). |

### Why four crates instead of one

Deployment-target boundaries are real and load-bearing:

- The **protocol** crate has to compile to WASM untouched so the PWA
  can speak the same wire format as the CLI without a parallel schema.
- The **client** crate has to compile to WASM without dragging in
  `reqwest`, `tokio`, or `rusqlite` — its `http` and `db` features are
  off by default precisely so the PWA build stays minimal.
- The **server** crate depends on `webauthn-rs`, `axum`, and
  `tokio-rustls`, none of which belong anywhere near the WASM clients.
- The **agent** crate depends on filesystem-watcher and pid-file
  primitives that have no place in the library that powers a browser.

A merged single crate would need feature-flag gates throughout to
preserve those boundaries, which is worse than the current split. The
boundaries are stable; the crates stay.

> If a downstream user ever asks for "one crate to depend on for sync,"
> a ~10-LOC umbrella crate `datum-sync` that re-exports the others is
> the right answer. We have not built it yet — hold until requested.

## Standing up a relay

```bash
# Build:
cargo build --release -p datum-sync-server

# Container:
docker build -t datum-sync-server \
    -f crates/datum-sync-server/Dockerfile .

# Run (local):
cargo run --release -p datum-sync-server -- --listen 127.0.0.1:8089
```

Required configuration (flags or env vars; see `--help`):

- **Listen address** — `--listen` / `DATUM_SYNC_LISTEN`.
- **SQLite path** — defaults to `./datum-sync.db`.
- **JWT keys** — RS256 keypair loaded from disk. Generated on first run
  if absent.
- **CORS origins** — required if the PWA will talk to this relay from
  a different origin.

The relay's storage is content-blind: it holds opaque ciphertext blobs,
wrapped master keys, passkey credentials, and JWT bookkeeping. It has
no view into entry contents and no ability to decrypt them.

### First-user / ownership

Today the relay accepts any successful passkey registration. The
single-owner model that ships with `da-0cog` (sync control-plane /
generation counter) will make the first successful registration the
cluster owner; subsequent registrations join that cluster. Until that
lands, treat the relay as single-tenant: register once, then put the
relay behind a firewall or reverse proxy that restricts further
registrations.

## Connecting a client

The user-facing surface is `datum sync ...`. The first-time setup is
three steps, in this order:

```bash
# 1. Point the CLI at your relay (once, global):
datum sync config set-server https://relay.example.com --global

# 2. Authenticate via passkey (device-code flow if headless):
datum sync login

# 3. Initialize the master key (first device) or unlock it (additional devices):
datum sync key init     # first device — generates master key,
                        #   wraps it with a passphrase,
                        #   uploads the wrapped bundle to the relay
datum sync key unlock   # additional devices — fetches the wrapped
                        #   bundle, decrypts with the passphrase
```

After that, `datum sync` is bidirectional pull-then-push; `datum sync
--push` and `datum sync --pull` are one-shot variants; `datum sync
--dry-run` reports without mutating; `datum sync status` shows per-DB
watermarks and conflict counts.

What goes over the wire on the first push:

1. The client packs entries newer than its push watermark into a
   changeset, encrypts with the master key, POSTs to `/v1/push`.
2. The relay stores the ciphertext keyed by `(device_id, seq)` and
   returns the new high-water mark.
3. Other devices see the changeset on their next `/v1/pull` and decrypt
   locally.

`content_hash` is computed over plaintext, on the client, and lives
inside the encrypted payload — the relay never sees it.

## Running the agent

The agent is a long-running daemon that watches your `datumrc.toml`
target list, debounces writes, and runs pushes / pulls without the user
having a CLI open.

```bash
# Build:
cargo build --release -p datum-sync-agent

# Run in the foreground (for testing):
datum-sync-agent

# Install as a systemd user unit (Linux):
# ~/.config/systemd/user/datum-sync-agent.service
# (see crates/datum-sync-agent/README.md for the full unit file)
systemctl --user enable --now datum-sync-agent
```

The agent uses a pid-file for liveness so `datum sync status` can show
"agent running" vs "agent dead." On macOS the equivalent is a `launchd`
plist; Windows is not yet planned.

Per-target tunables (quiet window, pull interval, backoff floor/ceiling)
live alongside the target entry in `datumrc.toml`. Defaults are
conservative. Targets with auto-sync disabled stay manual-only — the
agent skips them entirely.

## Cross-references

- [`datum-sync-protocol/`](../../crates/datum-sync-protocol/) — wire
  types (Rust definitions).
- [`datum-sync-client/README.md`](../../crates/datum-sync-client/README.md)
  — client architecture, module shape, crypto contract.
- [`datum-sync-server/README.md`](../../crates/datum-sync-server/README.md)
  — relay routes, module shape, run instructions.
- [`datum-sync-agent/README.md`](../../crates/datum-sync-agent/README.md)
  — scheduler, debounce / backoff math, systemd unit.
- [`docs/sync-protocol.md`](../sync-protocol.md) — the wire contract:
  JSON shapes, error codes, threat model, key-wrap bundle format.
- [`docs/parity-audit/sync.md`](../parity-audit/sync.md) — compatibility
  notes against the original datum.
