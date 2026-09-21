// «Reset default layouts» and the gear that opens it (da-jbcv).
//
// vividn, 2026-09-16 18:16: "Lets readd the 'Reset default layouts' in the
// global settings menu. Make the gear icon much bigger, it's almost
// invisible. Reset default layouts, should reset all built in layouts, as
// well as the tabs shown on the root. Other layouts are still accessible
// to be added with the + though, and are not reset."
//
// So there are two rules, and one fence. The rules: the gear is drawn big
// enough to press and to SEE, and the act puts every layout datum ships
// back to the tree it ships with and the root bar back to the one tab a
// fresh install shows. The fence is what the act it replaces got wrong —
// «Reset all layouts…» deleted every saved layout (da-luwe), so each test
// below that resets has a layout of the reader's own and a layout a kit
// installed standing beside the built-in, and says they came through it
// untouched and still offered by the `+`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, choose, chooseRoot, chooser, exactly } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The item, as it reads in the gear's menu. */
const RESET = 'Reset default layouts…';

/** `window.datum.layouts[fn](...args)` — the store itself. */
const layouts = (page, fn, ...args) =>
	page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/** The names the root bar's tabs hold, in order. */
const rootNames = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

/**
 * The tree the root tab for `name` holds, as its save would read it back
 * (`extract`) — the one lens a layout is compared through anywhere in the
 * shell. Read from a FRESH app it is the registry's shipped tree and
 * nothing else, which is what a reset is asserted to arrive back at.
 */
const treeOf = (page, name) =>
	page.evaluate((name) => {
		const d = window.datum;
		const tab = d.shell.rootTabs(d.layout.current()).find((t) => t.layout === name);
		return tab ? JSON.stringify(d.shell.extract(tab)) : null;
	}, name);

/** The strip's day-height handle says the setting; `aria-valuenow` is it. */
const dayHeight = (page) =>
	page.locator('.dayview button.day-grip').getAttribute('aria-valuenow').then(Number);

const dayHeightIs = (page, want) =>
	page.waitForFunction(
		(want) => Number(document.querySelector('.dayview button.day-grip')?.getAttribute('aria-valuenow')) === want,
		want
	);

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

/** Open the gear's menu and choose an item by its name. */
async function chooseSettings(page, label) {
	await page.locator('.root-bar button.gear').click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
}

/** Press `button` in the confirm the act opens, and wait for it to go. */
async function answer(page, button) {
	const dialog = page.locator('dialog.confirm');
	await dialog.waitFor();
	await dialog.getByRole('button', { name: button, exact: true }).click();
	await dialog.waitFor({ state: 'detached' });
}

/** Open layout `name` as a root tab, through the `+` and the one chooser. */
async function openRoot(page, name) {
	await page.locator('.root-bar button.add').first().click();
	await chooser(page).waitFor();
	// By the NAME span: a kit's layout wears its kit's name beside its own
	// (Chooser.svelte), so the button's whole text is not the layout's name.
	await chooser(page)
		.locator('button.layout')
		.filter({ has: page.locator('span.name', { hasText: exactly(name) }) })
		.click();
	await chooser(page).waitFor({ state: 'detached' });
	await page.locator('.root-bar [role="tab"]', { hasText: name }).first().waitFor();
}

/** What the chooser offers as layouts: name and source, in order. */
async function layoutOffers(page) {
	await page.locator('.root-bar button.add').first().click();
	await chooser(page).waitFor();
	const offered = await chooser(page)
		.locator('button.layout')
		.evaluateAll((els) => els.map((el) => ({ name: el.querySelector('.name').textContent, source: el.dataset.source })));
	await page.keyboard.press('Escape');
	await chooser(page).waitFor({ state: 'detached' });
	return offered;
}

/** A whole `Layout.root` of one named widget — enough to tell two trees apart. */
const kitRoot = (id, kind) => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-w`, kind, settings: {} } }],
	active: 0,
	bar: { edge: 'top', align: 'start' }
});

/** Install a kit through the seam, as `kit install -` reads it: TOML on stdin. */
async function installKit(app, name, layout) {
	const toml =
		`[kit]\nname = "${name}"\nversion = "1.0"\n` +
		`\n[[layout]]\nname = "${layout.name}"\nroot = '${JSON.stringify(layout.root)}'\n`;
	const out = await app.run(['kit', 'install', '-'], toml);
	assert.equal(out.code, 0, `kit install: ${out.stderr}`);
}

/**
 * The page a reset is asked of: a BUILT-IN modified and saved (so a save
 * of the reader's shadows the shipped tree), a layout of the reader's own
 * under a name nothing ships, a layout a kit installed, and a second tab
 * on the root — one of each thing the act must and must not touch.
 *
 * Returns what the assertions are made against: the built-in's shipped
 * tree, read off the app before anything was done to it, and the
 * reader's own layout as it was saved.
 */
async function stage(app) {
	const { page } = app;
	const grip = '.dayview button.day-grip';
	await page.locator(grip).waitFor();
	const shipped = await treeOf(page, 'day');
	const fallback = await dayHeight(page);
	const grown = fallback + 40;

	// A built-in, modified and saved: `day`'s save now shadows the tree
	// `defaults.ts` holds, which is the only thing a reset has to undo.
	// The drag is this session's until it is handed over (da-vhwq), so the
	// layout hears of it through «Save options with layout» first.
	await dragBy(page, grip, -40);
	await dayHeightIs(page, grown);
	await choose(page, 'strip', 'Save options with layout');
	await chooseRoot(page, 'Save layout');
	await page.waitForFunction(() => !document.querySelector('.root-bar [role="tab"].modified'));

	// A SECOND built-in, saved while it is not open: the act is about every
	// name datum ships, not only the one in front of the reader.
	await layouts(page, 'saveLayout', { version: 1, name: 'tail', root: kitRoot('tail-only', 'repl') });

	// A kit's layout, which lives in the DATA db and must not be reached at
	// all — opened as a SECOND root tab, so the root's tabs are not the
	// shipped set either.
	await installKit(app, 'focus', { name: 'focus-day', root: kitRoot('focus', 'nowview') });
	await openRoot(page, 'focus-day');
	await until(() => rootNames(page), (names) => names.join() === 'day,focus-day');

	// And, saved off it under a name nothing ships, the reader's own layout.
	page.once('dialog', (dialog) => dialog.accept('mine'));
	await chooseRoot(page, 'Save layout as…');
	await until(() => rootNames(page), (names) => names.join() === 'day,mine');

	return { shipped, fallback, grown, mine: JSON.stringify(await layouts(page, 'loadLayout', 'mine')) };
}

describe('the gear', () => {
	// vividn: "Make the gear icon much bigger, it's almost invisible." The
	// button was already the `+`'s 24px box; what was invisible was the
	// MARK inside it — the `⚙` character's ink is a scratch a third the
	// width of the box. So both are measured, and the mark is the half
	// that fails on the version this bead was written against.
	test('is at least the size of the bar’s other controls, and its mark is big enough to see', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.root-bar button.gear').waitFor();
			const gear = await page.locator('.root-bar button.gear').boundingBox();
			const add = await page.locator('.root-bar button.add').boundingBox();
			assert.ok(
				gear.width >= add.width && gear.height >= add.height,
				`the gear is ${gear.width}x${gear.height} and the + is ${add.width}x${add.height}`
			);

			const mark = await page.locator('.root-bar button.gear > *').boundingBox();
			assert.ok(mark.width >= 16 && mark.height >= 16, `the gear's mark is ${mark.width}x${mark.height}`);
		}, DESKTOP);
	});

	test('is a fingertip on a phone', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.root-bar button.gear').waitFor();
			const gear = await page.locator('.root-bar button.gear').boundingBox();
			assert.ok(gear.width >= 24 && gear.height >= 24, `the gear is ${gear.width}x${gear.height}`);
		}, PHONE);
	});
});

describe('Reset default layouts', () => {
	test('the built-ins and the root’s tabs go back; the reader’s own and a kit’s do not', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { shipped, fallback, mine } = await stage(app);

			await chooseSettings(page, RESET);
			await answer(page, 'reset');

			// The root's tabs are the shipped set: the one tab a fresh
			// install shows.
			await until(() => rootNames(page), (names) => names.join() === 'day');

			// And the built-in is the tree it shipped with — the same tree,
			// through the same lens, that this app started on.
			await dayHeightIs(page, fallback);
			assert.equal(await treeOf(page, 'day'), shipped);
			// Nothing of the reader's is left shadowing it, so the chooser
			// calls the name built-in again.
			await assert.rejects(() => layouts(page, 'loadLayout', 'day'));

			// The built-in that was never open went back the same way.
			await assert.rejects(() => layouts(page, 'loadLayout', 'tail'));

			// The fence (da-luwe): neither of the other two was touched, and
			// both are still one press of the `+` away.
			assert.equal(JSON.stringify(await layouts(page, 'loadLayout', 'mine')), mine);
			assert.deepEqual(
				(await layouts(page, 'listLayouts')).map((l) => `${l.name} ${l.source}`),
				['mine user', 'focus-day kit:focus']
			);
			// (`day` is the one built-in the chooser lists as a LAYOUT: the
			// others are each one widget, and are listed among the widgets.)
			assert.deepEqual(await layoutOffers(page), [
				{ name: 'day', source: 'builtin' },
				{ name: 'mine', source: 'user' },
				{ name: 'focus-day', source: 'kit:focus' }
			]);
		}, DESKTOP);
	});

	test('cancel changes nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { grown, mine } = await stage(app);
			const before = await treeOf(page, 'day');

			await chooseSettings(page, RESET);
			await answer(page, 'cancel');

			assert.deepEqual(await rootNames(page), ['day', 'mine']);
			assert.equal(await treeOf(page, 'day'), before);
			assert.equal(JSON.stringify(await layouts(page, 'loadLayout', 'mine')), mine);
			// The save the act would have removed is still there, taller day
			// and all.
			const saved = await layouts(page, 'loadLayout', 'day');
			assert.equal(JSON.stringify(saved).includes(`"dayHeight":${grown}`), true, 'day’s save lost its edit');
			assert.equal((await layouts(page, 'loadLayout', 'tail')).name, 'tail');
		}, DESKTOP);
	});
});
