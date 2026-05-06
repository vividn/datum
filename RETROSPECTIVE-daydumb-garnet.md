# daydumb retrospective — garnet's perspective

Companion to `RETROSPECTIVE-daydumb-polecat.md` (quartz). Quartz wrote the
general view: the cmd.rs/parse.rs mega-files, migration discipline, the
verb-derivation tar pit. I'm not going to repeat any of that. This is what
the codebase looks like from the corners I've actually lived in.

Seventeen commits. The big ones, by area:

- **`datum-parse`** — created the crate, moved `time/` into it, scaffolded
  `input/` + `alias/`, then ripped clap out of `datum-cli` and replaced it
  with a bespoke 2641-line `parse.rs` (R2 steps 1–3, da-jzj/da-t6t.2).
- **Sync** — wire protocol + `datum-core::sync` primitives (C4a/da-ndd.12),
  then `datum sync push/pull/status` end-to-end (C4c/da-ndd.14).
- **Conflicts** — D5 non-blocking queue + auto-resolver, including the
  `sync_conflicts` table that C4e later piggybacked on (da-gnj.5).
- **Renderer (S4-v2)** — visual-parity rebuild with MD5 + YIQ contrast and
  the LEAD/STATE/TRAIL slot grouping (da-u4k.13), then deleting the legacy
  `hash_to_ansi256` path once nothing referenced it (da-m20).
- **Update strategy** — collapsing update/merge/rekey behind one
  `update_with_strategy` (S11/da-u4k.11).
- **Migrations v2** — transactional runner + subcommands (da-htzs).
- **`doc_ref` + id= convention** — `field:id` slugs everywhere (da-ndd.18).
- **D2** — parser extensions: uncertainty, `-K/-k`, q-precision, retime.

So my beat is roughly: input → parse → core data primitives → render. I
have not touched dayview/nowview rendering except by deleting code that
fed it.

## 1. Where I lived

`crates/datum-parse/` is the place. I built the crate, so I have opinions.

Counting only files I created or substantially own: ~10,000 lines under
`datum-parse/src/` after R2. The two giants:

- `parse.rs` — 2641 lines. One function per subcommand
  (`parse_log_args`, `parse_tail_args`, `parse_get_args`, `parse_dayview_args`, …).
- `input/data_args.rs` — 1643 lines.
- `input/time_args.rs` — 1122 lines.

The renderer is the other neighborhood: `crates/datum-commands/src/render/`
— `mod.rs` (1400), `action.rs` (746), `colors.rs` (293), `characters.rs` (19).
I wrote most of `action.rs` and `colors.rs` for S4-v2 and then cleaned out
the ANSI-256 fallback once we'd committed to MD5.

Everything else is one or two commits, mostly in `datum-core/` (sync.rs
which has since moved to `datum-core/src/db/`-ish, `conflicts.rs`,
`migrations.rs`, `doc_ref.rs`).

## 2. What was harder than expected

### Replacing clap was 80% derivation, 20% sharp edges

The R2 work — pulling clap out of `datum-cli` and replacing it with a
bespoke parser — was *much* more work than I scoped. Not the parser
itself; the long tail of clap behaviors users had implicitly come to
depend on:

- Per-flag `=value` *and* space-separated forms (`--db foo`, `--db=foo`).
- Short flags with attached values (`-d-1`, `-q`, `-qq`, `-qqq` stacking).
- The `-h`/`--help` and `-V`/`--version` shortcuts on every subcommand.
- `datum help <cmd>` *and* `datum <cmd> --help` *and* `datum --help <cmd>`.
- The field-first/verb-first swap (`datum mood switch happy` vs
  `datum switch mood happy`) happening in the *pre-pass* before the
  parser sees argv. That pre-pass also has to play nicely with aliases.
- Alias expansion that happens *before* parse_argv but *after* a partial
  decode (we have to know what's a subcommand to know where to expand).

Every one of these was a single line in clap and is now a special case in
`parse.rs` or `alias/expand.rs`. The parser is faster and gives better
error messages, but every new flag is a touch in three files (the spec, the
registry, the parser fn) instead of one derive macro.

The `LiftedSpec`/`walk_registry` pattern (parse.rs:346) was the right
idea — it lets `OutputArgs`, `TimeArgs`, and now `LogArgs` share their
flag shapes across commands without copy-paste. But it only handles
flag-shape tokens. Positionals and `key=value` extras still need a
bespoke fold per command, which is why each `parse_X_args` is 30–80 lines.

### Color resolution looks trivial. It isn't.

`render::colors` is small (293 lines) but the rules are *deeply*
non-orthogonal because we're matching the original datum:

- `Bool(false)` is the "field has ended" sentinel and returns `None` — so
  TRAIL slots can render terminal-default whitespace, not a black bg.
- `Bool(true)` uses the field color.
- `String(IGNORED_STATE)` returns the special grey.
- `String(s)` hashes `s` (the *state* name), **not** the field — even
  though spec overrides on the field would seem like the intuitive
  fallback. They aren't, because the original TS doesn't do that.
- `Array` recurses into first element, with no length normalization.

The bug graveyard around the renderer (da-438y, da-741, da-bop, da-glb,
da-8mu, da-zxs) is mostly other polecats discovering that some piece of
that logic flowed wrong into nowview / dayview / dm get / preview /
gridlines. The renderer's correctness is local but its *integration* is
load-bearing in five other places I didn't touch.

### `data_args.rs` is the next mega-file

`crates/datum-parse/src/input/data_args.rs` at 1643 lines is well on its
way to becoming a `cmd.rs`-style problem. It owns the parsing of:
free-form `key=value` tokens, the JSON-typed value parser, unit
annotations (`weight=75.2kg`), array/object construction, the `-K` /
`-k` overrides, the q-precision rules from D2, the uncertainty syntax —
and they all live in a single fold loop with cross-cutting state.

I added to it (D2). I did not split it. That's a regret.

### Migrations v2 transactional runner had a sharp edge

The framework wraps each migration in a transaction, which is right.
But the `migrations.rs` file is 2462 lines and the migration code is
intertwined with the runner — the runner picks up migration metadata by
introspecting a registry that the migrations themselves register into.
A migration that panics during *registration* (not during run) is silent
unless you're looking at it. That happened to me once; the symptom was
"no migrations to run" with an unhealthy schema_version row. Worth
adding a sanity check that asserts the registered set is non-empty
before reporting "up to date".

## 3. What was easier than expected

### `LiftedSpec` made shared flags genuinely shared

Once the registry walker landed (`walk_registry` in parse.rs:346), adding
`OutputArgs` to a new subcommand became: extend the registry vec, parse
function picks them up. `OutputArgs::FLAG_SPECS` and
`time_args::FLAG_SPECS` are reused by maybe a dozen subcommands. That
piece of the design has held up.

### Sync wire protocol stayed simple because the server is content-blind

`crates/datum-sync-protocol/src/messages/sync.rs` is 75 lines. A
`PushRequest` is a `device_id`, a `seq`, and a list of opaque base64
`Changeset`s. A `PushResponse` is an ack and a new `seq`. All the
complexity — KDF params, wrapped keys, recovery passphrase handling — is
in `crypto/` and `auth.rs`. The "the server cannot decrypt anything"
constraint forced a clean separation, and the protocol stayed boring.

The push/pull/status command surface (`commands/sync/`) is similarly
small per file (push.rs 33, pull.rs 31, status.rs 164) because the
heavy lifting lives in `datum-sync-client`.

### `doc_ref` was 122 lines that paid off everywhere

`datum-core/src/doc_ref.rs` (122 lines, single helper) made the
`field:id` slug convention work across writes, lookups, and references
in `extra_data` without each call site re-implementing the lookup
fallback (slug → id → human-id). Best lines-shipped-to-features ratio
of anything I wrote.

### Conflict queue: the right design

D5 settled the question of "what does datum do when you log inconsistent
data?" — it accepts the data, files a `sync_conflicts` row, moves on.
That's a one-paragraph design decision but it determined the entire
shape of `conflicts.rs` (496 lines) and the C4e sync-pull reconciliation
that quartz/jasper later layered on top by reusing the same table with
a `detected_by` discriminator. That sharing was free because the design
already had `(local_entry, remote_entry)` as the schema, even though D5
only had one source. Lucky, not planned.

### `update_with_strategy` swallowed three commands

S11 collapsed `update`, `merge`, `rekey` (three nearly-identical
commands with subtly different merge semantics) behind a single
`update_with_strategy` enum. `commands/update_core.rs` is 1073 lines but
it replaced ~800 lines duplicated three times. The strategy enum
captured the actual axis of variation — what to do with conflicting
keys — and the dispatch became boring.

## 4. Bugs I suspect or noticed but didn't fix

1. **`strip_ansi` / `visible_width` only handle SGR (`\x1b[...m`).** Not
   cursor moves, not `\x1b]…\x07` OSC sequences, not bracketed paste.
   `colors.rs:215` and `colors.rs:235`. If anything ever embeds a hyperlink
   escape in rendered output (and nowview's `--watch` feels like a
   plausible candidate), the visible_width math will be wrong and the
   table alignment will drift.

2. **`alias/expand.rs` is 715 lines and recurses on alias bodies.** I do
   not believe there is a cycle detector. `datum alias set a "b"` then
   `datum alias set b "a"` then `datum a` — I haven't tested whether that
   stack-overflows or hits a limit gracefully. Worth a test either way.

3. **`detect_for_entry` runs post-insert, non-transactionally.** The
   `entries` insert and the `sync_conflicts` insert are two separate
   statements. If the process dies between them you have an entry that
   should have filed a conflict but didn't. Harmless until two devices
   are syncing and one is in that state. `conflicts.rs:194`.

4. **`migrations.rs` registration-time panics are silent.** See §2.4.
   Not user-visible until something else breaks first.

5. **Migration v2 transactional runner doesn't assert SQLite is in WAL
   when transactions begin.** If someone runs migrations against a DB
   opened without WAL, partial rollback semantics are different. Probably
   never happens in practice; would surface as silent corruption.

6. **`data_args.rs` JSON value coercion for `key=value`** has at least
   one edge case I'm 80% sure about and didn't write a test for: an
   integer-shaped string like `"007"` gets coerced to the integer 7,
   losing leading zeros. This matters for slug-as-data.

7. **Sync wire protocol has no negotiated version field on per-message
   level.** There's a `protocol_version` in `version.rs` but a single
   client/server mismatch across the same TCP stream isn't checked
   per-message. Probably fine because the handshake is upfront, but I'd
   want this if we ever do long-lived streams.

## 5. The non-obvious improvement (the only section that matters)

> **Push the `walk_registry` pattern down one more level: a
> `parse_command(registry, hooks)` helper that handles the per-command
> fold loop, leaving each `parse_X_args` as ~10 lines instead of ~50.**

Right now each subcommand parser in `parse.rs` looks like this
(simplified):

```rust
fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    let registry = log_args_registry();
    let mut out = LogArgs::default();
    let mut i = 0;
    while i < tail.len() {
        if let Some(ni) = walk_registry(tail, i, &mut out, &registry)? {
            i = ni;
            continue;
        }
        // Bespoke positional / key=value handling here, 20-60 lines.
        i += 1;
    }
    Ok(out)
}
```

The bespoke part — positional handling and `key=value` accumulation —
is *almost* identical between `parse_log_args`, `parse_tail_args`,
`parse_get_args`, `parse_now_args`, `parse_update_args`,
`parse_retime_args`. They each have a "consume positionals into
`field`/`required_keys`/`extras`" fold. They diverge on tiny details:
which positional is `field`, whether multiple positionals are allowed,
how `key=value` interacts with `--key`.

A `parse_command_with_hooks(tail, registry, PositionalRules,
KvHandler)` helper would let each parse fn shrink to:

```rust
fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    parse_command_with_hooks(
        tail,
        &log_args_registry(),
        PositionalRules::FieldThenValue,
        KvHandler::AccumulateExtras,
    )
}
```

This is not a yak-shave for its own sake. It pays for itself the next
time someone has to add a flag to all log-shape commands (which has
happened twice in the last two months — `-K`/`-k` and the q-precision
work). Right now that's a same-bug-fix-in-six-places change. With the
helper it's one.

The reason I didn't do this in R2 was that I wanted `parse.rs` to be
*readable*: each subcommand having its own fn means a new contributor
can grep for `parse_dayview_args` and see exactly what dayview takes.
That's still true. But after living in this file for several commits,
the grep value is overwhelmed by the maintenance cost. The helper lets
each parse fn stay grep-friendly (one fn per subcommand) while
extracting the boilerplate.

The companion move: split `data_args.rs` along its three independent
axes. The fold loop has three jobs that don't share state: (a) classify
each token (flag / kv / positional), (b) coerce values to JSON with
type rules, (c) accumulate into the output struct. Right now they're
interleaved. Splitting to `data_args/classify.rs`, `data_args/coerce.rs`,
`data_args/accumulate.rs` would drop the file from 1643 lines to three
~500-line modules and make the JSON coercion bug in §4.6 testable in
isolation.

Quartz wouldn't have noticed this because they didn't write the
registry walker, and from outside `parse.rs` the per-subcommand fns
look idiomatic — the duplication only shows up when you've written
them all in a row.

## 6. Relationship to the whole

`datum-parse` is a coupling hub pretending to be a leaf.

It's structured like a leaf: it has a clean public API (`parse_argv`,
`shell_split`, `Cli`, `Command`), no upward dependencies, and lives at
the bottom of the dependency graph. `datum-cli` calls into it; nothing
calls back.

But every behavioral change in datum — a new flag, a new alias rule,
a new positional — touches it. And because the original datum has ten
years of accumulated CLI surface that we are matching for parity, the
parser is *normative*: it defines what `datum` accepts, and the rest of
the system has to deal with whatever it produces. When `data_args.rs`
coerces `"007"` to `7`, that's not a bug we can fix in `cmd.rs`; it's a
parser correctness issue, even though it manifests three crates away as
"my entry has the wrong slug".

A refactor that split `data_args.rs` and pushed the fold loop into a
helper (§5) would localize that property: the parser would have a small,
testable type-coercion module that you could fuzz against the original
datum's JSON parser. The downstream crates wouldn't change. That's
exactly the kind of refactor that's safe to do — internal restructure
of a leaf with a small public API — *if* the public API really is
small. Which, today, it is: `parse_argv` returns `Cli`, and `Cli` is the
contract. Worth protecting that.

The renderer (`datum-commands/src/render/`) is the opposite shape: tiny
public API, deep behavioral entanglement with five callers. Every
nowview/dayview/dm/grep/preview bug fix in the last month touched code
that ultimately resolves through `render::action::render_entry_line`.
That is correctly factored — the renderer *should* be the single source
of truth — but it means changing the renderer is high blast radius. The
S4-v2 work added a ~500-line fixture test (`render_fixtures.rs`) for
exactly this reason: any visual regression in the renderer should be
caught before it ships to a downstream caller. That harness has earned
its keep twice already.

The sync work (`datum-core::sync`, `datum-sync-protocol`,
`commands/sync/`, `datum-sync-client`) is a clean pipe: protocol crate
defines the wire format, client implements transport, commands are
thin glue. The cleanest part of the codebase I worked on. If the rest
of daydumb were structured this way it would be half the size.

`conflicts.rs` and `doc_ref.rs` are small, well-bounded, and have
single callers each — proper leaves. Both could move to subdirectories
if `datum-core/src/` keeps growing, but neither is the right thing to
fix today.

The thing I'd do first, if I were doing it: split `data_args.rs` and
extract the parse-loop helper. Everything else can wait.

— garnet
