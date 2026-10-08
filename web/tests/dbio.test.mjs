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
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';

// The probe is the file's one top-level await, and it comes before any
// hook or test is registered (da-h9vv0): node:test runs the root `after`
// (stopApp) as soon as the tests registered SO FAR have finished — it does
// not wait for tests registered after a top-level await — so with
// `fileArgv` above the await, a probe slower than `fileArgv` closed the
// server and the browser before the browser legs ran. It takes a TEST
// registered before the await: hooks alone are safe (viewport.test.mjs,
// measured with a 3 s probe — `after` still ran last).
//
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

before(startApp);
after(stopApp);

// ONE APP for the two chromium tests below the browser legs (da-6b3dj,
// sharedApp in support/app.mjs): `solo` empties the database, so each
// still counts from zero. The legs keep an app each: their browser is a
// persistent profile of its own, and the phone's has an init script.
const desk = sharedApp({ viewport: { width: 1280, height: 1000 } });
// `main` as a draft of ONE REPL pane for the tests that never read the day
// frame, since drawing it costs what a boot did (da-m1m9a); the one that
// waits on the tail pane takes `main` as it ships.
const onSeam = (fn) => desk.solo(fn, ONE_PANE);

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

/** Recorded argvs, a restore's pin (da-2zqad) read as `FP`: the fingerprint of the live entries it was counted on. */
const pinned = (argvs) => argvs.map((argv) => argv.map((a, i) => (argv[i - 1] === '--if-unchanged' ? 'FP' : a)));

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
					// A second apart: the clock is held, and one instant
					// holds one coffee.
					for (let i = 0; i < 3; i++) {
						await app.submit('occur coffee');
						await app.clock.forward(1000);
					}
					await tailShows(page, 4);

					// The backup is a download; the transcript has the header, not the rows.
					const [download] = await Promise.all([
						page.waitForEvent('download'),
						app.submit('db backup')
					]);
					const saved = await download.path();
					// The four coffees and the spec the first one wrote (da-lc26c).
					const lines = (await readFile(saved, 'utf8')).split('\n').filter(Boolean);
					assert.equal(lines.length, 6);
					const header = JSON.parse(lines[0]);
					assert.equal(header.type, 'daydumb_backup');
					assert.equal(header.entry_count, 5);
					assert.match(download.suggestedFilename(), /^datum-\d{4}-\d{2}-\d{2}\.jsonl$/);
					const said = (await app.outputs()).at(-1);
					assert.match(said, /daydumb_backup/);
					assert.match(said, /5 entries/);
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
					assert.match((await app.stdouts()).at(-1), /5 inserted/);
					await tailShows(page, 4);

					// vividn's legacy backup, twice: the second is all skips.
					// The import declares every field it brings with a spec
					// alone, so the 120 rows are all it adds (da-r6ofr).
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
					assert.match((await app.outputs()).at(-1), /^rest\s+-\s+2\s+1\s+1\s+0\s+1\s+0$/m);
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
	await onSeam(
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
		}
	);
});

test('an option keeps its value, and only the source is the chosen file (da-1vz2)', async () => {
	await onSeam(
		async (app) => {
			const { page } = app;
			// What the CLI is asked — argv and stdin — through the seam's own hook.
			await page.evaluate(() => {
				window.__asked = [];
				const previous = window.datum.setRunner((argv, stdin, ...rest) => {
					if (argv[0] === 'db' || argv[0] === 'kit') window.__asked.push({ argv, stdin });
					return previous(argv, stdin, ...rest);
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
			// The 120 rows and nothing beside them: each imported field is
			// declared by its spec alone, which `count` does not count (da-r6ofr).
			assert.equal(await count(app), 120);

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
		}
	);
});

// A replace restore tombstones what the backup lacks, and the CLI asks
// first only on a TTY, which the browser has not got (da-jrbwc): the page
// asks, with the counts of the CLI's own dry run on the same bytes, and
// only when something would be deleted or the dry run warns (da-fnjku).
test('a replacing restore confirms its deletions first; one that deletes nothing and warns of nothing, or --merge, never asks', async () => {
	await desk.solo(
		async (app) => {
			const { page } = app;
			// Each `db` argv the CLI is asked, through the seam's own hook —
			// put back at the end, as the app is shared.
			await page.evaluate(() => {
				window.__restores = [];
				const previous = window.datum.setRunner((argv, stdin, ...rest) => {
					if (argv[0] === 'db') window.__restores.push(argv);
					return previous(argv, stdin, ...rest);
				});
				window.__unhook = () => window.datum.setRunner(previous);
			});
			const asked = () => page.evaluate(() => window.__restores.splice(0)).then(pinned);
			const coffee = async (n) => {
				for (let i = 0; i < n; i++) {
					await app.submit('occur coffee');
					await app.clock.forward(1000);
				}
			};
			await app.declare('coffee');
			await coffee(2);
			const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
			const saved = await download.path();
			await coffee(2);
			assert.equal(await count(app), 5);
			await asked();

			// Two newer coffees the backup lacks: the dialog names them, and cancel keeps them.
			const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
			await chooser.setFiles(saved);
			const dialog = page.locator('dialog.confirm');
			await dialog.waitFor();
			assert.match(
				await dialog.locator('p').innerText(),
				/^Restore replaces datum: 2 entries will be deleted, 0 added, 0 changed\. Undo reverts the whole restore\.$/
			);
			await answer(page, 'cancel');
			assert.match((await app.outputs()).at(-1), /db restore: aborted/);
			assert.deepEqual(await asked(), [['db', 'restore', '-', '--dry-run', '--json']]);
			assert.equal(await count(app), 5);

			// Restore replaces: the newer two are tombstoned, the backup's live.
			const [again] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
			await again.setFiles(saved);
			await dialog.waitFor();
			await answer(page, 'restore');
			assert.deepEqual(await asked(), [
				['db', 'restore', '-', '--dry-run', '--json'],
				['db', 'restore', '-', '--yes', '--if-unchanged', 'FP']
			]);
			assert.equal(await count(app), 3);
			await tailShows(page, 3);

			// Nothing left to delete: it runs as asked, with no dialog.
			await choose(page, 'db restore', saved);
			assert.equal(await dialog.count(), 0);
			assert.deepEqual(await asked(), [
				['db', 'restore', '-', '--dry-run', '--json'],
				['db', 'restore', '-', '--if-unchanged', 'FP']
			]);

			// `--merge` deletes nothing and never asks, not even a dry run.
			await coffee(1);
			await choose(page, 'db restore --merge', saved);
			assert.equal(await dialog.count(), 0);
			assert.deepEqual(await asked(), [['db', 'restore', '--merge', '-']]);
			assert.equal(await count(app), 4);
			await page.evaluate(() => window.__unhook());
			assert.deepEqual(app.errors, []);
		}
	);
});

// da-f5pg2: the page gave `retime` an empty stdin, which took the CLI's
// yes-default — a 16-minute-old coffee retimed without a word. Now the
// CLI's own question is a confirm, and only a yes retimes.
test('a stale _LAST question is asked in a confirm, in the CLI’s words, and only a yes retimes', async () => {
	await onSeam(async (app) => {
		const { page } = app;
		await app.declare('coffee');
		await app.submit('occur coffee');
		const { hid } = JSON.parse((await app.run(['get', '_LAST', '--json'])).stdout).docs[0];
		const id = async () => JSON.parse((await app.run(['get', hid, '--json'])).stdout).docs[0].id;
		const before = await id();
		await app.clock.forward(16 * 60 * 1000);

		await type(page, 'retime -t +5');
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		assert.match(
			await dialog.locator('p').innerText(),
			/^⚠ default `_LAST` is older than 15 minutes — showing target:\n.*\n.*coffee.*\nRetime this entry\? \[Y\/n\]$/
		);
		await answer(page, 'cancel');
		assert.match((await app.stdouts()).at(-1), /Aborted\.$/);
		assert.equal(await id(), before, 'no leaves the entry where it was');

		// `get` points `_LAST` at what it read, so it is fresh again.
		await app.clock.forward(16 * 60 * 1000);
		await type(page, 'retime -t +5');
		await dialog.waitFor();
		await answer(page, 'retime');
		assert.notEqual(await id(), before, 'yes retimes it');
		assert.deepEqual(app.errors, []);
	});
});

// da-84h6a: the answer re-ran the line, which resolved `_LAST` afresh — so
// with another tab's write in between, a cancel retimed the new entry the
// confirm never showed. The answer is now for the entry shown, and a moved
// `_LAST` is asked about again.
test('an answer to the stale _LAST question acts on the entry it showed, or asks again', async () => {
	await onSeam(async (app) => {
		const { page } = app;
		await app.declare('coffee');
		await app.declare('tea');
		await app.submit('occur coffee');
		// A doc's `id` is its slug, which a retime rewrites; its hid stays.
		const doc = async (tab, ref) => JSON.parse((await tab.run(['get', ref, '--json'])).stdout).docs[0];
		const coffee = await doc(app, '_LAST');
		await app.clock.forward(16 * 60 * 1000);

		await type(page, 'retime -t +5');
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		assert.match(await dialog.locator('p').innerText(), /coffee/);
		const other = await app.newTab();
		await other.submit('occur tea');
		const tea = await doc(other, '_LAST');

		await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
		await page.waitForFunction(() => /tea/.test(document.querySelector('dialog.confirm[open] p')?.textContent ?? ''));
		assert.match(
			await dialog.locator('p').innerText(),
			/^⚠ default `_LAST` has moved since — showing target:\n.*\n.*tea.*\nRetime this entry\? \[Y\/n\]$/
		);
		assert.equal((await doc(other, tea.hid)).id, tea.id, 'the cancel moved nothing');

		// The tea is fresh: the answered run says nothing again (da-e7739),
		// least of all that `_LAST` is older than 15 minutes.
		const said = page.locator('.repl pre[data-stream="stderr"]');
		const saidBefore = await said.count();
		await answer(page, 'retime');
		assert.equal(await said.count(), saidBefore, (await said.allInnerTexts()).join('\n'));
		assert.notEqual((await doc(other, tea.hid)).id, tea.id, 'yes retimes the entry it showed');
		assert.equal((await doc(other, coffee.hid)).id, coffee.id, 'and only that one');
		await other.page.close();
		assert.deepEqual(app.errors, []);
	});
});

// da-2zqad: the confirm counted one database and its yes acted on a later
// one, so an entry another tab logged while the question was open was
// deleted uncounted. The yes is now pinned to what was counted, and a
// database that changed since is counted and asked about again.
test('a restore’s yes deletes what its confirm counted; another tab’s write since is asked about again', async () => {
	await onSeam(async (app) => {
		const { page } = app;
		await app.declare('coffee');
		await app.declare('tea');
		await app.declare('juice');
		await app.submit('occur coffee');
		const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
		const saved = await download.path();
		await app.submit('occur tea');

		const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
		await chooser.setFiles(saved);
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		assert.match(await dialog.locator('p').innerText(), /^Restore replaces datum: 1 entries will be deleted, /);
		const other = await app.newTab();
		await other.submit('occur juice');

		await dialog.getByRole('button', { name: 'restore', exact: true }).click();
		await page.waitForFunction(() => /changed since/.test(document.querySelector('dialog.confirm[open] p')?.textContent ?? ''));
		assert.match(
			await dialog.locator('p').innerText(),
			/^⚠ The database changed since it was counted\. Restore replaces datum: 2 entries will be deleted, /
		);
		assert.equal(await count(other), 6, 'the refused yes deleted nothing');

		await answer(page, 'restore');
		assert.match((await app.stdouts()).at(-1), /, 2 deleted$/);
		assert.equal(await count(other), 4);
		await other.page.close();
		assert.deepEqual(app.errors, []);
	});
});

// A refused run is always asked about (PR #455), even when its recount
// deletes nothing and warns of nothing — else a database that keeps
// changing under the run would be retried unattended, with no end.
test('a restore refused by a write since its count asks, even when the recount deletes nothing', async () => {
	await onSeam(async (app) => {
		const { page } = app;
		await app.declare('coffee');
		await app.submit('occur coffee');
		const { hid } = JSON.parse((await app.run(['get', '_LAST', '--json'])).stdout).docs[0];
		const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
		const saved = await download.path();
		// Coffee is deleted inside the restore: after its count, before its first run.
		await page.evaluate((hid) => {
			const previous = window.datum.setRunner(async (argv, stdin, ...rest) => {
				if (argv.includes('--if-unchanged') && !window.__wrote) {
					window.__wrote = true;
					await previous(['delete', hid]);
				}
				return previous(argv, stdin, ...rest);
			});
			window.__unhook = () => window.datum.setRunner(previous);
		}, hid);

		const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
		await chooser.setFiles(saved);
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		assert.equal(
			await dialog.locator('p').innerText(),
			'⚠ The database changed since it was counted. Restore adds 1 entries and changes 0.'
		);
		await answer(page, 'cancel');
		assert.match((await app.outputs()).at(-1), /db restore: aborted/);
		assert.equal(await count(app), 1, 'coffee stays deleted');
		await page.evaluate(() => window.__unhook());
		assert.deepEqual(app.errors, []);
	});
});

// da-i4rqd: the confirm was built from the dry run's counts alone, so a
// backup whose header disagrees with its lines was confirmed as if whole.
test('a replacing restore names the dry run’s warnings in its confirm', async () => {
	await onSeam(
		async (app) => {
			const { page } = app;
			await app.declare('coffee');
			await app.submit('occur coffee');
			const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
			// The header declares one entry more than the file holds.
			const [head, ...rest] = (await readFile(await download.path(), 'utf8')).split('\n');
			const header = JSON.parse(head);
			header.entry_count += 1;
			const cut = join(await mkdtemp(join(tmpdir(), 'datum-')), 'cut.jsonl');
			await writeFile(cut, [JSON.stringify(header), ...rest].join('\n'));
			await app.clock.forward(1000);
			await app.submit('occur coffee');

			const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
			await chooser.setFiles(cut);
			const dialog = page.locator('dialog.confirm');
			await dialog.waitFor();
			const asked = await dialog.locator('p').innerText();
			const [, declared, restored] =
				asked.match(/ ⚠ Entry-count mismatch: header declared (\d+), restored (\d+)\.$/) ?? assert.fail(asked);
			assert.equal(Number(declared), Number(restored) + 1, asked);
			await answer(page, 'cancel');
			assert.deepEqual(app.errors, []);
		}
	);
});

// da-fnjku: the confirm came only when something would be deleted, so a
// truncated backup restored into an empty db warned in its dry run and
// then ran with no word before it. A warning asks too; a clean restore
// that deletes nothing still runs at once.
test('a restore whose dry run warns asks first, even when nothing would be deleted', async () => {
	await onSeam(
		async (app) => {
			const { page } = app;
			await page.evaluate(() => {
				window.__restores = [];
				const previous = window.datum.setRunner((argv, stdin, ...rest) => {
					if (argv[0] === 'db') window.__restores.push(argv);
					return previous(argv, stdin, ...rest);
				});
				window.__unhook = () => window.datum.setRunner(previous);
			});
			const asked = () => page.evaluate(() => window.__restores.splice(0)).then(pinned);
			await app.declare('coffee');
			await app.submit('occur coffee');
			const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
			const clean = await download.path();
			// The header declares one entry more than the file holds.
			const [head, ...rest] = (await readFile(clean, 'utf8')).split('\n');
			const header = JSON.parse(head);
			header.entry_count += 1;
			const cut = join(await mkdtemp(join(tmpdir(), 'datum-')), 'cut.jsonl');
			await writeFile(cut, [JSON.stringify(header), ...rest].join('\n'));
			await app.empty();
			assert.equal(await count(app), 0);
			await asked();

			// Truncated into an empty db: nothing to delete, but it asks, and says what it does.
			const dialog = page.locator('dialog.confirm');
			const restoreCut = async () => {
				const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore')]);
				await chooser.setFiles(cut);
				await dialog.waitFor();
				return dialog.locator('p').innerText();
			};
			const text = await restoreCut();
			assert.match(text, /^Restore adds \d+ entries and changes 0\. ⚠ Entry-count mismatch: header declared \d+, restored \d+\.$/);
			await answer(page, 'cancel');
			assert.equal((await app.outputs()).at(-1), 'db restore: aborted (nothing changed; --merge deletes nothing)');
			assert.deepEqual(await asked(), [['db', 'restore', '-', '--dry-run', '--json']]);
			assert.equal(await count(app), 0);

			await restoreCut();
			await answer(page, 'restore');
			assert.deepEqual(await asked(), [
				['db', 'restore', '-', '--dry-run', '--json'],
				['db', 'restore', '-', '--yes', '--if-unchanged', 'FP']
			]);
			assert.equal(await count(app), 2);

			// A clean backup into an empty db runs at once.
			await app.empty();
			await asked();
			await choose(page, 'db restore', clean);
			assert.equal(await dialog.count(), 0);
			assert.deepEqual(await asked(), [
				['db', 'restore', '-', '--dry-run', '--json'],
				['db', 'restore', '-', '--if-unchanged', 'FP']
			]);
			assert.equal(await count(app), 2);
			await page.evaluate(() => window.__unhook());
			assert.deepEqual(app.errors, []);
		}
	);
});

// da-h3yb3: a hard restore's name was typed to its dry run's counts, but
// its run was not pinned to them, so another tab's write while the dialog
// was open was dropped uncounted. Now it is refused, counted and asked again.
test('a hard restore’s name drops what its confirm counted; another tab’s write since is asked about again', async () => {
	await onSeam(async (app) => {
		const { page } = app;
		await app.declare('coffee');
		await app.declare('juice');
		await app.submit('occur coffee');
		const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
		const saved = await download.path();

		const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore --hard')]);
		await chooser.setFiles(saved);
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		const live = async () => Number((await dialog.locator('p').innerText()).match(/dropped: (\d+) live/)?.[1]);
		const counted = await live();
		const other = await app.newTab();
		await other.submit('occur juice');
		const name = dialog.locator('input');
		await name.fill('datum');
		await name.press('Enter');
		await page.waitForFunction(() => /changed since/.test(document.querySelector('dialog.confirm[open] p')?.textContent ?? ''));
		assert.match(await dialog.locator('p').innerText(), /^⚠ The database changed since it was counted\. This replaces ALL data in datum /);
		assert.equal(await live(), counted + 1, 'the recount counts the juice');
		assert.equal(await count(other), 4, 'the refused name dropped nothing');

		await name.fill('datum');
		await name.press('Enter');
		await page.waitForFunction(() => !document.querySelector('dialog.confirm[open]'));
		await page.waitForFunction(() => /cannot be undone/.test(document.querySelector('.repl')?.textContent ?? ''));
		assert.equal(await count(other), 3);
		await other.page.close();
		assert.deepEqual(app.errors, []);
	});
});

// da-qq7gb: a typed `--hard` went through the replace confirm, which knew
// nothing of it — clean, it asked nothing and the CLI refused on no TTY;
// warned, it said «adds N entries» and its yes dropped everything; and a
// typed `--yes`, which that refusal points at, asked nobody. Now `--hard`
// asks in the CLI's own words, behind the database's name, every time.
test('a hard restore asks with what it drops and the database’s name, even with a typed --yes', async () => {
	await onSeam(
		async (app) => {
			const { page } = app;
			await page.evaluate(() => {
				window.__restores = [];
				const previous = window.datum.setRunner((argv, stdin, ...rest) => {
					if (argv[0] === 'db') window.__restores.push(argv);
					return previous(argv, stdin, ...rest);
				});
				window.__unhook = () => window.datum.setRunner(previous);
			});
			const asked = () => page.evaluate(() => window.__restores.splice(0));
			await app.declare('coffee');
			await app.submit('occur coffee');
			const [download] = await Promise.all([page.waitForEvent('download'), app.submit('db backup')]);
			let saved = await download.path();
			await app.clock.forward(1000);
			await app.submit('occur coffee');
			assert.equal(await count(app), 3);
			await asked();

			const dialog = page.locator('dialog.confirm');
			const restoreHard = async (line) => {
				const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, line)]);
				await chooser.setFiles(saved);
				await dialog.waitFor();
				return dialog.locator('p').innerText();
			};
			const text = await restoreHard('db restore --hard');
			const [, written, live] =
				text.match(
					/^This replaces ALL data in datum with the backup \((\d+) written\); dropped: (\d+) live, \d+ tombstoned or superseded, \d+ in history\. It cannot be undone: the history `undo` reads goes with it\.$/
				) ?? assert.fail(text);
			// The coffee since the backup is the one more it drops.
			assert.equal(Number(live), Number(written) + 1, text);
			// The wrong name drops nothing.
			const name = dialog.locator('input');
			let before = await echoes(page);
			await name.fill('datu');
			await name.press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.outputs()).at(-1), 'db restore --hard: aborted (input did not match datum)');
			assert.deepEqual(await asked(), [['db', 'restore', '--hard', '-', '--dry-run', '--json']]);
			assert.equal(await count(app), 3);

			// A typed `--yes` asks too, and the name runs it.
			await restoreHard('db restore --hard --yes');
			before = await echoes(page);
			await name.fill('datum');
			await name.press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.match((await app.stdouts()).at(-1), /cannot be undone/);
			assert.deepEqual(pinned(await asked()), [
				['db', 'restore', '--hard', '-', '--dry-run', '--json'],
				['db', 'restore', '--hard', '-', '--yes', '--if-unchanged', 'FP']
			]);
			assert.equal(await count(app), 2);

			// An empty file has no fingerprint to pin (kimi-k3 on #458): it runs, unpinned, as the no-op it is.
			saved = join(await mkdtemp(join(tmpdir(), 'datum-')), 'empty.jsonl');
			await writeFile(saved, '');
			await restoreHard('db restore --hard');
			before = await echoes(page);
			await name.fill('datum');
			await name.press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.deepEqual(await asked(), [
				['db', 'restore', '--hard', '-', '--dry-run', '--json'],
				['db', 'restore', '--hard', '-', '--yes']
			]);
			assert.equal(await count(app), 2);
			await page.evaluate(() => window.__unhook());
			assert.deepEqual(app.errors, []);
		}
	);
});
