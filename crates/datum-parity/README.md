# datum-parity

Cross-runtime parity tests: daydumb (Rust) against the original
[vividn/datum](https://github.com/vividn/datum) (TypeScript).

The class of bug daydumb keeps shipping — md5 colors (`da-438y`),
switch-parser drift (`da-ffp6`), `_N` SQL slug regression (`da-luc8`),
nowview drift (`da-aw2o`), `-tq` cumulative time (`da-jp0b`) — is the
class that goldens against the *real* datum reference would catch.
This crate exists to be that reference.

## What it tests

| Surface | Fixture set | Runtime needed? |
|---|---|---|
| md5 color algebra | `goldens/colors/colors.tsv` (19 rows) | no |
| mySpec field/state map | `src/myspec.rs` (23 fields, 100+ states) | no |
| `get` text output | `goldens/text/fixtures.toml` (6 fixtures) | yes |
| `tail` text output | `goldens/text/fixtures.toml` (7 fixtures) | yes |
| `nowview` SVG output | `goldens/text/fixtures.toml` (4 fixtures, all `uncapturable` — see da-t5f5) | yes |
| `dayview` SVG output | `goldens/text/fixtures.toml` (6 fixtures) | yes |
| `content_hash` preimage | `goldens/hash/content_hash.toml` (6 fixtures) | no |
| Bug regressions (da-438y, etc.) | `tests/regressions.rs` (5 tests + lock) | no |
| Algorithm fuzz (proptest) | `tests/fuzz_color.rs` (10 props × 500 cases) | no |

## Running

### Without the runtime (default)

```bash
cargo test -p datum-parity
```

This runs the algorithm and fixture-presence tests. It does not require
node / pnpm or a clone of vividn/datum.

### With the runtime

```bash
npm install --global pnpm@11          # the major the pinned lockfile was written by
bash crates/datum-parity/scripts/setup_datum.sh
cargo test --features parity-runtime -p datum-parity
```

The setup script clones vividn/datum at the pinned SHA into
`target/parity-cache/datum`, runs `pnpm install --frozen-lockfile`, and
exits successfully when the clone is ready. The CLI is invoked via
`pnpm dev …` against an isolated PouchDB host directory per fixture
(managed by `tempfile::TempDir`).

The `parity-runtime` feature is intentionally off by default: a Rust
build shouldn't pull a node toolchain into CI unless the parity tests
are going to actually run.

### The heartbeat (da-g3za)

An off-by-default test isn't merely untested code — it is code whose
*environment* rots unobserved, and the rot is invisible precisely
because the gate is doing its job. This crate proved it: between the pin
(2025-08) and 2026-09 nobody ran the gated suite, and in that window
pnpm stopped reading `package.json`'s `pnpm` field, so
`--frozen-lockfile` began refusing the pinned lockfile. The reference
check didn't go red. It went silent, which looks the same as fine.

`.forgejo/workflows/parity.yml` is the alarm (on GitHub until da-16sxf,
where nobody read it). It runs the gated suite

* weekly (Mondays 06:17 UTC),
* on `workflow_dispatch`, and
* on any change to `crates/datum-parity/**`.

It deliberately does **not** gate merges: it clones a third-party repo
over the network and needs a node toolchain, and making every merge wait
on that trades one flaky dependency for the whole queue. Its job is to
make noise within a week of the path breaking, not to be a gate.

### Updating goldens after a deliberate change

If a fixture's expected output has drifted *intentionally* (a new datum
SHA, a deliberate format change), regenerate by running:

```bash
PARITY_REGEN=1 cargo test --features parity-runtime -p datum-parity \
    -- text_goldens_match_live_datum --nocapture
```

Audit the diff in `goldens/text/expected/*.txt` and commit the changes
together with the SHA bump.

## Pinned SHA

Goldens are reproducible only against a specific datum revision. The
pinned SHA lives in two places that must match:

* `src/lib.rs::DATUM_PINNED_SHA`
* `scripts/setup_datum.sh::PINNED_SHA`

`pinned_sha_agrees_with_setup_script` (a unit test in `src/lib.rs`, so it
runs on the default feature set) reads the script and asserts the two
agree. The script's header always claimed a mismatch was "rejected
loudly"; nothing rejected anything until that test existed.

`scripts/setup_datum.sh::PNPM_MAJOR` is the other half of the pin. A
pinned lockfile read by a floating package manager is a pin with one end
loose — `--frozen-lockfile` compares the lockfile against what the
*running* pnpm believes the config should be, so the pnpm major belongs
to the pin. Bump it alongside the SHA when a new datum revision
regenerates its lockfile.

To bump:

1. Update both files to the new SHA.
2. Re-read `target/parity-cache/datum/src/field/mySpecs.ts` and patch
   `src/myspec.rs::FIELD_SPECS` to match.
3. Run `PARITY_REGEN=1 cargo test --features parity-runtime` to refresh
   text goldens.
4. Commit all four (lib.rs, setup_datum.sh, myspec.rs, expected/*.txt)
   in one commit so the pin is atomic.

## Layout

```
crates/datum-parity/
├── Cargo.toml          # workspace member, feature `parity-runtime`
├── README.md           # this file
├── goldens/
│   ├── colors/
│   │   └── colors.tsv  # field/state name → expected hex (md5-derived)
│   ├── hash/
│   │   └── content_hash.toml  # entry → canonical preimage → SHA-256
│   └── text/
│       ├── fixtures.toml          # 23 fixtures across all 4 surfaces
│       └── expected/<name>.txt    # captured stdout/SVG, one per fixture
├── scripts/
│   └── setup_datum.sh  # clone + install pinned datum
├── src/
│   ├── lib.rs          # re-exports + DATUM_PINNED_SHA + datum_clone_path
│   ├── colors.rs       # md5_color, contrast_text_color, state_color (algorithm port)
│   ├── myspec.rs       # FIELD_SPECS port + emit_kit_toml
│   └── runtime.rs      # DatumRuntime (gated)
└── tests/
    ├── colors.rs           # color goldens TSV → algorithm assertion
    ├── fuzz_color.rs       # proptest harness over the color algebra
    ├── hash_goldens.rs     # content_hash preimage goldens (da-yira)
    ├── regressions.rs      # da-438y / da-ffp6 / da-luc8 / da-aw2o / da-jp0b locks
    └── text_goldens.rs     # text + SVG fixture runner (gated for live mode)
```

## Documented divergences

Datum has features daydumb intentionally doesn't, and vice-versa. The
parity harness focuses on the surfaces both runtimes implement; the
following are deliberate divergences and **not** regressions:

* **CouchDB sync.** Datum can sync to a remote CouchDB; daydumb does not.
  Out of scope for parity.
* **Named-hue color extension.** Daydumb's kit format accepts hue names
  (`red`, `blue`, …) that resolve to HSL bands. Datum has no such
  extension. The MD5 fallback path is parity; the named-hue extension
  is documented in `crates/datum-core/src/kit_color.rs` and not
  exercised by parity goldens.
* **`null` vs `IGNORED`.** Datum's "null" state branch (`#888888` grey)
  is renamed to `IGNORED` in daydumb (`crates/datum-commands/src/render/colors.rs`).
  The color value (`#888888`) is identical; the *name* differs. Parity
  goldens assert the color, not the name.
* **Timezone display.** Datum displays the offset as `+1` (compressed);
  daydumb does the same. Both read the system TZ database, so the zone
  is pinned rather than assumed: `DatumRuntime::run` forces
  `TZ=`[`GOLDEN_TZ`] (`Europe/Berlin`) into every spawned datum process.
  Without it the same fixture renders `+1` on the machine the goldens
  were captured on and `+0` on a CI runner in UTC — a failure that
  looks like a parity regression and isn't.

* **`nowview` at an instant.** Datum's nowview always ends at now
  (`nowviewCmd.ts` takes no time argument). Daydumb's takes the shared
  time flags (`-t -d -q -y -z`, da-5flt) and centres the window on the
  instant they name, its yellow now-line drawn only where now falls
  inside it. With no time flag the two agree.

[`GOLDEN_TZ`]: src/runtime.rs

## Adding fixtures

### Color goldens (`goldens/colors/colors.tsv`)

Append a row in the format:

```
<field>	<state>	<expected_hex>	<source>	<contrast_fg>	<note>
```

Compute `expected_hex`:
* For `source = spec`: read the value from
  `target/parity-cache/datum/src/field/mySpecs.ts`.
* For `source = md5`: run
  `node -e "const c=require('crypto'); console.log('#'+c.createHash('md5').update('NAME').digest('hex').slice(0,6))"`.

Compute `contrast_fg`: YIQ-brightness `(R*299 + G*587 + B*114) / 1000`;
`> 128` → `#000000`, else `#ffffff`.

`<note>` is mandatory and should explain why the row exists (e.g.,
"would have caught da-438y for body=run").

### Hash goldens (`goldens/hash/content_hash.toml`)

Append a `[[fixture]]` block describing the entry's hashed columns, then
the three derived values: `parts` (the seven preimage segments, in the
order `Entry::expected_content_hash` joins them),
`preimage_hex` (hex of `parts` joined by a single `0x00` byte), and
`content_hash` (lowercase-hex `SHA-256` of those bytes).

Derive them by hand rather than from daydumb's output — a golden copied
from the implementation cannot contradict it. `tests/hash_goldens.rs`
checks the three against each other *and* against
`Entry::expected_content_hash`, so a mistake in either surfaces as a
failure rather than a rubber stamp.

### Text goldens (`goldens/text/fixtures.toml`)

Append a `[[fixture]]` block. `name` must be unique and `[a-z0-9_]`.
Every fixture must end up with a *non-empty* `expected/<name>.txt`, or
declare why not:

```toml
uncapturable = "nowview renders relative to wall-clock now"
```

An empty expected file is a failure, not a skip. It used to be a skip,
and that is how 10 of the 21 fixtures came to be uncompared while the
suite reported green — the same shape of silence this crate exists to
catch. The four `nowview_*` fixtures carry `uncapturable`: nowview's SVG
embeds the current instant in both its `data-time` attributes and its
block geometry, so nothing about its output is stable between two runs a
second apart.

For new entries, populate `expected/<name>.txt` via:

```bash
touch crates/datum-parity/goldens/text/expected/<name>.txt
PARITY_REGEN=1 cargo test --features parity-runtime -p datum-parity \
    -- text_goldens_match_live_datum --nocapture
```

Audit the captured output before committing. Particular attention to:

* Wall-clock timestamps (createTime, modifyTime, humanId, _rev) — the
  `normalize` function in `tests/text_goldens.rs::runtime_tests`
  scrubs these. If your fixture exposes a new variable surface (e.g.,
  `_id` for a record without a deterministic slug), extend `normalize`
  rather than relaxing the comparison.
* SVG output for `nowview`/`dayview`: the harness writes to a temp
  `<name>.svg` and reads the file back. SVG content is byte-stable
  given fixed entries and CSS that doesn't include `now()`-derived
  timestamps in the body.
