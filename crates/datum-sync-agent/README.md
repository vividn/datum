# datum-sync-agent

Background daemon that wraps [`datum-sync-client`](../datum-sync-client/)
to push-on-write and pull on a periodic interval, so sync happens
without the user having a CLI open.

The library target (this crate's `lib.rs`) holds **pure** scheduling /
debounce / backoff types so they can be unit-tested without an async
runtime or filesystem. [`agent`] wires those types to a real tokio
runtime, a `notify`-based file watcher, and a pid-file.

Keeping the math pure is the whole point — the scheduler decides
"push now, pull in 30s" from inputs alone, and the runtime is just a
loop that asks it what to do next.

## Two entry points, one daemon

`datum sync agent` (in the `datum` binary) and the standalone
`datum-sync-agent` binary both call [`agent::run`] with the same
options. **The installed service unit execs `datum sync agent`** — the
standalone binary is a convenience for smoke tests.

That is deliberate. A second installed binary sits outside whatever
upgrades `datum`, so it silently goes stale: it starts, discovers no
targets under an older config shape, logs nothing alarming, and idles
forever while `datum sync` by hand keeps working. That was da-ge3e.
One binary cannot drift from itself.

## Shape

| Module       | Purpose                                              |
|--------------|------------------------------------------------------|
| [`agent`]    | The daemon runtime (workers, watcher, shutdown).     |
| [`debounce`] | Quiet-window write coalescing.                       |
| [`backoff`]  | Exponential retry schedule (5s → 15s → 60s → 5min).  |
| [`scheduler`]| Per-DB next-action policy driving push / pull.       |
| [`liveness`] | PID-file + alive-check.                              |
| [`log`]      | tracing-subscriber bootstrap for the daemon.         |
| [`notifications`] | Desktop notifications (notify-send / osascript). |
| [`status`]   | Daemon-liveness readout for `datum sync status`.     |
| [`targets`]  | Enumerate auto-sync DBs from `datumrc.toml`.          |

## Configuration

The agent reads its target list from the standard `datumrc.toml`
discovered through the same XDG path resolution as the CLI (see
[`datum_commands::config`]). Each target enables/disables auto-sync;
disabled targets stay manual-only.

Per-target tunables (quiet window, pull interval, backoff floor/ceiling)
live alongside the target entry; defaults are conservative.

## systemd unit (Linux)

A minimal user unit:

Run `datum sync install-daemon`; it writes the unit and enables it. The
unit it writes:

```ini
# ~/.config/systemd/user/datum-sync-agent.service
[Unit]
Description=datum sync daemon (push-on-write + periodic pull)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart="/home/you/.local/bin/datum" sync agent
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
```

`ExecStart` is the absolute path of the `datum` that ran
`install-daemon` (override with `DATUM_BIN`), quoted so a path
containing a space stays one argument. Point it at the `datum` your
upgrade path maintains, not at a build directory.

## When it looks like the daemon "isn't syncing"

The daemon exits 0 when no DB is configured for auto-sync, which
systemd reports as `inactive (dead)` — indistinguishable at a glance
from a healthy idle daemon. Read the journal, not the status line:

```sh
journalctl --user -u datum-sync-agent -n 200
```

- one `db=… path=… server=…` line per target at startup → discovery
  worked; look at the push/pull lines below it.
- `no auto-sync DBs configured` → discovery found nothing. The warning
  names the config file it read; a user unit runs with a minimal
  environment, so that may not be the file your shell sees.

On macOS the equivalent is a `launchd` plist; the Windows path is
not yet planned.

## Where to look next

- [`datum-sync-client`](../datum-sync-client/) — the library this
  daemon drives.
- [`datum-commands`](../datum-commands/) (`commands::sync::status`) —
  reads [`status`] to render `datum sync status` output.
