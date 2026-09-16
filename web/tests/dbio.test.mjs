// The REPL's file commands (da-podv, $lib/dbio.ts): `db backup` is a
// download, `db restore` / `db v2import` read the file the browser's dialog
// hands them, `db purge` asks for the database's name first, and `kit
// install` shows what the chosen kit brings and asks before it installs
// (da-axrk) — and no file ever lands in the transcript.
//
// Everything goes through the REPL's own input, as a reader types it:
// the dialog a file command opens is the reader's, and only a keypress
// carries the activation that opens it. (The harness's `run` is the quiet
// seam — da-6918 — so it would neither open a dialog nor refresh a pane.)
// Counts are the CLI's own `count`, never sizes. A dialog that silently never opens is the failure to look for,
// so every chooser is an awaited `filechooser` event — on chromium and on
// webkit, whose rules for user activation are the stricter.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, webkit } from 'playwright';
import { fileArgv } from '../src/lib/fileargv.ts';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

// The rewrite keeps every option and its value, in order, and makes only
// the input source `-` (da-1vz2): read off the CLI's option table, not
// guessed from the dash.
describe('fileArgv', () => {
	const cases = [
		['db v2import --skip-log skips.txt', ['db', 'v2import', '--skip-log', 'skips.txt', '-'], ['skips.txt']],
		['db v2import --from backup.json --dry-run', ['db', 'v2import', '--from', '-', '--dry-run'], ['backup.json']],
		['db v2import --from=backup.json --dry-run', ['db', 'v2import', '--from', '-', '--dry-run'], ['backup.json']],
		['db v2import backup.json --skip-log=s.txt', ['db', 'v2import', '-', '--skip-log', 's.txt'], ['backup.json', 's.txt']],
		['db v2import --dry-run', ['db', 'v2import', '--dry-run', '-'], []],
		['db v2import -', ['db', 'v2import', '-'], []],
		['db restore --force', ['db', 'restore', '--force', '-'], []],
		['db restore b.jsonl --dry-run --json', ['db', 'restore', '-', '--dry-run', '--json'], ['b.jsonl']],
		['db backup --overwrite out.jsonl', ['db', 'backup', '--overwrite', '-'], []],
		['db backup', ['db', 'backup', '-'], []],
		['kit install', ['kit', 'install', '-'], []],
		['kit install rest.toml --force -y', ['kit', 'install', '-', '--force', '-y'], ['rest.toml']],
		['kit install - --dry-run', ['kit', 'install', '-', '--dry-run'], []]
	];
	for (const [line, argv, paths] of cases) {
		test(line, () => {
			const got = fileArgv(line.split(' '));
			assert.deepEqual(got.argv, argv);
			assert.deepEqual(got.paths, paths);
		});
	}

	test('backup: the PATH names the download', () => {
		assert.equal(fileArgv(['db', 'backup', '--overwrite', 'out.jsonl']).name, 'out.jsonl');
		assert.equal(fileArgv(['db', 'backup']).name, undefined);
	});

	test('anything else runs as typed', () => {
		for (const line of ['db purge', 'db setup', 'tail', 'backup x', 'kit list', 'kit export rest']) {
			assert.equal(fileArgv(line.split(' ')), null);
		}
	});
});

/** vividn's legacy format: 120 docs over 2025-01-01..03. */
const LEGACY = fileURLToPath(
	new URL('../../crates/datum-cli/tests/fixtures/datum_backup_3days.json', import.meta.url)
);

/** Two fields, an alias, a view and a command; no index. */
const KIT = fileURLToPath(new URL('../../crates/datum-cli/tests/fixtures/rest_kit.toml', import.meta.url));

/** Type `kit install` into the bottom bar, choose the kit, and return its confirm. */
async function kitConfirm(page, line = 'kit install') {
	const [chooser] = await Promise.all([
		page.waitForEvent('filechooser'),
		type(page, line)
	]);
	await chooser.setFiles(KIT);
	const dialog = page.locator('dialog.confirm');
	await dialog.waitFor();
	return dialog;
}

/** Press `button` in the confirm and wait for the command to land. */
async function answer(page, button) {
	const before = await echoes(page);
	await page.locator('dialog.confirm').getByRole('button', { name: button, exact: true }).click();
	await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
}

const echoes = (page) => page.locator('.repl .echo').count();

/** Type `line` into the bottom bar and press Enter, without waiting for it to land. */
async function type(page, line) {
	const input = page.locator('.prompt-bar input');
	await input.fill(line);
	await input.press('Enter');
}

/** Type a file command, answer its chooser with `files`, and wait for it to land. */
async function choose(page, line, files) {
	const before = await echoes(page);
	const [chooser] = await Promise.all([
		page.waitForEvent('filechooser'),
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

/**
 * Record, for every file chooser the page clicks open, whether it was in
 * the same task as an Enter keydown. Playwright's `filechooser` fires for a
 * click long after the keypress too — headless enforces no activation — so
 * this is what holds the chooser to the keypress a real browser demands.
 */
const clickedInKeydown = (context) =>
	context.addInitScript(() => {
		let inEnter = false;
		addEventListener(
			'keydown',
			(e) => {
				if (e.key !== 'Enter') return;
				inEnter = true;
				setTimeout(() => (inEnter = false));
			},
			true
		);
		window.__chooserClicks = [];
		const click = HTMLInputElement.prototype.click;
		HTMLInputElement.prototype.click = function () {
			if (this.type === 'file') window.__chooserClicks.push(inEnter);
			return click.call(this);
		};
	});

/** Wait for the tail pane to show `n` rows. */
const tailShows = (page, n) =>
	page.waitForFunction(
		(n) => document.querySelectorAll('section.pane[data-title="tail"] tbody tr').length === n,
		n
	);

// CI installs chromium's headless shell only (`playwright install
// --with-deps --only-shell chromium`, .github/workflows/ci.yml): a leg
// whose browser cannot be launched here is SKIPPED with the reason on
// record, not failed — the webkit leg runs where webkit is (this machine,
// the merge gate) and CI stays green without pretending it ran it.
// 2026-09-11: two webkit legs failed CI on "Executable doesn't exist" and
// main was red and deployed for forty minutes. The probe is a LAUNCH, not
// `executablePath()`: with the headless shell installed the full browser's
// path is absent while the browser launches fine.
async function launchable(engine) {
	try {
		const browser = await engine.launch();
		await browser.close();
		return true;
	} catch {
		return false;
	}
}
const available = new Map(
	await Promise.all([chromium, webkit].map(async (engine) => [engine, await launchable(engine)]))
);
const missing = (engine) =>
	available.get(engine) ? false : `${engine.name()} cannot be launched here (playwright install ${engine.name()})`;

// A file whose every leg is skipped would pass having examined nothing;
// at least one browser must launch, or the file fails and says so.
assert.ok(
	[...available.values()].some(Boolean),
	'no browser can be launched here — nothing examined (playwright install chromium)'
);

for (const engine of [chromium, webkit]) {
	describe(engine.name(), { skip: missing(engine) }, () => {
		test('backup downloads, purge asks for the name, restore and v2import read the chosen file', async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					// The field exists first, so the three writes are three
					// rows: a field's first log writes the TRACK document
					// that declares it too (da-c8mh).
					await app.declare('coffee');
					for (let i = 0; i < 3; i++) await app.submit('occur coffee');
					await tailShows(page, 4);

					// The backup is a download; the transcript has the header, not the rows.
					const [download] = await Promise.all([
						page.waitForEvent('download'),
						app.submit('db backup')
					]);
					const saved = await download.path();
					const lines = (await readFile(saved, 'utf8')).split('\n').filter(Boolean);
					assert.equal(lines.length, 5);
					const header = JSON.parse(lines[0]);
					assert.equal(header.type, 'daydumb_backup');
					assert.equal(header.entry_count, 4);
					assert.match(download.suggestedFilename(), /^datum-\d{4}-\d{2}-\d{2}\.jsonl$/);
					const said = (await app.outputs()).at(-1);
					assert.match(said, /daydumb_backup/);
					assert.match(said, /4 entries/);
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
					assert.equal(await count(app), 4);

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

					// The downloaded backup, chosen in the dialog, brings them back.
					await choose(page, 'db restore', saved);
					assert.match((await app.stdouts()).at(-1), /4 inserted/);
					await tailShows(page, 4);

					// vividn's legacy backup, twice: the second is all skips.
					// The import declares every field it brings (da-c8mh), so
					// its own TRACK documents ride along with the 120 rows.
					await choose(page, 'db v2import', LEGACY);
					assert.match((await app.stdouts()).at(-1), /120 imported/);
					const imported = (await count(app)) - 4;
					await app.submit('dayview --start-date 2025-01-01 --end-date 2025-01-03');
					const drawn = await page.locator('.repl svg.svg').last().evaluate((svg) => svg.outerHTML);
					// The imported field draws as a mark: its name is the mark's
					// `data-title` since the page stopped keeping `<title>` (da-8jb8).
					assert.match(drawn, /data-title="music /);
					await choose(page, 'db v2import', LEGACY);
					assert.match((await app.stdouts()).at(-1), /120 skipped/);
					assert.equal(await count(app), 4 + imported);

					// A cancelled chooser runs nothing and says so once.
					await choose(page, 'db restore', []);
					assert.match((await app.outputs()).at(-1), /no file chosen/);
					assert.equal(await count(app), 4 + imported);
					assert.deepEqual(app.errors, []);
				},
				{ engine, viewport: { width: 1280, height: 1000 } }
			);
		});

		test('kit install shows what the chosen kit brings and installs it only on yes', async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					const said = await (await kitConfirm(page)).locator('p').innerText();
					assert.match(said, /^Would install kit 'rest': 2 field\(s\), 1 alias\(es\), 1 view\(s\), 1 command\(s\), 0 layout\(s\), 0 index\/indices$/m);
					assert.match(said, /install only kits you trust/);
					await answer(page, 'cancel');
					assert.equal((await app.outputs()).at(-1), 'kit not installed — nothing ran');
					await app.submit('kit list');
					assert.equal((await app.outputs()).at(-1), 'No kits installed.');
					assert.equal(await page.locator('.bar [data-field="nap"]').count(), 0);

					await kitConfirm(page);
					await answer(page, 'install');
					assert.match((await app.outputs()).at(-1), /^Installed kit 'rest': 2 field\(s\)/);
					await app.submit('kit list');
					assert.match((await app.outputs()).at(-1), /^rest\s+2\s+1\s+1\s+0\s+1\s+0$/m);
					// The fields widget follows without a reload.
					for (const field of ['nap', 'sleep']) {
						await page.locator(`.bar [data-field="${field}"]`).waitFor();
					}
					assert.deepEqual(app.errors, []);
				},
				{ engine, viewport: { width: 1280, height: 1000 } }
			);
		});

		// The bottom bar is the page's one prompt (da-rfs9), so it is asked
		// twice: the second import finds the first's rows.
		test('on a phone the chooser opens from the bottom bar, every time', async () => {
			await withApp(
				async ({ page }) => {
					for (const said of [/120 imported/, /120 skipped/]) {
						const [chooser] = await Promise.all([
							page.waitForEvent('filechooser'),
							type(page, 'db v2import')
						]);
						await chooser.setFiles(LEGACY);
						await page.waitForFunction(
							(said) => [...document.querySelectorAll('.repl pre')].some((p) => new RegExp(said).test(p.textContent)),
							said.source
						);
					}
					await kitConfirm(page);
					await answer(page, 'install');
					assert.match((await page.locator('.repl pre').allInnerTexts()).at(-1), /^Installed kit 'rest'/);
					assert.deepEqual(await page.evaluate(() => window.__chooserClicks), [true, true, true]);
				},
				{ engine, viewport: { width: 390, height: 844 }, prepare: clickedInKeydown }
			);
		});
	});
}

// A ~50 MB import on a phone ran in silence (da-h9vj). The CLI counts on
// stderr, and the worker streams it out while the process still holds it.
test('an import shows its progress while it runs, and the result after', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const utc = (i) => new Date(Date.UTC(2025, 0, 1) + i * 60000).toISOString();
			const docs = Array.from({ length: 5000 }, (_, i) => ({
				_id: `coffee:${utc(i)}`,
				data: { field: 'coffee', occurTime: { utc: utc(i), o: 0 } },
				meta: { createTime: { utc: utc(i), o: 0 } }
			}));
			const file = join(await mkdtemp(join(tmpdir(), 'datum-progress-')), 'big.json');
			await writeFile(file, JSON.stringify({ backupTime: '2025-02-01T00:00:00Z', docs }));

			// Every reading the REPL shows, and whether the command had landed by then.
			await page.evaluate(() => {
				window.__readings = [];
				const echoes = () => document.querySelectorAll('.repl .echo').length;
				const before = echoes();
				new MutationObserver(() => {
					const shown = document.querySelector('.repl .progress')?.textContent;
					if (shown && shown !== window.__readings.at(-1)?.shown) {
						window.__readings.push({ shown, landed: echoes() > before });
					}
				}).observe(document.body, { subtree: true, childList: true, characterData: true });
			});
			await choose(page, 'db v2import', file);

			const readings = await page.evaluate(() => window.__readings);
			const running = readings.filter((r) => !r.landed).map((r) => r.shown);
			assert.ok(new Set(running).size >= 2, JSON.stringify(readings));
			for (const shown of running) assert.match(shown, /^v2import: (reading|inserting) \d+\/5000$/);
			assert.ok((await app.outputs()).some((o) => /5000 imported/.test(o)));
			assert.equal(await page.locator('.repl .progress').count(), 0);
			assert.deepEqual(app.errors, []);
		},
		{ viewport: { width: 1280, height: 1000 } }
	);
});

test('an option keeps its value, and only the source is the chosen file (da-1vz2)', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// What the CLI is asked — argv and stdin — through the seam's own hook.
			await page.evaluate(() => {
				window.__asked = [];
				const previous = window.datum.setRunner((argv, stdin) => {
					if (argv[0] === 'db' || argv[0] === 'kit') window.__asked.push({ argv, stdin });
					return previous(argv, stdin);
				});
			});
			const asked = () => page.evaluate(() => window.__asked.at(-1));
			const legacy = await readFile(LEGACY, 'utf8');

			await choose(page, 'db v2import --dry-run', LEGACY);
			assert.match((await app.stdouts()).at(-1), /Migration summary: 120 would import/);
			assert.equal(await count(app), 0);

			await choose(page, 'db v2import --from backup.json --dry-run', LEGACY);
			assert.deepEqual(await asked(), { argv: ['db', 'v2import', '--from', '-', '--dry-run'], stdin: legacy });
			assert.match((await app.stdouts()).at(-1), /Migration summary: 120 would import/);
			// The page's own note is the last line of stderr, after the
			// progress the CLI wrote there as it read (da-h9vj).
			const said = (await app.outputs()).at(-1).split('\n');
			assert.equal(said.at(-1), 'not a file in the browser: backup.json');
			assert.equal(await count(app), 0);

			// `--skip-log` keeps its path, so `-` is still the backup.
			await choose(page, 'db v2import --skip-log skips.txt', LEGACY);
			assert.deepEqual(await asked(), { argv: ['db', 'v2import', '--skip-log', 'skips.txt', '-'], stdin: legacy });
			assert.match((await app.outputs()).at(-1), /not a file in the browser: skips\.txt$/);
			// The 120 rows, plus the TRACK document each imported field is
			// declared with (da-c8mh).
			assert.ok((await count(app)) > 120);

			const [download] = await Promise.all([
				page.waitForEvent('download'),
				app.submit('db backup --overwrite out.jsonl')
			]);
			assert.equal(download.suggestedFilename(), 'out.jsonl');
			assert.deepEqual((await asked()).argv, ['db', 'backup', '--overwrite', '-']);

			// `--yes` typed never reaches the CLI before the confirm is answered:
			// the kit is only looked at, and a no runs nothing more.
			await kitConfirm(page, 'kit install --yes --force');
			assert.deepEqual(await asked(), {
				argv: ['kit', 'install', '--force', '-', '--dry-run'],
				stdin: await readFile(KIT, 'utf8')
			});
			await answer(page, 'cancel');
			assert.deepEqual((await asked()).argv, ['kit', 'install', '--force', '-', '--dry-run']);
			await kitConfirm(page, 'kit install --yes --force');
			await answer(page, 'install');
			assert.deepEqual((await asked()).argv, ['kit', 'install', '--force', '-', '--yes']);
			assert.deepEqual(app.errors, []);
		},
		{ viewport: { width: 1280, height: 1000 } }
	);
});
