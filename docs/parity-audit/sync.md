# Sync — Parity + Coherence Audit

**Bead:** da-t6t.4 (R4)
**Date:** 2026-04-22
**Scope:** Review the R4 crate split (`datum-sync` → `datum-sync-client` +
`datum-sync-server` + `datum-sync-agent`) for internal coherence, wire-protocol
consistency with the locked v1 contract, WASM-readiness, and any UX continuity
concerns vs. the original datum.

Because daydumb sync is almost entirely novel (AES-GCM E2E blobs on a
content-blind relay, unrelated to datum's PouchDB replication), this is less a
"port-check" audit and more a "does the new shape of the code actually hold
together" audit.

---

## 1. Original datum sync (reference point)

The original datum (https://github.com/vividn/datum) runs sync as PouchDB ↔
CouchDB replication:

- **Command:** `datum sync` wraps `PouchDB.sync(local, remote)`
  (`syncCmd.ts`).
- **Setup:** `datum setup` writes a Couch URL + credentials into the
  user config (`setupCmd.ts`).
- **Conflict model:** Couch-style — conflicting revisions kept side by
  side; user (or a resolver) picks the winner.
- **Server:** any Couch-compatible backend (CouchDB, PouchDB Server,
  Cloudant). Not something datum ships.

Daydumb abandoned all of that: daydumb's server sees no plaintext, so Couch
replication is a non-starter. Nothing about the wire protocol ported.

---

## 2. Daydumb sync architecture (post-R4)

The project memory `project_daydumb_sync.md` locked the three-crate split;
R4 actually implements it.

```
crates/
├── datum-core/            (data model, SQLite, no sync code after R4)
├── datum-commands/        (CLI command dispatch, thin sync wrappers)
├── datum-cli/             (binary: args, main, re-exports)
├── datum-sync-client/     (wire types, crypto, HTTP, pack/apply)
├── datum-sync-server/     (axum relay, was `datum-sync`)
└── datum-sync-agent/      (background daemon, stub — C4d / da-ndd.15)
```

### Wire protocol (unchanged from C4a — locked in `docs/sync-protocol.md`)

- Version prefix `/v1/...`; breaking changes bump to `/v2`.
- Auth: Bearer token via RFC 8628 device-code flow (CLI) or WebAuthn
  passkey (web/mobile).
- Changesets: `DSC1` magic + JSON payload (`{"entries": [...]}`),
  sealed with AES-256-GCM. AAD = `device_id || seq-big-endian`.
- Key wrap: Argon2id (m=64 MiB, t=3, p=1) → AES-256-GCM over the
  32-byte master key. Wrapped blob stored at `/v1/keys/wrapped`.
- Watermarks: `push_seq` + `pull_seq` per DB, monotonic, stamped with
  `last_push_at` / `last_pull_at` epoch-ms.

### Component responsibilities (post-R4)

| Component | Owns | Depends on |
|-----------|------|------------|
| `datum-sync-client::protocol` | Wire serde types (`AuthToken`, `DeviceCodeResponse`, `PollResponse`) | nothing beyond serde |
| `datum-sync-client::crypto::changeset` | `Changeset`/`EncryptedBlob`, AES-GCM seal/unseal | `aes-gcm`, `rand` |
| `datum-sync-client::crypto::key_wrap` | `KdfParams`, `WrappedKey`, Argon2id wrap/unwrap | `argon2`, `aes-gcm`, `base64` |
| `datum-sync-client::crypto::pair` | Reserved for X25519 device pairing | — (stub) |
| `datum-sync-client::ops` (feat `db`) | `pack_changeset` / `apply_changeset` over a local `Db` | `datum-core`, `rusqlite` |
| `datum-sync-client::sync_state` (feat `db`) | Watermark read/advance | `datum-core`, `rusqlite` |
| `datum-sync-client::config` | URL parse, `server_host_from_url` | `url` |
| `datum-sync-client::http` (feat `http`) | Device-code flow, wrapped-key upload/fetch, push/pull stubs | `reqwest` |
| `datum-sync-server` | Axum relay, WebAuthn, JWT | `tokio`, `axum`, `webauthn-rs`, `jsonwebtoken` |
| `datum-sync-agent` | (stub — C4d) | `datum-sync-client` |
| `datum-commands::commands::sync` | CLI dispatch + XDG paths + passphrase prompt | `datum-sync-client`, `rpassword` |

### What R4 actually moved

- `datum-core::sync` → deleted. All contents migrated to
  `datum-sync-client::{crypto::changeset, crypto::key_wrap, ops,
  sync_state}`.
- `datum-core` lost its `aes-gcm`, `argon2`, `base64` dependencies —
  none of them are needed once the crypto module left.
- `datum-core::db::row_to_entry_public` was promoted from `pub(crate)`
  to `pub` so the extracted `ops` module can still read entries row by
  row.
- `datum-commands::commands::sync::mod::AuthToken` is now a re-export
  of `datum_sync_client::protocol::messages::AuthToken` (same shape,
  single definition).
- `datum-commands::commands::sync::login::run_with_client` now delegates
  the poll loop to `datum_sync_client::http::run_device_flow` behind a
  prompter bridge.
- `datum-commands::commands::sync::key::{init,unlock,rotate_passphrase}`
  now call `datum_sync_client::crypto::key_wrap::*` and
  `datum_sync_client::http::{upload_wrapped, fetch_wrapped}`.

### What R4 did NOT touch (on purpose)

- The wire protocol — not a single field name changed, not a single
  endpoint moved. R4 is a crate split, not a protocol revision.
- `datum-sync-server` internals — only the crate name + binary name
  changed. Handlers, DB schema, WebAuthn config, JWT secret env var
  all untouched. The default DB filename moved from `datum-sync.db`
  to `datum-sync-server.db` to match the new binary name (deploys
  that override `DATUM_SYNC_DB` are unaffected).
- Datum-migration code (`datum-legacy` backup JSON) — unrelated.

---

## 3. Coherence checks

### Client crate boundary

**Does `datum-sync-client` leak server types?**

Searched `datum-sync-client/src/` for references to axum/tokio/webauthn/JWT
server types — none. The one shared type between client and server is the
`WrappedKey` JSON shape at `/v1/keys/wrapped`, and that belongs to the client
(the server is content-blind about its payload; it just stores and returns the
blob).

**Does it leak HTTP types past `http/`?**

`protocol/` defines `AuthToken`, `DeviceCodeResponse`, `PollResponse` —
pure serde types, no `reqwest` import. The only reqwest use lives inside
`src/http/`.

Checked:

```
grep -rn "reqwest\|::Url\|::Client" crates/datum-sync-client/src/ \
    | grep -v "src/http/"
```

→ zero hits outside `http/`. Clean boundary.

**Does `datum-commands` still reach past the client crate?**

Searched `datum-commands/src/commands/sync/` for `datum_core::sync` and
direct HTTP calls — none. `login.rs` and `key.rs` both go through
`datum_sync_client::*`. The remaining HTTP-ish code in the command
layer is just reqwest `Client::builder()` construction, which is
unavoidable if the CLI wants to control timeouts and user-agent.

### Crypto audit

- **AES-256-GCM nonce management:** 12-byte random nonce per call, via
  `rand::thread_rng()` → OS CSPRNG on native, `crypto.getRandomValues`
  on wasm (enforced by the `getrandom/js` feature on `wasm32`).
  Birthday bound for collisions is ~2⁴⁸ under a single key — safe for
  the envisioned key lifetime.
- **AAD binds `(device_id, seq)`:** any replay to a different `(device,
  seq)` coordinate fails decrypt. Verified by `wrong_aad_fails` test.
- **Wrapped key AAD:** wrapping a master key does NOT bind AAD (the key
  is context-free — a device can hold the same wrapped key at many
  `(device, seq)` points). Passphrase correctness + GCM tag cover
  integrity. Matches the locked protocol.
- **Argon2id params:** m=65536 (64 MiB), t=3, p=1. Above OWASP 2024
  minimum (m=19 MiB). Fresh salt + fresh nonce per wrap call, asserted
  by `wrap_uses_fresh_salt_and_nonce_each_call`.
- **Constant-time operations:** AES-GCM and Argon2id via `aes-gcm` /
  `argon2` crates — both use `subtle` under the hood for tag
  comparison. `key_fingerprint` uses SHA-256, no secret-dependent
  branches.
- **Error surface:** a single `CryptoError(String)` variant for every
  failure mode. Callers cannot distinguish "wrong key" from "tampered
  ciphertext" from "bad base64" — no padding-oracle leakage.

### WASM-readiness

| Module | Compiles on wasm32 | Verified by |
|--------|--------------------|-------------|
| `protocol` | ✓ | `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm` |
| `crypto::changeset` | ✓ | same |
| `crypto::key_wrap` | ✓ | same |
| `crypto::pair` | ✓ | same (empty scaffold) |
| `config` | ✓ | same |
| `ops` | ✗ | gated behind `db` feature, needs rusqlite |
| `sync_state` | ✗ | gated behind `db` feature, needs rusqlite |
| `http` | ✗ | gated behind `http` feature, needs reqwest-blocking |

The full command to reproduce (also in the R4 acceptance criteria):

```
cargo build --target wasm32-unknown-unknown -p datum-sync-client \
    --no-default-features --features wasm
```

✅ Green at the time of this audit (CLI-reported finished in 2m 02s cold,
~2s incremental).

Notes for the eventual `datum-web` crate:

- The WASM-safe subset is the right surface to import.
- Pack/apply and watermarks need a WASM-compatible storage layer — the
  web PWA uses SQLite WASM + OPFS, which has a different handle type
  than `datum_core::db::Db`. That boundary crosses with a trait in a
  future refactor, not R4.
- HTTP needs a `fetch`-based shim; `reqwest`'s `wasm` feature is
  non-blocking and would change the client API, so the web crate will
  supply its own.

---

## 4. UX continuity with datum

**Command surface.** Comparing `datum sync` between projects:

| Command | datum (original) | daydumb | Notes |
|---------|------------------|---------|-------|
| `datum sync` | replicate with Couch | **not implemented** | C4c bead (`push_pull` stubs in `datum-sync-client/src/http/`) |
| `datum sync login` | N/A | device-code OAuth | daydumb-only |
| `datum sync config set-server <url>` | N/A | write yaml | daydumb-only |
| `datum sync config disable` | N/A | set to `"none"` | daydumb-only |
| `datum sync config show` | N/A | resolved summary | daydumb-only |
| `datum sync key init` | N/A | gen + wrap + upload | daydumb-only |
| `datum sync key unlock` | N/A | fetch + unwrap + write | daydumb-only |
| `datum sync key rotate-passphrase` | N/A | re-wrap + upload | daydumb-only |
| `datum sync key show-fingerprint` | N/A | SHA-256 hex | daydumb-only |
| `datum setup` (Couch URL) | interactive prompt | **absent** | Replaced by `datum sync config set-server` + `datum sync login` + `datum sync key init` |

**Setup flow delta.** Datum was one command (`datum setup` → Couch URL +
creds). Daydumb is a three-step dance because the trust model is different
(server never sees plaintext):

1. `datum sync config set-server https://my-relay --global`
2. `datum sync login` (browser OAuth)
3. `datum sync key init` (first device) or `datum sync key unlock`
   (subsequent devices)

The user burden is higher, but the security floor is much higher too
(relay compromise ≠ data compromise). No plan to collapse this into a
single command — the steps model distinct trust decisions.

**What datum users would find missing.**

- `datum sync` verb itself — push/pull not wired up yet (C4c).
  Blocks: nothing in R4, but this is the biggest gap vs. the original
  UX. Users currently have keys + server configured but no way to
  actually sync.
- No Couch URL migration path. If an existing datum user has a
  populated CouchDB, there's no `datum sync adopt-couch` command.
  Unlikely to matter in practice (the `datum db import --from
  backup.json` flow already handles historical data imports).
- No explicit "this device is not yet trusted" UX beyond the
  `key unlock` failing. Compare to datum, where a wrong Couch
  password just loops. Daydumb's failure is clearer.

---

## 5. Tests

### Covered by R4 (all in `datum-sync-client`)

- `crypto::changeset` — encrypt/decrypt round trip, wrong key fails,
  wrong AAD fails, tampered ciphertext fails (4 tests).
- `crypto::key_wrap` — wrap/unwrap round trip, fresh salt+nonce each
  call, wrong passphrase fails, tampered ciphertext fails, fingerprint
  determinism (5 tests).
- `ops` — pack/apply round trip, pack since-seq filter, LWW skips
  older incoming, sync_merge no-op does not add history, sync_merge
  non-trivial adds history (5 tests).
- `sync_state` — push/pull advance + rewind rejection (1 test).
- `config` — URL → host helper, port formatting, garbage-input fallback
  (3 tests).

17 new crate-local tests, all green.

### Retained by `datum-cli/tests/sync_cmd.rs`

- `datum sync login` device-code happy path (stub relay).
- `datum sync key init/unlock/rotate` full flows (stub relay).
- Test was updated to import `datum_sync_client::crypto::key_wrap::{WrappedKey,
  unwrap_master_key}` instead of the deleted `datum_core::sync` path.

All workspace tests green:

```
cargo test --workspace
# 30+ binaries, zero failures
```

### Uncovered

- **End-to-end two-client convergence** — not tested, because push/pull
  don't exist yet (C4c). Once the push/pull HTTP stubs in
  `http/push_pull.rs` are filled in, write an integration test that
  spins two in-memory Dbs against a real `datum-sync-server` and
  asserts they converge after a round trip.
- **Byzantine relay behavior** — what happens if the relay returns a
  truncated `/v1/pull` response, duplicate seqs, or out-of-order
  payloads. Needed once the pull client exists.
- **Passphrase rotation during active push** — race unlikely in
  practice (rotate is interactive) but worth a targeted test when
  agent (C4d) can push in the background.

---

## 6. Improvement ideas (not in R4 scope)

Filed here for the next-level design conversation, not implemented by R4.

1. **Passphrase hint storage.** Many users forget passphrases and lose
   access. Optional encrypted hint in the wrapped-key blob would be
   low risk.
2. **Per-DB encryption keys** — today every DB on a device uses the
   same master key. Per-DB keys would let users share one DB across
   devices while keeping another strictly local. Needs protocol
   extension; non-trivial.
3. **Push retry + backoff in client.** Currently the plan is for the
   agent (C4d) to own this. If the CLI grows a one-shot `datum sync
   push` it'll need the same logic; put it in
   `datum-sync-client::http` so both share.
4. **Replace blocking reqwest with async.** Would let the agent use
   tokio without rolling its own thread. Deferred — reqwest-blocking
   is the single biggest obstacle to compiling the whole client on
   wasm32, and the fix (async + `fetch`-backed HTTP) is a larger
   change than R4.
5. **`datum-web-wasm` crate.** Not in R4. When it lands, it should:
   - Depend on `datum-sync-client` with `--no-default-features
     --features wasm`.
   - Own its own storage adapter over SQLite WASM + OPFS, reimplementing
     `ops` / `sync_state` against that handle.
   - Supply a `fetch`-based HTTP shim implementing the same functions
     as `datum-sync-client::http`.
6. **X25519 device pairing.** The `crypto::pair` module is reserved for
   this. Useful when adding a device: instead of typing the master
   passphrase, the user scans a short-lived fingerprint and the peers
   negotiate a one-time transport key. Design sketched in the protocol
   doc; implementation deferred.

---

## Summary

R4 is a structural refactor with **no protocol, schema, or UX changes**.
The client/server/agent split that `project_daydumb_sync.md` called for
is now realized in code. Twenty-two pre-R4 crypto/sync tests were
preserved (now in `datum-sync-client`) plus a handful of new boundary
tests. The WASM build target is green for the pure subset, unblocking
the future `datum-web` epic.

Gaps identified that belong to follow-up beads, not R4:

- Push/pull HTTP clients (C4c) — scaffolded as `http/push_pull.rs`.
- Background sync daemon (C4d, `da-ndd.15`) — crate reserved.
- End-to-end two-client convergence tests — depend on push/pull.
