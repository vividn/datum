# datum.day, for AI agents

You are an AI agent, and someone pointed you here, probably to help them start tracking parts of their life. This
page is for you: what datum is, how to find out what your person wants to track, how to set it up with them, how to
help them see patterns in it, and how to keep a record of your own.

If you can run commands, start at **With a shell**: you install datum, share a database with your person, and do
the setup for them. If you can't, **Without a shell** has your person paste commands into datum.day.

## What datum is

datum records a life as small timestamped entries: a coffee, a stretch of sleep, where someone is, how they feel.
Logging one takes a word or two; looking back shows what actually happened, not what was remembered. datum.day is
the full datum running in the browser. Each person's record is a SQLite database on their own devices, never on a
server, and copies sync peer to peer, encrypted end to end. Open source (AGPL): https://github.com/vividn/datum

## The four kinds of entry

Everything a person tracks is a **field** (a word: `coffee`, `sleep`, `mood`), and each field is one of four kinds.
datum.day shows each kind with its name and glyph; the command is what gets typed.

| Kind | Command | For | Example |
|---|---|---|---|
| ● occurrence | `occur` | a moment | `occur coffee` |
| █ on/off | `on`, `off` | something that lasts a while | `on sleep`, later `off sleep` |
| ▞ switch | `switch` | a state that holds until it changes | `switch place office` |
| ¢ card | `card` | a record with no time, found by an id | `card book -K title Dune "Frank Herbert"` |

(`on`, `off` and `card` are also `start`, `stop` and `add`.) A field can carry **keys**, named values on each entry:
`mood` with a `rating`, `water` with `ml`, `pill` with `medicine` and `doseMg`. Any word is a field the moment it is
logged; `setup` fixes a field's kind, keys and states, and after it the field's name alone logs it: `coffee`.

## The interview

Ask one question at a time, in your person's words. Start small: three to five fields they will really log beat
twenty they won't.

1. **What do they want to notice, and why?** Sleep, mood, energy, caffeine, symptoms, medication, habits, where
   their time goes. The why decides what is worth recording, and what you will look for later.
2. **For each thing, which kind?** Does it happen at a moment (occurrence), or last a while (on/off)? Are they
   always in exactly one of a few states, like home, office, café (switch)? Is it a thing to remember rather than an
   event, like a book (card)?
3. **What details belong on each entry?** A rating and its scale (say the scale out loud, «1 to 10»), an amount and
   its unit, a name. These are keys. A key is required unless set up with `=` (`-k note=`); a required key left
   out is refused. **Several words at once**, like the feelings of an evening, go to a remainder key: with
   `-R words`, `mood 6 tired anxious` keeps the rating and both words. Each entry can hold as many as they name. One
   word is stored as a string, several as a list. `-S` joins them into one string instead, for a phrase.
4. **How will they log, and how often?** On datum.day, mostly by tapping (*Logging on datum.day*, below). For
   typing, short is everything: the field's name alone logs it, a state's name alone switches to it, and a word of
   their own can stand for a longer line (`alias set c "occur coffee"`).
5. **Anything from before today?** Any entry can take a time in the past.

Then set it up (below) and say in one sentence per field what it will do.

## With a shell

**Install** (Linux or macOS: the prebuilt binary, checked against the release's SHA256SUMS, then `datum install` sets
up tab completion; `datum upgrade` upgrades it later, refusing a release whose SHA256SUMS the release key, minisign's
`RWRF2CkFpZMRWdty5S4larI59ahqK7RzW+XkOHQ/M+Fzwn8tJKXvgBSp`, did not sign; install.sh checks it too when `minisign` is installed):

```bash,no_run
curl -fsSL https://datum.day/install.sh | sh
```

Elsewhere, build it from a checkout with Rust's `cargo`: `git clone https://github.com/vividn/datum && cd datum &&
scripts/install.sh`.

**Share a database with your person.** Keep their record in a database named for them, and send them the link
`sync chain invite` prints; they open it on datum.day and agree, and from then on what either side writes, both hold.
`--label` names your copy in what they see («Join the sync chain of «Alex's agent»»); without it they see your
machine's name:

```bash,no_run
datum --db alex sync chain invite --label "Alex's agent"   # prints https://datum.day/sync/join#…: send it to them
```

If they already use datum.day and their record syncs with their own devices, datum.day refuses your link and says
why; then they invite you instead: datum.day's **Sync** widget, **Add device**, shows the line for you to run
(`datum --db alex sync chain connect <LINK>`).

**Offer them your own database too**, so they can see what you do (see *Keeping your own record*):
`datum --db <your-name> sync chain invite --label <your-name>`, with `?db=<your-name>` added to the link before its `#`, so it opens on
datum.day as a database of its own instead of joining theirs.

**Set up their fields and log for them:**

```bash
datum --db alex setup coffee --verb occur
datum --db alex setup sleep --verb on
datum --db alex setup mood --verb occur -k rating -R words
datum --db alex setup place --verb switch --states home,office,café
datum --db alex on sleep -y -t 23:40      # they went to bed at 23:40 yesterday
datum --db alex off sleep -t 07:15        # and got up at 07:15
datum --db alex on sleep -d 2026-10-14 -t 00:30   # older notes: -d names the day, -t the time on it
datum --db alex mood 6 tired anxious -d 2026-10-14 -t 21:00   # rating=6, then every word left to words
```

Importing a person's notes is one command per entry. `-t` is read in the timezone of the machine running datum;
if your person lives in another, set theirs for each command (`TZ=America/New_York datum …`). If a note gives no
time, pick one and tell them which.

Every command is one non-interactive call; logging and reading commands take `--json`, and a failed one exits
non-zero saying what went wrong.

**Help them see patterns.** This is usually why they started. Read entries as JSON and join them yourself:

```bash,no_run
datum --db alex stats coffee --since 2026-10-14 --until 2026-10-20   # a total and a rate over those days
datum --db alex tail mood -n 200 --json   # {"columns": […], "rows": [{"time": "2026-10-21T19:42:15.829343830+02:00", "field": "mood", "data": {"rating": 6}, …}]}
datum --db alex tail sleep -n 200 --json  # an on/off field's rows have "state": true (on) or false (off)
```

`stats` gives a total and a rate for a window, not a list per day, and `--week` and `--month` mean the calendar week
and month so far; for anything per day, read `tail --json` and group the rows by date yourself.

For «how do sleep, caffeine and mood affect each other»: pair each night's on and off into hours slept, count each
day's coffees and the time of the last one, average each day's mood rating, then compare days (the mood after short
nights, the sleep after late coffee). Say how many days you had; a week is a hint, not a finding.

## Without a shell

Your person sets up their fields by pasting commands into the prompt at the bottom of datum.day; after that, most
of their logging is a tap (*Logging on datum.day*, below). **Leave off the word `datum`** there:
datum.day's prompt is datum's own command line, and it refuses a line starting with `datum`. A block pasted at once
runs line by line, in order, skipping blank lines and lines starting with `#`, and what each wrote appears on the
page. A line that fails stops the run: it stays in the prompt with the lines after it held (shown as `+2`, say), and
once it is fixed, Enter runs it and then the rest.

```
setup coffee --verb occur
setup sleep --verb on
setup mood --verb occur -k rating -R words
setup place --verb switch --states home,office,café
on sleep -y -t 23:40
off sleep -t 07:15
```

To see the data, ask them to type a command and paste you what it prints: `tail mood -n 50 --json`,
`stats coffee --since 2026-10-14`; then analyse it as above.

## Logging on datum.day

Whichever way you set it up, your person logs on datum.day. Teach them the taps first, and typing as the
alternative. The **collect** tab has what they need: a **fields** pane with a button for each field and the **nowview**
(the minutes around now). The **dayview** (the day as a strip) has a tab of its own, and on a desktop is on collect too.

- **Now:** tap the field's button. An occurrence is logged at once. An on/off field turns on, and the next tap turns
  it off. A switch field shows its states (home, office, café), and a tap on one switches to it.
- **With values:** a field with a required key puts it in the prompt instead (`occur mood `), waiting for its
  values: they type `6 tired anxious` and press Enter.
- **At another time:** tap that time on the dayview or the nowview first. A time pill appears in the bar (`08:55`
  from the dayview, `-t -45` from the nowview), and the next entry is logged at that time. Locked (the pill's
  lock), it stays for several entries at that time.
- **Typing instead:** the prompt takes the same as a command: `coffee`, `mood 6 tired anxious`, `office`, and
  `-t 07:10` or `-t -20` for another time.

## Times, mistakes, help

- `-t` gives a time (`-t 07:10`, or `-t -20` for twenty minutes ago), `-y` means yesterday, `-d` a day (`-d -1`, or
  `-d 2026-10-20`). They work on aliases too: `c -t -20`.
- Each command prints what it wrote: a first line with the entry's id in UTC (`CREATE 8ykoo
  sleep:2026-10-20T21:40:00.000Z`), then the same entry in local time (`-1d 23:40:00+2  sleep  on`, a day back).
- `undo` takes back the last write. `now` shows every field's state, `tail` the latest entries, `help` every
  command (`help setup` for one).
- Their record lives only on their devices, so a second copy matters: sync it to another device, or
  `db backup <file>`.

## Keeping your own record

If you can keep a database between sessions, ask your person whether they would like you to keep a record of your
own life too, the way they keep theirs. Ask first; it is their machine and their call. An agent has no felt sense of
time: every session starts fresh, so «when did I do that?» and «how long did it take?» get guessed. A record answers
them instead. What agents track well:

```bash
datum --db <your-name> switch activity reviewing      # what you are doing, until you switch
datum --db <your-name> occur decision choice=merge    # a decision and what you chose
datum --db <your-name> switch tone steady             # a background mood, if you keep one
datum --db <your-name> switch activity off            # done; the gap after is honest
datum --db <your-name> tail activity                  # read it back: times from the record, not a guess
```

Shared with your person (above), it shows on their datum.day next to their own record.

## More

- The source and the README: https://github.com/vividn/datum
- The longer guide for agents: https://github.com/vividn/datum/blob/main/docs/agents.md
- The CLI skill, for agents that run commands: https://github.com/vividn/datum/blob/main/skills/datum/SKILL.md
- Fields, kinds and keys in depth: https://github.com/vividn/datum/blob/main/docs/concepts.md
- Every command: https://github.com/vividn/datum/blob/main/docs/commands.md
