// Layouts a KIT shipped, and which device a layout is for (da-gg0v).
//
// vividn, 2026-09-15: "maybe if a layout was installed through a kit, it
// could reset to the kit's definition" (18:15) and "layouts have to go
// inside the db proper, not the _internal db … I guess there could be
// layouts in both; some layouts might be db specific" (18:53). So the page
// reads TWO stores of layouts: `_internal`, the person's own, and the data
// database, where `datum kit install` put whatever the kit shipped. The
// rules these hold to:
//
//   - a kit's layout is listed and opens, and says which kit it came from;
//   - a save of the same name SHADOWS it, and `Reset layout` goes back to
//     the kit's tree — the second arm of da-luwe's `shippedTree`;
//   - the install writes nothing to `_internal`;
//   - `designedFor` decides who wins a name on THIS device (18:01: "load a
//     different set of layouts by default when the device is detected to be
//     mobile, but still let them both load each other's custom layouts"),
//     and the order the chooser lists them in. It never hides one.
//
// The kit is installed through the seam, as a person would: `kit install -`
// with the TOML on stdin, which is the door the browser has (it has no
// filesystem for a path).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, chooser, rootMenuButton } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** A whole `Layout.root` of one named widget — enough to tell two trees apart. */
const root = (id, kind) => ({
	id,
	tabs: [{ id: `${id}-tab`, content: { id: `${id}-w`, kind, settings: {} } }],
	active: 0,
	bar: { edge: 'top', align: 'start' }
});

/** A kit TOML shipping the given layouts. */
const kitToml = (name, layouts) =>
	`[kit]\nname = "${name}"\nversion = "1.0"\n` +
	layouts
		.map(
			(l) =>
				`\n[[layout]]\nname = "${l.name}"\n` +
				(l.designedFor ? `designed_for = "${l.designedFor}"\n` : '') +
				`root = '${JSON.stringify(l.root)}'\n`
		)
		.join('');

/** Install a kit through the seam, as `kit install -` reads it: TOML on stdin. */
async function installKit(app, name, layouts) {
	const out = await app.run(['kit', 'install', '-'], kitToml(name, layouts));
	assert.equal(out.code, 0, `kit install: ${out.stderr}`);
	return out;
}

/** `window.datum.layouts[fn](...args)` — the store itself. */
const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/** `window.datum.shell[fn](...args)` — the root tabs and what a name resolves to. */
const shell = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.shell[fn](...args), [fn, args]);

/** Open the chooser from the root bar's `+`. */
async function openChooser(page) {
	await page.locator('.root-bar button.add').first().click();
	await chooser(page).waitFor();
}

/** The layout buttons the chooser offers: name and source, in order. */
const layoutOffers = (page) =>
	chooser(page)
		.locator('button.layout')
		.evaluateAll((els) =>
			els.map((el) => ({
				name: el.querySelector('.name').textContent,
				source: el.dataset.source
			}))
		);

describe('a kit ships a layout', () => {
	test('the chooser lists it under its kit, it opens, and `_internal` is untouched', async () => {
		await withApp(async (app) => {
			await installKit(app, 'focus', [{ name: 'focus-day', root: root('focus', 'nowview') }]);

			// Nothing of the install reached the person's own store — the
			// row went into the data db, beside the entries it belongs to.
			const mine = await app.run(['--db', '_internal', 'tail', 'layout', '--json', '-n', '50']);
			assert.equal(mine.code, 0, mine.stderr);
			assert.deepEqual(JSON.parse(mine.stdout).rows, []);

			await openChooser(app.page);
			assert.deepEqual(await layoutOffers(app.page), [
				{ name: 'day', source: 'builtin' },
				{ name: 'focus-day', source: 'kit:focus' }
			]);

			// Chosen, it draws the kit's tree — one nowview, filling the tab.
			await chooser(app.page).locator('button.layout[data-source="kit:focus"]').click();
			await chooser(app.page).waitFor({ state: 'detached' });
			await app.page.locator('.root-bar [role="tab"]', { hasText: 'focus-day' }).waitFor();
			assert.deepEqual(
				await app.page.evaluate(() =>
					[...document.querySelectorAll('section.pane')].map((p) => p.dataset.title)
				),
				['nowview']
			);
		}, DESKTOP);
	});

	test('a save of the same name shadows it, and `Reset layout` goes back to the kit’s tree', async () => {
		await withApp(async (app) => {
			const shipped = root('focus', 'nowview');
			await installKit(app, 'focus', [{ name: 'focus-day', root: shipped }]);
			await app.reload();

			// With nothing saved, the name means the kit's tree — and that is
			// also what a reset would go back to.
			assert.deepEqual((await shell(app, 'resolve', 'focus-day')).root, shipped);
			assert.deepEqual((await shell(app, 'shippedTree', 'focus-day')).root, shipped);

			// The person saves their own under that name. It shadows.
			const mine = { version: 1, name: 'focus-day', root: root('mine', 'tail') };
			await layouts(app, 'saveLayout', mine);
			await app.reload();
			assert.deepEqual((await shell(app, 'resolve', 'focus-day')).root, mine.root);
			// And the kit's is still what shipped: the reset's target.
			assert.deepEqual((await shell(app, 'shippedTree', 'focus-day')).root, shipped);

			// Listed once, as the person's — one name space across the two.
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => ({ name: l.name, source: l.source })),
				[{ name: 'focus-day', source: 'user' }]
			);

			// The reset itself, through the root menu, on the open tab.
			assert.equal(await shell(app, 'open', 'focus-day'), true);
			await app.page.locator('.root-bar [role="tab"]', { hasText: 'focus-day' }).waitFor();
			await rootMenuButton(app.page).click();
			await app.page.getByRole('menuitem', { name: 'Reset layout', exact: true }).click();
			await app.page.locator('[role="menu"]').waitFor({ state: 'detached' });
			await app.page.locator('section.pane[data-title="nowview"]').waitFor();
			// A draft, not a write (da-luwe): the save still holds the
			// person's tree until they save the reset.
			assert.deepEqual((await layouts(app, 'loadLayout', 'focus-day')).root, mine.root);
		}, DESKTOP);
	});
});

describe('the device a layout was designed for', () => {
	/** Save a mobile-designed `day`, then reload and report the panes on screen. */
	async function dayOn(options) {
		let panes;
		await withApp(async (app) => {
			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'day',
				designedFor: 'mobile',
				root: root('phone-day', 'aliases')
			});
			await app.reload();
			panes = await app.page.evaluate(() =>
				[...document.querySelectorAll('section.pane')].map((p) => p.dataset.title)
			);
		}, options);
		return panes;
	}

	test('a mobile save of `day` shadows the built-in on a phone and not on a desktop', async () => {
		// On the phone it IS the page; on the desktop the built-in day wins
		// and the save is simply not what `day` means there.
		assert.deepEqual(await dayOn(PHONE), ['aliases']);
		assert.notDeepEqual(await dayOn({ viewport: { width: 1280, height: 900 } }), ['aliases']);
	});

	test('the chooser puts this device’s layouts first, and hides neither', async () => {
		// `withApp` hands its app to the body and returns nothing, so the
		// reading comes out through a binding, as `dayOn` above does.
		async function order(options) {
			let names;
			await withApp(async (app) => {
				await installKit(app, 'both', [
					{ name: 'deskish', designedFor: 'desktop', root: root('d', 'tail') },
					{ name: 'phonish', designedFor: 'mobile', root: root('p', 'tail') }
				]);
				await openChooser(app.page);
				names = (await layoutOffers(app.page)).map((o) => o.name);
			}, options);
			return names;
		}

		assert.deepEqual(await order(DESKTOP), ['day', 'deskish', 'phonish']);
		assert.deepEqual(await order(PHONE), ['day', 'phonish', 'deskish']);
	});
});
