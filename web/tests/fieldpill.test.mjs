// The FIELD PILL (da-8810n): the gesture that mints it, the colour it is
// painted, the argv it lands in, and the widgets that narrow to it.
//
// vividn, 2026-09-21: «Create a field pill for this field and let's filter
// widgets accordingly. So stats widget would filter to this field. tail
// widget too. command widget would filter to commands that take fields.»
//
// The grammar's own rules — which commands survive a field pill, and where
// its word goes in an argv — are `selection-pure.test.mjs`, in node. This
// file is what a reader does and sees, so the page is written down before
// the app loads: the fields widget to hold from, the commands widget to be
// narrowed, a stats widget, a tail pane, a day strip, and a REPL for the
// echoes.
//
// Budget (da-lt0k, da-buqa): node's `--test-timeout` is per TEST and a FILE
// is a test, so the runner's 300 s is this file's whole budget —
// `before(startApp)`, the Chromium it launches, and every test in it.
// Measured alone on the 24-core build box: 84 s for the nine.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp as withAnyApp } from './support/app.mjs';
import { DESKTOP, along, draft, pane, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

/**
 * Every surface the pill touches, on one page.
 *
 * `command` and `widgetbar` because the pill is minted in one and filters
 * the other; `stats` and `tail` because those are the two widgets vividn
 * named; `daystrip` because a picture cannot filter and outlines instead;
 * `repl` because a press leaves its echo there.
 */
const PAGE = JSON.stringify(
	along(
		'column',
		along('row', widgetPane('strip', 'daystrip'), widgetPane('stats', 'stats')).root,
		along('row', widgetPane('tail', 'tail'), widgetPane('repl', 'repl')).root,
		widgetPane('cmd', 'command'),
		widgetPane('bar', 'widgetbar')
	)
);

const withApp = (fn, options = {}) =>
	withAnyApp(fn, {
		...DESKTOP,
		...options,
		prepare: async (context) => {
			await context.addInitScript(draft, PAGE);
		}
	});

/**
 * Two fields with entries of their own, and colours the CLI resolves
 * rather than hashes — so «painted in the field's colour» is a claim about
 * a value this file wrote, not about md5.
 */
const SEED = [
	'spec set coffee --verb occur --color #ffa500',
	'spec set mood --verb switch --state happy --color #0000ff',
	'occur coffee',
	'switch mood happy'
];

const ORANGE = 'rgb(255, 165, 0)';
const BLUE = 'rgb(0, 0, 255)';

/** Seed, and wait until the fields widget has drawn both buttons. */
async function ready(app) {
	for (const line of SEED) await app.submit(line);
	await fieldsBar(app.page).locator('[data-field="coffee"]').waitFor();
	await fieldsBar(app.page).locator('[data-field="mood"]').waitFor();
}

const fieldsBar = (page) => pane(page, 'bar').locator('.bar');
const chip = (page, field) => fieldsBar(page).locator(`[data-field="${field}"]`);
const fieldPill = (page) => page.locator('.prompt-bar .pill.field');

/** Hold a field: the gesture that mints the pill (right click is one of four). */
async function hold(page, field) {
	await chip(page, field).click({ button: 'right' });
}

/** The commands the commands widget is drawing, in its order. */
const commands = (page) =>
	pane(page, 'cmd').evaluate((el) =>
		[...el.querySelectorAll('button[data-command]')].map((b) => b.dataset.command)
	);

describe('the gesture, and the pill it mints', () => {
	test('a hold mints the field pill, in the field’s own colour', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			assert.equal(await fieldPill(page).count(), 0, 'a pill before the gesture');

			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			// The pill's text IS the argv (da-ui4i): the bare field name, and
			// the × is the mark beside it rather than part of what is sent.
			assert.equal(await fieldPill(page).locator('.label').innerText(), 'coffee');
			// vividn: "let's color them with the field color". The colour is
			// `fields --json`'s resolved one, which the seed above SET, so a
			// pill drawn in the accent instead says so here.
			assert.equal(
				await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor),
				ORANGE
			);
			// And the button says where the pill came from, so a second hold
			// on it has somewhere to be aimed.
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), '');

			// ONE field at a time, as the time cursor holds one instant.
			await hold(page, 'mood');
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .pill.field')?.textContent.includes('mood')
			);
			assert.equal(await fieldPill(page).count(), 1, 'two field pills at once');
			assert.equal(
				await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor),
				BLUE
			);
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), null);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn named four gestures and the panel had every one of them, so the
	// panel is a step BEHIND the pill rather than beside it: hold again on a
	// field already pilled and the `-t` / `dur` / `note` bubbles open, which
	// is what keeps them reachable by finger as well as by mouse.
	test('shift+click mints it too, and the same gesture again opens the panel', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);

			await chip(page, 'coffee').click({ modifiers: ['Shift'] });
			await fieldPill(page).waitFor();
			assert.equal(await page.locator('.bar .panel').count(), 0, 'the panel opened instead');
			// And nothing was logged: shift+click is the hold, not a press.
			assert.equal((await app.echoes()).at(-1), 'switch mood happy');

			await chip(page, 'coffee').click({ modifiers: ['Shift'] });
			await page.locator('.bar .panel .bubbles').first().waitFor();
			assert.equal(await fieldPill(page).count(), 1, 'the panel took the pill away');
			assert.deepEqual(app.errors, []);
		});
	});

	// The bead: «a field pill must be mintable and removable without a
	// pointer». The menu key raises `contextmenu`, which is the same door
	// the right click comes through, and the pill's × is a button in the
	// tab order.
	test('the keyboard mints it and the keyboard takes it away', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);

			await chip(page, 'coffee').focus();
			await page.keyboard.press('ContextMenu');
			await fieldPill(page).waitFor();

			const remove = fieldPill(page).locator('button.remove');
			await remove.focus();
			await page.keyboard.press('Enter');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});

	// «it behaves like every other pill»: unlocked, so a successful press
	// spends it and the filter is gone. Named here so it can be corrected
	// in a word if it turns out to be annoying in use.
	test('it is unlocked, so a successful command clears it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();

			await app.submit('tail');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal((await app.echoes()).at(-1), 'tail coffee');
			assert.deepEqual(app.errors, []);
		});
	});

	// THE SILENT ONE, at the seam rather than in the pure test: datum's line
	// is field-first, so `occur coffee` under a `mood` pill would log mood as
	// coffee's STATE. The pill is refused and stays loaded.
	test('a press that already names a field does not get the pill appended', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			await hold(page, 'mood');
			await fieldPill(page).waitFor();

			const seen = await app.spyOnRunner();
			await chip(page, 'coffee').click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'occur'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'occur'), [
				'occur',
				'coffee',
				'--show',
				'json'
			]);
			await app.idle();
			const logged = (await app.entries()).at(0);
			assert.deepEqual([logged.field, logged.state], ['coffee', null]);
			assert.equal(await fieldPill(page).count(), 1, 'a refused pill did not stay loaded');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('what narrows to it', () => {
	// The bead's trap: `fits` FAILS OPEN, so a `field` kind it had not been
	// taught would leave every command visible and the filter would do
	// nothing at all while looking as though it worked. The claim is the
	// HIDING, and that `spec` — the way to the spec editor — survives.
	test('the commands widget keeps the commands that name a field, and hides the rest', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			const whole = await commands(page);
			for (const name of ['get', 'now', 'delete', 'grep']) assert.ok(whole.includes(name), name);

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => !document.querySelector('[data-command="get"]')
			);
			const under = await commands(page);
			for (const name of ['occur', 'end', 'tail', 'stats', 'count', 'ids', 'spec'])
				assert.ok(under.includes(name), `${name} names a field and was hidden`);
			for (const name of ['get', 'now', 'delete', 'update', 'grep', 'dayview'])
				assert.ok(!under.includes(name), `${name} names no field and was not hidden`);
			// The order is the widget's usual one; the filter only removes.
			assert.deepEqual(under, whole.filter((name) => under.includes(name)));
			assert.deepEqual(app.errors, []);
		});
	});

	test('the stats widget narrows to that field, and widens back when the pill clears', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			const stats = pane(page, 'stats');
			// Every field until something narrows it (da-ptuwr): a bare `stats`.
			await stats.locator('code.argv').waitFor();
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats');

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);
			// And the body is that field's own, not the table of all of them.
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.startsWith('coffee')
			);

			// A filter, not a write: it widens back to every field.
			await fieldPill(page).locator('button.remove').click();
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats');
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.includes('all fields')
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the tail pane lists that field alone', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			const rows = () =>
				pane(page, 'tail').evaluate((el) =>
					[...el.querySelectorAll('tbody tr')].map((tr) => tr.querySelector('td.field')?.textContent.trim())
				);
			await page.waitForFunction(() => document.querySelectorAll('section[data-pane="tail"] tbody tr').length > 1);
			assert.ok((await rows()).includes('mood'), 'the whole listing does not hold mood');

			await hold(page, 'coffee');
			await page.waitForFunction(
				() =>
					[...document.querySelectorAll('section[data-pane="tail"] tbody tr')].length > 0 &&
					[...document.querySelectorAll('section[data-pane="tail"] tbody td.field')].every(
						(td) => td.textContent.trim() === 'coffee'
					)
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// A picture cannot filter, so it BRINGS OUT instead (vividn: "nothing too
	// intense, it must still be possible to see all the other data well").
	// The claim is both halves: the pilled field's marks carry the halo, and
	// nothing at all is done to the others.
	test('a picture outlines that field’s own marks and leaves every other field alone', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await ready(app);
			const groups = () =>
				pane(page, 'strip').evaluate((el) =>
					[...el.querySelectorAll('svg.svg .field[data-field]')].map((g) => ({
						field: g.dataset.field,
						pilled: g.hasAttribute('data-pilled'),
						filter: getComputedStyle(g).filter
					}))
				);
			await page.waitForFunction(
				() => document.querySelectorAll('section[data-pane="strip"] svg.svg .field[data-field]').length > 1
			);
			assert.deepEqual(
				[...new Set((await groups()).map((g) => g.filter))],
				['none'],
				'a group was already filtered before any pill'
			);

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => document.querySelector('section[data-pane="strip"] .field[data-pilled]')
			);
			const marked = await groups();
			assert.ok(marked.length > 1, 'only one field is drawn — there is nothing to compare');
			for (const g of marked) {
				assert.equal(g.pilled, g.field === 'coffee', g.field);
				// The class is only wiring; the CLAIM is that the browser
				// really paints one differently and the rest not at all.
				assert.equal(g.filter === 'none', g.field !== 'coffee', `${g.field}: ${g.filter}`);
			}

			await fieldPill(page).locator('button.remove').click();
			await page.waitForFunction(
				() => !document.querySelector('section[data-pane="strip"] .field[data-pilled]')
			);
			assert.deepEqual(
				[...new Set((await groups()).map((g) => g.filter))],
				['none'],
				'the halo outlived the pill'
			);
			assert.deepEqual(app.errors, []);
		});
	});
});
