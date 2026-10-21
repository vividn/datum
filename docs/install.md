# Installing datum

There are no published binaries yet. From a checkout:

```bash,no_run
cargo install --path crates/datum-cli --locked
```

Or run the equivalent helper, which is also the one-command upgrade:

```bash,no_run
scripts/install.sh
```

This produces a `datum` binary on your `PATH`, and `scripts/install.sh`
also installs tab completion for your shell (`datum completions install`,
into the directory your shell loads completions from). Skip that with
`scripts/install.sh --no-completions`; undo it with `datum completions
uninstall`. The installed script only asks the `datum` that installed it
for its answers, so an upgrade never needs a reinstall.

To upgrade, `git pull && scripts/install.sh`. A `--path` install always
builds and installs, replacing the `datum` cargo installed before even
at the same version, and `datum --version` shows the commit it was built
from — `3.0.<height>-beta (<sha7>)` — to confirm it. `--locked` builds
exactly the dependency versions in `Cargo.lock`, the ones CI tested.

If cargo says ``binary `datum` already exists in destination``, that
file is not one cargo installed from `datum-cli` (it was copied there by
hand, say); remove it and install again. And an alias or another `datum`
earlier on your `PATH` — `alias datum=target/release/datum`, for one —
shadows the installed binary: `type datum` says which one runs.
