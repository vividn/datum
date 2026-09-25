// `datum db backup -`: every live entry as the CLI dumps it — a header line,
// then one entry per line, the JSONL `db restore` reads back.
//
// Its reader here is `window.datum.entries`, which reads what got stored
// the way the CLI itself would report it. A backup as a FILE is
// $lib/dbio.ts's (`download`, `saveBackup`), and backup plans are
// $lib/backups.ts's.

import { run } from './run.js';

/** The backup document, as the CLI wrote it. Rejects on a failed command. */
export async function backup(): Promise<string> {
	const result = await run(['db', 'backup', '-']);
	if (result.code !== 0) throw new Error(result.stderr || `db backup exited ${result.code}`);
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

/** The backup's entries, newest first by occur time, at most `limit` of them. */
export async function storedEntries(limit = 50): Promise<StoredEntry[]> {
	const [, ...lines] = (await backup()).split('\n').filter(Boolean);
	// A timeless entry has no occur time and sorts last, as SQLite's NULL does.
	const at = (e: StoredEntry) => (e.occur_time ? Date.parse(e.occur_time.utc) : -Infinity);
	return lines
		.map((line) => JSON.parse(line) as StoredEntry)
		.sort((a, b) => at(b) - at(a))
		.slice(0, limit);
}

/** Hand `text` to the browser as a download called `name`. */
export function saveFile(text: string, name: string): void {
	const url = URL.createObjectURL(new Blob([text], { type: 'application/x-ndjson' }));
	const link = document.createElement('a');
	link.href = url;
	link.download = name;
	link.click();
	setTimeout(() => URL.revokeObjectURL(url), 0);
}
