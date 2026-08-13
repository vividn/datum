# daydumb sync — wire protocol

> Status: **v0 — unstable.** The contract between clients (CLI, web PWA,
> mobile) and the `datum-sync-server` relay while it is still being
> iterated. Fields and endpoints MAY change in place until the protocol
> ships to production; at that point v0 freezes and promotes to a stable,
> breaking-change-disciplined v1. Until then, treat nothing here as
> locked — the source of truth is the `datum-sync-protocol` crate, and
> this document is regenerated from it.

## Goals and constraints

- **E2E encrypted.** The relay is a dumb pipe. It never sees plaintext
  entries, keys, field names, or user data. Every changeset is
  AES-256-GCM ciphertext.
- **Content-blind storage.** The server stores opaque base64 blobs keyed
  by `(device_id, seq)` within a cluster. It MUST NOT decode, inspect,
  index, or transform them.
- **JSON over HTTPS.** The contract is the wire format, not Rust types.
  No language-specific tagging, no trait-object schemas, no type IDs
  that only mean something in one language.
- **Portable across WASM, FFI, and native.** Clients run in three runtimes
  (CLI on native Rust, PWA on Rust → wasm, mobile through FFI). The
  protocol MUST NOT depend on features that only one runtime has.

## Base URL and versioning

All endpoints are mounted under a version prefix:

```
https://<relay>/v0/...
```

The prefix mirrors [`PROTOCOL_VERSION_PATH`](../crates/datum-sync-protocol/src/version.rs)
(`"v0"`), pinned by a lock-step test to the integer
[`PROTOCOL_VERSION`](../crates/datum-sync-protocol/src/version.rs) (`0`).

`v0` is the **unstable, pre-production** surface. While on v0 we change
fields and semantics in place rather than versioning every revision — the
whole point of staying on v0 is to tighten the spec freely before it
solidifies. When the protocol reaches production it freezes and promotes
to `/v1`; from that point breaking changes bump the prefix (`/v2`, …) and
additive changes (new optional fields, new endpoints) stay on the current
prefix. That breaking-change discipline kicks in at promotion, not before.

> The changeset payload-format version (the `DSC1` magic / `ChangesetPayloadV1`,
> see [Encryption](#encryption)) is **orthogonal** to this URL/protocol
> version. The magic prefix versions the encrypted body; the URL prefix
> versions the HTTP contract. They move independently.

## Authentication

Every non-auth endpoint takes:

```
Authorization: Bearer <token>
```

Tokens are opaque strings (the current relay uses short-lived JWTs;
clients MUST NOT depend on this). A 401 response means "reauthenticate";
the client SHOULD trade its persisted `refresh_token` via
`POST /v0/auth/refresh` (see below) before falling back to the device-code
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

`POST /v0/auth/refresh`

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

`POST /v0/auth/device-code`

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

`GET /v0/auth/device-code/poll?device_code=<opaque>`

- 200 with `{"token": "<bearer>", "user_id": "<uuid>"}` once the user
  has completed auth in the browser.
- 202 with `{"status": "pending"}` while still waiting.
- 400 with `{"error": "expired"}` if the device code has timed out —
  restart the flow.
- 400 with `{"error": "slow_down"}` — double `interval` and keep polling.

Web and mobile clients skip device code and authenticate directly with
a WebAuthn passkey (see `crates/datum-sync-server/src/auth.rs`); that flow
predates this document and is out of scope here.

### Wrapped key exchange

The encryption key never exists on the relay as plaintext. Two paths are
supported; clients MAY ship one or both.

`POST /v0/keys/wrapped`

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

`GET /v0/keys/wrapped`

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

QR device-pairing (C4f; design in
[`docs/sync/device-pairing.md`](sync/device-pairing.md), bead da-28a4)
replaces this path with a direct peer-to-peer handshake; it is outside
the current contract.

## Clusters and generations

Every push and pull is scoped to a **cluster** at a specific
**generation**. Both are carried on the wire (see the message types in
[`crates/datum-sync-protocol/src/messages/sync.rs`](../crates/datum-sync-protocol/src/messages/sync.rs)).

- **`cluster_id`** — identifies one logical sync cluster. A user may have
  several (multi-DB; epic da-u6lm). The relay scopes every stored row to
  its cluster and never mixes them — a pull filters server-side by
  `cluster_id`. A client sends the `cluster_id` it has persisted from a
  prior sync, or — on a never-synced DB — its **default**, which is the
  DB file-name stem (`vremya.db` → `"vremya"`; an in-memory or path-less
  DB falls back to the literal `"default"`). So two devices tracking a
  same-named DB land on the same cluster with zero config, while
  differently-named DBs stay isolated. `datum sync login --cluster <name>`
  overrides the default. The server echoes the accepted `cluster_id`; the
  client persists it (see `set_cluster_id` /
  [`default_cluster_id`](../crates/datum-sync-client/src/sync_state.rs), da-3fnd).
- **`generation`** — a counter the server holds authoritatively per
  cluster. It is bumped whenever a cluster is **reinitialised**: a factory
  reset, or a restore-from-encrypted-backup. Bumping invalidates every
  pre-reset changeset so peers cannot replay stale state over the restored
  snapshot. The client stores the last generation the server echoed
  (`cluster_generation`); unlike the seq watermarks this value is **not**
  monotonic client-side — a fresh DB attaching to a long-lived cluster
  simply adopts whatever generation the server is already on.

### `GenerationMismatch`

When a client pushes or pulls with a `generation` that disagrees with the
cluster's current authoritative value, the relay rejects the request with
**HTTP 409** and the tagged-enum
[`SyncProtocolError::GenerationMismatch`](../crates/datum-sync-protocol/src/errors.rs)
body:

```json
{
  "kind": "generation_mismatch",
  "expected": 7,
  "found": 5,
  "generation_started_at": "2026-06-01T10:00:00Z"
}
```

- `expected` — the cluster's current authoritative generation (server side).
- `found` — the generation the client sent.
- `generation_started_at` — RFC 3339 UTC instant the server's current
  generation was first set (initial cluster creation, or the last
  `bump-generation`). Clients use this to bucket local unsynced edits
  during recovery (da-74xx): edits with `modify_time < generation_started_at`
  are pre-reinit *stale state* and should be discarded; edits with
  `modify_time >= generation_started_at` are post-reinit *fresh local work*
  and should be kept (re-pushed naturally on the next push after recovery).

Both client and server MUST treat this as **terminal for the current sync
attempt**. The contract for the client is: reset its local pull watermark
and re-pull from `generation = expected`, prompting the user before
discarding the pre-reinit local edits. Old changesets stay on the server
until garbage-collected by generation.

### Applying pulled rows (client contract)

A pull can return rows from more than one generation: the relay filters by
cluster, device and user, and `generation` is carried on each row rather
than applied as a filter. Clients therefore MUST judge every row on its
own value (da-pjhg):

- **A row whose `generation` is older than the client's current one MUST
  NOT be applied.** A bump means the cluster has declared what preceded it
  void — discarded by a purge, superseded by a restore. Applying such a
  row merges back exactly the state the bump retired. The pull watermark
  still advances past it: a superseded row never becomes applicable.
- **Generation adoption from pulled rows MUST be forward-only.** A lower
  generation on the wire is a superseded row, not a new authority;
  assigning it puts the device on a dead generation and every later push
  409s. Moving *backwards* is legitimate only in the reinit recovery
  above, where the relay stated the value directly.

> Client-side recovery (watermark reset + re-pull, coordinated with the
> restore/purge path) is being implemented under da-74xx. This section
> documents the protocol contract; consult that bead for current client
> completeness.

## Sync endpoints

### `POST /v0/push`

Upload a single encrypted changeset. The server validates the client's
predicted per-device `seq` against its own authoritative value before
storing.

A changeset is **one page of local history, not all of it** (da-5ua6).
Clients pack at most `DEFAULT_PUSH_PAGE_ENTRIES` (~1k) entries per request
and repeat until a page comes back empty, advancing the local `push_mark`
watermark after each accepted page — so an interrupted first sync resumes
at the first page that never landed, and the relay assigns one `seq` per
page. Request bodies are capped at the relay's `MAX_BODY_BYTES` (8 MiB, a
chosen ceiling rather than the framework default); a body past it is
`413 Payload Too Large`.

Request:

```json
{
  "device_id": "9f7e2c1a-...",
  "payload_b64": "<base64 url-safe no-pad of EncryptedBlob bytes>",
  "cluster_id": "<cluster identifier>",
  "generation": 0,
  "predicted_seq": 42
}
```

Response (200):

```json
{ "seq": 42, "cluster_id": "<cluster identifier>", "generation": 0 }
```

- `device_id` is an opaque client-chosen identifier (UUID in the default
  implementation). The relay auto-registers it on first push.
- `payload_b64` is the on-the-wire encoding of [`EncryptedBlob`](#encryption) —
  base64 url-safe no-pad.
- `cluster_id` / `generation` scope the row to a cluster and gate it on
  the cluster's authoritative generation (a mismatch → `409
  generation_mismatch`, see below).
- `predicted_seq` is the per-device seq the client bound into the
  changeset AAD at encrypt time (`device_seq + 1`). The server compares
  it to `MAX(seq)+1` for the device. On agreement it stores the blob at
  that seq and echoes it back; on disagreement it returns `409
  seq_mismatch` **without storing** (see [Seq-mismatch
  recovery](#seq-mismatch-recovery)).
- The response also echoes `cluster_id` + `generation` so a client can
  confirm it stayed on the expected generation across the round-trip
  (e.g. after a `bump-generation`).

#### Seq-mismatch recovery

Because the AAD binds `(device_id, seq, cluster_id)`, the client must
encrypt before the seq is final — so it predicts. If the client and relay seq views
drift (a crash between a successful push and the local watermark advance,
a DB restored from an older backup, or a relay reset), the prediction is
wrong. Rather than store a blob whose AAD seq disagrees with its row seq
(peers could never decrypt it) and leave the client permanently bailing,
the relay rejects the push:

```json
// 409
{ "kind": "seq_mismatch", "expected": 43, "predicted": 42 }
```

The client treats this as **recoverable, not terminal**: it re-bases
`device_seq` to `expected`, RE-ENCRYPTS the changeset with the corrected
seq, surfaces a warning, and retries the push once. Only a *second*
mismatch indicates a genuine `device_id` collision (two writers racing
the same id) and is fatal. The relay's `expected` may sit *below* the
client's stored seq (relay reset / older-backup restore), so recovery
force-sets `device_seq` past the normal monotonic guard.

### `GET /v0/pull?since=<id>&device=<device_id>&cluster_id=<cluster>&generation=<gen>&limit=<n>`

Fetch changesets from peers (the caller's own device is excluded
server-side) within the given cluster + generation, with a relay-global
row id strictly greater than `since`.

Response (200):

```json
[
  {
    "id": 128,
    "device_id": "other-device-uuid",
    "seq": 7,
    "payload_b64": "...",
    "uploaded_at": "2026-04-21T16:30:00Z",
    "cluster_id": "<cluster identifier>",
    "generation": 0
  }
]
```

- `id` is the relay-global monotonic row id — the ordering key and the
  client's pull watermark. **`since` is compared against `id`**, not `seq`.
- `seq` is the **author device's** per-device push counter. The client
  uses it as the AAD seq when unsealing this row's blob, so it must be the
  sender's `seq`, not the puller's.
- Rows MUST be ordered ascending by `id`.
- `limit` caps the rows in the response. It is clamped to the relay's
  `MAX_PULL_ROWS` (100), which is also the value used when the parameter
  is absent — pull is paged for the same reason push is (da-5ua6), so a
  peer's whole history is never one response.
- Clients advance
  [`pull_seq`](../crates/datum-sync-client/src/sync_state.rs) to the last
  processed `id` only after a batch has been fully applied to the local DB,
  then re-ask from that `id` until a page comes back empty (or short).
- Empty response = nothing new; the client keeps its `pull_seq`.
- A generation disagreement returns **409 GenerationMismatch**.

### Sequence-number semantics

Two distinct counters travel under confusingly-similar names. They are
not the same number and are not interchangeable:

- **`id`** (relay-global row id) — the server's own autoincrement row id
  across the whole cluster. It is the **pull watermark** (`pull_seq`
  client-side): clients page through history with `?since=<id>` and
  advance to the highest `id` they have fully applied. Opaque to clients
  beyond being monotonic and ascending.
- **`seq`** (per-device counter) — one independent sequence per
  `device_id`. The relay is authoritative (`MAX(seq)+1` per device), but
  the client **predicts** the next value (`device_seq + 1`) before
  pushing, because `seq` is bound into the changeset AAD and so must be
  known at encrypt time. The push response echoes the assigned seq and the
  client asserts equality. Across devices, `seq` values overlap and carry
  no global ordering. `(device_id, seq)` is the relay's content-blind
  storage key; a duplicate is a **409 conflict**.

(An earlier draft said "the server assigns a per-device monotonic seq"
without the prediction half — accurate as far as it went, but it hid why
the client maintains its own `device_seq` and why a mismatch is fatal.)

The `sync_state` table in `datum-core`'s schema mirrors these concerns —
see [`crates/datum-sync-client/src/sync_state.rs`](../crates/datum-sync-client/src/sync_state.rs)
for the accessors:

| Column               | Meaning                                                          |
|----------------------|------------------------------------------------------------------|
| `db_id`              | Local sync identity. One row per DB (default: `"local"`).        |
| `push_mark`           | Highest local `modified_at` epoch-ms included in a push (watermark). |
| `device_seq`        | This device's per-device push counter (the predicted `seq`).     |
| `pull_seq`           | Highest relay-global `id` successfully pulled.                   |
| `cluster_id`         | Cluster this DB is bound to (echoed by the relay; nullable until first sync). |
| `cluster_generation` | Last cluster generation the relay echoed (NOT monotonic client-side). |
| `last_push_at`       | Epoch-ms of the most recent successful push.                     |
| `last_pull_at`       | Epoch-ms of the most recent successful pull.                     |

`push_mark` is a local watermark (epoch-ms of `modified_at`), **not** a
server seq — the next pack only needs to know which local rows changed,
and `modified_at` is the authoritative answer. `device_seq` is the
separate per-device counter the client predicts and the relay confirms on
each push.

## Merge semantics (last-write-wins)

Incoming entries are reconciled against the local table by
**last-write-wins on `modify_time`** (wall-clock), keyed on `hid` — the
primary key, replicated across devices with fidelity, and the identity
`entry_history`, `integrity_warnings` and prefix-addressing already join
on. When two devices have edited the same `hid`, the row with the larger
`modify_time` survives; the loser's pre-merge state is snapshotted into
`entry_history` as a `sync_merge` change, so it is recoverable and
auditable. See `crates/datum-sync-client/src/ops.rs::upsert_lww`.

**Why not `slug`.** `slug` is *derived* from `field` + `occur_time`, and
`retime` rewrites it in place while preserving the hid
(`datum-core/src/db/time.rs`). The relay is an append-only log, so a
retimed entry sits on it under two slugs with one hid; a slug-keyed probe
misses the second changeset, calls it a fresh arrival, and inserts it onto
its own primary key — `UNIQUE constraint failed: entries.hid` (da-zlug). A
key derived from mutable data cannot be an identity.

**Identity collisions (`slug`, `(field, occur_time)`).** Keying on `hid`
means two genuinely *different* entries can now contend for one slug, or
for one live `(field, occur_time)` — the constraints slug-keyed merge hid
by collapsing both sides into a single row. Neither is an error and neither
side is discarded:

- The winner is the row with the greater `(modify_time, content_hash,
  device_id, hid)` — the equal-`modify_time` tiebreaker below, extended
  with `hid` so the order is total.
- The loser is **parked, not dropped**: its slug becomes `<slug>_<hid>`
  when the two contend for a slug, and it is tombstoned with `deleted_at =
  <winner's modify_time>` when they cannot both be live. Its content is
  untouched.
- `modify_time` is not bumped. Parking is a pure function of the two rows,
  so every device reaches the same pair of rows independently, in any
  arrival order, with no extra traffic.
- One `integrity_warnings` row with `kind = identity_collision` names the
  pair; `datum check` is where the user meets it. Sync never blocks — the
  daemon has no one to ask.

Two rows with *essentially identical* content still park rather than
dedupe. Deduping them silently is da-yiq9, and it waits on `content_hash`
covering `following_state` — until then "identical content" is a predicate
that conflates semantically different entries.

**Equal-`modify_time` tiebreaker (part of the contract).** When the stored
and incoming rows carry *exactly* equal `modify_time`, the winner is chosen
by a deterministic total order: the row with the lexically greater
`(content_hash, device_id)` pair survives (equal `content_hash` is an
identical row — a replay — and is skipped with no history row or warning).
Every client MUST implement this tiebreaker identically: without it each
device keeps its own row on a tie and both consider themselves up to date —
a permanent, silent divergence that no future sync heals (da-22zm). The
identity-collision parking above extends the same order with `hid` as a
final discriminator; clients MUST implement that identically too, for the
same reason.

**Clock-skew caveat (accepted, not mitigated).** `modify_time` is a raw
device wall-clock with no logical-clock component. A device whose clock
runs fast wins every tie; a device whose clock lags can silently lose an
edit even if it wrote *later* in real time. For one user's own devices —
the only model v0 supports — this is a deliberate simplification:
near-synchronized clocks make collisions rare, and the overwritten edit is
never destroyed (it lives in `entry_history`). It is **not** hardened with
a hybrid-logical-clock or vector clock; that is explicitly out of scope for
v0 (decision da-t96a).

**Overwrites are surfaced, not silent.** Each LWW overwrite of an existing
local row also files an `integrity_warnings` row with `kind =
lww_overwrite`, `detected_by = sync_pull`: `local_entry` is the overwritten
local edit, `remote_entry` the winning remote edit. The
conflict-resolution UX (C4e) reads these to tell the user "a remote edit
overwrote yours" and offer take-local / take-remote, sourced from the same
`sync_merge` history snapshot. A *fresh* row arriving for an hid not yet
present locally is not an overwrite and files no warning.

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
  are NOT required — a single user-wide master key is sufficient because
  the AAD binds each blob to its `(device_id, seq, cluster_id)`.
- **Nonce:** 12 random bytes, generated fresh per encryption. MUST be
  prepended to the ciphertext. MUST NOT be reused with the same key.
- **AAD:** binds the blob to its `(device_id, seq, cluster_id)` coordinates
  (da-3nkx) so replay, misattribution, or a cross-cluster splice surfaces as
  a decrypt failure. Encoding:
  `len32(device_id) ‖ device_id ‖ big-endian-u64(seq) ‖ len32(cluster_id) ‖ cluster_id`,
  where the two variable-width fields are length-prefixed (big-endian u32)
  so distinct `(device_id, cluster_id)` tuples can't alias to the same AAD.
  `seq` is the per-device push counter described above. Every failure mode
  MUST look identical to callers (no oracle).
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
payload type by the magic (`DSC1` → `DSC2` → …), not by the HTTP content
type, and independently of the URL/protocol version.

## Post-quantum posture

The protocol is **E2E-encrypted**, which decouples the stored-data
surface from the authentication surface. That split is what determines
the quantum threat model: a future quantum attacker who defeats the
public-key crypto gets *authentication*, not *plaintext*.

**Quantum-safe today (the data plane).** The bytes the relay stores are
protected only by symmetric primitives, which Grover's algorithm merely
square-roots:

- **Changeset blobs:** AES-256-GCM. Grover leaves ~128-bit effective
  security — still infeasible.
- **Wrapped master key:** Argon2id-derived KEK wrapping AES-256-GCM (see
  [Wrapped key exchange](#wrapped-key-exchange)). Memory-hardness blunts
  quantum speedups too.
- The master key never appears on the wire in plaintext, so there is no
  harvested ciphertext that decrypts it.

**Classically broken, hence harvest-now-decrypt-later (HNDL) exposed.**
Everything public-key is vulnerable to Shor's algorithm:

- **WebAuthn passkeys** (Ed25519 / P-256) — long-lived, stored
  server-side; a forged signature is the prize.
- **TLS 1.3 handshake** — ephemeral X25519; recorded today, the session
  key recoverable once a CRQC exists.

**Auth bypass does not yield data.** Because auth and decryption are
decoupled, a quantum attacker who forges a passkey reaches only the
relay's content-blind surface: encrypted blobs, the wrapped key,
`(device_id, seq, cluster_id)` metadata, and a DOS lever. To read
anything they still need the **passphrase** to unwrap the master key —
so user passphrase strength × Argon2id memory cost is the real gate, not
the public-key layer.

**What still defeats E2E** (quantum-independent, and the actual priority):

- A weak passphrase — offline brute-force against the wrapped key.
- A compromised device — the master key lives in process memory.
- QR device-pairing (C4f, bead da-28a4) — its out-of-band channel needs
  its own quantum-safe handshake; it is outside this contract today.

**Mitigations live upstream and are adopt-when-available.** TLS hybrid
PQ key exchange (X25519Kyber768) is already shipping in major browsers
and closes the HNDL window on the transport; PQ-WebAuthn is still in
draft. daydumb tracks both and turns them on as the platforms expose
them — no protocol change here is required for either.

## Error responses

Two error shapes coexist:

**HTTP-status-mapped errors** use:

```json
{ "error": "<machine_tag>", "detail": "optional human-readable string" }
```

**Control-plane errors** ([`SyncProtocolError`](../crates/datum-sync-protocol/src/errors.rs))
use a tagged-enum shape so clients route on `kind` regardless of
transport:

```json
{ "kind": "<snake_case_variant>", "...": "variant payload" }
```

The control-plane variants are `generation_mismatch` (cluster re-init —
client resets watermark + re-pulls) and `seq_mismatch` (per-device seq
drift — client re-bases + retries, see [Seq-mismatch recovery](#seq-mismatch-recovery)),
both delivered on **409**; clients route on `kind`.

HTTP status mapping:

| Status | When                                                                 |
|--------|----------------------------------------------------------------------|
| 400    | Malformed JSON, missing required field, oversized payload            |
| 401    | Missing/invalid Bearer token, expired JWT                            |
| 403    | Valid token, but user can't touch the requested resource             |
| 404    | `/v0/keys/wrapped` GET with nothing uploaded, unknown routes         |
| 409    | `generation_mismatch` (re-init) or `seq_mismatch` (re-base + retry) — tagged `SyncProtocolError` body |
| 413    | Payload exceeds relay's configured max (e.g. 10 MB default)          |
| 429    | Rate limit — honor `Retry-After` header                              |
| 500    | Relay internal error — retry with backoff                            |

`<machine_tag>` values are stable across minor versions: `invalid_token`,
`expired`, `slow_down`, `pending`, `oversize`, `conflict`, `not_found`,
`internal`.

## Portability notes

- All bytes on the wire are either JSON scalars or base64 url-safe
  no-pad. No MessagePack, no CBOR, no protobuf.
- All timestamps are RFC 3339 UTC strings (`2026-04-21T16:30:00Z`) in
  JSON, or epoch-ms `INTEGER` inside the local SQLite schema.
- No Rust-specific enum tagging by struct shape — control-plane errors use
  an explicit string `kind` discriminator, not `{"Foo": {}}` positional
  variants. Public sync types expose only `&[u8]` / `Vec<u8>` / `i64` /
  `u64` / `&str` — safe for FFI and WASM.
- The changeset magic `"DSC1"` is ASCII; new formats bump the digit
  (`DSC2`, ...) so consumers can branch without parsing JSON first.

## What's intentionally out of scope for v0

These are either later waves or explicitly deferred; documented here so
reviewers of later beads don't re-litigate the design:

- Conflict resolution UX (C4e) — but the backing `lww_overwrite`
  integrity warnings are now filed on every overwrite (da-t96a), so the UX
  has data to render when it lands.
- Daemon-based auto-sync cadence (C4d).
- QR device-pairing key handoff (C4f; design in
  [`docs/sync/device-pairing.md`](sync/device-pairing.md), bead da-28a4).
- Server-side compaction of superseded changesets.
- Partial / range pulls (everything-since-N is enough for current volumes).
- Multi-user sharing. The current model is one user ↔ many devices, each
  user owning one or more clusters.
