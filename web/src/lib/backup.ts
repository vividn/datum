// `datum db backup -`: every live entry as the CLI dumps it — a header line,
// then one entry per line, the JSONL `db restore` reads back.
//
// Its reader here is `window.datum.entries`, which reads what got stored
// the way the CLI itself would report it. A backup as a FILE is
// #lib/dbio.ts's (`download`, `saveBackup`), and backup plans are
// #lib/backups.ts's.

import { backupArgv } from './calls.js';
import { cliError, run } from './run.js';

/** The backup document, as the CLI wrote it — of `db` when named. Rejects on a failed command. */
export async function backup(db?: string): Promise<string> {
	const result = await run(db ? ['--db', db, ...backupArgv()] : backupArgv());
	if (result.code !== 0) throw cliError(result.stderr, `db backup exited ${result.code}`);
	return result.stdout;
}

/** One stored entry, in the CLI's own serialization (`datum_core::entry::Entry`). */
export interface StoredEntry {
	hid: string;
	slug: string;
	field: string;
	verb: string;
	state?: unknown;
	dur?: string | null;
	data?: unknown;
	occur_time?: { utc: string } | null;
	create_time: string;
	[key: string]: unknown;
}

/** The backup's entries, newest first by occur time, at most `limit` of them — of `db` when named. */
export async function storedEntries(limit = 50, db?: string): Promise<StoredEntry[]> {
	const [, ...lines] = (await backup(db)).split('\n').filter(Boolean);
	// A timeless entry has no occur time and sorts last, as SQLite's NULL does.
	const at = (e: StoredEntry) => (e.occur_time ? Date.parse(e.occur_time.utc) : -Infinity);
	return lines
		.map((line) => JSON.parse(line) as StoredEntry)
		.sort((a, b) => at(b) - at(a))
		.slice(0, limit);
}

/** Hand `text` (or a Blob) to the browser as a download called `name`. */
export function saveFile(text: string | Blob, name: string): void {
	const url = URL.createObjectURL(typeof text === 'string' ? new Blob([text], { type: 'application/x-ndjson' }) : text);
	const link = document.createElement('a');
	link.href = url;
	link.download = name;
	link.click();
	setTimeout(() => URL.revokeObjectURL(url), 0);
}
