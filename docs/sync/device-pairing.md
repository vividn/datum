# QR / X25519 device pairing (C4f)

> Status: **design only — parked**. Tracked by bead **da-28a4**; nothing
> is implemented. The code slot is the reserved scaffold at
> [`crates/datum-sync-client/src/crypto/pair.rs`](../../crates/datum-sync-client/src/crypto/pair.rs).
> Implementation waits for multi-device UX work to start.

## Problem

Today a second device obtains the 32-byte master key by downloading the
passphrase-wrapped copy from `GET /v0/keys/wrapped` and asking the user
to type the recovery passphrase (see
[`docs/sync-protocol.md`](../sync-protocol.md), "Key management"). That
works, but the passphrase is meant to be a rarely-touched recovery
artifact — routine add-a-device should not exercise it.

## Design intent

A direct peer-to-peer handoff between a provisioned device and the new
device, with the relay (and any other network party) unable to learn the
master key:

1. **Ephemeral X25519 agreement.** Each side generates a short-lived
   X25519 keypair for this pairing session only. Public keys cross an
   out-of-band channel — the new device renders its public key as a QR
   code, the provisioned device scans it (CLI fallback: print/paste a
   base64 string).
2. **Fingerprint verification (SAS).** Both sides derive and display a
   short fingerprint of the session transcript (both public keys); the
   user visually compares them before confirming. This is the defense
   against an active MITM on the out-of-band channel — the channel
   itself is not assumed confidential or authentic.
3. **Master-key wrap for transport.** The provisioned device derives a
   wrapping key from the X25519 shared secret (HKDF), wraps the master
   key with AES-256-GCM, and sends the wrapped blob to the new device,
   which unwraps with the same derived key. The ephemeral secrets are
   discarded after the session.

This replaces the `/keys/wrapped` + passphrase path for add-a-device;
that path remains as recovery (all-devices-lost).

## Constraints (locked in by the R4 refactor)

- **WASM-safe.** `pair.rs` lives in the WASM-safe core of
  `datum-sync-client`: no I/O, no `reqwest`, no filesystem. Pure
  key-agreement/wrap primitives only; transport and QR rendering belong
  to the callers (CLI, PWA).
- **Dependency budget.** The only heavy dependency allowed is
  `x25519-dalek` (or similar), gated behind a feature flag if the
  compile-size cost is material.

## Open questions (decide at implementation time)

- Transport for the wrapped blob: a relay-mediated opaque pairing
  payload vs. a second QR in the reverse direction.
- Exact SAS encoding (hex words vs. emoji vs. digits) and length.
- Quantum safety of the handshake: the out-of-band channel needs its own
  treatment — it is explicitly outside the current E2E contract's PQ
  story (see the threat-model section of `sync-protocol.md`).

## History

"C4f" is the planning-memory id this design originally lived under. The
plan was captured here (and in bead da-28a4) by da-psdg after night-walk
cycle 141 flagged it as doc-referenced but tracked nowhere durable.
