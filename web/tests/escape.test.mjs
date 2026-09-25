// Escape peels the command line, and a grey × clears it (da-al892).
//
// vividn, 2026-09-25: «escape should then clear unlocked pills -> clear locked
// pills too -> clear typed in text, in that progression», and «x shows when
// any text or pills are in the input bar and clears it». Escape is the
// page's LAST meaning for the key: a menu, Help, the log or a field's state
// list takes it first, and then the pills stay (PromptBar.svelte).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { dragBar } from './support/bar.mjs';
import { MOOD, seed } from './support/fields.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const shared = sharedApp({ ...DESKTOP });
const onPage = (fn) => shared.solo(fn, PAGE);

const BOX = '.prompt-bar input';
const CLEAR = '.prompt-bar button[aria-label="clear the input"]';

const UNLOCKED = { id: 'u', kind: 'note', text: '-c one' };
const LOCKED = { id: 'k', kind: 'note', text: '-c two', locked: true };

/** Load the line: `text` typed, `pills` beside it. */
async function load(page, text, pills) {
	await page.evaluate((all) => window.datum.bar.setPills(all), pills);
	await page.locator(BOX).fill(text);
}

/** What the line holds, as the reader sees it. */
const line = async (page) => ({
	text: await page.locator(BOX).inputValue(),
	pills: await page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => p.dataset.pill))
});

/** Escape, and wait for the line to read `want`. */
async function escape(page, want) {
	await page.keyboard.press('Escape');
	await page.waitForFunction(
		([sel, want]) =>
			document.querySelector(sel).value === want.text &&
			[...document.querySelectorAll('.prompt-bar .pill')].map((p) => p.dataset.pill).join() === want.pills.join(),
		[BOX, want]
	);
}

describe('Escape peels the line', () => {
	test('unlocked pills, then locked ones, then the text', async () => {
		await onPage(async ({ page }) => {
			await load(page, 'occur coffee', [UNLOCKED, LOCKED]);
			await page.locator(BOX).focus();
			await escape(page, { text: 'occur coffee', pills: ['k'] });
			await escape(page, { text: 'occur coffee', pills: [] });
			await escape(page, { text: '', pills: [] });
		});
	});

	test('with only text, one Escape clears it; with only locked pills, one Escape clears them', async () => {
		await onPage(async ({ page }) => {
			await load(page, 'occur coffee', []);
			await escape(page, { text: '', pills: [] });
			await load(page, '', [LOCKED]);
			await escape(page, { text: '', pills: [] });
		});
	});
});

describe('Escape that something else takes leaves the line alone', () => {
	/** After `open` and one Escape that `closed` waits for, the line is as it was. */
	const untouched = (open, closed, prepare) =>
		onPage(async (app) => {
			const { page } = app;
			// Before the line is loaded: a command run spends its pills.
			await prepare?.(app);
			// No verb in it: a verb typed filters the fields widget, which
			// would close a state list before the Escape ever came.
			await load(page, 'coffee 2', [UNLOCKED, LOCKED]);
			await open(app);
			await page.keyboard.press('Escape');
			await closed(page);
			// The peel waits for the dispatch to end: give it that turn.
			await page.evaluate(() => new Promise((r) => setTimeout(r, 50)));
			assert.deepEqual(await line(page), { text: 'coffee 2', pills: ['u', 'k'] });
		});

	test('a menu open', () =>
		untouched(
			({ page }) => page.locator('.prompt-bar .brand').click(),
			(page) => page.locator('[role="menu"][aria-label="databases"]').waitFor({ state: 'detached' })
		));

	// The window's Escape in PaneMenu, which the menu's own keys never see.
	test('a menu open, with the focus outside it', () =>
		untouched(
			async ({ page }) => {
				await page.locator('.prompt-bar .brand').click();
				await page.locator('[role="menu"][aria-label="databases"]').waitFor();
				await page.locator(BOX).focus();
			},
			(page) => page.locator('[role="menu"][aria-label="databases"]').waitFor({ state: 'detached' })
		));

	test('Help armed', () => {
		const help = (page) => page.locator('.bar.commands button[data-command="help"]');
		return untouched(
			({ page }) => help(page).click(),
			(page) => page.waitForFunction(() => !document.querySelector('.bar.commands button[aria-pressed="true"]'))
		);
	});

	test('the log open', () =>
		untouched(
			async ({ page }) => {
				await dragBar(page, -300);
				await page.waitForFunction(
					() => parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')) > 0.5
				);
			},
			(page) =>
				page.waitForFunction(
					() => !(parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')) > 0.5)
				)
		));

	test('a field’s state list showing', () =>
		untouched(
			async ({ page }) => {
				await page.locator('.bar [data-field="mood"]').click();
				await page.locator('.bar [data-state="happy"]').waitFor();
			},
			(page) => page.locator('.bar [data-state="happy"]').waitFor({ state: 'detached' }),
			(app) => seed(app, MOOD, 'mood')
		));
});

describe('the ×', () => {
	test('is drawn only while the line holds something', async () => {
		await onPage(async ({ page }) => {
			assert.equal(await page.locator(CLEAR).count(), 0, 'drawn over an empty line');
			await load(page, 'occur', []);
			await page.locator(CLEAR).waitFor();
			await load(page, '', [UNLOCKED]);
			await page.locator(CLEAR).waitFor();
			await load(page, '', []);
			await page.locator(CLEAR).waitFor({ state: 'detached' });
		});
	});

	test('clears the text and every pill at once, and leaves the focus where it was', async () => {
		await onPage(async ({ page }) => {
			await load(page, 'occur coffee', [UNLOCKED, LOCKED]);
			await page.locator(BOX).blur();
			await page.locator(CLEAR).click();
			assert.deepEqual(await line(page), { text: '', pills: [] });
			assert.notEqual(
				await page.evaluate(() => document.activeElement?.tagName),
				'INPUT',
				'the × put the focus in the box'
			);
			await page.locator(CLEAR).waitFor({ state: 'detached' });
		});
	});

	test('a locked pill it cleared does not come back on a reload', async () => {
		await withApp(async (app) => { // reloads
			const { page } = app;
			await load(page, '', [UNLOCKED, LOCKED]);
			await page.locator(CLEAR).click();
			assert.deepEqual(await line(page), { text: '', pills: [] });
			await app.reload();
			await page.locator(BOX).waitFor();
			assert.deepEqual((await line(page)).pills, [], 'a cleared locked pill came back');
			assert.deepEqual(app.errors, []);
		});
	});

	// The positive arm of the one above: a locked pill does survive a reload,
	// so its absence there is the × and not the reload.
	test('a locked pill left alone does come back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await load(page, '', [LOCKED]);
			await app.reload();
			await page.locator('.prompt-bar .pill[data-pill="k"]').waitFor();
		});
	});
});
