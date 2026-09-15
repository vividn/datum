// One pane, filling the page — the maximize badge (da-00am).
//
// vividn, on the deployed build: «Next to the + icon on the pane frame,
// let's add a little maximize icon, that then maximizes the pane, leaving
// the layout and coming back should unmaximize the pane, the maximize
// shouldn't be too sticky in general.»
//
// So there are two halves to hold the app to, and both are here. The first
// is that maximizing SHOWS the pane alone, filling the root's content —
// including a pane drawn inside a nested layout, which fills the page and
// not the box it was in, because a maximize into a small box is not one.
//
// The second is that it is VIEW STATE and nothing else
// (`$lib/layout/maximize.ts`): the tree is not touched, so no draft is
// written, no layout wears the modified dot, the ratios come back exactly,
// and the state is gone on a reload, on leaving the layout and coming back,
// and whenever the pane it names is no longer there. That is what «not too
// sticky» means, and it is the half a reader would only notice by losing
// their layout to it.
//
// Driven through the built app in a real browser, like the other `layout-*`
// files, and reading what a reader sees: the panes on screen, their boxes,
// the badge's own label.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	bar,
	choose,
	chooseRoot,
	chooser,
	exactly,
	fill,
	maximizeButton,
	pane,
	settled,
	titles,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The desktop default (defaults.ts), as the panes are titled. */
const SIX = ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields'];

/** Maximize or restore `id`, and wait for the page to settle on it. */
async function press(page, id) {
	await maximizeButton(page, id).click();
	await settled(page);
}

/** Every border's ratio, in DOM order: what «the tree did not change» reads as. */
const ratios = (page) =>
	page.locator('[role="separator"]').evaluateAll((all) => all.map((el) => el.getAttribute('aria-valuenow')));

/** The `day` layout's draft, as stored — null when nothing has been edited. */
const draftOf = (page) => page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.day'));

/** How many root tabs wear the orange modified dot. */
const dots = (page) => page.locator('.root-bar [role="tab"].modified').count();

const boxOf = async (locator) => {
	const box = await locator.boundingBox();
	assert.ok(box, 'nothing to measure');
	return box;
};

/** Two boxes the same, to within a bar's row — the slack a frame's title has. */
function sameBox(got, want, what) {
	for (const side of ['x', 'y', 'width', 'height'])
		assert.ok(
			Math.abs(got[side] - want[side]) <= 16,
			`${what}: ${side} is ${got[side].toFixed(1)}, not ${want[side].toFixed(1)}`
		);
}

/** Whether a maximize is in force at all, as the badges report it. */
const pressed = (page) => page.locator('button.maximize[aria-pressed="true"]').count();

describe('the maximize badge', () => {
	test('fills the root with the one pane, and puts the tree back exactly', async () => {
		await withApp(async ({ page, errors }) => {
			// The box the whole tree has is the box the one pane must take.
			const whole = await boxOf(page.locator('[data-split="main"]'));
			const before = { ratios: await ratios(page), draft: await draftOf(page) };
			assert.deepEqual(await titles(page), SIX);

			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			sameBox(await boxOf(pane(page, 'repl')), whole, 'the maximized repl');

			const badge = maximizeButton(page, 'repl');
			assert.equal(await badge.getAttribute('aria-label'), 'restore the repl pane');
			assert.equal(await badge.getAttribute('aria-pressed'), 'true');

			await press(page, 'repl');
			assert.deepEqual(await titles(page), SIX);
			assert.deepEqual(await ratios(page), before.ratios, 'the ratios moved');
			assert.equal(await draftOf(page), before.draft, 'maximizing wrote a draft');
			assert.equal(await dots(page), 0, 'maximizing marked a layout modified');
			assert.equal(
				await maximizeButton(page, 'repl').getAttribute('aria-label'),
				'maximize the repl pane'
			);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	// «leaving the layout and coming back should unmaximize the pane».
	test('is dropped by opening another layout and coming back', async () => {
		await withApp(async ({ page }) => {
			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);

			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').filter({ hasText: 'stats' }).waitFor();

			await page.locator('.root-bar [role="tab"]').filter({ hasText: exactly('day') }).click();
			await settled(page);
			assert.deepEqual(await titles(page), SIX);
			assert.equal(await pressed(page), 0);
		}, DESKTOP);
	});

	test('is gone after a reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);

			await app.reload();
			await settled(page);
			assert.deepEqual(await titles(page), SIX);
			assert.equal(await pressed(page), 0);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The healing: the state names a pane, and a pane that is not in the
	// tab's tree any more is simply not honoured. Nothing has to notice.
	test('heals when the maximized pane is closed', async () => {
		await withApp(async ({ page, errors }) => {
			await press(page, 'repl');
			await choose(page, 'repl', 'Close pane');
			await settled(page);
			assert.deepEqual(
				await titles(page),
				SIX.filter((t) => t !== 'repl')
			);
			assert.equal(await pressed(page), 0);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	// A maximize into a small box is not a maximize: a pane drawn inside a
	// nested layout tab fills the ROOT, not the layout that holds it.
	test('a pane inside a nested layout fills the root, not the nested box', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const whole = await boxOf(page.locator('[data-split="main"]'));
			await page.evaluate((mine) => window.datum.layouts.saveLayout(mine), MINE);
			await choose(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'mine');
			await pane(page, 'm-a').waitFor();
			await settled(page);

			const inner = await boxOf(pane(page, 'm-a'));
			assert.ok(inner.width < whole.width / 2, `the nested pane is already ${inner.width}px wide`);

			await press(page, 'm-a');
			assert.deepEqual(await titles(page), ['nowview']);
			sameBox(await boxOf(pane(page, 'm-a')), whole, "the nested layout's maximized pane");
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// A lock keeps the LAYOUT from changing; maximizing changes nothing
	// saved, so it is the one badge that stays.
	test('a locked layout keeps the badge and loses the `+`', async () => {
		await withApp(async ({ page }) => {
			await chooseRoot(page, 'Lock layout');
			await settled(page);
			assert.equal(await page.locator('section.pane button.add').count(), 0, 'a locked layout kept its `+`');

			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			await press(page, 'repl');
			assert.deepEqual(await titles(page), SIX);
		}, DESKTOP);
	});

	// Nothing to fill, and no bar to put it on.
	test('the root and a bare holder draw none', async () => {
		await withApp(async ({ page }) => {
			const stray = await page.evaluate(() =>
				[...document.querySelectorAll('button.maximize')]
					.map((el) => el.closest('section'))
					.filter((s) => s.classList.contains('root') || s.classList.contains('bare'))
					.map((s) => s.dataset.title)
			);
			assert.deepEqual(stray, []);
			// The badge is on the panes that DO draw a bar, all six of them.
			assert.equal(await page.locator('section.pane > .titlebar button.maximize').count(), SIX.length);
		}, DESKTOP);
	});

	// `align: end` mirrors the row, so the order is worth asserting rather
	// than reading off the source: title/tabs, then `+`, then maximize.
	test('the badges come after the title, the `+` first', async () => {
		await withApp(async ({ page }) => {
			const along = await pane(page, 'repl')
				.locator('> .titlebar > .along')
				.evaluate((el) =>
					[...el.children].map(
						(c) => ['titlebox', 'tabs', 'add', 'maximize'].find((name) => c.classList.contains(name)) ?? c.className
					)
				);
			assert.deepEqual(along, ['titlebox', 'add', 'maximize']);
		}, DESKTOP);
	});
});

/** Two panes side by side, so a nested layout has an INSIDE to maximize out of. */
const MINE = {
	version: 1,
	name: 'mine',
	root: {
		id: 'm',
		active: 0,
		bar,
		tabs: [
			{
				id: 'm-tab',
				content: {
					id: 'm-split',
					direction: 'row',
					ratios: [0.5, 0.5],
					children: [widgetPane('m-a', 'nowview'), widgetPane('m-b', 'fields')]
				}
			}
		]
	}
};
