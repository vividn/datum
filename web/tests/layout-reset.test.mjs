// `Reset widget` (da-7ymx): the act in a pane's menu that puts a widget
// back to the defaults its KIND declares.
//
// vividn, asked whether the dayview's `↻` should become a double-tap on the
// day-height handle: "We can put a reset widget button in the widget
// menu." So it is not the strip's button moved — it is the general act,
// for every kind that has defaults written down, in the one menu
// (`menu.ts`) both bars are built from.
//
// The rule these hold to: the defaults come from the registry and nowhere
// else, the act is a DRAFT change down `setWidget` — so the layout wears
// its dot and `Save layout` is what writes it — and a tab with no widget
// of its own (it holds a layout, or a split) has no such act at all.
//
// Since da-vhwq the widget's own `change` does NOT take that path: the
// strip's handle is this session's until «Save options with layout» is
// asked for. So the reset is one of the two acts that still write a widget
// into the tree, and it drops what the session holds as well — «defaults»
// means the widget as it STARTS.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, addTab, chooseRoot, chooser, choose, exactly, leaveLayoutMode, manage, menuButton, menuLines, pane } from './support/layout.mjs';

/** «Save options with layout» (da-vhwq): what makes a widget's own move the layout's. */
const SAVE = 'Save options with layout';

before(startApp);
after(stopApp);

/** The item, as it reads in the menu. */
const RESET = 'Reset widget';

/** Open a pane's menu, read its lines, and close it again. */
async function linesOf(page, id) {
	await menuButton(page, id).click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}

/** The strip's day-height handle says the setting; `aria-valuenow` is it. */
const dayHeight = (page) =>
	pane(page, 'strip')
		.locator('.today-line')
		.getAttribute('aria-valuenow')
		.then(Number);

const dayHeightIs = (page, want) =>
	page.waitForFunction(
		(want) => Number(document.querySelector('.dayview .today-line')?.getAttribute('aria-valuenow')) === want,
		want
	);

/** Whether the root tab for `name` wears the modified dot. */
const dotted = async (page, name) =>
	(await page.locator('.root-bar [role="tab"].modified').filter({ hasText: name }).count()) > 0;

/** Drag `selector` by `dy` — up is a taller day (da-hsba). */
async function dragBy(page, selector, dy) {
	const box = await page.locator(selector).boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + dy, { steps: 8 });
	await page.mouse.up();
}

describe('Reset widget', () => {
	// The whole round trip on the kind the act was asked for. What the
	// default IS is the registry's business, so it is read off the app it
	// starts in rather than restated here.
	test('the strip: dragged taller, saved, reset to the default, and restored to the save', async () => {
		await withApp(async ({ page }) => {
			const grip = '.dayview .today-line';
			await page.locator(grip).waitFor();
			const fallback = await dayHeight(page);
			const grow = 40;

			await dragBy(page, grip, -grow);
			await dayHeightIs(page, fallback + grow);
			// da-vhwq: the drag is what THIS TAB shows, and nothing more —
			// until it is asked for, which is what lights the dot.
			assert.equal(await dotted(page, 'main'), false, 'a session setting dirtied the layout');
			await choose(page, 'strip', SAVE);
			await page.waitForFunction(() => document.querySelector('.root-bar [role="tab"].modified'));

			// Saved, so that `Restore layout` has something of its own to put
			// back and the dot below is the reset's alone.
			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar [role="tab"].modified'));

			// First in the NAME's menu, before the widget's own options
			// (da-vhwq). Split and the rest are the `▾`'s (da-bzhq), and so
			// is Change Widget (da-z8klv).
			const lines = await linesOf(page, 'strip');
			// The freeze wears its icon before its words (da-2hd21).
			assert.deepEqual(lines, [RESET, '⤓ today frozen at the bottom', 'orientation']);

			await choose(page, 'strip', RESET);
			await dayHeightIs(page, fallback);
			assert.equal(await dotted(page, 'main'), true, 'the reset was not a draft change');

			// A draft change and nothing more: the save still holds the +40.
			await chooseRoot(page, 'Restore layout');
			await dayHeightIs(page, fallback + grow);
			assert.equal(await dotted(page, 'main'), false, 'a restored layout is still modified');
		}, DESKTOP);
	});

	// A second kind, whose settings are a whole object rather than one
	// number: the same one act, reading the same one registry.
	test('stats: a chosen range goes back to this month', async () => {
		await withApp(async ({ page, run }) => {
			for (const argv of [['occur', 'coffee', '-t', '-30'], ['switch', 'mood', 'happy', '-t', '-20']]) {
				const r = await run(argv);
				assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
			}

			// The tail pane becomes the stats widget, which is a pane tab
			// holding a widget — what the act is for.
			await manage(page, 'tail', 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await leaveLayoutMode(page);
			await page.locator('.stats .range button', { hasText: 'week' }).click();
			await page.waitForFunction(() => document.querySelector('.stats .argv')?.textContent === 'stats --week');

			const lines = await linesOf(page, 'tail');
			assert.equal(lines[0], RESET, `the stats menu reads ${lines.join(' | ')}`);

			await choose(page, 'tail', RESET);
			// DEFAULT_STATS is this month over every field: `stats --month`,
			// which is the argv the widget shows it is running (da-80do8).
			await page.waitForFunction(() => document.querySelector('.stats .argv')?.textContent === 'stats --month');
			assert.equal(await page.locator('.stats .range button[aria-pressed="true"]').innerText(), 'month');
		}, DESKTOP);
	});

	test('which tabs have the item: the strip yes, a kind with no defaults no, a tab holding a layout no', async () => {
		await withApp(async ({ page }) => {
			// The contrast is the whole assertion, so the kind that HAS the
			// item is read in the same app as the kinds that have not.
			assert.equal((await linesOf(page, 'strip'))[0], RESET, 'the strip has no Reset widget');

			// `custom` declares no settings, so there is nothing to put back.
			// It is the example since da-hkjp gave `tail` a declared `type`,
			// which puts both items in that kind's NAME menu. Neither item
			// and no layout leaves it with NO name menu at all, Change
			// Widget having moved to the `▾` (da-z8klv).
			assert.equal(await menuButton(page, 'custom').count(), 0);

			// The root tab holds the `main` layout, so the root menu has none
			// either — the same rule, not a second one.
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').click();
			const root = await menuLines(page);
			assert.ok(!root.includes(RESET), `the root menu reads ${root.join(' | ')}`);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// And a PANE tab holding a layout: its content is a split of
			// panes, not a widget of its own. (`main` is choosable in a pane
			// only once the root tab is called something else.)
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as');
			await page.locator('.root-bar [role="tab"]', { hasText: 'mine' }).waitFor();
			await addTab(page, 'tail', 'main');
			await pane(page, 'tail').locator('[role="tab"]', { hasText: 'main' }).waitFor();
			const held = await linesOf(page, 'tail');
			assert.ok(!held.includes(RESET), `a tab holding a layout reads ${held.join(' | ')}`);
		}, DESKTOP);
	});
});
