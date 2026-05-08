# datum-sync-agent

Background daemon that wraps [`datum-sync-client`](../datum-sync-client/)
to push-on-write and pull on a periodic interval, so sync happens
without the user having a CLI open.

The library target (this crate's `lib.rs`) holds **pure** scheduling /
debounce / backoff types so they can be unit-tested without an async
runtime or filesystem. The binary (`src/main.rs`) wires those types to
a real tokio runtime, a `notify`-based file watcher, and a pid-file.

Keeping the math pure is the whole point — the scheduler decides
"push now, pull in 30s" from inputs alone, and the binary is just a
loop that asks it what to do next.

## Shape

| Module       | Purpose                                              |
|--------------|------------------------------------------------------|
| [`debounce`] | Quiet-window write coalescing.                       |
| [`backoff`]  | Exponential retry schedule (5s → 15s → 60s → 5min).  |
| [`scheduler`]| Per-DB next-action policy driving push / pull.       |
| [`liveness`] | PID-file + alive-check.                              |
| [`log`]      | tracing-subscriber bootstrap for the daemon.         |
| [`notify`]   | Filesystem-watcher adapter.                          |
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

```ini
# ~/.config/systemd/user/datum-sync-agent.service
[Unit]
Description=daydumb sync agent
After=network-online.target

[Service]
ExecStart=%h/.cargo/bin/datum-sync-agent
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Enable with `systemctl --user enable --now datum-sync-agent`. The
`install` module (when wired up) generates a unit file for you;
until then, the snippet above is the canonical form.

On macOS the equivalent is a `launchd` plist; the Windows path is
not yet planned.

## Where to look next

- [`datum-sync-client`](../datum-sync-client/) — the library this
  daemon drives.
- [`datum-commands`](../datum-commands/) (`commands::sync::status`) —
  reads [`status`] to render `datum sync status` output.
