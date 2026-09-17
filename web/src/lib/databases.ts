// The pool of databases on this device (da-y992).
//
// The worker keeps every database as a file in ONE directory in OPFS
// ($lib/cli.ts, $lib/wasiFs.ts), so the list of databases the app has is
// the list of `<name>.db` in that directory. That is what the logo's menu
// offers ($lib/Brand.svelte).
//
// Read from the PAGE, not through the worker, and by NAME only: a
// directory listing takes no lock, where reading a file's contents would
// contend with the sync access handle a running command is holding
// (da-dvzx). Nothing here opens, creates or writes anything — the CLI
// makes a database, this only says which ones are already there.
//
// This module imports nothing on purpose: the worker takes `OPFS_DIR` from
// it, and a worker must not be made to pull the page's dependencies in
// behind one constant.

/** Where the CLI's files live in OPFS — the preopen's backing directory. */
export const OPFS_DIR = 'datum-cli';

/** `datum.db` → `datum`; anything else (a journal, a stray file) → null. */
function nameOf(file: string): string | null {
	return file.endsWith('.db') ? file.slice(0, -'.db'.length) : null;
}

/**
 * Every database the pool holds, by name, sorted — `_internal` among them,
 * since excluding it is the menu's rule and not the pool's.
 *
 * Empty when this browser has no OPFS, or refuses it: a tab that fell back
 * to a database in memory ($lib/cli.ts) has no pool to list, and a menu
 * with nothing in it is still handed the page's own database by its caller.
 */
export async function listDatabases(): Promise<string[]> {
	try {
		const root = await navigator.storage.getDirectory();
		const dir = await root.getDirectoryHandle(OPFS_DIR);
		const names: string[] = [];
		for await (const file of (dir as unknown as { keys(): AsyncIterable<string> }).keys()) {
			const name = nameOf(file);
			if (name) names.push(name);
		}
		return names.sort();
	} catch {
		return [];
	}
}
