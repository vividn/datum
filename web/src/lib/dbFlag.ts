/**
 * The `--db` flag `argv` carries: `--db X` or `--db=X`, the first of them,
 * stopping at `--` — the CLI's own pre-scan to the token
 * (`extract_global_db_flag`, `crates/datum-commands/src/config.rs`).
 * Undefined when there is none; `{}` for a `--db` with nothing after it,
 * which names no database but is still a flag the caller wrote.
 *
 * `-D` is NOT one, and reading it as one was a bug, not a cosmetic
 * disagreement (vremya on #22): the CLI's `-D` is `--full-day`
 * (`crates/datum-parse/src/alias/scope.rs`), so `occur coffee -D` on a
 * `?db=other` page looked like a command that had already chosen its
 * database, got no `DATUM_DB=other`, and logged the day to `datum`.
 *
 * The one scanner: #lib/run.ts reads it for the page's database, #lib/cli.ts
 * (the worker) for which file to open. `tests/fixtures/db-flag.json` holds it
 * and the CLI's to the same cases.
 */
export function dbFlag(argv: readonly string[]): { db?: string } | undefined {
	for (let i = 0; i < argv.length; i += 1) {
		const word = argv[i];
		if (word === '--') return undefined;
		if (word === '--db') return i + 1 < argv.length ? { db: argv[i + 1] } : {};
		if (word.startsWith('--db=')) return { db: word.slice(5) };
	}
	return undefined;
}
