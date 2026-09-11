// The REPL's file commands (da-podv, $lib/dbio.ts): `db backup` is a
// download, `db restore` / `db v2import` read the file the browser's dialog
// hands them, and `db purge` asks for the database's name first — and no
// file ever lands in the transcript.
//
// Everything goes through the REPL's own input, as a reader types it:
// `window.datum.run` does not bump the engine's epoch, so it would not
// show that the panes refresh. Counts are the CLI's own `count`, never
// sizes. A dialog that silently never opens is the failure to look for,
// so every chooser is an awaited `filechooser` event — on chromium and on
// webkit, whose rules for user activation are the stricter.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium, webkit } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** vividn's legacy format: 120 docs over 2025-01-01..03. */
const LEGACY = fileURLToPath(
	new URL('../../crates/datum-cli/tests/fixtures/datum_backup_3days.json', import.meta.url)
);

const echoes = (page) => page.locator('.repl .echo').count();

/** Type `line` into the REPL and press Enter, without waiting for it to land. */
async function type(page, line) {
	const input = page.locator('.repl input');
	await input.fill(line);
	await input.press('Enter');
}

/** Type a file command, answer its chooser with `files`, and wait for it to land. */
async function choose(page, line, files) {
	const before = await echoes(page);
	const [chooser] = await Promise.all([
		page.waitForEvent('filechooser', { timeout: 5000 }),
		type(page, line)
	]);
	await chooser.setFiles(files);
	await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
}

/** `count`, typed into the REPL; the number it printed. */
async function count(app) {
	await app.submit('count');
	return Number((await app.outputs()).at(-1));
}

const tailRows = (page) =>
	page.locator('section.pane[data-title="tail"] tbody tr').count();

/** Wait, at most two seconds, for the tail pane to show `n` rows. */
const tailShows = (page, n) =>
	page.waitForFunction(
		(n) => document.querySelectorAll('section.pane[data-title="tail"] tbody tr').length === n,
		n,
		{ timeout: 2000 }
	);

for (const engine of [chromium, webkit]) {
	describe(engine.name(), () => {
		test('backup downloads, purge asks for the name, restore and v2import read the chosen file', async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					for (let i = 0; i < 3; i++) await app.submit('occur coffee');
					await tailShows(page, 3);

					// The backup is a download; the transcript has the header, not the rows.
					const [download] = await Promise.all([
						page.waitForEvent('download', { timeout: 10000 }),
						app.submit('db backup')
					]);
					const saved = await download.path();
					const lines = (await readFile(saved, 'utf8')).split('\n').filter(Boolean);
					assert.equal(lines.length, 4);
					const header = JSON.parse(lines[0]);
					assert.equal(header.type, 'daydumb_backup');
					assert.equal(header.entry_count, 3);
					assert.match(download.suggestedFilename(), /^datum-\d{4}-\d{2}-\d{2}\.jsonl$/);
					const said = (await app.outputs()).at(-1);
					assert.match(said, /daydumb_backup/);
					assert.match(said, /3 entries/);
					assert.doesNotMatch(said, /coffee/);

					// A layout in `_internal`, which a purge of `datum` must not touch.
					await page.evaluate(() =>
						window.datum.layouts.saveLayout({ ...window.datum.layout.current(), name: 'kept' })
					);

					// The wrong name purges nothing.
					let before = await echoes(page);
					await type(page, 'db purge');
					const name = page.locator('dialog.confirm input');
					await name.fill('datu');
					await name.press('Enter');
					await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
					assert.match((await app.outputs()).at(-1), /aborted/);
					assert.equal(await count(app), 3);

					// The right one does, and every pane follows without a reload.
					before = await echoes(page);
					await type(page, 'db purge');
					await name.fill('datum');
					await name.press('Enter');
					await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
					await tailShows(page, 0);
					assert.equal(await count(app), 0);
					const layouts = await page.evaluate(() => window.datum.layouts.listLayouts());
					assert.ok(layouts.some((l) => l.name === 'kept'), JSON.stringify(layouts));

					// The downloaded backup, chosen in the dialog, brings the three back.
					await choose(page, 'db restore', saved);
					assert.match((await app.outputs()).at(-1), /3 inserted/);
					await tailShows(page, 3);

					// vividn's legacy backup, twice: the second is all skips.
					await choose(page, 'db v2import', LEGACY);
					assert.match((await app.outputs()).at(-1), /120 imported/);
					assert.equal(await count(app), 123);
					await app.submit('dayview --start-date 2025-01-01 --end-date 2025-01-03');
					const drawn = await page.locator('.repl svg.svg').last().evaluate((svg) => svg.outerHTML);
					assert.match(drawn, /<title>music /);
					await choose(page, 'db v2import', LEGACY);
					assert.match((await app.outputs()).at(-1), /120 skipped/);
					assert.equal(await count(app), 123);

					// A cancelled chooser runs nothing and says so once.
					await choose(page, 'db restore', []);
					assert.match((await app.outputs()).at(-1), /no file chosen/);
					assert.equal(await count(app), 123);
					assert.deepEqual(app.errors, []);
				},
				{ engine, viewport: { width: 1280, height: 1000 } }
			);
		});

		test('on a phone the chooser opens from the REPL', async () => {
			await withApp(
				async ({ page }) => {
					const [chooser] = await Promise.all([
						page.waitForEvent('filechooser', { timeout: 5000 }),
						type(page, 'db v2import')
					]);
					await chooser.setFiles(LEGACY);
					await page.waitForFunction(() =>
						[...document.querySelectorAll('.repl pre')].some((p) => /120 imported/.test(p.textContent))
					);
				},
				{ engine, viewport: { width: 390, height: 844 } }
			);
		});
	});
}
