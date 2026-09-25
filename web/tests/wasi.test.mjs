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
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

// ONE APP for the tests in the reader's default zone (da-6b3dj, sharedApp
// in support/app.mjs): `solo` empties the database between them. The
// Berlin ones open their own — a zone, and a clock of their own.
const shared = sharedApp();

const plain = (text) => text.replace(/\x1b\[[0-9;]*m/g, '');

describe('the wasi CLI', () => {
	test('logs, reads back through a reload, and passes check', async () => {
		await shared.solo(async (app) => {
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
		await shared.solo(async (app) => {
			const result = await app.run(['nosuch']);
			assert.equal(result.code, 1);
			assert.equal(result.stderr, "Error: Invalid argument: unknown subcommand: 'nosuch'");
		});
	});

	// @bjorn3/browser_wasi_shim sizes argv in UTF-16 units and writes UTF-8,
	// so the argument after a non-ASCII one arrived mangled ($lib/cli.ts).
	test('a non-ASCII argument arrives intact, and so does the next one', async () => {
		await shared.solo(async (app) => {
			const set = await app.run(['spec', 'set', '日本語', '--verb', 'occur']);
			assert.equal(set.code, 0, set.stderr);
			const logged = await app.run(['occur', 'café', 'note=très bien', '-t', '0700']);
			assert.equal(logged.code, 0, logged.stderr);
			const [stored] = await app.entries();
			assert.equal(stored.field, 'café');
			assert.deepEqual(stored.data, { note: 'très bien' });
		});
	});

	test('a command reads its standard input', async () => {
		await shared.solo(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			const dump = await app.run(['db', 'backup', '-']);
			assert.equal(dump.code, 0, dump.stderr);
			// Restoring what is already there reads every line and keeps none:
			// content-hash idempotent, so "skipped" is the proof it was read.
			const restored = await app.run(['db', 'restore', '-'], dump.stdout);
			assert.equal(restored.code, 0, restored.stderr);
			// Two rows: the entry, which declared its own field in its own
			// `following_state` rather than beside a TRACK document
			// (da-gvcen), and the spec that first log wrote (da-lc26c).
			assert.match(plain(restored.stdout), /0 inserted, 0 updated, 2 skipped/);
			const empty = await app.run(['db', 'restore', '-']);
			assert.match(plain(empty.stdout), /0 inserted, 0 updated, 0 skipped/);
		});
	});

	// Insurance before sync (vividn, 2026-09-10): the one copy of the data that
	// is not in this browser. One command, handed over as a file — from the
	// gear's «Backups…» since da-9ow8m, and as a download where the browser
	// has no SAVE picker (which this one is made not to have).
	test('«Save a backup file…» hands over `db backup -` as a file `db restore` reads', async () => {
		await shared.solo(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			// An own property over the prototype's, taken away again below.
			await app.page.evaluate(() => (window.showSaveFilePicker = undefined));
			await app.page.locator('.root-bar button.gear').click();
			await app.page.getByRole('menuitem', { name: 'Backups…', exact: true }).click();
			const [download] = await Promise.all([
				app.page.waitForEvent('download'),
				app.page.locator('dialog.sheet button[data-backup="save"]').click()
			]);
			await app.page.keyboard.press('Escape');
			await app.page.evaluate(() => delete window.showSaveFilePicker);
			assert.match(download.suggestedFilename(), /^datum-\d{4}-\d\d-\d\d\.jsonl$/);
			const text = await readFile(await download.path(), 'utf8');
			const [header, ...rows] = text.trim().split('\n');
			assert.equal(JSON.parse(header).type, 'daydumb_backup');
			// The entry and the spec its first log wrote (da-lc26c).
			assert.deepEqual(
				rows.map((row) => JSON.parse(row).field),
				['coffee', 'spec']
			);
			const restored = await app.run(['db', 'restore', '-'], text);
			assert.match(plain(restored.stdout), /2 skipped/);
		});
	});

	// `track` and `ignore` stamp their entry with the CLI's own clock rather
	// than a parsed time, and under WASI that clock had the `TZ` name but
	// an offset of +0 — an entry `check` then called an ERROR (da-y6ej).
	for (const [when, ms, offset] of [
		['September', Date.UTC(2026, 8, 11, 12), 120],
		['January', Date.UTC(2026, 0, 15, 12), 60]
	]) {
		test(`an entry the CLI stamps itself carries its zone's offset, in ${when}`, async () => {
			await withApp(
				async (app) => {
					assert.equal((await app.run(['track', 'x', '--verb', 'occur'])).code, 0);
					assert.equal((await app.run(['ignore', 'y'])).code, 0);
					const check = await app.run(['check']);
					assert.doesNotMatch(plain(check.stdout), /tz_offset_mismatch|ERROR/, check.stdout);
					// `allEntries`: `track` writes only a declaration (da-c8mh),
					// which is the very row this test is about.
					const stored = (await app.allEntries())
						.filter(({ field }) => field !== 'spec')
						.map(({ field, occur_time: t }) => [field, Date.parse(t.utc), t.offset_minutes, t.timezone])
						.sort();
					assert.deepEqual(
						stored,
						['x', 'y'].map((field) => [field, ms, offset, 'Europe/Berlin'])
					);
				},
				{ clock: ms, timezone: 'Europe/Berlin' }
			);
		});
	}

	// The views read the reader's clock through the same zone: under WASI
	// `chrono::Local` is UTC, which labelled a 12:26 cursor 10:26 and made
	// today yesterday from midnight until the zone's offset (da-y6qj).
	test("the nowview's clock is the zone's", async () => {
		await withApp(
			async (app) => {
				const view = await app.run(['nowview', '-o', '-', '--history', '15m', '-t', '2026-09-11T10:26:00Z']);
				assert.equal(view.code, 0, view.stderr);
				const labels = [...view.stdout.matchAll(/class="tick-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
				assert.ok(labels.includes('12:26') && !labels.includes('10:26'), labels.join(' '));
			},
			{ timezone: 'Europe/Berlin' }
		);
	});

	test("the dayview's today is the zone's date", async () => {
		await withApp(
			async (app) => {
				const view = await app.run(['dayview', '--n-days', '1']);
				assert.equal(view.code, 0, view.stderr);
				assert.match(view.stdout, /data-from="2026-09-11T22:00:00\.000Z"/);
			},
			{ clock: Date.parse('2026-09-12T00:30:00+02:00'), timezone: 'Europe/Berlin' }
		);
	});
});
