# Grammar tokens

Flags are documented by `datum --help` and the man pages. This page is for
the *other* half of the surface: the punctuation the parser gives meaning
to. A reader meets these in alias bodies, in error messages (`unbalanced
'(' in expanded command (use '\(' for a literal paren)`) and in what
`datum tail` prints back (`±15m`, `_3`), and until this page there was
nowhere to look them up.

A token lives in one of four places, and that is how this page is
organised:

1. as a **whole argv token** — whitespace on both sides, or it is not that
   token at all;
2. **inside one argument's value** — after the `=`, or as a bare value;
3. **inside a time**, as a suffix;
4. **inside a template** — `--id`, `-o`.

Every `bash` block below is run against the built binary by
`crates/datum-cli/tests/readme_snippets.rs`, each in its own throwaway
database — that test asserts the exit code, so a block that stops working
fails CI. The transcript under each block is what a run of it printed,
pasted rather than described; hids and timestamps differ per run, and
nothing checks those.

## 1. Whole argv tokens

| token      | means                                                          |
|------------|----------------------------------------------------------------|
| `,`        | separate sides of one atomic multi-entry write                 |
| `,,`       | open a *trailing region* binding every side to its left        |
| `(` `)`    | group sides; a region inside binds only that group             |
| `\(` `\)`  | a literal `(` / `)` as data                                    |
| `--`       | end of flags, for this side or region only                     |
| `.`        | skip this slot                                                 |
| `\.`       | a literal `.` as data                                          |
| `-`        | stdin / stdout, where a command takes a file path              |

`,,`, `(` and `)` are mostly machinery — every alias resolve emits them —
but they are typeable, and the expanded stream you see in an error message
is made of them. The full contract is
[`trailing-and-slots.md`](trailing-and-slots.md).

### `,` — one write, N sides

```bash
datum switch body vertical , switch project gardening
```

```
CREATE m29u1 body:2026-09-22T15:59:03.422Z
17:59:03+2  body  vertical
CREATE t45xa project:2026-09-22T15:59:03.422Z
17:59:03+2  project  gardening
```

Both rows carry the same `occur_time` and are written atomically. The
comma must be its own token — whitespace on both sides — so `vertical,`
and `,sit` are single values and `vertical,sit` is one word. There is no
escape syntax; whitespace is the whole trigger. See the README's "Atomic
multi-entry aliases" for what the sides share.

### `,,` — bind a value to every side to its left

```bash
datum start run , start bike ,, 30m
```

```
CREATE t7dho run:2026-09-22T15:59:03.470Z
17:59:03+2  run  start  30m
CREATE knn04 bike:2026-09-22T15:59:03.470Z
17:59:03+2  bike  start  30m
```

One `30m`, both sides. Without the `,,` the `30m` would belong to `bike`
alone.

### `(` `)` — how far left "every side to its left" reaches

A region binds within its innermost enclosing group, so a group is how
you keep one out of another's way. Quote them past the shell.

```bash
datum occur a , '(' occur b , occur c ,, k=1 ')'
```

```
CREATE l6mpc a:2026-09-22T15:59:03.522Z
17:59:03+2  a ∅●
CREATE 3vgr2 b:2026-09-22T15:59:03.522Z
17:59:03+2  b ∅●
  k: 1
CREATE xbijk c:2026-09-22T15:59:03.522Z
17:59:03+2  c ∅●
  k: 1
```

`k=1` reached `b` and `c`. It did not reach `a`, which is outside the
group.

### `\(` `\)` — a paren that is data

Parens are structural unconditionally — `--` does not demote them — so a
literal one is spelled with a backslash.

```bash
datum occur f -k expr ,, '\('
```

```
CREATE qf416 f:2026-09-22T15:59:03.584Z
17:59:03+2  f ∅●
  expr: "("
```

Unbalance them and the error names the escape:

```bash,fails_with=1
datum occur f ,, '('
```

```
Error: Invalid argument: unbalanced '(' in expanded command (use '\(' for a literal paren)
```

Only a *bare* paren token is structural: `(x)` and `:-)` never were.

### `--` — end of flags, side-scoped

```bash,fails_with=1
datum occur g -k note --verbose
```

```
Error: Invalid argument: unexpected flag: --verbose
```

```bash
datum occur f -k note -- --verbose
```

```
CREATE wnlyu f:2026-09-22T15:59:03.648Z
17:59:03+2  f ∅●
  note: "--verbose"
```

The effect ends at the next `,` or `,,`, and a `--` inside an alias body
never leaks to the call site.

### `.` — skip a slot, and `\.` — a dot that is data

```bash
datum alias set scaff "switch body sit , occur caffeine -k doseMg="
datum scaff .
```

```
CREATE ljz13 body:2026-09-22T15:59:03.750Z
17:59:03+2  body  sit
CREATE za2rq caffeine:2026-09-22T15:59:03.750Z
17:59:03+2  caffeine ∅●
```

The `.` consumed the first unfilled slot and wrote nothing into it, so
`caffeine` has no `doseMg`. For a shared slot the skip applies on every
side that declares it.

`\.` is the way to say the character:

```bash
datum switch tv -I the end '\.'
```

```
CREATE mz3aa tv:2026-09-22T15:59:03.816Z
17:59:03+2  tv  the end .
```

### `-` — stdin or stdout instead of a file

```bash
datum occur x
datum db backup -
```

The backup goes to stdout rather than to a file, so the first bytes after
the `occur`'s own CREATE line are the archive itself:

```
{"type":"daydumb_backup","schema_version":37,"ba…
```

`db restore -` and `db import --from -` read the same way. A bare `-` is
never parsed as a flag, and neither is a dash followed by a digit or a
dot (`-5`, `-5.3` are values).

### `ISLAST` — what `--if-changed` answers instead of writing

`switch`, `start` and `end` take `--if-changed`: when the field already
holds that state at the entry's time (`-t`, or now), nothing is written
and the entry that set it prints as `ISLAST`, exit 0. It is a flag like
any other, so an alias body can carry it:

```bash
datum alias set act "switch activity --if-changed"
datum act reading
datum act reading
datum act cooking
```

```
CREATE vs2yj activity:2026-09-24T14:47:29.896Z
16:47:29+2  activity  reading
ISLAST vs2yj activity:2026-09-24T14:47:29.896Z
16:47:29+2  activity  reading
CREATE 19zx6 activity:2026-09-24T14:47:29.927Z
16:47:29+2  activity  cooking
```

## 2. Inside a value

| token                | means                                               |
|----------------------|-----------------------------------------------------|
| `key=value`          | a data key; an empty key (`=2`) is raw data         |
| `.key=`              | shorthand for `state.key=`                          |
| `a.b=`               | a dotted path into nested data                      |
| `a,b` `,a`           | a glued or leading comma → an array                 |
| `a,b,`               | a list drops one trailing comma → `["a","b"]`       |
| `a, b` `a,` `,` `,,` | a comma before a space, ending a word, or alone, is text |
| `[]`                 | the empty array                                     |
| `1_000`              | `_` between digits separates them → `1000`          |
| `\.`                 | the one-character string `"."`                      |
| `''` `""`            | the empty string                                    |
| `true` `{"a":1}`     | anything strict JSON parses keeps its JSON shape    |
| `75.2kg` `4th`       | a string, exactly as typed — no unit is inferred    |
| `'"42"'`             | JSON quotes force a string                          |
| `"'a,b'"` `"['a']"`  | single quotes mean what double quotes do            |
| `null` `undefined`   | a JSON null; no key (`'"null"'` is the string)      |
| `=2`                 | an empty key is no key — the whole token is data    |
| `ref://<slug>`       | a reference to the entry `<slug>`                   |
| `str://ref://…`      | a literal `ref://…` string, shown without `str://`  |

### `.key` and `a.b` — paths

A leading dot means "under `state`", and the bare word `state` is itself
shorthand for `state.id` — so `state=happy` and a bare positional
`happy` put the same value in the same place.

```bash
datum switch mood happy .intensity=7
datum switch mood state=happy .intensity=7
datum get mood -A
```

Two writes, one state — the `!` on the second only marks that the field
was already in it:

```
CREATE kymo7 mood:2026-09-22T16:08:44.707Z
18:08:44+2  mood  {happy}
  { .id: "happy", .intensity: 7 }
CREATE or5mr mood:2026-09-22T16:08:44.738Z
18:08:44+2  mood !{happy}
  { .id: "happy", .intensity: 7 }
```

And the part of `get -A` that shows where it landed — in the `state`
column, not in `data`:

```
── What ────────────────────────────────
  field                     mood
  verb                      switch
  state
    {
      "id": "happy",
      "intensity": 7
    }
```

An interior dot is a path into `data`:

```bash
datum occur run gear.shoes=trail
```

```
CREATE odafg run:2026-09-22T15:59:04.026Z
17:59:04+2  run ∅●
  gear: {"shoes":"trail"}
```

### `,` inside a value — arrays

A comma *inside* a token never splits sides. A leading comma, or one
glued to what follows, makes the value an array (da-fwsre); an empty item
between two glued commas becomes `null`, and a list drops one trailing
comma (`,work,home,` is `["work","home"]`). A comma followed by a space is
text, so `met Anna, Ben` stays a sentence, and only the glued comma
splits: `a,b, c` is `["a","b, c"]`. A trailing comma on a value that does
not otherwise split, and a value of nothing but commas (`,`, `,,`), are
text too: `Anna,` is `"Anna,"`. An
item can't hold a glued comma — the bracket form can: `'["a,b", "c"]'`;
`[]` is the empty array. This departs from v2, where a leading or trailing
comma made the array — a trailing one alone, too — and `,` was `[]`.

```bash
datum occur tagged tags=work,home
datum occur one tags=,work
datum occur gappy tags=x,,y
datum occur prose 'note=met Anna, Ben' left=Anna,
datum occur cleared 'tags=[]'
```

```
CREATE dxtmj tagged:2026-10-07T12:13:40.762Z
14:13:40+2  tagged ∅●
  tags: ["work","home"]
CREATE veslm one:2026-10-07T12:13:40.783Z
14:13:40+2  one ∅●
  tags: ["work"]
CREATE t8ti1 gappy:2026-10-07T12:13:40.804Z
14:13:40+2  gappy ∅●
  tags: ["x",null,"y"]
CREATE eehnx prose:2026-10-07T12:13:40.825Z
14:13:40+2  prose ∅●
  left: "Anna,", note: "met Anna, Ben"
CREATE 0m1wk cleared:2026-10-07T12:13:40.843Z
14:13:40+2  cleared ∅●
  tags: []
```

Each element is inferred on its own, so an array carries JSON shapes
too — `tags=null,1` stores `[null,1]`, and `n=1_000,2` stores `[1000,2]`:
`_` between digits is a separator, as in JavaScript and Rust.

### `null` is JSON null, `undefined` is no key

As in v2 (da-revva): `null` stores a JSON null, and `undefined`, like an
empty value, leaves the key out. The string `"null"` is reachable through
the JSON-quote escape below.

```bash
datum occur n k=null j=undefined s='"null"'
```

```
CREATE zlpeh n:2026-09-28T13:22:34.918Z
15:22:34+2  n ∅●
  k: null, s: "null"
```

A null is how a chore is retired: `next_date=null` overrides the date
before it. A `null` switch STATE is IGNORED, which is what it meant in
v2 — `datum switch mood null` is `datum ignore mood`.

### JSON shapes, and the two escapes around them

Anything strict JSON parses keeps its JSON shape. `''` and `""` are the
empty string — quote them past the shell.

```bash
datum occur shapes flag=true obj='{"a":1}' empty="''"
```

```
CREATE k4opv shapes:2026-09-22T16:06:14.598Z
18:06:14+2  shapes ∅●
  empty: "", flag: true, obj: {"a":1}
```

A single-quoted string means exactly what the double-quoted one does, as
in v2 (relaxed-json) and JSON5, inside brackets too: `'a,b'` is the text
«a,b», `'5'` the string `"5"`, `['a', 'b']` the array `["a","b"]`. Inside
one, `\'` is a quote and `"` needs no escape. There is no shell-style
difference between the two, since datum expands nothing in a value. The
shell takes one layer of quotes itself, so on a command line it is
`"'a,b'"` or `'"a,b"'`. A lone or unbalanced `'` is text.

```bash
datum occur quoted -k key "',,'" "v='a,b'" "l=['a', 'b']"
```

```
CREATE mfqdd quoted:2026-10-07T17:39:46.343Z
19:39:46+2  quoted ∅●
  key: ",,", l: ["a","b"], v: "a,b"
```

An `=` with nothing before it is not a key at all — the whole token,
`=` included, is data:

```bash
datum occur f -k raw ,, '=2'
```

```
CREATE qe1ts f:2026-09-22T16:06:14.628Z
18:06:14+2  f ∅●
  raw: "=2"
```

### `75.2kg` — a string, as typed

Digits followed by letters are not a number with a unit: `75.2kg`,
`4th`, `5pm` and an id like `6orhg` are stored exactly as typed. Only
what strict JSON parses is a number.

```bash
datum occur lift weight=75.2kg reps=10 set=4th
```

```
CREATE f2f8i lift:2026-09-22T15:59:04.152Z
17:59:04+2  lift ∅●
  reps: 10, set: "4th", weight: "75.2kg"
```

A key whose name ends in `time`, `date`, `dur` or `duration` (at a word
boundary, or camelCased, optionally with trailing digits) is parsed as
that kind first. So `dur=5m` stores the duration `PT5M`, while
`splits.first=5m` stores the string `"5m"`.

### `comment` — an array, only ever appended to

`-c`/`--comment` and `-C`/`--comment-remainder` each APPEND one string to
`data.comment`, on every logging verb and on `update`; nothing replaces
one. The key is an array however many it holds (one prints as itself), and
a comment is kept as typed: `-c 42` is `"42"`. `delete -c WHY` appends
`deleted: WHY` in the same write that deletes the row, to every row of a
comma list.

```bash
datum occur coffee -c decaf
datum update coffee:_ -c "second cup"
datum delete coffee:_ -c "wrong day"
```

```
CREATE dtgnq coffee:2026-09-24T14:54:52.113Z
16:54:52+2  coffee ∅●
  comment: "decaf"
UPDATE dtgnq coffee:2026-09-24T14:54:52.113Z
16:54:52+2  coffee ●
  comment: ["decaf","second cup"]
DELETE dtgnq coffee:2026-09-24T14:54:52.113Z
16:54:52+2  coffee ●
  comment: ["decaf","second cup","deleted: wrong day"]
```

### `ref://` — a reference, and `str://ref://` — a literal one

A stored string beginning `ref://` IS a reference to the entry whose slug
follows, on any key: the prefix is reserved, as `:` is in a state. It is
stored as typed, so every reader, replay and sync carries it unchanged.
Since the prefix is reserved, a literal string beginning `ref://` cannot
be stored bare: both `str://ref://…` and the JSON-quoted `'"ref://…"'`
store `str://ref://…`, which is shown without its `str://` and refers to
nothing. `str://` means this only in front of `ref://`. An array takes
the rule per element.

On a key whose subfield `value_type` is `ref` — set with `spec set
--from-json` or in a kit's field spec — a bare slug is a reference too,
stored as typed; a marker there is accepted and reads alike.

`get <id>::<key>::<name>` follows a reference as it follows a state (§4
`::`), and `datum check refs` names each one that finds no live entry, one
line per field, key and slug. Nothing is refused at write.

```bash
datum add book id=dune title=Dune
datum occur read what=ref://book:dune
datum occur quote said='"ref://book:dune"'
datum get read:_::what::title
datum get quote:_::said
datum spec set task --from-json '{"field":"task","verb":"occur","subfields":{"blocker":{"value_type":"ref"}}}'
datum occur task blocker=issue:k7f2a
datum occur plan needs=ref://book:dune,ref://book:emma
datum check refs
```

```
CREATE rp2sb book:dune
c21:40:09+2  book ¢
  title: "Dune"
CREATE 54ii3 read:2026-09-28T19:40:09.998Z
21:40:09+2  read ∅●
  what: "ref://book:dune"
CREATE p41in quote:2026-09-28T19:40:10.013Z
21:40:10+2  quote ∅●
  said: "ref://book:dune"
Dune
ref://book:dune
✓ Spec saved for 'task'
CREATE bdd2a task:2026-09-28T19:40:10.066Z
21:40:10+2  task ∅●
  blocker: "issue:k7f2a"
CREATE j285u plan:2026-09-28T19:40:10.083Z
21:40:10+2  plan ∅●
  needs: ["ref://book:dune","ref://book:emma"]
[WARN ] dangling_data_ref (x1): 1 plan entry refers to book:emma, which is no live entry (key needs)
    example: j285u
    example field: plan
    hids: j285u
[WARN ] dangling_data_ref (x1): 1 task entry refers to issue:k7f2a, which is no live entry (key blocker)
    example: bdd2a
    example field: task
    hids: bdd2a

Summary: 0 errors, 2 warnings.
```

`said` stores `str://ref://book:dune`, so `quote` is not listed: a literal
is no reference.

## 3. Inside a time

| token        | means                                                        |
|--------------|--------------------------------------------------------------|
| `±N[unit]`   | how precisely the time or duration is known                  |
| `+-N[unit]`  | the ASCII spelling of the same thing                         |
| `+` `-`      | trailing, on a year-less date: force forwards / backwards    |
| `-` `@` `,`  | the year separator on a compact date (`apr29-2025`)          |

### `±` — uncertainty

```bash
datum occur coffee -t '14:00±15m'
datum get coffee -A
```

```
CREATE 6r55f coffee:2026-09-22T12:00:00.000Z
14:00:00+2±15m  coffee ∅●
```

`get -A` shows it stored beside the instant, in a column of its own:

```
  occur_time                2026-09-22T12:00:00+00:00
  occur_time_offset         120 min
  occur_time_tz             Europe/Berlin
  occur_time_uncertainty    PT15M
```

`14:00+-15m` is the same input — the ASCII form only splits when
something precedes it, so a leading-sign offset like `-t +5` is still an
offset. A bare number defaults to minutes; `s`/`m`/`h`/`d` and ISO
(`PT5M`) are accepted. A duration takes it too, `dur=` or the trailing
positional, in the same forms (`dur=90m+-5`, `end task 90m±5m`):

```bash
datum start task dur=PT1H30M±PT5M
datum get task -A
```

Two lines of `get -A`, from different sections of it — the span and how
precisely it is known are separate columns:

```
  dur                       PT1H30M
  dur_uncertainty           PT5M
```

Uncertainty is deliberately outside the content hash — `coffee ±5m` and
`coffee ±1m` at the same instant are one coffee, and hashing the
precision would stop them ever deduping.

### `+` and `-` on a date — which way to resolve

```bash
datum occur probe -d sunday-
datum occur probe -d sunday+
```

```
CREATE e3h0w probe:2026-09-20
CREATE qty4x probe:2026-09-27
```

(Run on a Tuesday: the Sunday before, then the Sunday after.) Without a
marker the command's own default direction decides. Fully pinned forms —
an ISO date, a compact date with a year — ignore it.

### `-` `@` `,` — the year on a compact date

```bash
datum occur probe -d apr29
datum occur probe -d apr29-2025
datum occur probe -d apr29@2024
datum occur probe -d oct10,2022
```

```
CREATE eoe38 probe:2026-04-29
CREATE 6wm5i probe:2025-04-29
CREATE zkkr4 probe:2024-04-29
CREATE gtudw probe:2022-10-10
```

The first is the nearest April 29; the other three are pinned. All three
separators mean the same thing, so `apr29-2025` and `apr29@2025` name one
day and the second would answer `EXISTS` rather than write a row.

## 4. Naming an entry

Every command that takes an `ID_OR_SLUG` takes these.

| token                   | means                                          |
|-------------------------|------------------------------------------------|
| `_` `__` `___`          | 1st, 2nd, 3rd most recent entry                |
| `_4`                    | the 4th, for counts past a few underscores     |
| `_2-_10`                | the whole run between two ordinals             |
| `<field>:_3`            | the same counting, inside one field            |
| `_LAST`                 | everything the last write touched              |
| `_LAST_WITH_PROTECTION` | the same, but refuses if older than 15 minutes |
| `*` `?`                 | slug/hid wildcards — any run, any one character|
| `a1b2,c3d4`             | a comma-batched list of targets                |

An existing slug that holds a comma — `tx:[electricity,paid]`, from an
`--id` slot holding a list — is ONE target: the whole argument is tried as
a slug before the list splits.

A wildcard, like `_N`, skips the meta entries (specs, aliases, views, kit
commands) unless it names their field: `c*` is your coffee and never a
kit's `command:hydration`, while `command:*` is every kit command.

`datum tail` prints the ordinals in its `_n` column, so the number you
read is the one you type.

```bash
datum occur a
datum occur b
datum occur c
datum occur d
datum tail
datum get _2-_3
datum get b:_
datum get _1,_3
datum get 'a:*'
```

`datum tail`:

```
_n  time (+2)  f  s   hid
_4   17:59:04  a  ∅●  wvf06
_3   17:59:04  b  ∅●  rtzbg
_2   17:59:04  c  ∅●  78gwx
_1   17:59:04  d  ∅●  giwrb
```

`datum get _2-_3` — a range names the whole run between two ordinals:

```
EXISTS 78gwx c:2026-09-22T15:59:04.623Z
17:59:04+2  c ∅●
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+2  b ∅●
```

Ordinals count every live entry, so a field-filtered listing shows gaps —
`_3` means the same row everywhere. `<field>:_` counts inside the field
instead, and `datum get b:_` gives that field's newest row:

```
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+2  b ∅●
```

`datum get _1,_3` — a comma-batched list resolves per token:

```
EXISTS giwrb d:2026-09-22T15:59:04.643Z
17:59:04+2  d ∅●
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+2  b ∅●
```

`datum get 'a:*'` — `*` and `?` glob, against the WHOLE slug or hid
rather than a prefix of it (`?` stands for exactly one character, so a
hid needs eleven other characters beside it):

```
EXISTS wvf06 a:2026-09-22T15:59:04.574Z
17:59:04+2  a ∅●
```

`_LAST` is every entry the last invocation wrote, which after a
multi-entry write is more than one — that is the point of it:

```bash
datum alias set oov "switch environment outside , switch body vertical"
datum oov
datum get _LAST
```

```
EXISTS 1dkhw environment:2026-09-22T16:03:12.118Z
18:03:12+2  environment  outside
EXISTS 5jcj6 body:2026-09-22T16:03:12.118Z
18:03:12+2  body  vertical
```

A command that can only act on one entry says so rather than picking —
`datum why _LAST` here answers `'_LAST' is ambiguous and may refer to:`
with both rows listed.

`_LAST_WITH_PROTECTION` resolves the same pointer but refuses when it is
more than fifteen minutes old — the guard for commands that should not
quietly act on something you logged this morning.

### `::` — one value of an entry

`datum get <id>::<name>` prints one value of the entry alone on stdout: a
column (`hid`, `state`, …), `.x.y` into the state, anything else into
`data`. `::name` alone reads the last entry. A further `::` is a hop:
`<id>::state::<name>` follows the state to the entry it refers to (a `:`
state is that slug, a bare one goes through the spec's state lookup field)
and reads `<name>` there, up to 8 hops. A data key hops the same way when
its value is a reference (§2 `ref://`): `<id>::needs::<name>`. A value that refers to nothing, an
array, a missing entry or a loop is an error naming it.

```bash
datum switch media book
datum get _::hid
datum get ::state
```

```
t91w30mlcq32
book
```

### `:` inside a state — a reference

A state holding `:` is ALWAYS a full slug `field:id`, whatever field it
is logged on: `:` inside a state value is reserved to mean "reference",
as `::` is reserved in an id. It is a rule of the grammar, not a guess
from the string's shape — `12:30` is a reference too, and one that names
nothing.

A state without `:` refers to an entry only when its field's spec names a
lookup field: `spec set issue --state-lookup-field issue_state` makes the
state `queued` refer to `issue_state:queued`. Anything else is a plain
state. An array state takes the rule per element; `false`, `true`, a
number, and the TRACKED and IGNORED sentinels never refer.

Nothing is refused at write — the entry named may yet arrive by sync —
and `datum check refs` names each reference that finds no live entry, one
line per field and slug:

```bash
datum add book id=great_gatsby
datum switch media book:great_gatsby
datum spec set issue --verb switch --state-lookup-field issue_state
datum switch issue queued
datum switch clock 12:30
datum check refs
```

```
[WARN ] dangling_state_ref (x1): 1 clock entry refers to 12:30, which is no live entry
    example: z90mo
    example field: clock
    hids: z90mo
[WARN ] dangling_state_ref (x1): 1 issue entry refers to issue_state:queued, which is no live entry
    example: emfed
    example field: issue
    hids: emfed

Summary: 0 errors, 2 warnings.
```

`media`'s reference is not listed: `book:great_gatsby` exists.

## 5. Inside a template

`--id` takes the part of the slug after `<field>:`; `-o` /
`--format-string` renders a row. Both use the same `%…%` language.

| token       | means                                                       |
|-------------|-------------------------------------------------------------|
| `%key%`     | interpolate — a column, or a dotted path into `data`        |
| `%?key%`    | the same, marked optional                                   |
| `\%`        | a literal `%`                                               |

The column names are `field`, `uid`, `state`, `dur`, `following_state`,
`occur_time`, `create_time`, `modify_time`. `occurTime` / `createTime` /
`modifyTime` also resolve, for templates carried over from the original
datum. Anything else is a dot-path lookup into `data`.

In an `--id`, a path segment also indexes a list — `%comment.0%` is the
first comment, `%comment.-1%` the last — and a list or object slugs as
relaxed JSON: no spaces, and quotes only on a key or string that is empty
or holds one of `, [ ] { } :`. Two comments give `tx:[electricity,paid]`,
an object `{a:1,b:x}`; one comment is just itself.

```bash
datum note word correr --id '%field%:%srcLang%:%pos%' srcLang=pt pos=verb
```

```
CREATE rvpcn word:word:pt:verb
```

A missing key renders empty and the delimiters around it collapse, so a
template need not know which keys a given entry has:

```bash
datum note word correr --id '%?srcLang%:%pos%' srcLang=pt pos=verb
datum note word andar --id '%?srcLang%:%pos%' pos=verb
```

```
CREATE tof44 word:pt:verb
CREATE aiye2 word:verb
```

`%?key%` and `%key%` behave identically today — the `?` records that the
key is expected to be absent sometimes, and reserves the spelling for a
future strict mode. `--id-delimiter` changes which character collapses:

```bash
datum note w3 x --id '%field%-%pos%' --id-delimiter - pos=verb
```

```
CREATE 9ad7v w3:w3-verb
```

`\%` is a literal percent:

```bash
datum note pct y --id '100\%-%field%'
```

```
CREATE ow8wk pct:100%-pct
```

`-o` reads the same way, against each row a read command prints:

```bash
datum note word correr --id '%field%:%srcLang%:%pos%' srcLang=pt pos=verb
datum tail -o '%slug% %field%'
```

```
word:word:pt:verb word
```

## Where these live in the code

| area                          | file                                                 |
|-------------------------------|------------------------------------------------------|
| sides, regions, groups        | `crates/datum-parse/src/alias/regions.rs`            |
| flag tokenisation             | `crates/datum-parse/src/parse/core.rs`               |
| value coercion                | `crates/datum-parse/src/input/data_args/coerce.rs`   |
| paths and key suffixes        | `crates/datum-parse/src/input/data_args/classify.rs` |
| `±` and the direction marker  | `crates/datum-parse/src/time/uncertainty.rs`         |
| compact dates                 | `crates/datum-parse/src/time/date.rs`                |
| quick-ids                     | `crates/datum-core/src/ids/resolve.rs`               |
| `:` inside a state            | `crates/datum-core/src/doc_ref.rs` (`state_refs`)    |
| `ref://` in a data value      | `crates/datum-core/src/doc_ref.rs` (`data_refs`)     |
| `%…%` templates               | `crates/datum-core/src/field_spec/interpolation.rs`  |
