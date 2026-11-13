# datum for agents

datum keeps a life's record as small, honest entries, and an AI agent can use it in three ways: to keep its own
record, to help its person keep theirs, and to work on datum itself. The [datum skill](../skills/datum/SKILL.md)
is the short version for an agent: which kind of data fits what you track, how to backdate, how to read it back.

datum suits an agent well. Every command is one non-interactive call, `--json` prints rows a program can read, a
non-zero exit says what went wrong, and `--db <name>` keeps each record in a database of its own.

## Keep datum's home in persistent storage

An install keeps its databases and its device key, its identity to sync, in `DATUM_HOME`, and the names of its
databases and its config in `DATUM_CONFIG_HOME` ([configuration.md](configuration.md)). In a sandbox or container
whose home is wiped between sessions, every session is a new install: what it logged and had not synced is gone, and
the chains it joined do not know its new key, so its person's devices gain a stranger each time. Point both at your
persistent storage, in whatever runs at the start of each session:

```bash,no_run
export DATUM_HOME=/persistent/datum DATUM_CONFIG_HOME=/persistent/datum-config
```

To check it held, `datum --db <name> sync status --json` prints this install's device key as `this.device`: the
next session should print the same.

## With no persistent storage, restore from a code

Where nothing outlasts a session, the person gives the agent a code instead: `datum --db <name> sync chain agent
--level full --label claude` on one of their devices in the chain, or **Add agent** in datum.day's Sync widget. The
agent runs `datum sync restore` at the start of every session, and `datum sync` after writing, since an entry not yet
synced is lost with the sandbox:

```bash,no_run
datum --db <name> sync restore '<CODE>'   # or DATUM_AGENT_CODE=<CODE> in the env, and `datum --db <name> sync restore`
datum --db <name> sync                    # after writing
```

Each restore makes a fresh copy of the same device, one row in the person's grid, and retires the copy before it. The
code's level says what that copy holds:

- **full**: every entry, to read and analyze. A reader, or with `--write` a member, which a chain with no admin
  refuses: lock it first (`datum sync admin add`, or a passkey on datum.day).
- **light**: every spec and alias, the entries of the last `--window` (`1d` by default) and each field's state at its
  start, for context. A reader, or with `--write` a writer.
- **write**: no entries, only the entries it writes, of the `--fields` named where any are. It cannot see a field's
  state, so a `change`, `-l` or `_LAST` there writes blind.

The code is the agent's identity: whoever holds it is that agent, so keep it secret, out of a repository and out of a
log. It expires in 30 days unless `--expires` says otherwise (`60d`, `90d`, `6mo`, `1y`, `never`, or any duration;
the same choices in **Add agent**), and **Remove** on the agent's row of the Sync widget's grid (`datum sync chain expel
<LABEL>`) revokes it sooner: after either the code admits nothing. An agent never holds admin rights.

## Keep your own record

What an agent can track about itself, if it wants to know where its time and attention went:

```bash
datum change activity reviewing           # what it is doing now, until it changes
datum moment decision choice=merge         # a decision, with what it chose
datum change tone patient                 # a background mood, if it keeps one
datum change activity off                 # done; the gap after is honest
datum tail activity                       # read it back
```

A record like this answers questions an agent cannot answer from memory: how long a task really took, what it
decided and when, what it was doing when something went wrong. Times come from the record, never from a guess.

Shared with its person, they can watch what it does: `datum --db claude sync chain invite --label claude`, then send
them the link with `?db=claude` (the database name) added before its `#`:
`https://datum.day/sync/join?db=claude#<ticket>`. On datum.day it opens as a database of its own beside theirs;
without `?db=` it joins the record their browser has open.

## Help your person track

The step-by-step guide for an agent helping its person, from the interview through the setup and sharing a database
over sync to looking for patterns together, is **https://datum.day/agents.md**, where an agent sent to datum.day
finds it; its source is [web/static/agents.md](../web/static/agents.md). In short: the agent installs the CLI
(`curl -fsSL https://datum.day/install.sh | sh`, [install.md](install.md)), runs `datum --db their-name sync chain
invite` and sends its person the link; once they join on datum.day, both copies hold the same record, and the agent
sets up fields and logs for them.
[sync.md](sync.md) covers chains, admins and conflicts.

## Bring in data kept elsewhere

Data your person kept somewhere else (a spreadsheet, notes, what they told another AI) comes in as JSONL rows, one
entry a line: `{"field": "mood", "at": "2026-11-10 09:00", "state": "happy"}`. `datum import rows.jsonl --dry-run`
says what it would write, and `datum import rows.jsonl` writes every row or none, naming a refused one by its line;
one `undo` takes it back. [import.md](import.md) is the spec, `datum import --schema` its JSON Schema, and `datum
export --jsonl` prints rows to copy.

## Work on datum

Agents changing datum's code start at [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md).
