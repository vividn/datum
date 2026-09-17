// A command button focuses the line without raising a phone's keyboard
// (da-k34p). vividn, 2026-09-17: «when hitting occur, start, etc commands,
// don't auto open the keyboard on mobile. The flow is much more likely that
// they will then hit a field button rather than type something.»
//
// No browser here can show a virtual keyboard, so this file holds the
// MECHANISM: after a command tap the box is focused with `inputmode="none"`;
// a tap on the box, the fields widget's `add <field> ` prefill, and every
// way out of the quiet state (run, clear, blur, verb spent) leave it with
// its normal input mode — a `none` left behind is a prompt nobody can type
// in on a phone. The phone is the real check.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp as withAnyApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, draft, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

const PHONE = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true } };

const page_ = (...extra) =>
	JSON.stringify(
		along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'), ...extra)
	);

const withApp = (fn, options = PHONE, layout = page_()) =>
	withAnyApp(fn, { ...options, prepare: (context) => context.addInitScript(draft, layout) });

const SEED = ['spec set coffee --verb occur', 'spec set sleep --verb start', 'spec set rec --verb add'];

const BAR = '.prompt-bar input';
const commands = (page) => page.locator('.bar.commands');
const fieldsBar = (page) => page.locator('section[data-pane="bar"] .bar');

/** The box's text, whether the caret is in it, and its input mode. */
const state = (page, selector = BAR) =>
	page.evaluate((s) => {
		const input = document.querySelector(s);
		return { text: input.value, focused: document.activeElement === input, mode: input.inputMode };
	}, selector);

const shellHeight = (page) =>
	page.evaluate(() => document.querySelector('.shell').getBoundingClientRect().height);

/** Tap a command button and wait for its line to be focused in `selector`. */
async function tapCommand(page, name, selector = BAR) {
	await commands(page).locator(`button[data-command="${name}"]`).tap();
	await page.waitForFunction(
		([n, s]) => {
			const input = document.querySelector(s);
			return input.value === `${n} ` && document.activeElement === input;
		},
		[name, selector]
	);
}

describe('a command tap focuses the line without the keyboard', () => {
	test('occur: focused, no keyboard, fields filtered, shell unchanged; a field spends it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const height = await shellHeight(page);

			await tapCommand(page, 'occur');
			assert.deepEqual(await state(page), { text: 'occur ', focused: true, mode: 'none' });
			await page.waitForFunction(() => document.querySelectorAll('section[data-pane="bar"] [data-field]').length === 1);
			assert.equal(await shellHeight(page), height, 'the shell moved');

			await fieldsBar(page).locator('[data-field="coffee"]').tap();
			await page.waitForFunction((s) => document.querySelector(s).value === '', BAR);
			await app.idle();
			assert.equal((await app.entries(1))[0].field, 'coffee');

			await page.locator(BAR).tap();
			assert.deepEqual(await state(page), { text: '', focused: true, mode: '' });
		});
	});

	test('a tap on the box brings the keyboard on that first tap', async () => {
		await withApp(async ({ page }) => {
			await tapCommand(page, 'start');
			await page.locator(BAR).tap();
			assert.deepEqual(await state(page), { text: 'start ', focused: true, mode: '' });
		});
	});

	test('update opens the keyboard: what comes next is typing', async () => {
		await withApp(async ({ page }) => {
			await tapCommand(page, 'update');
			assert.deepEqual(await state(page), { text: 'update ', focused: true, mode: '' });
		});
	});

	test('retime with the hid alone fills; the tap after the time pill arrives runs it and empties the line', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...SEED, 'occur coffee -t -60']);
			const [entry] = await app.entries(1);
			const load = (pills) => page.evaluate((all) => window.datum.bar.setPills(all), pills);
			const hid = { id: 'h', kind: 'hid', text: entry.hid };

			await load([hid]);
			await tapCommand(page, 'retime');
			assert.deepEqual(await state(page), { text: 'retime ', focused: true, mode: 'none' });

			// The second pill arrives after the fill; with no Enter on screen
			// the thumb taps retime again, and that tap empties the line.
			await load([hid, { id: 't', kind: 'time', text: '-t -5' }]);
			const echoes = (await app.echoes()).length;
			await commands(page).locator('button[data-command="retime"]').tap();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);
			assert.equal((await app.echoes()).at(-1), `retime ${entry.hid} -t -5`);
			await app.idle();
			const [moved] = await app.entries(1);
			assert.equal(moved.hid, entry.hid);
			assert.notEqual(moved.occur_time, entry.occur_time, 'the entry was not retimed');
			assert.equal((await state(page)).text, '', 'the tap put something on the line');
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), [], 'the pills were not spent');

			// Pills first, on an empty line: ONE tap runs it and the line stays empty.
			await load([hid, { id: 't', kind: 'time', text: '-t -10' }]);
			const before = (await app.echoes()).length;
			await commands(page).locator('button[data-command="retime"]').tap();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), `retime ${entry.hid} -t -10`);
			assert.equal((await state(page)).text, '', 'the single tap put something on the line');
		});
	});

	test('the fields widget’s `add <field> ` opens the keyboard at once', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await fieldsBar(page).locator('[data-field="rec"]').tap();
			await page.waitForFunction((s) => document.querySelector(s).value === 'add rec ', BAR);
			assert.deepEqual(await state(page), { text: 'add rec ', focused: true, mode: '' });
		});
	});

	test('every way out of the quiet state gives the next focus the keyboard', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const refocused = async (why) => {
				await page.evaluate((s) => document.querySelector(s).focus(), BAR);
				assert.equal((await state(page)).mode, '', `still none after ${why}`);
			};

			// Run: a hardware keyboard finishes the line, Enter runs it.
			await tapCommand(page, 'occur');
			await page.keyboard.type('coffee');
			await page.keyboard.press('Enter');
			await page.waitForFunction((s) => document.querySelector(s).value === '', BAR);
			await refocused('a run');

			// Cleared.
			await tapCommand(page, 'occur');
			for (let i = 0; i < 'occur '.length; i++) await page.keyboard.press('Backspace');
			await refocused('a clear');

			// Blur, with the line still pending.
			await tapCommand(page, 'occur');
			await page.evaluate((s) => document.querySelector(s).blur(), BAR);
			assert.equal((await state(page)).mode, '', 'still none after a blur');

			// The verb spent by a field.
			await tapCommand(page, 'occur');
			await fieldsBar(page).locator('[data-field="coffee"]').tap();
			await page.waitForFunction((s) => document.querySelector(s).value === '', BAR);
			await refocused('the verb was spent');
		});
	});

	test('an input widget on the page follows the same rule', async () => {
		const WIDGET = '.input-widget input';
		await withApp(
			async ({ page }) => {
				await tapCommand(page, 'occur', WIDGET);
				assert.deepEqual(await state(page, WIDGET), { text: 'occur ', focused: true, mode: 'none' });
				await page.locator(WIDGET).tap();
				assert.equal((await state(page, WIDGET)).mode, '');
			},
			PHONE,
			page_(widgetPane('in', 'input'))
		);
	});

	test('desktop: a command click focuses the line and typing works', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await commands(page).locator('button[data-command="occur"]').click();
			await page.waitForFunction((s) => document.activeElement === document.querySelector(s), BAR);
			await page.keyboard.type('coffee');
			await page.keyboard.press('Enter');
			await page.waitForFunction((s) => document.querySelector(s).value === '', BAR);
			await app.idle();
			assert.equal((await app.entries(1))[0].field, 'coffee');
		}, DESKTOP);
	});
});
