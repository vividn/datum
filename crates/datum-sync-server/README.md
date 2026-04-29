# datum-sync-server

Relay server for daydumb's E2E-encrypted sync. Owns HTTP routes,
WebAuthn passkey auth, JWT issuance, and storage of opaque encrypted
changesets and wrapped key bundles. The server is **content-blind**:
it never sees plaintext entries, master keys, or `content_hash` values
— only ciphertext, device IDs, and sequence numbers.

This crate is shipped as **both** a library and a binary. The library
is exposed so integration tests in sibling crates (notably
`datum-cli/tests/sync_support`) can spawn the server in-process instead
of stubbing it out.

## HTTP routes

All routes live under `/v1` per `docs/sync-protocol.md`:

| Route                       | Method | Auth     | Purpose                              |
|-----------------------------|--------|----------|--------------------------------------|
| `/v1/health`                | GET    | none     | Liveness check.                      |
| `/v1/auth/register/start`   | POST   | none     | Begin passkey registration.          |
| `/v1/auth/register/finish`  | POST   | reg-state| Complete registration, mint JWT.     |
| `/v1/auth/login/start`      | POST   | none     | Begin passkey assertion.             |
| `/v1/auth/login/finish`     | POST   | login-state | Complete login, mint JWT.         |
| `/v1/auth/device-code/*`    | POST   | varies   | Headless device-code flow for CLI.   |
| `/v1/keys/wrapped`          | GET/PUT| JWT      | Read/write the user's wrapped master key bundle. |
| `/v1/push`                  | POST   | JWT      | Upload a changeset blob (opaque ciphertext). |
| `/v1/pull`                  | GET    | JWT      | Stream changesets newer than `?since=`. |

## Shape

| Module          | What it owns                                                  |
|-----------------|---------------------------------------------------------------|
| [`auth`]        | Passkey-based registration/login (WebAuthn) + JWT extraction. |
| [`device_code`] | Device-code OAuth flow for the CLI.                           |
| [`jwt`]         | Token signing/verification. RS256 keys loaded from disk.      |
| [`keys`]        | Wrapped-key bundle storage (opaque to the server).            |
| [`sync`]        | Push/pull handlers; persistence of opaque blobs.              |
| [`db`]          | SQLite connection management (separate from `datum-core::db`). |
| [`models`]      | Auth/user record types and JWT claims.                        |
| [`error`]       | [`AppError`](error::AppError) — uniform HTTP error mapping.   |

## Run instructions

```bash
# Local dev (SQLite at ./datum-sync.db, JWT keys in ./keys/):
cargo run --release -p datum-sync-server -- --listen 127.0.0.1:8089

# Container build:
docker build -t datum-sync-server -f crates/datum-sync-server/Dockerfile .
```

Configure CORS origins, listen address, and key paths via flags or
environment variables (see `cargo run -p datum-sync-server -- --help`).

## What does NOT belong here

- Plaintext-anything. If a route would need to look at entry contents,
  it is in the wrong crate.
- Conflict resolution — that runs on the client in
  `datum-core`'s `conflicts` module.
- CLI parsing, output formatting — see
  [`datum-commands`](../datum-commands/).

## Where to look next

- [`datum-sync-client`](../datum-sync-client/) — the client that talks
  to these routes.
- `docs/sync-protocol.md` — wire format, error codes, threat model.
