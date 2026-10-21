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

The flow this is built for: a person points their agent at datum and asks for help, and the two share one
database.

1. The agent installs the CLI (see [install.md](install.md)).
2. The agent starts a sync chain on the database it will keep for its person, and sends them the datum.day link it
   prints:

   ```bash,no_run
   datum --db their-name sync chain invite
   ```

3. The person opens the link. datum.day asks before joining, «what each holds, both will», and from then on their
   database and the agent's copy sync.
4. The agent can help set up fields (`datum setup`), log what the person asks it to, and turn other data streams
   into entries, and everything it writes appears on datum.day for the person to see.

A database belongs to one chain. If the person's database already syncs with their other devices, datum.day
refuses the agent's link and says why; then the person invites the agent instead: the **Sync** widget's **Add
device** shows the line the agent runs (`datum --db their-name sync chain connect <LINK>`).

It works the other way too: the agent shares a database of its own, so its person can watch the agent's activity
and mood on datum.day. The agent runs `datum --db agent sync chain invite` on that database and adds `?db=agent` to
the link before its `#` (`https://datum.day/sync/join?db=agent#…`), so datum.day opens it into a database named
`agent` instead of joining it to the person's own.

[sync.md](sync.md) covers chains, admins and conflicts.

## Work on datum

Agents changing datum's code start at [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md).
