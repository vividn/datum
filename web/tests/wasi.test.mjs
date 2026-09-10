// The real datum CLI behind the seam (da-1vat, da-g8o8).
//
// The CLI built for wasm32-wasip1, one process per command, its database in
// OPFS. What these hold it to is what makes it the CLI rather than a
// likeness: its own output and exit codes, its own errors, and a database
// that survives a reload and passes `datum check`. The tabs and the storage
// it shares between them are storage.test.mjs's.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const plain = (text) => text.replace(/\x1b\[[0-9;]*m/g, '');

describe('the wasi CLI', () => {
	test('logs, reads back through a reload, and passes check', async () => {
		await withApp(async (app) => {
			const logged = await app.run(['occur', 'coffee', '-t', '0915']);
			assert.equal(logged.code, 0, logged.stderr);
			assert.match(plain(logged.stdout), /^CREATE [0-9a-z]{5} coffee:\d{4}-\d\d-\d\dT09:15:00\.000Z$/m);

			await app.reload();
			const tail = await app.run(['tail']);
			assert.equal(tail.code, 0, tail.stderr);
			assert.match(plain(tail.stdout), /09:15:00 +coffee/);

			const check = await app.run(['check']);
			assert.equal(check.code, 0, check.stdout + check.stderr);
		});
	});

	test('an unknown command is the CLI’s own error', async () => {
		await withApp(async (app) => {
			const result = await app.run(['nosuch']);
			assert.equal(result.code, 1);
			assert.equal(result.stderr, "Error: Invalid argument: unknown subcommand: 'nosuch'");
		});
	});

	// @bjorn3/browser_wasi_shim sizes argv in UTF-16 units and writes UTF-8,
	// so the argument after a non-ASCII one arrived mangled ($lib/cli.ts).
	test('a non-ASCII argument arrives intact, and so does the next one', async () => {
		await withApp(async (app) => {
			const set = await app.run(['spec', 'set', '日本語', '--default-verb', 'occur']);
			assert.equal(set.code, 0, set.stderr);
			const logged = await app.run(['occur', 'café', 'note=très bien', '-t', '0700']);
			assert.equal(logged.code, 0, logged.stderr);
			const [stored] = await app.entries();
			assert.equal(stored.field, 'café');
			assert.deepEqual(stored.data, { note: 'très bien' });
		});
	});

	test('a command reads its standard input', async () => {
		await withApp(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			const dump = await app.run(['db', 'backup', '-']);
			assert.equal(dump.code, 0, dump.stderr);
			// Restoring what is already there reads every line and keeps none:
			// content-hash idempotent, so "skipped" is the proof it was read.
			const restored = await app.run(['db', 'restore', '-'], dump.stdout);
			assert.equal(restored.code, 0, restored.stderr);
			assert.match(plain(restored.stdout), /0 inserted, 0 updated, 1 skipped/);
			const empty = await app.run(['db', 'restore', '-']);
			assert.match(plain(empty.stdout), /0 inserted, 0 updated, 0 skipped/);
		});
	});

	// Insurance before sync (vividn, 2026-09-10): the one copy of the data that
	// is not in this browser. One command, handed over as a file.
	test('export hands over `db backup -` as a file `db restore` reads', async () => {
		await withApp(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			const [download] = await Promise.all([
				app.page.waitForEvent('download'),
				app.page.locator('header button.export').click()
			]);
			assert.match(download.suggestedFilename(), /^datum-backup-\d{4}-\d\d-\d\d\.jsonl$/);
			const text = await readFile(await download.path(), 'utf8');
			const [header, ...rows] = text.trim().split('\n');
			assert.equal(JSON.parse(header).type, 'daydumb_backup');
			assert.deepEqual(
				rows.map((row) => JSON.parse(row).field),
				['coffee']
			);
			const restored = await app.run(['db', 'restore', '-'], text);
			assert.match(plain(restored.stdout), /1 skipped/);
		});
	});
});
