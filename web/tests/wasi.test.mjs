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
import { ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP for the tests in the reader's default zone (da-6b3dj, sharedApp
// in support/app.mjs): `solo` empties the database between them. The
// Berlin ones open their own — a zone, and a clock of their own.
const shared = sharedApp();
// `main` as a draft of ONE CLI widget: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

const plain = (text) => text.replace(/\x1b\[[0-9;]*m/g, '');

describe('the wasi CLI', () => {
	test('logs, reads back through a reload, and passes check', async () => {
		await onSeam(async (app) => {
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

	// vividn, 2026-10-08: «add a DEBUG db feature that generates conflicts so
	// that I can easily test it on the web» (da-4dvk7). The two lines, typed
	// into the bar.
	test('`debug conflicts 3` in the bar, and `conflicts` lists four', async () => {
		await onSeam(async (app) => {
			const typed = async (line) => {
				await app.submit(line);
				return (await app.page.evaluate(() => window.datum.bar.last())).result;
			};
			const made = await typed('debug conflicts 3');
			assert.equal(made.code, 0, made.stderr);
			const listed = await typed('conflicts');
			assert.equal(listed.code, 0, listed.stderr);
			// One header line an entry with conflicts (da-05rkv).
			assert.equal(plain(listed.stdout).match(/^\S+ {2}debug:/gm)?.length, 4, listed.stdout);
			// The listing, typed, opens the conflicts place too (da-6cutb), some
			// time after `submit` returns: waited for, so the next test's `home`
			// finds the modal up and closes it. One that landed between `home`'s
			// count of the modals and its wait stayed open for the 30s (da-f68s9).
			await app.page.locator('dialog.sheet[open][data-place="conflicts"]').waitFor();
		});
	});

	test('an unknown command is the CLI’s own error', async () => {
		await onSeam(async (app) => {
			const result = await app.run(['nosuch']);
			assert.equal(result.code, 1);
			assert.match(result.stderr, /^Error: .*'nosuch'/, 'the CLI\'s error, naming the word');
		});
	});

	// Help's examples start with `datum`, and the bar is the CLI after it
	// (da-lszdo): the example pasted says to leave it off, and writes nothing.
	test('`datum occur coffee` pasted from Help says to leave off `datum`', async () => {
		await onSeam(async (app) => {
			await app.submit('datum occur coffee');
			const { result } = await app.page.evaluate(() => window.datum.bar.last());
			assert.equal(result.code, 1);
			assert.ok(result.stderr.includes('`occur coffee`'), `it gives the line without \`datum\`: ${result.stderr}`);
			assert.deepEqual(await app.entries(), []);
		});
	});

	// @bjorn3/browser_wasi_shim sizes argv in UTF-16 units and writes UTF-8,
	// so the argument after a non-ASCII one arrived mangled (#lib/cli.ts).
	test('a non-ASCII argument arrives intact, and so does the next one', async () => {
		await onSeam(async (app) => {
			const set = await app.run(['setup', '日本語', '--verb', 'occur']);
			assert.equal(set.code, 0, set.stderr);
			const logged = await app.run(['occur', 'café', 'note=très bien', '-t', '0700']);
			assert.equal(logged.code, 0, logged.stderr);
			const [stored] = await app.entries();
			assert.equal(stored.field, 'café');
			assert.deepEqual(stored.data, { note: 'très bien' });
		});
	});

	test('a command reads its standard input', async () => {
		await onSeam(async (app) => {
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
	// gear's «Backups» since da-9ow8m, and as a download where the browser
	// has no SAVE picker (which this one is made not to have).
	test('«Save a backup file» hands over `db backup -` as a file `db restore` reads', async () => {
		await onSeam(async (app) => {
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			// An own property over the prototype's, taken away again below.
			await app.page.evaluate(() => (window.showSaveFilePicker = undefined));
			await app.page.locator('.root-bar button.gear').click();
			await app.page.getByRole('menuitem', { name: 'Backups', exact: true }).click();
			const [download] = await Promise.all([
				app.page.waitForEvent('download'),
				app.page.locator('section.pane[data-title="backups"] button[data-backup="save"]').click()
			]);
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
				const view = await app.run(['nowview', '-o', '-', '--history', '15m']);
				assert.equal(view.code, 0, view.stderr);
				// Written under the now section (da-dv45l).
				const labels = [...view.stdout.matchAll(/class="current-time-text"[^>]*>([^<]*)</g)].map((m) => m[1]);
				assert.deepEqual(labels, ['12:26']);
			},
			{ clock: Date.parse('2026-09-11T10:26:00Z'), timezone: 'Europe/Berlin' }
		);
	});

	test("the dayview's today is the zone's date", async () => {
		await withApp(
			async (app) => {
				const view = await app.run(['dayview', '--days', '1']);
				assert.equal(view.code, 0, view.stderr);
				assert.match(view.stdout, /data-from="2026-09-11T22:00:00\.000Z"/);
			},
			{ clock: Date.parse('2026-09-12T00:30:00+02:00'), timezone: 'Europe/Berlin' }
		);
	});

	// And a kit view's: SQLite's own local time is UTC here, wasi-libc having
	// no zone data, so chores read a chore done at 00:30 as not done today
	// until `local_today()` (da-5fonh).
	test("a kit view's today is the zone's date: chores done after midnight are done", async () => {
		await withApp(
			async (app) => {
				const chores = await readFile(new URL('../../kits/chores.toml', import.meta.url), 'utf8');
				const installed = await app.run(['kit', 'install', '--yes', '-'], chores);
				assert.equal(installed.code, 0, installed.stderr);
				assert.equal((await app.run(['occur', 'vacuum', 'next_date=+7'])).code, 0);
				const listed = await app.run(['chorelist', '--show', 'json']);
				assert.equal(listed.code, 0, listed.stderr);
				assert.deepEqual(
					JSON.parse(listed.stdout).map(({ field, due, status }) => [field, due, status]),
					[['vacuum', '2026-09-19', 'done']]
				);
			},
			{ clock: Date.parse('2026-09-12T00:30:00+02:00'), timezone: 'Europe/Berlin' }
		);
	});

	// `dur` is ISO text; `dur_seconds(dur)` is what a view adds up (da-es4em).
	test('a kit view totals time spent with dur_seconds', async () => {
		await onSeam(async (app) => {
			const kit = `[kit]\nname = "practice"\n\n[[view]]\nname = "practised"\nsql = "SELECT SUM(dur_seconds(dur)) AS seconds, COUNT(*) AS n FROM entries WHERE field = 'practice'"\n`;
			const installed = await app.run(['kit', 'install', '--yes', '-'], kit);
			assert.equal(installed.code, 0, installed.stderr);
			// A time each: the app's clock is held, and one instant is one entry.
			for (const argv of [['dur=30m', '-t', '0900'], ['dur=1:30', '-t', '1100'], ['-t', '1200']]) {
				const logged = await app.run(['occur', 'practice', ...argv]);
				assert.equal(logged.code, 0, logged.stderr);
			}
			const view = await app.run(['view', 'practised', '--format', 'json']);
			assert.equal(view.code, 0, view.stderr);
			assert.deepEqual(JSON.parse(view.stdout), [{ seconds: 7200, n: 3 }]);
		});
	});
});
