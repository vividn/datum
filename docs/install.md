# Installing datum

On Linux or macOS (x86_64 or aarch64), with no Rust:

```bash,no_run
curl -fsSL https://datum.day/install.sh | sh
```

It downloads the `datum` built from main's latest commit (the `latest`
release on GitHub, which each push to main replaces), checks it against
the release's `SHA256SUMS` and refuses a mismatch (with `minisign`
installed, it first checks that the release key signed `SHA256SUMS`:
README's «Verify»), and installs it to `~/.local/bin`, or
`$DATUM_INSTALL_DIR`, saying so if that is not on your `PATH`. Then the binary sets itself up with `datum install`, which
takes the script's arguments: `| sh -s -- --no-completions`, say.
`datum --version` names the commit.

## What `datum install` sets up

- Tab completion for your shell (`datum completions install`, into the
  directory your shell loads completions from; `--shell zsh|bash|fish`
  names another shell, `--no-completions` skips it). The installed
  script asks the `datum` that installed it for its answers, so an
  upgrade never needs a reinstall.
- The sync daemon, when a database syncs (`datum sync daemon install`),
  restarted onto this binary; `--daemon` installs it before one does.
- A line naming the backup daemons installed, or how to install one:
  `datum db backup daemon install --every 1h` takes a database's Tower of
  Hanoi rotation every hour on a systemd user timer, into `backups/<db>/` in
  datum's data directory (`--to DIR` another, `DIR/<db>/`; one daemon per destination).
  `--db a,b` installs one for each database, and `--all` one that backs up
  every database datum knows at each run.
- Whether to upgrade daily (`datum upgrade --auto`, below), asked on a
  terminal; `--yes` or `--no` answers it.
- It ends with the line that makes your aliases and the verbs commands
  of their own in your shell, for you to add if you want it.

Running it again is harmless. `datum check` then says when any of it goes
missing, or when the daemon runs another or an older `datum`, and of a
database's backup daemons, each by its destination, when its timer is not
running or its last backup failed.

## Upgrading

```bash,no_run
datum upgrade            # or: datum upgrade --check, which only says
```

downloads the newest release for your machine, checks that
`SHA256SUMS` is signed by the release key built into this datum
(`release/minisign.pub`) and the binary against it (a missing or bad
signature, or a mismatch, changes nothing), puts it in place of the
running binary in one rename, and restarts the sync daemon if one runs.
It upgrades only the binary `datum install` set up, or one cargo built
from a checkout (below).

`datum upgrade --auto` upgrades daily: a systemd user timer runs `datum
upgrade` once a day, a day missed once the machine is on, and `datum
upgrade --no-auto` removes it; `--check` says whether it is on. `datum
install` offers it, yes by default and unasked with no terminal: `--yes`
or `--no` answers for a script. A binary cargo built gets no timer (`datum
upgrade` moves it onto the release first), and with no systemd (macOS)
nothing is installed.

datum makes no network call of its own unless you sync, upgrade, or turn
on update checks: with `update_check = true` in `datumrc.toml` (in
`~/.config/datum/`), `datum check` says when a newer release is out.

## From a checkout

For contributors, or another platform, build it with Rust's `cargo`:

```bash,no_run
scripts/install.sh
```

It runs `cargo install --path crates/datum-cli --locked`, then `datum
install` (its arguments go there). To upgrade, `git pull &&
scripts/install.sh`. `datum upgrade` moves a binary cargo built onto the
release's builds instead, verified with the release key that build
carries, and says so; `scripts/install.sh` puts a checkout build back. A
`--path` install always builds and installs, replacing the `datum` cargo
installed before even at the same version, and `datum --version` shows
the commit it was built from — `3.0.<height>-beta (<sha7>)` — to confirm
it. `--locked` builds exactly the dependency versions in `Cargo.lock`, the
ones CI tested.

If cargo says ``binary `datum` already exists in destination``, that
file is not one cargo installed from `datum-cli` (it was copied there by
hand, say); remove it and install again. And an alias or another `datum`
earlier on your `PATH` — `alias datum=target/release/datum`, for one —
shadows the installed binary: `type datum` says which one runs.

## Uninstall

```bash,no_run
datum uninstall
```

undoes the install: the sync daemon, every backup daemon, tab completion, the alias loop in
`~/.zshrc` or `~/.bashrc`, then the binary (by `cargo uninstall` when
cargo put it there). It keeps your record, the databases and the config,
and says where they are and how to remove them by hand. A step that
fails says so and the rest still run.
