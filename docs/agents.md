# datum for agents

datum keeps a life's record as small, honest entries, and an AI agent can use it in three ways: to keep its own
record, to help its person keep theirs, and to work on datum itself. The [datum skill](../skills/datum/SKILL.md)
is the short version for an agent: which verb fits which kind of moment, how to backdate, how to read it back.

datum suits an agent well. Every command is one non-interactive call, `--json` prints rows a program can read, a
non-zero exit says what went wrong, and `--db <name>` keeps each record in a database of its own.

## Keep your own record

What an agent can track about itself, if it wants to know where its time and attention went:

```bash
datum switch activity reviewing           # what it is doing now, until it switches
datum occur decision choice=merge         # a decision, with what it chose
datum switch tone patient                 # a background mood, if it keeps one
datum switch activity off                 # done; the gap after is honest
datum tail activity                       # read it back
```

A record like this answers questions an agent cannot answer from memory: how long a task really took, what it
decided and when, what it was doing when something went wrong. Times come from the record, never from a guess.

## Help your person track

The step-by-step guide for an agent helping its person, from the interview through the setup and sharing a database
over sync to looking for patterns together, is **https://datum.day/agents.md**, where an agent sent to datum.day
finds it; its source is [web/static/agents.md](../web/static/agents.md). In short: the agent installs the CLI
([install.md](install.md)), runs `datum --db their-name sync chain invite` and sends its person the link; once they
join on datum.day, both copies hold the same record, and the agent sets up fields and logs for them.
[sync.md](sync.md) covers chains, admins and conflicts.

## Work on datum

Agents changing datum's code start at [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md).
