// `datum db backup -`: every live entry as the CLI dumps it — a header line,
// then one entry per line, the JSONL `db restore` reads back.
//
// Two readers: the export button, which hands the document over as a file
// (insurance before sync exists), and `window.datum.entries`, which reads
// what got stored the way the CLI itself would report it.

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

/** Offer the backup as a file download named for today. */
export async function downloadBackup(): Promise<void> {
	const blob = new Blob([(await backup()) + '\n'], { type: 'application/x-ndjson' });
	const url = URL.createObjectURL(blob);
	const link = document.createElement('a');
	link.href = url;
	link.download = `datum-backup-${new Date().toISOString().slice(0, 10)}.jsonl`;
	link.click();
	setTimeout(() => URL.revokeObjectURL(url), 0);
}
