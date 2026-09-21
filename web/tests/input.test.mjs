// The input widget (da-0dhc, back as da-3zon).
//
// vividn: «Let's bring back the input widget, I'm thinking about positioning
// the input box somewhere else on desktop, it's too invisible there. But
// keep the fixed input bar on the bottom still for now.» So the widget is
// about PLACEMENT, and the claim this file carries is that it is not a
// SECOND command line but the same one drawn in a second place: one
// buffer, one set of pills, one Enter, one receipt.
//
// Everything here is asked of the page as a reader would ask it — what is
// in each box, what each receipt says. What a receipt's text IS belongs to
// bar.test.mjs, which compares it with the CLI's own echo; the claim here
// is only that the widget shows the same one.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { choose, chooser, exactly } from './support/layout.mjs';
import { openPanel } from './support/fields.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 800 } };

const WIDGET = '.input-widget input';
const BAR = '.prompt-bar input';

/** What is in a box right now. */
const valueOf = (page, selector) => page.locator(selector).inputValue();

/** What a frame's result area is showing — the bar's, or the widget's. Every
 *  document, joined: a field's first log echoes two — the TRACK one that
 *  declares it and the entry (da-c8mh) — and the claim here is that the two
 *  frames show the SAME receipt, however many documents that receipt has. */
/**
 * What a surface's receipt says, wherever that surface keeps it.
 *
 * No `.result` in the path: the bar's output section is row one of the
 * history now (da-ckw3) and so has no `BarResult` wrapper, while the input
 * widget's receipt still does. `.doc` is what both draw.
 */
const receipt = async (page, frame) =>
	(await page.locator(`${frame} .doc`).allInnerTexts()).join('\n');

/** Type into a box the way a person does, one key at a time. */
async function typeIn(page, selector, line) {
	await page.locator(selector).click();
	await page.keyboard.type(line);
}

/** Save a layout that holds `kinds` side by side, and open it. */
async function open(app, name, kinds) {
	const bar = { edge: 'top', align: 'start' };
	await app.page.evaluate(
		([name, kinds, bar]) =>
			window.datum.layouts.saveLayout({
				version: 1,
				name,
				root: {
					id: 'r',
					active: 0,
					bar,
					tabs: [
						{
							id: 'r-tab',
							content: {
								id: 'row',
								direction: 'row',
								ratios: kinds.map(() => 1 / kinds.length),
								children: kinds.map((kind) => ({
									id: kind,
									active: 0,
									bar,
									tabs: [{ id: `${kind}-tab`, content: { id: `${kind}-w`, kind, settings: {} } }]
								}))
							}
						}
					]
				}
			}),
		[name, kinds, bar]
	);
	await app.goto(`/${name}`);
	await app.page.locator(WIDGET).waitFor();
}

describe('/input', () => {
	test('is one input widget, with the bottom bar still under it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await page.locator(WIDGET).waitFor();
			assert.equal(await page.locator('section.pane').count(), 1);
			assert.equal(await page.locator('section.pane[data-title="input"]').count(), 1);
			// «Keep the fixed input bar on the bottom still for now»: nothing
			// in this bead hides it, and the widget is not it.
			assert.equal(await page.locator('.prompt-bar').count(), 1);
			assert.equal(await page.locator('.input-widget .prompt-bar').count(), 0);
			// Not the REPL: no output pane and no scrollback of its own.
			assert.equal(await page.locator('.input-widget .repl').count(), 0);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('the widget and the bar are two views of one command line', () => {
	test('what is typed in either is in both, and Enter in the widget runs it once and empties both', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await page.locator(WIDGET).waitFor();

			await typeIn(page, WIDGET, 'occur coffee');
			await page.waitForFunction(
				(bar) => document.querySelector(bar).value === 'occur coffee',
				BAR
			);
			assert.equal(await valueOf(page, WIDGET), 'occur coffee');

			// And the other way: the bar is not a mirror that only reads.
			await page.locator(BAR).fill('occur tea');
			await page.waitForFunction(
				(w) => document.querySelector(w).value === 'occur tea',
				WIDGET
			);

			// Enter in the WIDGET runs it — once — and both boxes are empty.
			await page.locator(WIDGET).fill('occur coffee');
			await page.locator(WIDGET).press('Enter');
			await page.waitForFunction(
				async () => (await window.datum.entries(50)).some((e) => e.field === 'coffee')
			);
			assert.deepEqual(
				(await app.entries()).map((e) => e.field),
				['coffee'],
				'the line ran in both views'
			);
			assert.equal(await valueOf(page, WIDGET), '');
			assert.equal(await valueOf(page, BAR), '');

			// One receipt, drawn twice. Not asserted against a string written
			// here — what a receipt SAYS is bar.test.mjs's claim, against the
			// CLI's own echo — only that the widget is showing the same one.
			await page.locator('.input-widget .result .doc').first().waitFor();
			const shown = await receipt(page, '.input-widget');
			assert.equal(shown, await receipt(page, '.prompt-bar'), 'the two receipts differ');
			assert.match(shown, /coffee/, `the receipt does not name the field: ${shown}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a pill is in both, and removing it in one removes it in the other', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await page.locator(WIDGET).waitFor();

			// From the time cursor's own door, which is how a pill is ever
			// made: a reader clicking the dayview, not a reader typing (the
			// pills are state the next command carries, not text in the box).
			await page.evaluate(() =>
				window.datum.bar.setPills([{ id: 'when', text: '-t 14:30', kind: 'time' }])
			);
			for (const frame of ['.prompt-bar', '.input-widget']) {
				await page.locator(`${frame} .pill`).waitFor();
				assert.equal(await page.locator(`${frame} .pill .label`).innerText(), '-t 14:30');
			}

			// Removed in the BAR; gone from the widget, because there is one
			// set of pills and not one per box.
			await page.locator('.prompt-bar .pill button.remove').click();
			await page.locator('.input-widget .pill').waitFor({ state: 'detached' });
			assert.equal(await page.locator('.prompt-bar .pill').count(), 0);

			// And a pill goes into the argv of a line run from the WIDGET,
			// after the verb, exactly as it does from the bar.
			await page.evaluate(() =>
				window.datum.bar.setPills([{ id: 'when', text: '-t 14:30', kind: 'time' }])
			);
			await page.locator('.input-widget .pill').waitFor();
			await page.locator(WIDGET).fill('occur coffee');
			await page.locator(WIDGET).press('Enter');
			await page.waitForFunction(
				async () => (await window.datum.entries(50)).some((e) => e.field === 'coffee')
			);
			// The zone is UTC here (`TIMEZONE` in support/app.mjs), so 14:30
			// on the line is 14:30 in what was stored.
			const [entry] = await app.entries();
			assert.match(
				entry.occur_time.utc,
				/T14:30/,
				`the pill did not reach a line run from the widget: ${JSON.stringify(entry.occur_time)}`
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('a focus nobody clicked', () => {
	/** Press a tracked field's `note`, which prefills the command line and runs nothing. */
	async function pressNote(page) {
		await openPanel(page, 'coffee');
		const note = page.locator('.bar .panel button.note');
		await note.waitFor();
		await note.click();
	}

	test('lands in the input widget when one is on screen', async () => {
		await withApp(async (app) => {
			const { page, submit } = app;
			await submit('spec set coffee --verb occur');
			await open(app, 'both', ['widgetbar', 'input']);
			await page.locator('.bar [data-field="coffee"]').waitFor();

			await pressNote(page);
			await page.waitForFunction(
				(w) => {
					const box = document.querySelector(w);
					return box.value === 'note coffee ' && document.activeElement === box;
				},
				WIDGET
			);
			// One buffer still: the bar has the line too, it just does not
			// have the caret.
			assert.equal(await valueOf(page, BAR), 'note coffee ');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('lands in the bar when there is none', async () => {
		await withApp(async (app) => {
			const { page, submit } = app;
			await submit('spec set coffee --verb occur');
			await page.locator('.bar [data-field="coffee"]').waitFor();
			assert.equal(await page.locator('.input-widget').count(), 0, 'the day page has an input widget');

			await pressNote(page);
			await page.waitForFunction(
				(bar) => {
					const box = document.querySelector(bar);
					return box.value === 'note coffee ' && document.activeElement === box;
				},
				BAR
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

// `reversed` (da-gvdc, vividn 16:25: «options about being reversed or not,
// which then determine if the input bar of the input widget is on the top
// or the bottom»): only where the two rows sit changes.
describe('reversed', () => {
	/** The prompt row's box and the receipt's, top edges. */
	const tops = (page) =>
		page.evaluate(() => {
			const top = (sel) => {
				const { top, bottom, height } = document.querySelector(`.input-widget ${sel}`).getBoundingClientRect();
				return { top, bottom, height };
			};
			return { prompt: top('.line'), receipt: top('.receipt') };
		});

	test('from Options…: the prompt row on top, and typing, a pill and Enter work as before', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'custom', 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('input') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator(WIDGET).waitFor();
			// A receipt to measure: with nothing run it is an empty row.
			await app.submit('occur water');
			await page.locator('.input-widget .result .doc').first().waitFor();
			let box = await tops(page);
			assert.ok(box.receipt.height > 0 && box.receipt.bottom <= box.prompt.top + 1, `the default: ${JSON.stringify(box)}`);

			await choose(page, 'custom', 'Options…');
			await page.locator('dialog.options [data-option="reversed"] input').setChecked(true);
			await page.keyboard.press('Escape');
			await page.locator('dialog.options').waitFor({ state: 'detached' });
			box = await tops(page);
			assert.ok(box.prompt.bottom <= box.receipt.top + 1, `reversed: ${JSON.stringify(box)}`);

			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 14:30', kind: 'time' }]));
			await page.locator('.input-widget .pill').waitFor();
			await typeIn(page, WIDGET, 'occur coffee');
			assert.equal(await valueOf(page, BAR), 'occur coffee');
			await page.locator(WIDGET).press('Enter');
			await page.waitForFunction(async () => (await window.datum.entries(50)).some((e) => e.field === 'coffee'));
			const entry = (await app.entries()).find((e) => e.field === 'coffee');
			assert.match(entry.occur_time.utc, /T14:30/);
			assert.equal(await valueOf(page, WIDGET), '');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('at /input on a phone, reversed, both rows are on the screen', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.goto('/input');
				await page.locator(WIDGET).waitFor();
				// No REPL here to wait on: the widget's own Enter, and its receipt.
				await page.locator(WIDGET).fill('occur water');
				await page.locator(WIDGET).press('Enter');
				await page.locator('.input-widget .result .doc').first().waitFor();
				await page.evaluate(() =>
					window.datum.layout.apply((l) =>
						JSON.parse(JSON.stringify(l), (key, v) => (v?.kind === 'input' ? { ...v, settings: { reversed: true } } : v))
					)
				);
				await page.waitForFunction(() => document.querySelector('.input-widget.reversed'));
				const box = await tops(page);
				assert.ok(box.prompt.bottom <= box.receipt.top + 1, JSON.stringify(box));
				for (const b of [box.prompt, box.receipt]) {
					assert.ok(b.top >= 0 && b.bottom <= 640 && b.height > 0, JSON.stringify(b));
				}
			},
			{ device: { viewport: { width: 360, height: 640 }, hasTouch: true, isMobile: true } }
		);
	});
});
