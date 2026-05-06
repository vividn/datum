# daydumb sync — wire protocol

> Status: **v1 draft, locked for C4a.** Binding contract between clients
> (CLI, web PWA, mobile) and the `datum-sync-server` relay. Additions are OK in
> later versions; semantics of existing fields are frozen.

## Goals and constraints

- **E2E encrypted.** The relay is a dumb pipe. It never sees plaintext
  entries, keys, field names, or user data. Every changeset is
  AES-256-GCM ciphertext.
- **Content-blind storage.** The server stores opaque base64 blobs keyed
  by `(device_id, seq)`. It MUST NOT decode, inspect, index, or
  transform them.
- **JSON over HTTPS.** The contract is the wire format, not Rust types.
  No language-specific tagging, no trait-object schemas, no type IDs
  that only mean something in one language.
- **Portable across WASM, FFI, and native.** Clients run in three runtimes
  (CLI on native Rust, PWA on Rust → wasm, mobile through FFI). The
  protocol MUST NOT depend on features that only one runtime has.

## Base URL and versioning

All endpoints are mounted under a version prefix:

```
https://<relay>/v1/...
```

Clients SHOULD prefer `/v1`. The existing relay (see
`crates/datum-sync-server`) currently exposes its handlers under `/auth/*`
and `/sync/*` without a version segment — C4c will introduce the `/v1`
mount when the HTTP client is wired up. Readers of this document
should treat the version prefix as the forward-compatible surface
even while older unversioned endpoints still exist.

Breaking changes bump the prefix to `/v2`. Additive changes (new fields
with defaults, new endpoints) stay on `/v1`.

## Authentication

Every non-auth endpoint takes:

```
Authorization: Bearer <token>
```

Tokens are opaque strings (the current relay uses short-lived JWTs;
clients MUST NOT depend on this). A 401 response means "reauthenticate";
the client SHOULD trade its persisted `refresh_token` via
`POST /v1/auth/refresh` (see below) before falling back to the device-code
flow. Retry the original request once with the rotated bearer.

### Refresh token rotation

Every fresh credential issuance — `/register/finish`, `/login/finish`,
`/device-code/poll` (success branch), and `/auth/refresh` itself —
returns the bearer alongside a single-use `refresh_token`:

```json
{
  "token": "<bearer>",
  "user_id": "<uuid>",
  "refresh_token": "<32-byte URL-safe base64>"
}
```

The relay stores only the SHA-256 hash of the refresh token, so a
database leak does NOT yield reusable credentials.

`POST /v1/auth/refresh`

Request:

```json
{ "refresh_token": "<previously-issued value>" }
```

Response (200): a new `AuthToken` payload with the same shape as above —
a fresh bearer AND a freshly-rotated `refresh_token`. The submitted
refresh token is invalidated server-side as part of the rotation.

Failure modes:
- 401: the refresh token is unknown, expired, or already-rotated. The
  client MUST surface a "session expired — run `datum sync login` again"
  prompt; the device-code flow is the only recovery path.
- non-2xx with body: transient — clients MAY retry with backoff.

Clients that lose track of their refresh token (corrupted on-disk
state, manual edit) MUST treat any 401 as terminal and fall back to
device-code flow.

### OAuth device-code flow

The CLI is headless and cannot hold a passkey. It uses the RFC 8628
device-authorization grant adapted to our relay.

`POST /v1/auth/device-code`

Request body:

```json
{}
```

Response (200):

```json
{
  "device_code":     "BOGUS-OPAQUE-STRING",
  "user_code":       "ABCD-WXYZ",
  "verification_uri":"https://<relay>/login",
  "verification_uri_complete":"https://<relay>/login?code=ABCD-WXYZ",
  "expires_in":      600,
  "interval":        5
}
```

The CLI shows `verification_uri` + `user_code` to the user and then polls:

`GET /v1/auth/device-code/poll?device_code=<opaque>`

- 200 with `{"token": "<bearer>", "user_id": "<uuid>"}` once the user
  has completed auth in the browser.
- 202 with `{"status": "pending"}` while still waiting.
- 400 with `{"error": "expired"}` if the device code has timed out —
  restart the flow.
- 400 with `{"error": "slow_down"}` — double `interval` and keep polling.

Web and mobile clients skip device code and authenticate directly with
a WebAuthn passkey (see `crates/datum-sync-server/src/auth.rs`); that flow
predates this document and is out of scope for v1.

### Wrapped key exchange

The encryption key never exists on the relay as plaintext. Two paths are
supported; clients MAY ship one or both.

`POST /v1/keys/wrapped`

Upload (or replace) the passphrase-wrapped master key for the
authenticated user. The wrapping is opaque to the server — typical
clients use Argon2id(passphrase) → key-encryption key → AES-256-GCM
wrap of the master key.

```json
{
  "wrapped_key_b64": "<base64 url-safe no-pad>",
  "kdf":        "argon2id",
  "kdf_params": { "m_cost": 65536, "t_cost": 3, "p_cost": 1, "salt_b64": "..." },
  "wrap_alg":   "aes-256-gcm",
  "nonce_b64":  "<base64 url-safe no-pad>"
}
```

Response: 200 `{}` on success.

`GET /v1/keys/wrapped`

Response (200):

```json
{
  "wrapped_key_b64": "...",
  "kdf":        "argon2id",
  "kdf_params": { "...": "..." },
  "wrap_alg":   "aes-256-gcm",
  "nonce_b64":  "..."
}
```

404 if no wrapped key has been uploaded yet.

QR device-pairing (memory: C4f) replaces this path with a direct
peer-to-peer handshake; it is outside the v1 contract.

## Sync endpoints

### `POST /v1/push`

Upload a single encrypted changeset. The server assigns a per-device
monotonic `seq`.

Request:

```json
{
  "device_id": "9f7e2c1a-...",
  "payload_b64": "<base64 url-safe no-pad of EncryptedBlob bytes>"
}
```

Response (200):

```json
{ "seq": 42 }
```

- `device_id` is an opaque client-chosen identifier (UUID in the default
  implementation). The relay auto-registers it on first push.
- `payload_b64` is the on-the-wire encoding of [`EncryptedBlob`](#encryption) —
  base64 url-safe no-pad.
- The response `seq` is what the relay assigned. Clients MUST store it
  (via [`advance_push_seq`](../crates/datum-core/src/sync.rs)).

### `GET /v1/pull?since=<seq>&device=<device_id>`

Fetch changesets from peers (the caller's own device is excluded) with a
server-assigned global sequence strictly greater than `since`.

Response (200):

```json
[
  {
    "device_id": "other-device-uuid",
    "seq": 7,
    "payload_b64": "...",
    "uploaded_at": "2026-04-21T16:30:00Z"
  }
]
```

- Order MUST be ascending by `seq`.
- Clients advance
  [`pull_seq`](../crates/datum-core/src/sync.rs) to the last processed
  `seq` only after a batch has been fully applied to the local DB.
- Empty response = nothing new; the client should keep its `pull_seq`.

### Sequence-number semantics

- `seq` is **per-device monotonic**: the relay tracks the highest seq it
  has assigned for each `device_id` and increments on every `/push`.
- Across devices, `seq` values overlap and are NOT globally ordered.
- The `pull_seq` stored client-side is the relay's internal global row id
  (`changesets.id` in the current server schema), not a per-device seq.
  It exists so clients can stream without duplicates; it is opaque to
  clients beyond being monotonic.

The `sync_state` table in `datum-core`'s schema mirrors these two
separate concerns:

| Column         | Meaning                                                       |
|----------------|---------------------------------------------------------------|
| `db_id`        | Local sync identity. One row per DB (default: `"local"`).     |
| `push_seq`     | Highest local `modified_at` epoch-ms included in a push.      |
| `pull_seq`     | Highest server-assigned global seq successfully pulled.       |
| `last_push_at` | Epoch-ms of the most recent successful push.                  |
| `last_pull_at` | Epoch-ms of the most recent successful pull.                  |

`push_seq` is intentionally stored as a local watermark (epoch-ms of
`modified_at`), not as a server-assigned seq. The server-assigned seq
returned by `/push` is discarded after it has been acknowledged — the
next pack only needs to know which local rows to include, and
`modified_at` is the authoritative answer for that.

## Encryption

Every `payload_b64` decodes to an `EncryptedBlob`:

```
┌───────────┬───────────────────────┐
│ nonce (12)│ ciphertext + tag (n+16)│
└───────────┴───────────────────────┘
```

- **Algorithm:** AES-256-GCM.
- **Key:** 32 bytes. Derived from the master key (either passphrase-wrapped
  via `/keys/wrapped` or handed off via QR pairing). Per-device subkeys
  are NOT required for v1 — a single user-wide master key is sufficient
  because AAD binds each blob to its `(device_id, seq)`.
- **Nonce:** 12 random bytes, generated fresh per encryption. MUST be
  prepended to the ciphertext. MUST NOT be reused with the same key.
- **AAD:** `utf-8(device_id)` || `big-endian-u64(seq)`. Tampering with
  either input surfaces as a decrypt failure — callers MUST treat every
  failure identically (no oracle).
- **Tag:** the 16-byte GCM authenticator is appended to the ciphertext
  by the `aes-gcm` crate (standard behavior across implementations).

The plaintext is the `Changeset` bytes:

```
┌─────────────┬──────────────────────────────────────────┐
│ magic "DSC1"│ JSON payload (ChangesetPayloadV1)         │
│  (4 bytes)  │                                           │
└─────────────┴──────────────────────────────────────────┘
```

`ChangesetPayloadV1`:

```json
{
  "entries": [ /* one per changed Entry — see crates/datum-core/src/entry.rs */ ]
}
```

Tombstones ride the same channel: entries with `deleted_at` set to a
non-null ISO 8601 UTC string are soft-deletes.

A future revision MAY swap the JSON body for cr-sqlite's native binary
changeset format. The magic prefix is the switch: consumers detect the
payload type by the magic, not by the HTTP content type.

## Error responses

All errors use the shape:

```json
{ "error": "<machine_tag>", "detail": "optional human-readable string" }
```

HTTP status mapping:

| Status | When                                                        |
|--------|-------------------------------------------------------------|
| 400    | Malformed JSON, missing required field, oversized payload   |
| 401    | Missing/invalid Bearer token, expired JWT                   |
| 403    | Valid token, but user can't touch the requested resource     |
| 404    | `/v1/keys/wrapped` GET with nothing uploaded, unknown routes|
| 409    | `(device_id, seq)` collision on push (client retry w/ fresh)|
| 413    | Payload exceeds relay's configured max (e.g. 10 MB default) |
| 429    | Rate limit — honor `Retry-After` header                     |
| 500    | Relay internal error — retry with backoff                   |

`<machine_tag>` values are stable across minor versions: `invalid_token`,
`expired`, `slow_down`, `pending`, `oversize`, `conflict`, `not_found`,
`internal`.

## Portability notes

- All bytes on the wire are either JSON scalars or base64 url-safe
  no-pad. No MessagePack, no CBOR, no protobuf in v1.
- All timestamps are RFC 3339 UTC strings (`2026-04-21T16:30:00Z`) in
  JSON, or epoch-ms `INTEGER` inside the local SQLite schema.
- No Rust-specific enum tagging (`{"Foo": {}}` variants forbidden).
  Public `datum-core::sync` types expose only `&[u8]` / `Vec<u8>` /
  `u64` / `&str` — safe for FFI and WASM.
- The changeset magic `"DSC1"` is ASCII; new formats bump the digit
  (`DSC2`, ...) so consumers can branch without parsing JSON first.

## What's intentionally out of scope for v1

These are either later waves or explicitly deferred; document them here
so reviewers of later beads don't re-litigate the design:

- Conflict resolution UX (C4e).
- Daemon-based auto-sync cadence (C4d).
- QR device-pairing key handoff (C4f).
- Server-side compaction of superseded changesets.
- Partial / range pulls (everything-since-N is enough for v1 volumes).
- Multi-user sharing. The current model is one user ↔ many devices.
