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
import { sharedApp, startApp, stopApp, withApp as withAnyApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, draft, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { atRows, axes, boxes, gaps, lastReached, listed, packed } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** Ten past nine, so `-t -60` and `-t -30` land on the same day. */
const AT = Date.UTC(2026, 8, 10, 9, 10);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const withApp = (fn, options = {}) =>
	withAnyApp(fn, {
		...DESKTOP,
		clock: AT,
		...options,
		prepare: (context) => context.addInitScript(draft, PAGE)
	});

// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs), the
// clock held at `AT` throughout and `main` opened from `PAGE` for each test.
// The bar's line is the shell's and not the layout's, so `solo` leaves what
// the test before typed in it: it is put back as a fresh page has it —
// empty, unfocused, no input mode. What navigates (`/command`) or moves the
// clock (Undo) keeps an app of its own.
const shared = sharedApp({ ...DESKTOP, clock: AT });

const onPage = (fn) =>
	shared.solo(async (app) => {
		await app.page.evaluate(() => {
			const el = document.querySelector('.prompt-bar input');
			el.value = '';
			el.dispatchEvent(new Event('input', { bubbles: true }));
			el.removeAttribute('inputmode');
			el.blur();
		});
		return fn(app);
	}, PAGE);

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
const drawn = async (page) => (await listed(commands(page), 'button[data-command]')).map((b) => b.text);

/**
 * The fields the fields widget is showing, by name: WHICH fields a verb
 * leaves is the claim here — the order they come in is `fields.test.mjs`'s.
 */
const showing = async (page) => (await listed(fieldsBar(page), '[data-field]')).map((b) => b.field).sort();

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
		await onPage(async ({ page }) => {
			const buttons = await drawn(page);
			assert.deepEqual(buttons.slice(0, 8), [
				'Undo',
				'↷',
				'Help',
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
		await onPage(async (app) => {
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
			await page.locator('dialog.spec').waitFor();
			assert.equal(
				await page.locator('dialog.spec button[data-verb="occur"]').getAttribute('aria-checked'),
				'true'
			);
			await page.keyboard.press('Escape');
			await page.locator('dialog.spec').waitFor({ state: 'detached' });

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
		await onPage(async (app) => {
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
			await page.locator('dialog.spec').waitFor();
			assert.equal(
				await page.locator('dialog.spec button[data-verb="start"]').getAttribute('aria-checked'),
				'true'
			);
		});
	});

	test('the word typed into the line filters exactly as the button does', async () => {
		await onPage(async (app) => {
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
	// vividn, 2026-09-22 (da-vu7pi): «we should always maintain horizontal
	// scrolling in the commands widget, so when it is tall enough, do three
	// rows that scroll horizontally» — and each row packs on its own.
	test('one, two and three rows high draw that many rows, each packed, all reached sideways; taller stays three', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);

			for (const want of [1, 2, 3]) {
				await atRows(page, 'cmd', want);
				assert.deepEqual(await axes(commands(page)), {
					x: 'auto',
					y: 'hidden',
					display: 'flex',
					rows: want,
					fits: true
				});
				const drawn = await boxes(commands(page), 'button[data-command]');
				assert.deepEqual(drawn.filter((b) => !b.inside).map((b) => b.name), [], `${want} rows: clipped`);
				// Packed by ITEM: Undo and its redo icon are one item, joined
				// with no gap between them (da-zq91a).
				packed(await gaps(commands(page), '.lane > *'), `${want} rows`);
				if (want === 2) {
					// Dealt by index: Undo and Help start the two rows (da-vjwid),
					// occur follows Undo and start follows Help.
					const at = Object.fromEntries(drawn.map((b) => [b.name, b]));
					assert.deepEqual(
						[
							at.redo.line === at.undo.line && at.redo.left > at.undo.left,
							at.undo.left === at.help.left,
							at.undo.line < at.help.line,
							at.occur.line === at.undo.line && at.occur.left > at.redo.left,
							at.start.line === at.help.line && at.start.left > at.help.left
						],
						[true, true, true, true, true],
						'the rows are not Undo ↷ occur … over Help start …'
					);
				}
				// And the rest really is reached by scrolling, not by spilling —
				// one row is always crowded; at three, this width fits them all.
				if (want === 1)
					assert.ok(
						await commands(page).evaluate((bar) => bar.scrollWidth > bar.clientWidth + 1),
						'every command already fits: nothing is being scrolled to'
					);
				assert.ok(await lastReached(commands(page), 'button[data-command]'), `${want} rows: the last command is not reached`);

				// The fields widget alike — the same component, the same rule.
				const short = await axes(fieldsBar(page));
				assert.deepEqual([short.x, short.y, short.display, short.fits], ['auto', 'hidden', 'flex', true]);
			}

			// Room for far more than three: still three, still sideways.
			const { width, height } = page.viewportSize();
			await page.setViewportSize({ width, height: height + 400 });
			const tall = await until(
				async () => ({
					...(await axes(commands(page))),
					room: await commands(page).evaluate((bar) => bar.clientHeight / bar.parentElement.querySelector('.probe button').offsetHeight)
				}),
				(t) => t.room > 4.5 && t.rows === 3
			);
			assert.deepEqual([tall.x, tall.y, tall.display, tall.fits], ['auto', 'hidden', 'flex', true]);
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
			await app.clock.set(AT + 11 * 60_000);
			await commands(page).locator('button[data-command="undo"]').click();
			await page.waitForFunction(() => window.__datumInflight === 0);
			assert.equal((await app.entries(1)).length, 0, 'the older change was refused');
			assert.match((await app.outputs()).join('\n'), /warning: undoing a change from 11 minutes ago/);
		});
	});

	// da-zq91a (vividn: «make the undo button … have a little redo section
	// … only enabled when there is an undo to redo»). Lit by the CLI's
	// `redo --dry-run`, so the page holds no rule of its own — and an undo
	// it did not run, in another tab, lights it as well.
	test('its redo icon is lit exactly when the CLI has an undo to redo', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const undo = commands(page).locator('button[data-command="undo"]');
			const redo = commands(page).locator('button[data-command="redo"]');
			/** Wait for the icon to settle lit or not, and say what it says. */
			const lit = async (on) => {
				await page.waitForFunction(
					(want) => !document.querySelector('.bar.commands [data-command="redo"]').disabled === want,
					on
				);
				await app.idle();
				return { label: await redo.getAttribute('aria-label'), title: await redo.getAttribute('title') };
			};
			const press = async (button) => {
				await echoed(app, () => button.click());
				await app.idle();
			};

			await seedLines(app, ['spec set coffee --verb occur', 'occur coffee']);
			assert.deepEqual(await lit(false), { label: 'redo', title: 'nothing to redo' });
			assert.equal(await undo.getAttribute('aria-label'), 'undo');

			// Tapped while unlit, it runs nothing.
			const echoes = (await app.echoes()).length;
			await redo.click({ force: true });
			await app.idle();
			assert.equal((await app.echoes()).length, echoes, 'the unlit redo ran something');

			await press(undo);
			assert.equal((await app.entries(1)).length, 0);
			assert.equal((await lit(true)).title, 'run redo');

			await press(redo);
			assert.equal((await app.echoes()).at(-1), 'redo');
			assert.equal((await app.entries(1))[0].field, 'coffee', 'the redo did not bring coffee back');
			await lit(false);

			// An undo, then a new log: the new change is the one on top.
			await press(undo);
			await lit(true);
			await app.clock.forward(60_000);
			await app.submit('occur coffee');
			await app.idle();
			await lit(false);

			// An undo from ANOTHER tab, on the seam and not the button, lights it here.
			const other = await app.newTab();
			assert.equal((await other.run(['undo'])).code, 0);
			await lit(true);
		});
	});
});
