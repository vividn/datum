// The commands widget, in a browser (da-ltux): the buttons as they are
// drawn, a verb filling the command line and filtering the fields widget,
// a field tap running at once, the `+` at the end, the scroll rule, and
// Undo.
//
// The rules themselves are `commands-pure.test.mjs`; this file is about
// what a reader does and sees. The page it does it on is a column of the
// two widgets over a REPL, written down before the app loads: both
// widgets have to be on screen, since the whole behaviour is one talking
// to the other through the command line they share, and the REPL is where
// a press leaves its echo.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp as withAnyApp, seedLines, workerClock } from './support/app.mjs';
import { DESKTOP, along, draft, widgetPane } from './support/layout.mjs';
import { atRows, axes, boxes, lastReached } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** Ten past nine, so `-t -60` and `-t -30` land on the same day. */
const AT = Date.UTC(2026, 8, 10, 9, 10);
const clock = workerClock(AT);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const withApp = (fn, options = {}) =>
	withAnyApp(fn, {
		...DESKTOP,
		...options,
		prepare: async (context) => {
			await clock.prepare(context);
			await context.addInitScript(draft, PAGE);
		}
	});

/**
 * One field of every shape, and two of them running.
 *
 * `work` is an open block and `mood` a switch in a state, so `end` has
 * something to show and `start` has something to hide; `light` is a
 * switch with `true` among its states, which is the other half of
 * `start`'s rule.
 */
const SEED = [
	'spec set coffee --verb occur',
	'spec set sleep --verb start',
	'spec set work --verb start',
	'spec set mood --verb switch --state good --state bad',
	'spec set light --verb switch --state true --state false',
	'spec set rec --verb add',
	'start work -t -60',
	'switch mood good -t -30'
];

const commands = (page) => page.locator('.bar.commands');
const fieldsBar = (page) => page.locator('section[data-pane="bar"] .bar');

/** The commands widget's buttons, in the order it draws them. */
const drawn = (page) =>
	commands(page).evaluate((bar) =>
		[...bar.querySelectorAll('button[data-command]')].map((b) => b.textContent.trim())
	);

/** The fields the fields widget is showing, in the CLI's own order. */
const showing = (page) =>
	fieldsBar(page).evaluate((bar) =>
		[...bar.querySelectorAll('[data-field]')].map((b) => b.dataset.field)
	);

/**
 * Where a button sits in the bar's reading order: the line it is on and
 * how far along it.
 *
 * The LINE is the middle of its box, not its top: the bar centres what it
 * lays out, and a record's underlined name is shorter than a chip — so
 * two buttons side by side have different tops and the same centre.
 */
const place = (locator) =>
	locator.evaluate((el) => {
		const box = el.getBoundingClientRect();
		return { line: Math.round(box.top + box.height / 2), along: Math.round(box.left) };
	});

/** Run `act` and wait for the REPL to echo the command it started. */
async function echoed(app, act) {
	const before = await app.page.locator('.repl .echo').count();
	await act();
	await app.page.waitForFunction(
		(n) => document.querySelectorAll('.repl .echo').length > n,
		before
	);
}

/** What is typed in the bottom bar's command line, and whether the caret is in it. */
const line = (page) =>
	page.evaluate(() => {
		const input = document.querySelector('.prompt-bar input');
		return { text: input.value, focused: document.activeElement === input };
	});

/** Press a command button and wait for the line to say so. */
async function tap(page, name) {
	await commands(page).locator(`button[data-command="${name}"]`).click();
	await page.waitForFunction(
		(n) => document.querySelector('.prompt-bar input').value === `${n} `,
		name
	);
}

describe('the bar of commands', () => {
	test('Undo leads, the verbs follow, and note is not among them', async () => {
		await withApp(async ({ page }) => {
			const buttons = await drawn(page);
			assert.deepEqual(buttons.slice(0, 7), [
				'Undo',
				'Redo',
				'occur',
				'start',
				'end',
				'switch',
				'add'
			]);
			assert.equal(buttons.includes('note'), false, 'note is typed, not pressed (da-py5w, da-e78x6)');
			assert.ok(buttons.includes('sync'), 'the tail of the list is drawn too');
		});
	});

	test('its own layout opens on its name', async () => {
		await withApp(async (app) => {
			await app.goto('/command');
			await commands(app.page).waitFor();
			assert.deepEqual(
				await app.page.locator('section.pane').evaluateAll((panes) => panes.map((p) => p.dataset.title)),
				['command']
			);
		});
	});
});

describe('a verb fills the line and filters the fields', () => {
	test('occur leaves the occur fields, and a tap on one runs at once', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);

			await tap(page, 'occur');
			assert.deepEqual(await line(page), { text: 'occur ', focused: true });
			assert.deepEqual(await showing(page), ['coffee']);

			// The `+` at the END of the filtered list: after the last field a
			// reader reads, where the whole list has `Track +` before the first.
			const plus = await place(fieldsBar(page).locator('[data-add-field]'));
			const coffee = await place(fieldsBar(page).locator('[data-field="coffee"]'));
			assert.ok(
				plus.line > coffee.line || (plus.line === coffee.line && plus.along > coffee.along),
				'the + is not after the fields'
			);
			assert.equal(await fieldsBar(page).locator('button.track').count(), 0, 'Track + is still there');

			// And it is the Track modal (da-94q1) with the pending verb already
			// chosen — the door that presets one, which `startTracking` leaves
			// to whoever has a verb in hand.
			await fieldsBar(page).locator('[data-add-field]').click();
			await page.locator('dialog.track').waitFor();
			assert.equal(
				await page.locator('dialog.track button[data-verb="occur"]').getAttribute('aria-checked'),
				'true'
			);
			await page.keyboard.press('Escape');
			await page.locator('dialog.track').waitFor({ state: 'detached' });

			await echoed(app, () => fieldsBar(page).locator('[data-field="coffee"]').click());
			assert.equal((await app.echoes()).at(-1), 'occur coffee');
			assert.equal((await line(page)).text, '', 'the verb was not spent');
			// The verb is spent: the list is whole again.
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);
			await app.idle();
			assert.equal((await app.entries(1))[0].field, 'coffee');
		});
	});

	test('start, end, switch and add each leave what they can be spent on', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);

			// A block that is off, and a switch with `true` among its states.
			// `work` is running, so it is not something to start.
			await tap(page, 'start');
			assert.deepEqual(await showing(page), ['light', 'sleep']);

			// The ones that are on, by `now`'s truth and nothing remembered.
			await tap(page, 'end');
			assert.deepEqual(await showing(page), ['mood', 'work']);

			await tap(page, 'switch');
			assert.deepEqual(await showing(page), ['light', 'mood']);

			await tap(page, 'add');
			assert.deepEqual(await showing(page), ['rec']);

			// `end` is not a verb a field can be BORN with, so the `+` under it
			// opens the modal on `start`: a block you are ending is a block.
			await tap(page, 'end');
			await fieldsBar(page).locator('[data-add-field]').click();
			await page.locator('dialog.track').waitFor();
			assert.equal(
				await page.locator('dialog.track button[data-verb="start"]').getAttribute('aria-checked'),
				'true'
			);
		});
	});

	test('the word typed into the line filters exactly as the button does', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const input = page.locator('.prompt-bar input');

			await input.fill('switch ');
			await page.waitForFunction(
				() => document.querySelectorAll('section[data-pane="bar"] .bar [data-field]').length === 2
			);
			assert.deepEqual(await showing(page), ['light', 'mood']);

			// `switch <field>` has no state in it, so the field asks which —
			// and the state it is given runs the pending verb (da-g635).
			await fieldsBar(page).locator('[data-field="mood"]').click();
			await echoed(app, () => fieldsBar(page).locator('[data-state="bad"]').click());
			assert.equal((await app.echoes()).at(-1), 'switch mood bad');
			assert.equal((await line(page)).text, '', 'the verb was not spent');

			// And a word that is not one of the five pends nothing.
			await input.fill('tail ');
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);
		});
	});
});

describe('the scroll rule', () => {
	test('one row high draws one row and two draw two, both reached sideways; three wraps and is reached down', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);

			for (const want of [1, 2]) {
				await atRows(page, 'cmd', want);
				assert.deepEqual(await axes(commands(page)), {
					x: 'auto',
					y: 'hidden',
					display: 'grid',
					rows: want,
					fits: true
				});
				const drawn = await boxes(commands(page), 'button[data-command]');
				assert.deepEqual(drawn.filter((b) => !b.inside).map((b) => b.name), [], `${want} rows: clipped`);
				if (want === 2) {
					// Column-major: Undo over Redo, then occur over start.
					const [undo, redo, occur, start] = drawn;
					assert.deepEqual(
						[undo.left === redo.left, undo.line < redo.line, occur.left === start.left, occur.line < start.line],
						[true, true, true, true],
						'the first two columns are not Undo/Redo and occur/start'
					);
					assert.ok(occur.left > undo.left, 'occur is not right of Undo');
				}
				// And the rest really is reached by scrolling, not by spilling.
				assert.ok(
					await commands(page).evaluate((bar) => bar.scrollWidth > bar.clientWidth + 1),
					'every command already fits: nothing is being scrolled to'
				);
				assert.ok(await lastReached(commands(page), 'button[data-command]'), `${want} rows: the last command is not reached`);

				// The fields widget alike — the same rule, the same box.
				const short = await axes(fieldsBar(page));
				assert.deepEqual([short.x, short.y, short.display, short.fits], ['auto', 'hidden', 'grid', true]);
			}

			// Narrow enough that the commands need three lines to wrap onto.
			await page.setViewportSize({ width: 390, height: page.viewportSize().height });
			await atRows(page, 'cmd', 3);
			const tall = await axes(commands(page));
			assert.deepEqual([tall.x, tall.y, tall.display, tall.fits], ['hidden', 'auto', 'flex', true]);
			assert.ok(tall.rows >= 3, `the commands drew ${tall.rows} rows`);
			const fields = await axes(fieldsBar(page));
			assert.deepEqual([fields.x, fields.y, fields.display, fields.fits], ['hidden', 'auto', 'flex', true]);
		});
	});
});

describe('Undo', () => {
	// da-s6no took the fifteen-minute gate off undo — with a way forward it
	// cost more than it saved — and left a warning past ten minutes. So
	// there is no second clock in the web: the widget runs `undo`, and
	// whatever the CLI says about the age comes back with the echo.
	test('runs at once, and past ten minutes runs anyway and says how old', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur', 'occur coffee']);
			assert.equal((await app.entries(1))[0].field, 'coffee');

			await commands(page).locator('button[data-command="undo"]').click();
			await page.waitForFunction(() => window.__datumInflight === 0);
			assert.equal((await app.echoes()).at(-1), 'undo');
			assert.equal((await app.entries(1)).length, 0, 'the entry was not reverted');
			assert.equal(
				(await app.outputs()).join('\n').includes('warning:'),
				false,
				'a fresh change was warned about'
			);

			// Eleven minutes on: it still reverts, and the CLI's warning is shown.
			await app.submit('occur coffee');
			await clock.set(page, AT + 11 * 60_000);
			await commands(page).locator('button[data-command="undo"]').click();
			await page.waitForFunction(() => window.__datumInflight === 0);
			assert.equal((await app.entries(1)).length, 0, 'the older change was refused');
			assert.match((await app.outputs()).join('\n'), /warning: undoing a change from 11 minutes ago/);
		});
	});
});
