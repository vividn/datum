# datum-sync-client

Client-side sync library — the engine behind `datum sync` and the
`datum-sync-agent` daemon. Owns the wire types, on-device crypto, and
HTTP client; does not own the relay (that is
[`datum-sync-server`](../datum-sync-server/)) or any CLI surface (that
is [`datum-commands`](../datum-commands/)).

## Architecture

Sync runs as a state machine:

1. **Push** — collect entries newer than the last push watermark, pack
   into a changeset, encrypt with the device's master key, POST to the
   relay.
2. **Pull** — request changesets from other devices since the last
   pull watermark, decrypt, dedupe by `content_hash`, apply via
   [`ops::apply_changeset`].
3. **Watermarks** — per-device monotonic sequence numbers stored in
   the local `sync_state` table; stash + restore on retry.

Conflict resolution is delegated to `datum-core`'s `conflicts` module;
this crate only ferries opaque blobs.

## Shape

| Module       | Default features | WASM-safe |
|--------------|------------------|-----------|
| [`protocol`] | (none)           | ✓         |
| [`crypto`]   | (none)           | ✓         |
| [`config`]   | (none)           | ✓         |
| [`ops`]      | `db`             | native only (rusqlite) |
| [`sync_state`] | `db`           | native only (rusqlite) |
| [`http`]     | `http`           | no (reqwest blocking)  |

A future PWA build depends on this crate with `--no-default-features
--features wasm` and supplies its own HTTP shim:

```bash
cargo build --target wasm32-unknown-unknown -p datum-sync-client \
    --no-default-features --features wasm
```

## Crypto contract

- AES-256-GCM with random per-changeset nonce.
- Master key never leaves the device in plaintext. A wrapped copy is
  stored on the relay (encrypted with the user's recovery passphrase)
  for re-pairing new devices; see [`crypto::key_wrap`].
- Server is content-blind. Plaintext entries, IDs, slugs, and timestamps
  are inside the encrypted payload. The relay sees only `(device_id,
  seq, ciphertext, created_at)`.
- `content_hash` is computed over plaintext; the relay never sees it.

See `docs/sync-protocol.md` for the full wire format.

## Where to look next

- [`datum-sync-server`](../datum-sync-server/) — the relay this client
  talks to.
- [`datum-sync-agent`](../datum-sync-agent/) — the daemon that wraps
  this library.
- `docs/sync-protocol.md` — wire format, wrapped-key bundle, push/pull
  semantics, error codes.
- `docs/parity-audit/sync.md` — sync compatibility map.
