// Aliases in the REPL.
//
// An alias is a name the user gives a command: `alias set cof 'occur
// coffee strength=strong'`, and from then on `cof` logs a strong coffee.
// The expansion is not this app's code — the browser runs the CLI, which
// expands it (da-5kbj, da-g8o8). So what these tests check is not "does the
// expansion work" (tests/parity.test.mjs holds that against the native
// binary, case by case) but the round trip in the browser: the row is
// written, it is read back on the very next line, and removing it stops
// the name expanding.
//
// Everything runs through `run(argv)`, the one seam, so these are the
// commands a person types and nothing behind them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { fill, manage, ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP for the REPL's tests (da-6b3dj, sharedApp in support/app.mjs):
// an alias is a row, and `solo` empties the database between tests. The
// reload, the navigation and the lone test at 1280x900 keep their own.
const shared = sharedApp();
// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

/** `run`, asserting the command succeeded, and handing back its stdout. */
async function ok(run, argv) {
	const result = await run(argv);
	assert.equal(result.code, 0, `datum ${argv.join(' ')}: ${result.stderr}`);
	return result.stdout;
}

describe('aliases in the REPL', () => {
	test('an alias set in one line fires on the next, with no reload', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal(
				await ok(run, ['alias', 'set', 'cof', 'occur coffee strength=strong']),
				"✓ Alias 'cof' saved."
			);

			await ok(run, ['cof', '-t', '0915']);
			const [stored] = await entries();
			assert.equal(stored.field, 'coffee');
			assert.deepEqual(stored.data, { strength: 'strong' });
			// The provenance the CLI stamps: what was typed, and what it
			// became. Sync carries these columns, so the shape is not the
			// browser's to choose.
			assert.equal(stored.raw_input, 'cof -t 0915');
			assert.deepEqual(stored.alias_chain, [
				{ name: 'cof', expansion: 'occur coffee strength=strong' }
			]);
		});
	});

	// vividn's case, verbatim from datum v2: an alias carrying `--id
	// %comment%`, so two transactions on one day are two rows keyed by what
	// each was for, rather than one row per minute.
	test('an `--id %comment%` alias slugs the row on its own data', async () => {
		await onSeam(async ({ run, entries }) => {
			await ok(run, ['alias', 'set', 'tx', 'occur tx --id %comment%']);
			await ok(run, ['tx', 'amount=40', 'comment=electricity', '-d', '2026-09-01']);
			await ok(run, ['tx', 'amount=12', 'comment=coffee', '-d', '2026-09-01']);

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => e.slug).sort(),
				['tx:coffee', 'tx:electricity'],
				'each row is filed under its comment, not under its minute'
			);
			assert.deepEqual(
				stored.map((e) => e.id_structure),
				['%comment%', '%comment%']
			);
		});
	});

	test('a field spec makes its own names commands', async () => {
		await onSeam(async ({ run, entries }) => {
			await ok(run, ['spec', 'mood', '--verb', 'switch', '--state', 'happy']);

			// The bare field name, and the state name, both without a verb.
			await ok(run, ['mood', '-t', '0910']);
			await ok(run, ['happy', '-t', '0920']);

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => [e.field, e.state]),
				[
					['mood', 'happy'],
					['mood', true]
				],
				'newest first: `happy` switched mood, the bare name started it'
			);
		});
	});

	test('`alias list` prints the CLI’s lines, and `alias remove` unmakes one', async () => {
		await onSeam(async ({ run }) => {
			assert.equal(await ok(run, ['alias', 'list']), 'No aliases defined.');

			await ok(run, ['alias', 'set', 'cof', 'occur coffee']);
			await ok(run, ['alias', 'set', 'tea', 'occur tea']);
			assert.equal(
				await ok(run, ['alias', 'list']),
				'  cof → datum occur coffee\n  tea → datum occur tea'
			);
			assert.equal(await ok(run, ['alias', 'show', 'cof']), 'cof → datum occur coffee');

			assert.equal(await ok(run, ['alias', 'remove', 'cof']), "✓ Alias 'cof' removed");
			assert.equal(await ok(run, ['alias', 'list']), '  tea → datum occur tea');

			// And the name stops expanding immediately — no reload.
			const gone = await run(['cof']);
			assert.notEqual(gone.code, 0, 'a removed alias must not still fire');

			const missing = await run(['alias', 'remove', 'cof']);
			assert.notEqual(missing.code, 0);
			assert.match(missing.stderr, /no user alias 'cof'/);
		});
	});

	test('an alias survives a reload, because it is a row and not a variable', async () => {
		await withApp(async (app) => { // reloads
			await ok(app.run, ['alias', 'set', 'cof', 'occur coffee']);
			await app.reload();
			await ok(app.run, ['cof', '-t', '0915']);
			const [stored] = await app.entries();
			assert.equal(stored.field, 'coffee');
		});
	});
});

// The aliases widget (da-x6yj): every alias `alias list --json` lists, the
// user's first, then the derived ones under their field in its colour.
describe('the aliases widget', () => {
	const DESKTOP = { viewport: { width: 1280, height: 900 } };
	const rows = (page) => page.locator('section.pane[data-title="aliases"] li').allInnerTexts();

	test('picked into a pane, it lists every alias and grows a row on a typed alias set', async () => {
		await withApp(async ({ page, run }) => { // alone at 1280x900
			// Hex, not a CSS colour name: the fields widget's `color` is now
			// resolved server-side by `crates/datum-commands/src/render/colors.rs`,
			// which only understands `#rrggbb` (or a legacy ANSI-256 number) —
			// a name like `orange` would fall through to `md5(field)` (da-yiyj).
			await ok(run, ['spec', 'coffee', '--verb', 'occur', '--color', '#ffa500']);
			await ok(run, ['alias', 'set', 'cof', 'occur coffee cups=1']);
			// Split is layout mode's (da-vvcxc).
			await manage(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'aliases');
			await page.locator('section.pane[data-title="aliases"] li').nth(1).waitFor();
			assert.deepEqual(await rows(page), ['cof → occur coffee cups=1', 'coffee → occur coffee']);

			const input = page.locator('.prompt-bar input');
			await input.fill('alias set tea "occur tea"');
			await input.press('Enter');
			await page.locator('section.pane[data-title="aliases"] li', { hasText: 'tea → occur tea' }).waitFor();
			assert.deepEqual(await rows(page), ['cof → occur coffee cups=1', 'tea → occur tea', 'coffee → occur coffee']);
			// In the field's colour: the fields widget's, read on the beat a typed line runs on.
			await page.waitForFunction(() =>
				[...document.querySelectorAll('section.pane[data-title="aliases"] li')].some(
					(li) => li.textContent === 'coffee → occur coffee' && getComputedStyle(li).color === 'rgb(255, 165, 0)'
				)
			);
		}, DESKTOP);
	});

	test('datum.day/aliases opens it alone', async () => {
		await withApp(async (app) => { // navigates
			await app.goto('/aliases');
			await app.page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'aliases' }).waitFor();
			assert.deepEqual(
				await app.page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title)),
				['aliases']
			);
		}, DESKTOP);
	});
});
