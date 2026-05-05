# Dependency Audit

**Date**: 2026-05-05
**Workspace HEAD**: `polecat/obsidian/da-8rnt@moswyvan` (off `main` at `f8962dd`)
**Scope**: all 10 workspace crates (`crates/*` + `xtask`).

## Summary

| Metric | Value |
|---|---|
| Total unique transitive packages (incl. dev-deps + duplicates) | **407** |
| Normal-only transitive packages (no dev) | **287** |
| Lockfile crate-dependencies (`cargo audit`) | 401 |
| Workspace-declared direct deps | 34 (in `Cargo.toml [workspace.dependencies]`) + 14 crate-local |
| Known security advisories (`cargo audit`) | **0 vulnerabilities, 0 informational warnings** |
| `target/release/datum` size | 24.5 MiB (text section: 9.9 MiB) |
| `target/release/datum-sync-server` size | 13.3 MiB (text section: 4.6 MiB) |
| Unused deps detected by `cargo machete` (verified) | 13 (some false positives — see §4) |
| Duplicate version splits | 11 (see §3) |

Tool versions used: `cargo-tree` (cargo 1.91), `cargo-machete 0.9.x`, `cargo-bloat 0.13.x`, `cargo-audit 0.22.1` (advisory-db 2026-05-01).

---

## 1. Top-level inventory

### Workspace shared deps (`Cargo.toml`)

| Crate | Version | Used for | Kind |
|---|---|---|---|
| `serde` (+`derive`) | 1 | Wire serialization for entries, configs, sync protocol | runtime |
| `serde_json` | 1 | `extra_data` JSON, sync protocol payloads | runtime |
| `rusqlite` (`bundled`) | 0.32 | Local-first storage; bundles SQLite C library | runtime |
| `chrono` (`serde`) | 0.4 | Time arithmetic on `DateTime` everywhere | runtime |
| `chrono-tz` (`serde`) | 0.10 | IANA tz lookups in `datum-parse` | runtime |
| `ulid` (`serde`) | 1 | Entry IDs, alias IDs, kit IDs (CRDT-safe) | runtime |
| `thiserror` | 2 | `DatumError`, `ParseError` derive | runtime |
| `anyhow` | 1 | Command-layer error propagation | runtime |
| `tokio` (`full`) | 1 | Async runtime for `datum-sync-server` and `-agent` only | runtime |
| `axum` | 0.7 | HTTP framework, sync-server only | runtime |
| `aes-gcm` | 0.10 | E2E payload encryption in sync-client | runtime |
| `sha2` | 0.10 | `content_hash` (entry dedup), HKDF | runtime |
| `md-5` | 0.10 | Datum-compatible field-color hashing (legacy parity) | runtime |
| `rand` | 0.8 | Nonce generation, ULID seeding | runtime |
| `argon2` | 0.5 | Recovery-passphrase KDF (sync key wrapping) | runtime |
| `rpassword` | 7 | Password prompt for sync setup CLI | runtime |
| `reqwest` (`blocking,json,rustls-tls`, `default-features=false`) | 0.12 | Sync-client HTTP — blocking on purpose for sub-5ms CLI hot path | runtime |
| `base64` | 0.22 | URL-safe-no-pad encoding per sync-protocol.md | runtime |
| `regex` | 1 | `datum grep` only (one call site) | runtime (cold) |
| `flate2` | 1 | gzip layer of backup `.tar.gz` | runtime (cold) |
| `tar` | 0.4 | Backup/restore archive | runtime (cold) |
| `toml` | 0.8 | Read/write `dbs.toml` registry, kit TOML | runtime |
| `serde_yaml` | 0.9 (**deprecated**) | Read/write `datumrc.yml` user config | runtime |
| `url` | 2 | Parse sync-server URL + WebAuthn origin | runtime |
| `notify` | 6 | Filesystem watcher in `datum-sync-agent` only | runtime (agent only) |
| `libc` | 0.2 | `getuid`, `kill(pid,0)` for daemon-status pid checks | runtime |
| `tracing` + `tracing-subscriber` (`env-filter`) | 0.1 / 0.3 | Daemon/server structured logs | runtime (server/agent) |

### Crate-local direct deps

| Crate | Direct dep | Why |
|---|---|---|
| `datum-cli` | `rustyline = "18"` | Interactive REPL (`datum` no-args) |
| `datum-commands` | `dirs-next = "2"` | XDG config-dir lookup |
| `datum-commands` | `tempfile = "3"` | `datum edit` editor staging |
| `datum-commands` | **`resvg = "0.47"`** (`text,system-fonts,memmap-fonts`, `default-features=false`) | Pure-Rust SVG→PNG for `dayview --output foo.png` |
| `datum-core` | `iana-time-zone = "0.1"` | (declared, **unused** — see §4) |
| `datum-parse` | `iana-time-zone = "0.1"` | Detect local timezone in `TimeBuilder` / `clock` |
| `datum-sync-protocol` | `uuid = "1"` (`v4,serde`) | (declared, **unused** in this crate) |
| `datum-sync-server` | `tower-http = "0.5"` (`cors,trace`) | CORS for browser clients |
| `datum-sync-server` | **`webauthn-rs = "0.5"`** | Passkey auth (the only auth path) |
| `datum-sync-server` | `jsonwebtoken = "9"` | Issue session JWTs after WebAuthn |
| `datum-sync-server` | `uuid = "1"` | Session/device IDs |
| `datum-sync-client` | `getrandom` (wasm only, `js`) | Browser entropy when compiled to wasm32 |
| `datum-cli` (dev) | `uuid = "1"`, `tokio`, `axum`, `tempfile`, `reqwest`, `url`, `serde_json`, `md-5`, `rusqlite` | Integration tests bring up an in-process sync-server |

---

## 2. Heavy hitters

### By transitive sub-tree size (count of distinct packages)

`cargo tree -p <crate>` — counts include the crate itself, so subtract 1 for "added to tree".

| Direct dep | Sub-tree size | Notes |
|---|---:|---|
| `reqwest` | **101** | Pulls full HTTP/2 + rustls + ICU normalization stack (idna→`icu_*` is huge) |
| `webauthn-rs` | **99** | OpenSSL, x509-parser, asn1-rs family, all on `thiserror v1`, plus `time` / `serde_cbor_2` |
| `axum` | 65 | Most overlaps with reqwest's hyper/tokio/tower set |
| `resvg` | **50** | `usvg` + `tiny-skia` + `rustybuzz` + `ttf-parser` + `fontdb` + `fontconfig-parser` — only used by one CLI command (`dayview --output *.png`) |
| `url` | 35 | Drags in the `icu_*` IDNA stack (icu_collections / icu_normalizer / icu_properties / zerocopy / yoke / writeable / litemap) |
| `jsonwebtoken` | 35 | `ring`, `simple_asn1`, `pem`, `time`, `num-bigint` |
| `tracing-subscriber` (`env-filter`) | 23 | `env-filter` pulls `matchers` → full `regex-automata` + `regex-syntax` + `aho-corasick` |
| `rusqlite` | 23 | `libsqlite3-sys` (bundled C build), `hashlink` → `hashbrown v0.14` |
| `aes-gcm` | 20 | `aes` + `ctr` + `polyval` + `ghash` + `cipher` |
| `tower-http` (`cors,trace`) | 20 | `iri-string`, `tower`, http/body-util |
| `tokio` (`full`) | 19 | `mio`, `socket2`, `parking_lot`, `signal-hook-registry` |
| `argon2` | 16 | `password-hash` + `blake2` |
| `rustyline` | 16 | `nix`, `radix_trie`, `unicode-segmentation` |
| `toml` | 16 | `toml_edit` (uses `winnow`), `toml_datetime`, `indexmap` |
| `chrono-tz` | 15 | `phf` build-time tz table |
| `serde_yaml` | 14 | `unsafe-libyaml` (C-port), `indexmap`, **deprecated upstream since 2024** |
| `notify` | 13 (agent-only) | `inotify` (Linux), `mio v0.8`, `bitflags v1` (only consumer of either v1) |

### By compiled binary size (`cargo bloat --release --crates`)

Top contributors to `target/release/datum` `.text` (9.9 MiB total):

| Crate | Size | Why |
|---|---:|---|
| `datum_commands` | 1.0 MiB | application code |
| `datum_core` | 906 KiB | application code |
| `usvg` | 450 KiB | resvg dep |
| `reqwest` | 450 KiB | HTTP client |
| `rustls` | 421 KiB | TLS |
| `regex_automata` | 318 KiB | regex engine |
| `ring` | 296 KiB | TLS crypto |
| `rustybuzz` | 267 KiB | resvg dep (text shaping) |
| `tiny_skia` | 213 KiB | resvg dep (raster backend) |
| `toml_edit` | 209 KiB | TOML config |
| `aho_corasick` | 189 KiB | regex dep |
| `ttf_parser` | 151 KiB | resvg dep |
| `rustyline` | 127 KiB | REPL |
| `unsafe_libyaml` | 84 KiB | serde_yaml dep |

**The resvg family** (`usvg + rustybuzz + tiny_skia + ttf_parser + fontdb + fontconfig_parser + roxmltree + resvg`) accounts for **~1.5 MiB of `.text`** in the CLI binary, second only to first-party code, and is touched only by `datum dayview --output *.png`.

---

## 3. Duplicate version splits

`cargo tree --workspace -d` flags 11 crates compiled at multiple versions:

| Crate | Versions | Cause | Cost |
|---|---|---|---|
| `base64` | 0.21.7 + 0.22.1 | `webauthn-rs` ecosystem locks `base64 0.21` via `base64urlsafedata` | Compile both; ~120 KiB extra |
| `bitflags` | 1.3.2 + 2.11.1 | `inotify` (under `notify`) still on v1 | trivial |
| `getrandom` | 0.2 + 0.3 + 0.4 | `tempfile`/`ring`/`rand 0.9` each pin a different major | non-trivial — three FFI shims |
| `hashbrown` | 0.14.5 + 0.17.0 | `rusqlite`→`hashlink` on v0.14 | ~60 KiB |
| `mio` | 0.8.11 + 1.2.0 | `notify`→`inotify` on v0.8; `tokio` on v1 | ~80 KiB (agent only) |
| `rand` / `rand_chacha` / `rand_core` | 0.8 + 0.9 | Workspace pins `rand = 0.8`, but `ulid` v1.2 pulls `rand 0.9` | RNG impls duplicated |
| `roxmltree` | 0.20 + 0.21 | `usvg` and `fontdb` disagree | resvg internal |
| `thiserror`/`thiserror-impl` | 1.0.69 + 2.0.18 | webauthn-rs ecosystem on v1; we pin v2 | both proc-macros expand |
| `tower-http` | 0.5.2 + 0.6.8 | We pin 0.5; transitive 0.6 from elsewhere | review what pulls 0.6 |

The high-leverage ones are **base64** and **thiserror** (both kept alive only by webauthn-rs). Cutting webauthn-rs would resolve both at once.

---

## 4. Reimplement / classification

### Vestigial / dead — **delete now (no replacement needed)**

`cargo machete` flagged 13 unused; manual verification of each:

| Crate | Dep | Status | Notes |
|---|---|---|---|
| `datum-core` | `anyhow` | **truly unused** | not referenced in `crates/datum-core/` at all |
| `datum-core` | `ulid` | **truly unused** | only used downstream in `datum-commands` |
| `datum-core` | `chrono-tz` | **truly unused** | tz handling lives in `datum-parse` |
| `datum-core` | `iana-time-zone` | **truly unused** | tz detection lives in `datum-parse` |
| `datum-commands` | `base64` | **truly unused** | base64 work is in sync-client/server |
| `datum-commands` | `sha2` | **truly unused** | content_hash work is in datum-core/sync-client |
| `datum-commands` | `thiserror` | **truly unused** | command layer uses `anyhow` only |
| `datum-sync-server` | `chrono` | **truly unused** | server stores opaque blobs only |
| `datum-sync-client` | `thiserror` | **truly unused** | client uses `anyhow` |
| `datum-sync-protocol` | `uuid` | **truly unused** | wire types use `String` device IDs |
| `datum-sync-protocol` | `serde_json` | **truly unused** | crate is types-only; serde does the work |
| `datum-sync-agent` | `serde` | **truly unused** | nothing in `crates/datum-sync-agent/` |
| `datum-sync-agent` | `serde_json` | **truly unused** | nothing in `crates/datum-sync-agent/` |
| `datum-layout` | `serde_json` | **truly unused** | `serde` derives are enough |

False positives `cargo machete` reported but **must be kept**:
- `datum-core` `md-5` — used (`use md5::{Digest, Md5}` — crate name `md-5`, lib name `md5` confuses the linter).
- `datum-commands` `md-5` — same false-positive cause; used in `render/colors.rs`.

### Trivial — small API surface, reimplement to drop sub-trees

| Dep | Surface in use | Replacement |
|---|---|---|
| `rpassword` | `rpassword::prompt_password` (one call) | ~30 lines using `libc` (already a dep) for `tcgetattr`/`tcsetattr` ECHO toggle, or use `crossterm`'s tty (we don't have crossterm). **Low ROI** — keep. |
| `dirs-next` | `dirs_next::config_dir()` (one call) | `std::env::var("XDG_CONFIG_HOME")` with `$HOME/.config` fallback — 5 lines, removes the `dirs-sys-next` sub-tree. **Worth doing.** |
| `tempfile` | `tempfile::Builder::new().prefix().suffix().tempfile()` for `datum edit` staging | `std::env::temp_dir()` + `format!("{}-{ulid}{}", prefix, suffix)` — 20 lines. **Borderline** — tempfile is small (~5 deps) and battle-tested. |
| `pico-args` (only inside resvg) | n/a | Removed when resvg is gated. |
| `anyhow` | `anyhow::Error`, `anyhow::Context` | Kept — zero dep cost (`cargo tree -p anyhow` is a 1-node tree). **Keep.** |
| `thiserror` | `#[derive(Error)]` | Replaceable with hand-written `Display` impls; **not worth it** — error structs benefit from the derive. |

### Replaceable with std

| Dep | Surface in use | std replacement | Notes |
|---|---|---|---|
| `dirs-next` | config dir | `std::env::var_os("HOME")` + path join | std doesn't ship XDG, but it's 6 lines. |
| (none of the others) | | | We're not using `lazy_static` / `once_cell` directly — already on `std::sync::OnceLock` (1.70+) for crate-level globals. Good. |

### Worth keeping (load-bearing or audit-heavy)

| Dep | Why it stays |
|---|---|
| `serde`, `serde_json`, `serde_derive` | Non-negotiable; ~7 transitive deps accepted as cost of doing business. |
| `rusqlite` (`bundled`) | Local-first storage. `bundled` adds the C library but a single Rust dep — keep. |
| `tokio` (`full`) | Async server runtime. `full` is overkill for the CLI but the CLI doesn't link tokio (only sync-server/-agent do, and those need it). |
| `axum` | Server framework, well-audited. |
| `chrono` + `chrono-tz` | Time math is too gnarly to roll our own. |
| `aes-gcm`, `argon2`, `sha2`, `md-5`, `rand` | RustCrypto family, well-audited. Non-negotiable for E2E sync. |
| `ring` (transitive via rustls) | TLS crypto, audited. |
| `clap` family | Not used (we're on a custom argv pre-pass + clap is *not* in the tree at all — verified). Kudos. |

### Worth keeping but heavy — opt-in via cargo features

| Dep | Why heavy | Recommendation |
|---|---|---|
| **`resvg`** | 50-crate sub-tree, ~1.5 MiB binary; only used by `dayview --output *.png` | Gate behind `--features png` on `datum-commands` (default off). Browser/web builds and CLI users who don't render PNG never compile it. **Highest-ROI single change.** |
| **`webauthn-rs`** | 99-crate sub-tree, drags `openssl`, `x509-parser`, `asn1-rs`, the only consumer of `thiserror v1` and `base64 v0.21` | Server-only — already not in the CLI binary. Keep, but document that *every* CLI integration test (`datum-cli/tests/` dev-deps `datum-sync-server`) pulls it. Consider splitting auth from `datum-sync-server` so unit tests of CLI ↔ sync handshake don't need webauthn. **Medium-ROI**. |
| `tracing-subscriber` (`env-filter`) | `env-filter` adds `matchers` + full regex stack (~250 KiB) | Sync-server uses bare `fmt::init()` (no env filter). **Drop the `env-filter` feature for the workspace dep**, opt back in only in `datum-sync-agent`'s Cargo.toml where `EnvFilter` *is* used. |
| `regex` | One call site (`datum grep`) | Switch to `regex-lite` (1 dep, ~5x smaller, sufficient grammar for user-supplied patterns). **Easy win, ~250 KiB binary**. |
| `serde_yaml` | Officially **deprecated** since 2024-03 | Switch to `serde_yaml_ng` (drop-in fork, actively maintained) **or** migrate `datumrc.yml` to TOML and remove serde_yaml + `unsafe-libyaml` entirely (~84 KiB). Migration is the cleaner long-term path. |

---

## 5. Specific recommendations (ranked by ROI)

ROI = (transitive deps removed) × (security surface reduced) ÷ (engineering hours).

| # | Action | Removes (transitive) | Hours | Notes |
|---|---|---:|---:|---|
| **1** | Delete the 13 unused direct-dep entries (§4 vestigial table). Just remove from `Cargo.toml`s. | 0 (already not compiled) | **0.5** | Pure hygiene. Compile-checks confirm immediately. Will reduce `cargo machete` noise to zero and prevent future "I'll just use the dep that's already there" drift. |
| **2** | Gate `resvg` behind `cargo feature = "png"` on `datum-commands`, default-off; turn it on in `datum-cli` only when a `--features png` build is requested. | **~50 deps** off the default tree, ~1.5 MiB off `datum` binary | **2** | Mechanical: move the `resvg` dep behind `[features]`, wrap the `rasterize_to_png` fn and its call site in `#[cfg(feature="png")]`, return a friendly error for `--output *.png` when the feature is off. PNG users opt in. |
| **3** | Replace `regex` with `regex-lite` in `datum-commands`. | ~3 deps (`regex-automata`, `regex-syntax`, `aho-corasick`); ~500 KiB binary | **1** | Only call site is `datum grep` (`Regex::new(pattern).is_match(text)`); `regex-lite` covers that grammar. |
| **4** | Drop `env-filter` feature from the workspace `tracing-subscriber` dep; re-enable it only in `datum-sync-agent` (the lone consumer of `EnvFilter`). | ~5 deps (`matchers`, `regex-automata` v2nd-copy, `regex-syntax` v2nd-copy, `nu-ansi-term`, `sharded-slab`) off the **default** path; the sync-server build shrinks visibly | **0.5** | `datum-sync-server/main.rs` uses `tracing_subscriber::fmt::init()` only — no env-filter consumer there. |
| **5** | Migrate `datumrc.yml` → `datumrc.toml`; drop `serde_yaml`. | 5 deps (`serde_yaml`, `unsafe-libyaml`, `indexmap`+, `equivalent`, `hashbrown v0.17`); 84 KiB binary; eliminates the **deprecated** crate | **3** | Pre-v1 rip-and-replace — no shim. Read existing yml file once at migration boundary, write toml, delete yml. Documented `datum migrate config` one-shot command. |
| **6** | Replace `dirs-next` with 6-line `XDG_CONFIG_HOME` helper in `datum-commands`. | 3 deps (`dirs-next`, `dirs-sys-next`) | **0.5** | Linux/macOS only; we have no Windows-target story. |
| **7** | Drop `webauthn-rs` from `datum-cli` *dev*-dependency surface by extracting a `datum-sync-server-test` helper that boots a server with auth disabled. Production stays as-is. | ~99 deps off CI test compile (incl. openssl, x509-parser, asn1-rs family, `thiserror v1`, `base64 v0.21`) | **4** | Test compile-time and CI cache size shrink dramatically. Production binary unaffected (already had it). |
| **8** | Investigate the `tower-http v0.5 + v0.6` duplicate. | 1 version copy (~30 KiB) | **0.5** | `cargo tree -i tower-http@0.6.8` to identify the unintended consumer; either bump our `tower-http` to 0.6 or pin transitive. |
| **9** | Pin `ulid` (currently 1.2.x) to a version line that doesn't pull `rand 0.9`, **or** bump our workspace `rand` to 0.9 unconditionally so the `0.8/0.9` split collapses. | 4 dup copies (`rand`, `rand_chacha`, `rand_core`, `getrandom v0.3`) | **2** | The `0.8/0.9` rand fork in 2025 is industry-wide pain; bumping to 0.9 is the strategic fix once `aes-gcm`, `argon2`, `password-hash` all support it (most already do as of 2026-04). |
| **10** | Split `datum-sync-agent` from `tracing-subscriber`'s default features: it imports `EnvFilter` *and* uses `fmt`, no need for `json` etc. | (already only what's needed; verify no surplus features) | **0.5** | Hygiene; future-proofs against `tracing-subscriber` adding heavy features under `default`. |

**Easy wins**: actions 1, 4, 6 take a combined ~1.5 hours and remove zero **functionality**. Action 2 is the single biggest binary-size lever. Actions 3 and 5 each remove a deprecated/heavy crate.

Combined removal if all 10 land: **~80 transitive crates and ~2.5 MiB of `datum` binary**, with no user-visible behavior loss aside from the `--output *.png` flag requiring a feature flag (and that feature can be on by default in `datum-cli` releases — gating only protects WASM and minimal-build downstreams).

---

## 6. Out-of-scope notes

Deps we are **keeping despite their cost**, and why:

| Dep | Cost | Reason kept |
|---|---|---|
| `serde` family | ~7 transitives | Non-negotiable. The whole entry/extra_data/protocol model is serde-derived. |
| `chrono` + `chrono-tz` | ~16 transitives, `phf` build-time tz table | Time math is the most-tested piece of `datum-parse`; `chrono`'s `chrono-tz` is the only mature IANA-aware option in Rust. `time` crate would be a sideways move. |
| `tokio` (`full`) | 19 transitives | Sync server and agent need it. CLI does not link tokio (verified — `cargo tree -p datum-cli` shows no tokio in the *normal* deps; tokio is dev-only for `datum-cli`). |
| `rusqlite` (`bundled`) | 23 transitives + C build | Local-first principle requires SQLite. Bundled mode keeps `cargo install datum-cli` zero-friction across distros. |
| `axum` + `tower-http` | 65+20 transitives | Server-only. Sync server is independently deployed; binary size doesn't impact the CLI hot path. |
| `webauthn-rs` | 99 transitives | We require passwordless auth (see CLAUDE.md sync architecture). Alternatives are ad-hoc; webauthn-rs is the one audited Rust crate for FIDO2/Passkeys. Keep, but isolate test surface (see action #7). |
| `aes-gcm`, `argon2`, `sha2`, `md-5`, `rand` | 20 + 16 + 13 + 12 + 13 transitives | RustCrypto family. The whole E2E sync threat model assumes these. `md-5` specifically is for **legacy datum parity** (color hashing) — not for security; cannot drop without a visual diff. |
| `ring` (transitive) | ~5 transitives, BoringSSL fork | Pulled by rustls + jsonwebtoken. Audited; we don't add a direct dep. |
| `flate2` + `tar` | 8 + 6 transitives | Backup/restore is a load-bearing feature; both crates are de-facto standards. |
| `notify` + `inotify` + `mio v0.8` (the v1 `bitflags` consumer) | 13 transitives | `datum-sync-agent` only. The agent is a separate binary; the CLI is unaffected. Worth revisiting when `notify` 7.x ships with `mio v1`. |
| `rustyline` | 16 transitives | REPL UX; the alternative is rolling our own `read_line` with raw-mode tty handling — not justified. |
| `iana-time-zone` | 1 transitive | `datum-parse` only; already minimal. |

### Things we are NOT doing
- We are **not** removing `anyhow` and `thiserror` despite some "trivially reimplementable" ratings online. Both are zero/near-zero transitive cost and provide significant ergonomics. Their value dwarfs supply-chain risk.
- We are **not** vendoring deps into the tree. The audit value of upstream crates' own CI and `cargo audit` integration outweighs supply-chain pinning gains.
- We are **not** removing `bundled` from `rusqlite`. System-sqlite reduces binary size but breaks the "near-instant `cargo install datum-cli` on a fresh box" promise.

---

## Appendix A — How to reproduce

```bash
# Counts
cargo tree --workspace --prefix none -e normal | awk '{print $1}' | sort -u | wc -l
cargo tree --workspace -d                                    # duplicate versions
cargo tree -p <dep>                                          # sub-tree for a single direct dep

# Unused-dep linter (note the md-5/md5 false-positive — see §4)
cargo machete

# Binary-size breakdown
cargo bloat --release --crates -p datum-cli -n 30
cargo bloat --release --crates -p datum-sync-server -n 20

# Vulnerability scan
cargo audit                                                  # currently 0 / 0
```

## Appendix B — Things to file as follow-up beads (if recommendations accepted)

1. *"datum-deps: remove 13 unused direct-dep entries"* — recommendation #1.
2. *"datum-deps: gate resvg behind `png` cargo feature"* — recommendation #2.
3. *"datum-deps: swap `regex` → `regex-lite`"* — recommendation #3.
4. *"datum-deps: drop tracing-subscriber `env-filter` from workspace, re-enable in agent only"* — recommendation #4.
5. *"datum-config: migrate datumrc.yml → datumrc.toml, drop serde_yaml"* — recommendation #5 (touches user-facing config; coordinate with v1 cutover).
6. *"datum-deps: replace dirs-next with std XDG helper"* — recommendation #6.
7. *"datum-cli tests: extract auth-disabled sync-server test harness, drop webauthn-rs from CLI dev-deps"* — recommendation #7.
8. *"datum-deps: collapse rand 0.8/0.9 split"* — recommendation #9.
