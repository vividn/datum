# Action links

datum.day reads a few links as things to do: go to a tab, open a place, log a line. Written in a
text widget's markdown, each is a button labelled with the link's words, and a tap on it does it; its
title says what. One more draws a field's collect button itself, where the link is. A space in a line
is written `+`.

| link | what it does | example |
|---|---|---|
| `act:tab/<name>` | goes to that tab, opening it if it is not open | `[Morning](act:tab/morning)` |
| `act:place/<name>` | opens a place, a widget on its own (a modal on a desktop, a tab on a phone) | `[Open Sync](act:place/sync)` |
| `act:run?line=<line>` | runs one logging line as the bar would, its echo and its undo as any command's | `[Coffee](act:run?line=moment+coffee)` |
| `act:field/<name>` | draws that field's collect button: a tap logs it, a hold opens it in the collect widget | `[coffee](act:field/coffee)` |

The vocabulary is small on purpose. A run takes the logging commands alone: `moment`, `on`, `off`,
`change`, `record`, `remark` and `allday` (and their old words), on every side of a `,`. Their flags are
the ones that say what the entry is: its time (`-t`, `-d`, `-y`, `-q`, `-z`, `-D`), its keys and
comment (`-k`, `-c`, `-R`, `-S`, `-C`, `-I`), the state it leaves (`-l`, `-L`), `-m`, `-T`,
`--if-changed`, `--dry-run` and `--show`. Any other line, `delete`, `db`, `sync`, `config`, `kit` or an
alias, which may be any of them, and any other flag, which writes more than the entry (`--alias`,
`--id`, `--conflict`, `-u`, `--setup`) or names another database (`--db`), is drawn as its words and
why it is refused, naming the command or the flag, and runs nothing. Any other kind of `act:` link is its
words, as a link to a scheme datum.day does not follow is.

Each is a link of datum.day too, `https://datum.day/act/<kind>/…`, the same words after `act:`:
`https://datum.day/act/run?line=moment+coffee`. Opened from anywhere else, a message or another site,
it never runs on its own: the page says what the link would do, and does it only once you tap. In a
text widget your tap on the button is that yes.

The guides in datum.day's Help read the same links, so a guide's «Open Sync» is `act:place/sync`.
