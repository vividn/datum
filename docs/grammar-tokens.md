# Grammar tokens

Flags are documented by `datum --help` and the man pages. This page is for
the *other* half of the surface: the punctuation the parser gives meaning
to. A reader meets these in alias bodies, in error messages (`unbalanced
'(' in expanded command (use '\(' for a literal paren)`) and in what
`datum tail` prints back (`±`, `_3`), and until this page there was
nowhere to look them up.

A token lives in one of four places, and that is how this page is
organized:

1. as a **whole argv token** — whitespace on both sides, or it is not that
   token at all;
2. **inside one argument's value** — after the `=`, or as a bare value;
3. **inside a time**, as a suffix;
4. **inside a template** — `--id`, `-o`.

Every `bash` block below is run against the built binary by
`crates/datum-cli/tests/readme_snippets.rs`, each in its own throwaway
database, in UTC. The transcript under each block is what a run of it
printed, pasted rather than described, and the test holds it to that: a
block's one transcript is the whole of what it printed, and one of several,
or one after prose, a run of lines in it. Hids, instants, clocks and dates
differ per run, and are masked; anything else that changes fails CI until
it is pasted again (`DATUM_REPASTE=1` on that test re-pastes each whole
one).

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
datum change body vertical , change project gardening
```

```
CHANGE g5ug0 body:2026-11-05T20:01:08.065Z
20:01:08+0  body   vertical
CHANGE ss64b project:2026-11-05T20:01:08.065Z
20:01:08+0  project   gardening
```

Both rows carry the same `at` and are written atomically. The
comma must be its own token — whitespace on both sides — so `vertical,sit`
is one value, a parallel state, and a comma touching one side only
(`vertical, sit`) is refused. There is no
escape syntax; whitespace is the whole trigger. See
[`aliases.md`](aliases.md) (`datum help alias`, ATOMIC MULTI-ENTRY ALIASES)
for what the sides share.

### `,,` — bind a value to every side to its left

```bash
datum on run , on bike ,, 30m
```

```
PERIOD 4pw23 run:2026-11-05T20:01:08.142Z
20:01:08+0  run   on   30m
PERIOD 92wcq bike:2026-11-05T20:01:08.142Z
20:01:08+0  bike   on   30m
```

One `30m`, both sides. Without the `,,` the `30m` would belong to `bike`
alone.

### `(` `)` — how far left "every side to its left" reaches

A region binds within its innermost enclosing group, so a group is how
you keep one out of another's way. Quote them past the shell.

```bash
datum moment a , '(' moment b , moment c ,, k=1 ')'
```

```
MOMENT b9hil a:2026-11-05T20:01:08.216Z
20:01:08+0  a ∅●
MOMENT ceo10 b:2026-11-05T20:01:08.216Z
20:01:08+0  b ∅●
  k: 1
MOMENT 06k45 c:2026-11-05T20:01:08.216Z
20:01:08+0  c ∅●
  k: 1
```

`k=1` reached `b` and `c`. It did not reach `a`, which is outside the
group.

### `\(` `\)` — a paren that is data

Parens are structural unconditionally — `--` does not demote them — so a
literal one is spelled with a backslash.

```bash
datum moment f -k expr ,, '\('
```

```
MOMENT nwpeh f:2026-11-05T20:01:08.293Z
20:01:08+0  f ∅●
  expr: "("
```

Unbalance them and the error names the escape:

```bash,fails_with=1
datum moment f ,, '('
```

```
Error: Invalid argument: unbalanced '(' in expanded command (use '\(' for a literal paren)
```

Only a *bare* paren token is structural: `(x)` and `:-)` never were.

### `--` — end of flags, side-scoped

```bash,fails_with=1
datum moment g -k note --verbose
```

```
Error: Invalid argument: unexpected flag: --verbose
```

```bash
datum moment f -k note -- --verbose
```

```
MOMENT 01lfb f:2026-11-05T20:01:08.376Z
20:01:08+0  f ∅●
  note: "--verbose"
```

The effect ends at the next `,` or `,,`, and a `--` inside an alias body
never leaks to the call site.

### `.` — skip a slot, and `\.` — a dot that is data

```bash
datum alias set scaff "change body sit , moment caffeine -k doseMg="
datum scaff .
```

```
✓ Alias 'scaff' saved.
CHANGE 0cak0 body:2026-11-05T20:01:08.457Z
20:01:08+0  body   sit
MOMENT 6yyor caffeine:2026-11-05T20:01:08.457Z
20:01:08+0  caffeine ∅●
```

The `.` consumed the first unfilled slot and wrote nothing into it, so
`caffeine` has no `doseMg`. For a shared slot the skip applies on every
side that declares it.

`\.` is the way to say the character:

```bash
datum change tv -I the end '\.'
```

```
CHANGE n0too tv:2026-11-05T20:01:08.534Z
20:01:08+0  tv   the end .
```

### `-` — stdin or stdout instead of a file

```bash
datum moment x
datum db backup -
```

The backup goes to stdout rather than to a file, so the first bytes after
the `moment`'s own echo are the archive itself:

```
{"type":"daydumb_backup",…
```

`db restore -` and `db import --from -` read the same way. A bare `-` is
never parsed as a flag, and neither is a dash followed by a digit or a
dot (`-5`, `-5.3` are values).

### `ISLAST` — what `--if-changed` answers instead of writing

`change`, `on` and `off` take `--if-changed`: when the field already
holds that state at the entry's time (`-t`, or now), nothing is written
and the entry that set it prints as `ISLAST`, exit 0. It is a flag like
any other, so an alias body can carry it:

```bash
datum alias set act "change activity --if-changed"
datum act reading
datum act reading
datum act cooking
```

```
✓ Alias 'act' saved.
CHANGE gzvr8 activity:2026-11-05T20:01:08.685Z
20:01:08+0  activity   reading
ISLAST gzvr8 activity:2026-11-05T20:01:08.685Z
20:01:08+0  activity   reading
CHANGE dz0m3 activity:2026-11-05T20:01:08.756Z
20:01:08+0  activity   cooking
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
datum change mood happy .intensity=7
datum change mood state=happy .intensity=7
datum get mood -A
```

Two writes, one state — the `!` on the second only marks that the field
was already in it:

```
CHANGE avm03 mood:2026-11-05T20:02:07.250Z
20:02:07+0  mood   {happy}
  { .id: "happy", .intensity: 7 }
CHANGE ujlzn mood:2026-11-05T20:02:07.278Z
20:02:07+0  mood  !{happy}
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
datum moment run gear.shoes=trail
```

```
MOMENT so79z run:2026-11-05T20:01:08.922Z
20:01:08+0  run ∅●
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
`[]` is the empty array. A bracket or brace value that isn't JSON is read
as v2's relaxed JSON read it, a bare word a string: `[calm,steady]` is
`["calm","steady"]` and `{a: [b, 1]}` is `{"a":["b",1]}` (da-2wrzv). A
list-typed key and a state read it the same way, each element then
checked as its type (`change mood [calm,steady]` is the states calm and
steady, da-0d7e1). An untyped key, a list-typed key and a state refuse a
value that begins with `[` or ends with `]`, holds a comma and does not
read as a list (`[5pm,6pm]`, `[a,b`, `a,b]`) rather than split it with a
bracket kept; with no comma (`[a`) an untyped key and a state take it as
text, and a list-typed key refuses it too (da-kbdlg). A text-typed key
takes the value as written. This departs from v2, where a leading or trailing
comma made the array — a trailing one alone, too — and `,` was `[]`.

Quote a bracket value for the shell: zsh reads an unquoted `[` as a glob,
so `datum change mood [calm,steady]` stops at «zsh: no matches found»
before datum sees it, and bash passes it on only while no file matches.
Write `'[calm,steady]'`, or in zsh `alias datum='noglob datum'`.

```bash
datum moment tagged tags=work,home
datum moment one tags=,work
datum moment gappy tags=x,,y
datum moment prose 'note=met Anna, Ben' left=Anna,
datum moment cleared 'tags=[]'
```

```
MOMENT c2vko tagged:2026-11-05T20:01:08.985Z
20:01:08+0  tagged ∅●
  tags: ["work","home"]
MOMENT qgg0t one:2026-11-05T20:01:09.031Z
20:01:09+0  one ∅●
  tags: ["work"]
MOMENT zoie9 gappy:2026-11-05T20:01:09.072Z
20:01:09+0  gappy ∅●
  tags: ["x",null,"y"]
MOMENT jzdci prose:2026-11-05T20:01:09.114Z
20:01:09+0  prose ∅●
  left: "Anna,", note: "met Anna, Ben"
MOMENT ygwgc cleared:2026-11-05T20:01:09.150Z
20:01:09+0  cleared ∅●
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
datum moment n k=null j=undefined s='"null"'
```

```
MOMENT i19xd n:2026-11-05T20:01:09.211Z
20:01:09+0  n ∅●
  k: null, s: "null"
```

A null is how a chore is retired: `next_date=null` overrides the date
before it. A `null` STATE of a change is IGNORED, which is what it meant in
v2 — `datum change mood null` is `datum ignore mood`.

### JSON shapes, and the two escapes around them

Anything strict JSON parses keeps its JSON shape. `''` and `""` are the
empty string — quote them past the shell.

```bash
datum moment shapes flag=true obj='{"a":1}' empty="''"
```

```
MOMENT 1xew8 shapes:2026-11-05T20:01:09.272Z
20:01:09+0  shapes ∅●
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
datum moment quoted -k key "',,'" "v='a,b'" "l=['a', 'b']"
```

```
MOMENT x31lg quoted:2026-11-05T20:01:09.323Z
20:01:09+0  quoted ∅●
  key: ",,", l: ["a","b"], v: "a,b"
```

An `=` with nothing before it is not a key at all — the whole token,
`=` included, is data:

```bash
datum moment f -k raw ,, '=2'
```

```
MOMENT n5amn f:2026-11-05T20:01:09.392Z
20:01:09+0  f ∅●
  raw: "=2"
```

### `75.2kg` — a string, as typed

Digits followed by letters are not a number with a unit: `75.2kg`,
`4th`, `5pm` and an id like `6orhg` are stored exactly as typed. Only
what strict JSON parses is a number.

```bash
datum moment lift weight=75.2kg reps=10 set=4th
```

```
MOMENT 0zi1x lift:2026-11-05T20:01:09.497Z
20:01:09+0  lift ∅●
  reps: 10, set: "4th", weight: "75.2kg"
```

A key whose name ends in `time`, `date`, `dur` or `duration` (at a word
boundary, or camelCased, optionally with trailing digits) is parsed as
that kind first. So `dur=5m` stores the duration `PT5M`, while
`splits.first=5m` stores the string `"5m"`.

### `comment` — an array, only ever appended to

`-c`/`--comment` and `-C`/`--comment-remainder` each APPEND one string to
`data.comment`, on every logging command and on `update`; nothing replaces
one. The key is an array however many it holds (one prints as itself), and
a comment is kept as typed: `-c 42` is `"42"`. `delete -c WHY` appends
`deleted: WHY` in the same write that deletes the row, to every row of a
comma list.

```bash
datum moment coffee -c decaf
datum update coffee:_ -c "second cup"
datum delete coffee:_ -c "wrong day"
```

```
MOMENT 3kd5j coffee:2026-11-05T20:01:09.564Z
20:01:09+0  coffee ∅●
  comment: "decaf"
UPDATE 3kd5j coffee:2026-11-05T20:01:09.564Z
20:01:09+0  coffee  ●
  comment: "decaf" → ["decaf","second cup"]
DELETE 3kd5j coffee:2026-11-05T20:01:09.564Z
20:01:09+0  coffee  ●
  comment: ["decaf","second cup","deleted: wrong day"]
DELETE rjf33 spec:coffee
1970-01-01 00:00:00+0  spec  ●
   allowed_states: null
            color: null
        data_keys: []
  derived_aliases: [{"expands_to":"occur coffee","kind":"bare_field","state":"coffee"}]
      description: null
            field: "coffee"
          id_keys: []
      id_template: null
     no_autoalias: false
     state_colors: {}
        subfields: {}
             verb: "occur"
```

The row was `coffee`'s only entry, so the delete takes the spec its first
log made with it, and prints that too.

### `ref://` — a reference, and `str://ref://` — a literal one

A stored string beginning `ref://` IS a reference to the entry whose slug
follows, on any key: the prefix is reserved, as `:` is in a state. It is
stored as typed, so every reader, replay and sync carries it unchanged.
Since the prefix is reserved, a literal string beginning `ref://` cannot
be stored bare: both `str://ref://…` and the JSON-quoted `'"ref://…"'`
store `str://ref://…`, which is shown without its `str://` and refers to
nothing. `str://` means this only in front of `ref://`. An array takes
the rule per element.

On a key whose subfield `value_type` is `ref` — set with `setup F
--from-json` or in a kit's field spec — a bare slug is a reference too,
stored as typed; a marker there is accepted and reads alike.

`get <id>::<key>::<name>` follows a reference as it follows a state (§4
`::`), and `datum check refs` names each one that finds no live entry, one
warning per field, key and slug, listing its entries. A reference to an entry is never refused at
write.

A slug with nothing after its `::`, `ref://issue__k7f2a9x3::`, refers to a
field's RECORD (`get <field>::`), not to an entry. Written, it names the
field its name names as a read does (`datum help names`): `ref://issue__k7f::`
is stored `ref://issue__k7f2a9x3::` while that field alone starts
`issue__k7f`, so the stored reference never depends on which prefixes are
unique later. One starting several fields is refused, naming them, and one
naming none where no write may create it (a strict template, `auto_setup
false`). `check refs` names one whose field has no entries and no spec, a
hop through one reads the record, and `get <field> --backlinks` lists each
record that refers to the field's, with the keys that do.

```bash
datum record book --id dune title=Dune
datum moment read what=ref://book:dune
datum moment quote said='"ref://book:dune"'
datum get read:_::what::title
datum get quote:_::said
datum setup task --from-json '{"field":"task","verb":"moment","subfields":{"blocker":{"value_type":"ref"}}}'
datum moment task blocker=issue:k7f2a
datum moment plan needs=ref://book:dune,ref://book:emma
datum check refs
```

```
RECORD zms7v book:dune
c20:01:09+0  book ¢
  title: "Dune"
MOMENT hyszo read:2026-11-05T20:01:09.715Z
20:01:09+0  read ∅●
  what: "ref://book:dune"
MOMENT sqlcy quote:2026-11-05T20:01:09.760Z
20:01:09+0  quote ∅●
  said: "ref://book:dune"
Dune
ref://book:dune
MOMENT aigp4 spec:task
task  moment  tracked
  keys    -k blocker=
MOMENT ed9x6 task:2026-11-05T20:01:09.872Z
20:01:09+0  task ∅●
  blocker: "issue:k7f2a"
MOMENT ea05c plan:2026-11-05T20:01:09.900Z
20:01:09+0  plan ∅●
  needs: ["ref://book:dune","ref://book:emma"]
entries:
[WARN ] dangling_data_ref (x1): 1 plan entry refers to book:emma, which is no live entry (key needs)
    ea05c  20:01:09+0  plan  ∅●
[WARN ] dangling_data_ref (x1): 1 task entry refers to issue:k7f2a, which is no live entry (key blocker)
    ed9x6  20:01:09+0  task  ∅● issue:k7f2a

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
datum moment coffee -t '14:00±15m'
datum get coffee -A
```

```
MOMENT pbutc coffee:2026-11-05T14:00:00.000Z
14:00:± +0  coffee ∅●
```

A gray `±` takes the seconds' place after the colon, false precision on a
time known to fifteen minutes, and a space follows it, so the stamp is as wide
as an exact one and no column shifts. The amount is not printed there; `get -A` shows it stored beside the
instant, in a column of its own, as `--json` carries it:

```
  at                        2026-11-05T14:00:00+00:00
  at_offset                 0 min
  at_tz                     UTC
  at_uncertainty            PT15M
```

`datum update coffee at_uncertainty=5m` sets it on an entry already logged,
stored as `PT5M` whichever form it is typed in, and `at_uncertainty=` or
`at_uncertainty=null` drops it.

`14:00+-15m` is the same input — the ASCII form only splits when
something precedes it, so a leading-sign offset like `-t +5` is still an
offset. A bare number defaults to minutes; `s`/`m`/`h`/`d` and ISO
(`PT5M`) are accepted. A duration takes it too, `dur=` or the trailing
positional, in the same forms (`dur=90m+-5`, `off task 90m±5m`):

```bash
datum on task dur=PT1H30M±PT5M
datum get task -A
```

Two lines of `get -A` — the span and how precisely it is known are
separate columns, the first in its What section and the second in its When:

```
  dur                       PT1H30M
```

```
  dur_uncertainty           PT5M
```

`update` takes the same `dur_uncertainty=`, and `dur_uncertainty=` or
`dur_uncertainty=null` drops it. `dur=` or `dur=null` drops the duration, and
the trailing state that ended it: `on task 30m`, so updated, is `on task`.

Wherever the duration is printed — the echo, `get`, `tail`'s dur column —
a gray `±` and its amount follow it:
`20:02:08+0  task   on   1h30m±5m`.

Uncertainty is deliberately outside the content hash — `coffee ±5m` and
`coffee ±1m` at the same instant are one coffee, and hashing the
precision would stop them ever deduping.

### `+` and `-` on a date — which way to resolve

```bash
datum moment probe -d sunday-
datum moment probe -d sunday+
```

```
ALLDAY xr027 probe:2026-11-01
2026-11-01  probe ∅●
ALLDAY 28quf probe:2026-11-08
2026-11-08  probe  ●
```

(Run on Thursday 2026-11-05: the Sunday before, then the Sunday after.)
`-d` and no clock names the whole day, so each is an ALLDAY entry. Without a
marker the command's own default direction decides. Fully pinned forms —
an ISO date, a compact date with a year — ignore it.

### `-` `@` `,` — the year on a compact date

```bash
datum moment probe -d apr29
datum moment probe -d apr29-2025
datum moment probe -d apr29@2024
datum moment probe -d oct10,2022
```

```
ALLDAY c6vyo probe:2027-04-29
2027-04-29  probe ∅●
ALLDAY qgokn probe:2025-04-29
2025-04-29  probe ∅●
ALLDAY nu17l probe:2024-04-29
2024-04-29  probe ∅●
ALLDAY 549vh probe:2022-10-10
2022-10-10  probe ∅●
```

The first is the nearest April 29; the other three are pinned. All three
separators mean the same thing, so `apr29-2025` and `apr29@2025` name one
day and the second would answer `EXISTS` rather than write a row.

## 4. Naming an entry

Every command that takes an `ID_OR_SLUG` takes these.

| token                   | means                                          |
|-------------------------|------------------------------------------------|
| `_` `__` `___`          | 1st, 2nd, 3rd most recent entry, not in the future |
| `_4`                    | the 4th, for counts past a few underscores     |
| `_+1` `_+2`             | 1st, 2nd entry in the future, nearest first    |
| `_2-_10`, `_+1-_+3`     | the whole run between two ordinals             |
| `<field>:_3`            | the same counting, inside one field            |
| `<field>:<state>:_`     | inside one field, only entries in that state   |
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

`datum tail` prints the ordinals in its `ref` column, so the number you
read is the one you type; a row with no ordinal (a meta entry, or one past
`_99`) shows its hid there instead.

```bash
datum moment a
datum moment b
datum moment c
datum moment d
datum tail
datum get _2-_3
datum get b:_
datum get _1,_3
datum get 'a:*'
```

`datum tail`:

```
ref  time (+0)  f  s
_4    17:59:04  a  ∅●
_3    17:59:04  b  ∅●
_2    17:59:04  c  ∅●
_1    17:59:04  d  ∅●
```

`datum get _2-_3` — a range names the whole run between two ordinals:

```
EXISTS 78gwx c:2026-09-22T15:59:04.623Z
17:59:04+0  c ∅●
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+0  b ∅●
```

Ordinals count every live entry, so a field-filtered listing shows gaps —
`_3` means the same row everywhere. `<field>:_` counts inside the field
instead, and `datum get b:_` gives that field's newest row:

```
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+0  b ∅●
```

`body:run:_` counts only the `body` entries whose state is `run`, or holds
it among parallel states: after `change body run -t 9:30` and `change body
lift -t 9:50 3`, `body:_` is the lift and `body:run:_` the run. The state
is everything between the field and the last `:`, so `media:book:gatsby:_`
reads the state `book:gatsby`. A slug spelled the same still wins.

`datum get _1,_3` — a comma-batched list resolves per token:

```
EXISTS giwrb d:2026-09-22T15:59:04.643Z
17:59:04+0  d ∅●
EXISTS rtzbg b:2026-09-22T15:59:04.598Z
17:59:04+0  b ∅●
```

`datum get 'a:*'` — `*` and `?` glob, against the WHOLE slug or hid
rather than a prefix of it (`?` stands for exactly one character, so a
hid needs eleven other characters beside it):

```
EXISTS wvf06 a:2026-09-22T15:59:04.574Z
17:59:04+0  a ∅●
```

`_LAST` is every entry the last invocation wrote, which after a
multi-entry write is more than one — that is the point of it:

```bash
datum alias set oov "change environment outside , change body vertical"
datum oov
datum get _LAST
```

```
✓ Alias 'oov' saved.
CHANGE qtti0 environment:2026-11-05T20:01:10.728Z
20:01:10+0  environment   outside
CHANGE mnnq1 body:2026-11-05T20:01:10.728Z
20:01:10+0  body   vertical
EXISTS qtti0 environment:2026-11-05T20:01:10.728Z
20:01:10+0  environment   outside
EXISTS mnnq1 body:2026-11-05T20:01:10.728Z
20:01:10+0  body   vertical
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
datum change media book
datum get _::hid
datum get ::state
```

```
CHANGE 9vywg media:2026-11-05T20:01:10.815Z
20:01:10+0  media   book
9vywg9wu94x7
book
```

### `:` inside a state — a reference

A state whose first `:` can join a field to an id is ALWAYS a full slug
`field:id`, whatever field it is logged on: `:` there is reserved to mean
"reference", as `::` is reserved in an id. That means neither side empty and
no space touching the `:`, so `note: a plain sentence` is prose and refers to
nothing. Whether the field exists is not asked: `12:30` is a reference too, and
one that names nothing, and so is `note:a plain sentence`, since an id may hold
a space (`book:great gatsby`), as a field may inside its name (`my coffee:x`).

Any other state refers to an entry only when its field's spec names a
lookup field: `setup issue --state-lookup-field issue_state` makes the
state `queued` refer to `issue_state:queued`. Anything else is a plain
state. An array state takes the rule per element; `false`, `true`, a
number, and the TRACKED and IGNORED sentinels never refer.

Nothing is refused at write — the entry named may yet arrive by sync —
and `datum check refs` names each reference that finds no live entry, one
warning per field and slug, listing its entries:

```bash
datum record book --id great_gatsby
datum change media book:great_gatsby
datum setup issue --kind change --state-lookup-field issue_state
datum change issue queued
datum change clock 12:30
datum check refs
```

```
RECORD 6tuuh book:great_gatsby
c20:01:10+0  book ¢
CHANGE mkf9g media:2026-11-05T20:01:10.936Z
20:01:10+0  media   book:great_gatsby
MOMENT oi572 spec:issue
issue  change  tracked
  state lookup  issue_state
CHANGE 9bq9e issue:2026-11-05T20:01:11.010Z
20:01:11+0  issue   queued
CHANGE ur8ty clock:2026-11-05T20:01:11.062Z
20:01:11+0  clock   12:30
entries:
[WARN ] dangling_state_ref (x1): 1 clock entry refers to 12:30, which is no live entry
    ur8ty  20:01:11+0  clock    12:30
[WARN ] dangling_state_ref (x1): 1 issue entry refers to issue_state:queued, which is no live entry
    9bq9e  20:01:11+0  issue    queued

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
`at`, `create_time`, `modify_time`. `occur_time` also resolves, and
`occurTime` / `createTime` / `modifyTime`, for templates carried over from the
original datum. Anything else is a dot-path lookup into `data`.

In an `--id`, a path segment also indexes a list — `%comment.0%` is the
first comment, `%comment.-1%` the last — and a list or object slugs as
relaxed JSON: no spaces, and quotes only on a key or string that is empty
or holds one of `, [ ] { } :`. Two comments give `tx:[electricity,paid]`,
an object `{a:1,b:x}`; one comment is just itself.

```bash
datum remark word correr --id '%field%:%srcLang%:%pos%' srcLang=pt pos=verb
```

```
REMARK 0l8ug word:word:pt:verb
c20:01:11+0  word ¢
  comment: "correr"
      pos: "verb"
  srcLang: "pt"
```

A missing key renders empty and the delimiters around it collapse, so a
template need not know which keys a given entry has:

```bash
datum remark word correr --id '%?srcLang%:%pos%' srcLang=pt pos=verb
datum remark word andar --id '%?srcLang%:%pos%' pos=verb
```

```
REMARK 2v15s word:pt:verb
c20:01:11+0  word ¢
  comment: "correr"
      pos: "verb"
  srcLang: "pt"
REMARK zvyp0 word:verb
c20:01:11+0  word ¢
  comment: "andar", pos: "verb"
```

`%?key%` and `%key%` behave identically today — the `?` records that the
key is expected to be absent sometimes, and reserves the spelling for a
future strict mode. `--id-delimiter` changes which character collapses:

```bash
datum remark w3 x --id '%field%-%pos%' --id-delimiter - pos=verb
```

```
REMARK xbb0r w3:w3-verb
c20:01:11+0  w3 ¢
  comment: "x", pos: "verb"
```

`\%` is a literal percent:

```bash
datum remark pct y --id '100\%-%field%'
```

```
REMARK ova3y pct:100%-pct
c20:01:11+0  pct ¢
  comment: "y"
```

`-o` reads the same way, against each row a read command prints:

```bash
datum remark word correr --id '%field%:%srcLang%:%pos%' srcLang=pt pos=verb
datum tail -o '%slug% %field%'
```

```
REMARK 1bxsq word:word:pt:verb
c20:01:11+0  word ¢
  comment: "correr"
      pos: "verb"
  srcLang: "pt"
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
