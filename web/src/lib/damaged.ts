// A database SQLite cannot read, and the way out that does not go through
// SQLite (da-0v940).
//
// Every command is a SQLite process, `db purge` among them, so a damaged
// file had no way out inside the app — and the browser's own «clear site
// data» chores (cookies, local storage, IndexedDB) never reach it, because
// it is a file in OPFS. vividn spent an afternoon on exactly that. What is
// here are plain OPFS calls from the page: a copy of the files' raw bytes,
// kept as evidence, and their removal. No CLI process runs for either.
//
// Both take `datum-cli-files`, the Web Lock every tab's worker holds while
// it has the files open ($lib/cli.ts), so neither reads or removes a file
// under a running command. A tab that keeps it past `WAIT_MS` is one the
// reader has to close, and they are told so.

import { writable } from 'svelte/store';
import { OPFS_DIR } from './databases.js';

/** What SQLite says of a file it cannot read: SQLITE_CORRUPT, SQLITE_NOTADB. */
export const DAMAGED = /database disk image is malformed|file is not a database/;

/** The database a command found damaged, by name; null while none has. */
export const damaged = writable<string | null>(null);

/** How long to wait for the files before telling the reader to close the tab holding them. */
const WAIT_MS = 5000;

/** Said when another tab keeps the files. */
export const HELD =
	'Another datum.day tab or window is using this database. Close every other datum.day tab and window, then try again.';

/** Whether `file` is one of `db`'s: the database, its journal, a copy an upgrade kept. */
const ofDb = (db: string, file: string) => file === `${db}.db` || file.startsWith(`${db}.db-`) || file.startsWith(`${db}.db.`);

/** `job` on the OPFS directory, holding the files; rejects with `HELD` if another tab will not let go. */
async function withDir<T>(job: (dir: FileSystemDirectoryHandle) => Promise<T>): Promise<T> {
	const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle(OPFS_DIR, { create: true });
	try {
		return await navigator.locks.request('datum-cli-files', { signal: AbortSignal.timeout(WAIT_MS) }, () => job(dir));
	} catch (e) {
		const name = (e as { name?: string } | null)?.name;
		if (name === 'TimeoutError' || name === 'NoModificationAllowedError') throw new Error(HELD);
		throw e;
	}
}

/** Every file `db` has in OPFS, byte for byte, as one tar. */
export function copyOf(db: string): Promise<Blob> {
	return withDir(async (dir) => {
		const parts: BlobPart[] = [];
		for await (const [name, handle] of (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) {
			if (handle.kind !== 'file' || !ofDb(db, name)) continue;
			// Read now, under the lock: a File is read lazily, and fails once the file changes.
			const bytes = new Uint8Array(await (await (handle as FileSystemFileHandle).getFile()).arrayBuffer());
			parts.push(tarHeader(name, bytes.length), bytes, new Uint8Array(-bytes.length & 511));
		}
		return new Blob([...parts, new Uint8Array(1024)], { type: 'application/x-tar' });
	});
}

/** A ustar header for a plain file `name` of `size` bytes. */
function tarHeader(name: string, size: number): Uint8Array<ArrayBuffer> {
	const head = new Uint8Array(512);
	const put = (at: number, text: string) => head.set(new TextEncoder().encode(text), at);
	const octal = (n: number, width: number) => n.toString(8).padStart(width - 1, '0') + '\0';
	put(0, name);
	put(100, octal(0o644, 8));
	put(124, octal(size, 12));
	put(136, octal(Math.floor(Date.now() / 1000), 12));
	put(148, ' '.repeat(8));
	put(156, '0');
	put(257, 'ustar\x0000');
	put(148, octal(head.reduce((sum, byte) => sum + byte, 0), 7));
	return head;
}

/** Remove `db`'s file and its journal, so the next command makes it afresh, empty. */
export function reset(db: string): Promise<void> {
	return withDir(async (dir) => {
		for (const name of [`${db}.db`, `${db}.db-journal`]) {
			await dir.removeEntry(name).catch((e: { name?: string }) => {
				if (e?.name !== 'NotFoundError') throw e;
			});
		}
	});
}
