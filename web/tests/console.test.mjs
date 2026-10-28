// The browser console's two doors (#lib/console.ts, da-6918).
//
// `window.datum.run` is the one a hand reaches for, so it does what a hand
// at the prompt does: the command goes through `engine.press` — the mounted
// CLI widget echoes it, the bottom bar receipts it, and the epoch it bumps is the
// beat every pane re-reads on. A silent `run` is what cost a false bug
// report on 2026-09-11: the tail was driven from the console, nothing
// refreshed, and the defect was in the door rather than in the pane.
//
// `runQuiet` is the same seam with none of that, for the caller that wants
// a write and no refresh — the harness's own `run`, and the latency script.
// Both are asserted here, and the loud one twice: with a CLI widget on screen and
// with none, which are the two halves of `press`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE desktop app for the file (da-6b3dj, sharedApp in support/app.mjs):
// `solo` empties the database between tests, the scrollback is read from
// where the test found it, and the page with no CLI widget is `main` opened from
// a draft of its own.
const shared = sharedApp(DESKTOP);

/** A page with a tail pane and no CLI widget at all: `press`'s other half, as `main`'s draft. */
const NO_CLI = JSON.stringify(along('column', widgetPane('tail', 'tail'), widgetPane('nowview', 'nowview')));

const receipt = (page) => page.locator('.prompt-bar .group:last-child .receipt');

/**
 * The bar's receipt line, or '' when no command has run.
 *
 * The result area draws a SECTION per document since da-xfa4, so the
 * receipt is that section's first line and there is none before the first
 * command — where the old bar kept an empty span.
 */
const receiptText = async (page) =>
	(await receipt(page).count()) ? receipt(page).innerText() : '';

/** `window.datum[door](argv)` in the page — the console, as a hand uses it. */
const console_ = (page, door, argv) =>
	page.evaluate(([d, a]) => window.datum[d](a), [door, argv]);

/** Wait for the tail pane to show `field`. */
const tailShows = (page, field) =>
	page.waitForFunction(
		(f) => document.querySelector('section.pane[data-title="tail"] .body')?.innerText.includes(f),
		field
	);

/** A mark the page keeps and a reload would lose. */
const markNoReload = (page) => page.evaluate(() => (window.__stillTheSamePage = true));
const samePage = (page) => page.evaluate(() => window.__stillTheSamePage === true);

describe('window.datum.run — the loud door', () => {
	test('through the mounted CLI widget: the pane shows the row, the bar receipts the hid, the scrollback echoes it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// The field exists first, so the bar receipts one document: a
			// field's first log draws its TRACK declaration too (da-c8mh).
			await app.declare('coffee');
			await markNoReload(page);
			const scrolled = (await app.echoes()).length;

			const result = await console_(page, 'run', ['occur', 'coffee']);
			assert.equal(result.code, 0, result.stderr);

			// The pane, with no reload — which is the whole claim.
			await tailShows(page, 'coffee');
			assert.equal(await samePage(page), true, 'the page reloaded');

			// The receipt is the CLI's own `CREATE <hid>`, and the hid is the
			// row's: the same assertion shell.test.mjs makes of a typed line.
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .group:last-child .receipt').textContent.startsWith('CREATE ')
			);
			const [word, hid] = (await receipt(page).innerText()).split(' ');
			const { rows } = await app.tailRows();
			assert.equal(word, 'CREATE');
			// The declaration `declare` wrote, and the entry under it.
			assert.equal(rows.length, 2);
			assert.ok(
				hid.length >= 4 && rows.at(-1).hid.startsWith(hid),
				`the receipt's ${hid} is not the row's ${rows.at(-1).hid}`
			);

			// And it taught: the command is in the scrollback as typed.
			assert.deepEqual((await app.echoes()).slice(scrolled), ['occur coffee']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a line is shell-split and echoed as written', async () => {
		await shared.solo(async (app) => {
			// A quoted word, so the split is visible in the argv: the app
			// does the splitting, as it does for a typed line.
			const line = 'switch mood "quite happy"';
			const scrolled = (await app.echoes()).length;
			const result = await app.page.evaluate((l) => window.datum.runLine(l), line);
			assert.equal(result.code, 0, result.stderr);
			// The words the app split: `result.argv` is what the runner
			// received, plumbing included.
			assert.deepEqual(result.argv.slice(0, 3), ['switch', 'mood', 'quite happy']);
			assert.deepEqual((await app.echoes()).slice(scrolled), [line]);
			await tailShows(app.page, 'quite happy');
		});
	});

	test('with no CLI widget mounted it records and bumps the beat itself', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			assert.equal(await page.locator('section.pane[data-title="cli"]').count(), 0);
			await app.declare('cocoa');
			await markNoReload(page);

			const result = await console_(page, 'run', ['occur', 'cocoa']);
			assert.equal(result.code, 0, result.stderr);

			// No scrollback to echo into — there is no CLI widget — but the beat is
			// bumped here, so the pane still refreshes, and the bar still
			// receipts what ran.
			await tailShows(page, 'cocoa');
			assert.equal(await samePage(page), true, 'the page reloaded');
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .group:last-child .receipt').textContent.startsWith('CREATE ')
			);
			assert.deepEqual(app.errors, []);
		}, NO_CLI);
	});
});

describe('window.datum.runQuiet — the exception', () => {
	test('the entry is stored and nothing on the page moves', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.declare('tea');
			const before = {
				tail: await app.paneText('tail'),
				echoes: await app.echoes(),
				receipt: await receiptText(page)
			};

			const result = await console_(page, 'runQuiet', ['occur', 'tea']);
			assert.equal(result.code, 0, result.stderr);

			// Stored — `tail --json` is the CLI answering about the database,
			// not the screen.
			const { rows } = await app.tailRows();
			// The entry, and the TRACK document declaring its field ahead of
			// the test's own quiet run (da-c8mh).
			assert.deepEqual(
				rows.map((row) => row.field),
				['tea', 'tea']
			);

			// And nothing moved. `idle` first: a refresh, had one been coming,
			// would be a command in flight behind the seam.
			await app.idle();
			assert.equal(await app.paneText('tail'), before.tail, 'the tail pane refreshed');
			assert.deepEqual(await app.echoes(), before.echoes, 'the scrollback grew');
			assert.equal(await receiptText(page), before.receipt, 'the bar receipted it');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('window.datum.tailRows — a read that fails', () => {
	test('rejects with the CLI’s reason, without its «Error: » — what the tail pane shows (da-nepqe)', async () => {
		await shared.solo(async ({ page }) => {
			const message = await page.evaluate(() =>
				window.datum.tailRows({ count: 1, above: 'nosuchrow' }).then(
					() => 'resolved',
					(e) => e.message
				)
			);
			assert.match(message, /nosuchrow/, 'the reason names the row');
			assert.doesNotMatch(message, /^Error: /);
		});
	});
});
