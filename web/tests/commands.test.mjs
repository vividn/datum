// The commands widget, in a browser (da-ltux): the buttons as they are
// drawn, a verb filling the command line and filtering the collect widget,
// a field tap running at once, the `+` at the end, the scroll rule, and
// Undo.
//
// The rules themselves are `commands-pure.test.mjs`; this file is about
// what a reader does and sees. The page it does it on is a column of the
// two widgets over a CLI widget, written down before the app loads: both
// widgets have to be on screen, since the whole behaviour is one talking
// to the other through the command line they share, and the CLI widget is where
// a press leaves its echo.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp as withAnyApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, draft, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { atRows, axes, boxes, gaps, lastReached, listed, packed } from './support/rows.mjs';
import { reach } from './support/commands.mjs';
import { BASE, MENUS } from '../src/lib/commands.ts';

before(startApp);
after(stopApp);

/** Ten past nine, so `-t -60` and `-t -30` land on the same day. */
const AT = Date.UTC(2026, 8, 10, 9, 10);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'cli'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
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
 * a switch to show and `start` one to hide; `sleep` is a stopped block,
 * which both still show (da-vpzbv); `light` is a switch with `true` among
 * its states, which is the other half of `start`'s rule.
 */
const SEED = [
	'setup coffee --verb moment',
	'setup sleep --verb start',
	'setup work --verb start',
	'setup mood --verb switch --states good,bad',
	'setup light --verb switch --states true,false',
	'setup rec --verb add',
	'start work -t -60',
	'switch mood good -t -30'
];

const commands = (page) => page.locator('main .bar.commands');
const fieldsBar = (page) => page.locator('section[data-pane="bar"] .bar');

/** The commands widget's buttons, in the order it draws them. */
const drawn = async (page) => (await listed(commands(page), 'button[data-command]')).map((b) => b.text);

/**
 * The fields the collect widget is showing, by name: WHICH fields a verb
 * leaves is the claim here — the order they come in is `fields.test.mjs`'s.
 */
const showing = async (page) => (await listed(fieldsBar(page), '[data-field]')).map((b) => b.field).sort();

/** Run `act` and wait for the CLI widget to echo the command it started. */
async function echoed(app, act) {
	const before = await app.page.locator('.cli .echo').count();
	await act();
	await app.page.waitForFunction(
		(n) => document.querySelectorAll('.cli .echo').length > n,
		before
	);
}

/** What is typed in the bottom bar's command line, and whether the caret is in it. */
const line = (page) =>
	page.evaluate(() => {
		const input = document.querySelector('.prompt-bar input');
		return { text: input.value, focused: document.activeElement === input };
	});

/** Load these pills onto the bar, in place of whatever it held. */
const load = (page, pills) => page.evaluate((all) => window.datum.bar.setPills(all), pills);

/** Press a command button and wait for the line to say so. */
async function tap(page, name) {
	await (await reach(commands(page), name)).click();
	await page.waitForFunction(
		(n) => document.querySelector('.prompt-bar input').value === `${n} `,
		name
	);
}

/** Each of the bar's circles as drawn: what it is, the name under it, and where both stand. */
const circles = (page) =>
	commands(page).evaluate((bar) => {
		// Dealt back into the widget's order: the first of every row, then the second (`listed` in support/rows.mjs).
		const lanes = [...bar.querySelectorAll('.lane')].map((lane) => [...lane.querySelectorAll('button.round')]);
		const dealt = [];
		for (let k = 0; lanes.some((lane) => k < lane.length); k += 1) for (const lane of lanes) if (k < lane.length) dealt.push(lane[k]);
		return dealt.map((b) => {
			const box = b.getBoundingClientRect();
			const name = b.querySelector('.name');
			const under = name.getBoundingClientRect();
			const style = getComputedStyle(b);
			return {
				is: b.dataset.command ?? (b.dataset.menu ? `menu ${b.dataset.menu}` : 'back'),
				name: name.textContent,
				round: style.borderRadius === '50%' && Math.abs(box.width - box.height) < 0.5,
				under: under.top >= box.bottom && Math.abs(under.left + under.width / 2 - (box.left + box.width / 2)) < 1,
				top: Math.round(box.top),
				right: box.right,
				left: box.left
			};
		});
	});

/** The base row as the widget lists it (`BASE`): a submenu by `menu <name>`. */
const ROW = BASE.map((c) => (c.press === 'menu' ? `menu ${c.name}` : c.name));

describe('the bar of commands', () => {
	// da-fg4w4. vividn, 2026-11-03: «Let's make the command buttons circular
	// with an icon in the circle and the name of the command beneath them»,
	// and the base: «undo write read setup kit db sync settings».
	test('the base row is eight circles in one row, each named beneath; no nowview, dayview, ids or conflicts', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const fits = await commands(page).evaluate((el) => Math.floor((el.clientHeight + 6.4) / (el.parentElement.querySelector('.probe').offsetHeight + 6.4)));
			assert.ok(fits >= 3, `the box fits ${fits} rows`);
			const row = await circles(page);
			assert.deepEqual(
				row.map((c) => c.is),
				['undo', 'menu write', 'menu read', 'menu fields', 'kit', 'db', 'sync', 'menu settings']
			);
			assert.deepEqual(row.map((c) => c.name), ['undo', 'write', 'read', 'fields', 'kit', 'db', 'sync', 'settings']);
			assert.deepEqual(row.filter((c) => !c.round || !c.under).map((c) => c.is), [], 'not a circle with its name under it');
			assert.equal(new Set(row.map((c) => c.top)).size, 1, 'with room for three rows, the base row is still one row');
			assert.equal(await commands(page).locator('button[data-command="redo"]').count(), 1, 'undo keeps its redo');
			for (const name of ['nowview', 'dayview', 'ids', 'conflicts', 'view'])
				assert.equal(await page.locator(`.bar.commands [data-command="${name}"]`).count(), 0, name);
		});
	});

	// vividn: «Write and read expand into a submenu when clicked of the various commands inside of them.»
	for (const menu of ['write', 'read', 'fields', 'settings'])
		test(`${menu} opens its submenu in the row's place, back first, and back comes back`, async () => {
			await onPage(async ({ page }) => {
				await commands(page).locator(`button[data-menu="${menu}"]`).click();
				const want = ['back', ...MENUS[menu].map((c) => c.name)];
				await until(async () => (await circles(page)).map((c) => c.is), (is) => is.join() === want.join());
				assert.ok((await circles(page)).every((c) => c.round && c.under));
				await commands(page).locator('button[data-back]').click();
				await until(async () => (await circles(page)).map((c) => c.is), (is) => is.join() === ROW.join());
				// Escape is back too, and a command chosen there takes the bar back to the row.
				await commands(page).locator(`button[data-menu="${menu}"]`).click();
				await commands(page).locator('button[data-back]').waitFor();
				await page.keyboard.press('Escape');
				await commands(page).locator('button[data-back]').waitFor({ state: 'detached' });
				if (menu === 'read') {
					await tap(page, 'tail');
					assert.deepEqual((await circles(page)).map((c) => c.is), ROW, 'a command chosen takes the bar back to the base row');
				}
			});
		});

	// vividn: «When a pill is loaded, there is still a filtering action which
	// then only shows the available commands» — every one that takes it,
	// whichever submenu it is in, with no submenu of its own.
	test('with an hid pill, only the commands that take an hid, flat, and the row again once it goes', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const [entry] = await app.entries(1);
			await load(page, [{ id: 'hid', kind: 'hid', text: entry.hid }]);
			const takes = ['help', 'update', 'retime', 'delete', 'revert', 'get', 'history'];
			const flat = await until(async () => (await circles(page)).map((c) => c.is), (is) => is.includes('delete'));
			assert.deepEqual(flat.filter((is) => !takes.includes(is)), [], 'a command that takes no hid');
			assert.deepEqual(takes.filter((is) => !flat.includes(is)), [], 'a command that takes an hid is missing');
			assert.equal(await commands(page).locator('button[data-menu]').count(), 0, 'a submenu under a pill');
			await load(page, []);
			await until(async () => (await circles(page)).map((c) => c.is), (is) => is.join() === ROW.join());
		});
	});

	// da-y6jfb. The icon is DRAWN — a glyph by CSS, a drawing with ink in
	// it — and the button's text is still its word; `track` is the one
	// command in a color of its own.
	test('every command wears its mark in its circle, no two alike, and track alone is green', async () => {
		await onPage(async ({ page }) => {
			const looks = {};
			const read = () =>
				commands(page).evaluate((bar) =>
					[...bar.querySelectorAll('button.round:not([data-back])')].map((b) => {
						const path = b.querySelector('svg path');
						const glyph = getComputedStyle(b, '::before').content;
						return [
							b.dataset.command ?? `menu ${b.dataset.menu}`,
							{
								icon: path ? (path.getBoundingClientRect().width > 4 ? path.getAttribute('d') : 'no ink') : glyph,
								text: b.textContent.trim(),
								color: getComputedStyle(b).color,
								border: getComputedStyle(b).borderTopColor
							}
						];
					})
				);
			Object.assign(looks, Object.fromEntries(await read()));
			for (const menu of ['write', 'read', 'fields', 'settings']) {
				await commands(page).locator(`button[data-menu="${menu}"]`).click();
				await commands(page).locator('button[data-back]').waitFor();
				Object.assign(looks, Object.fromEntries(await read()));
				await commands(page).locator('button[data-back]').click();
			}
			const glyphs = { undo: '↶', moment: '●', on: '▬', off: '■', change: '▨', record: '¢', remark: '✎', track: '+' };
			for (const [name, glyph] of Object.entries(glyphs)) assert.ok(looks[name].icon.startsWith(`"${glyph}"`), `${name} draws ${looks[name].icon}`);
			for (const name of ['delete', 'retime', 'tail', 'stats', 'sync', 'menu write']) assert.match(looks[name].icon, /^M/, `${name} draws ${looks[name].icon}`);
			const icons = Object.values(looks).map((l) => l.icon);
			assert.deepEqual(Object.entries(looks).filter(([, l]) => l.icon === 'none' || l.icon === 'no ink').map(([name]) => name), [], 'a circle with no mark');
			assert.equal(new Set(icons).size, icons.length, 'two commands draw the same icon');
			assert.equal(looks.tail.text, 'tail', 'the mark is no part of its name');

			// The green the CLI paints a new row's tag (`Outcome::Create`).
			const GREEN = 'rgb(38, 162, 105)';
			assert.deepEqual([looks.track.color, looks.track.border], [GREEN, GREEN]);
			const others = Object.entries(looks).filter(([name, l]) => name !== 'track' && (l.color === GREEN || l.border === GREEN));
			assert.deepEqual(others, [], 'track is not the only green command');
		});
	});

	// vividn, 13:28: «track shouldn't open the setup widget, it should just put the word track in the command line.»
	test('track writes its word on the line, and opens no setup widget', async () => {
		await onPage(async ({ page }) => {
			await tap(page, 'track');
			assert.equal(await page.locator('.host[data-widget="setup"]').count(), 0, 'track opened the setup widget');
		});
	});
});

// da-803mk. vividn, 2026-09-25: «for commands with subcommands, like db,
// clicking on them should then show the subcommands in the commands widget
// instead.» Which are subcommands is `datum help --json`'s to say.
describe('a command with subcommands drills into them', () => {
	const back = (page) => commands(page).locator('button[data-back]');
	const DB = ['purge', 'backup', 'restore', 'migrate', 'v2import', 'config', 'rename'];

	test('db shows its subcommands under back, and migrate fills `db migrate `', async () => {
		await onPage(async ({ page }) => {
			await commands(page).locator('button[data-command="db"]').click();
			await back(page).waitFor();
			assert.equal((await back(page).innerText()).trim(), 'back');
			assert.deepEqual(await drawn(page), ['help', ...DB], 'Help stays, to explain a subcommand');
			assert.equal((await line(page)).text, '', '`db ` runs nothing, so a tap never writes it');

			await tap(page, 'db migrate');
			assert.equal(await back(page).count(), 1, 'a fill stays among the subcommands');
			// It takes the new name, so it fills and runs nothing (da-o7ay8).
			await tap(page, 'db rename');
		});
	});

	// da-9ow8m: vividn, «the "backup" command in the commands widget … should
	// that open this backups menu directly? It could». So `db backup` opens
	// the Backups place (da-ea5q2), and `db restore` opens it at the backups
	// to restore from; neither writes the line, and both are drawn once-outlined.
	for (const sub of ['backup', 'restore']) {
		test(`db ${sub} opens «Backups», and writes nothing on the line`, async () => {
			await onPage(async ({ page }) => {
				await commands(page).locator('button[data-command="db"]').click();
				const button = commands(page).locator(`button[data-command="db ${sub}"]`);
				assert.match(await button.getAttribute('class'), /\bopens\b/);
				await button.click();
				// A tab or a modal, as the screen has it (da-jozgv): backups.test.mjs holds the tab.
				await page.locator('section.pane[data-title="backups"] .backups').waitFor();
				assert.equal((await line(page)).text, '', `db ${sub} wrote the line`);
			});
		});
	}

	test('back and Escape each return to the base row, and Escape peels only that', async () => {
		await onPage(async ({ page }) => {
			await commands(page).locator('button[data-command="db"]').click();
			await back(page).click();
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('kit'), 'back is the base row');

			await commands(page).locator('button[data-command="db"]').click();
			await tap(page, 'db migrate');
			await page.keyboard.press('Escape');
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('db'));
			assert.equal((await line(page)).text, 'db migrate ', 'the same Escape peeled the line too');
		});
	});

	test('a command without subcommands still fills as before', async () => {
		await onPage(async ({ page }) => {
			await tap(page, 'tail');
			assert.equal(await back(page).count(), 0);
		});
	});
});

// da-st2xl. vividn, 2026-09-25: «long pressing on a command, like moment
// should add it to the input box as well, and then display various flags
// that you can add, like -q, -t, etc, sorted in a useful order … for -t
// keyboard should be opened on mobile». A REAL long press: the timers are
// the page's own, so the hold is held for as long as a finger would.
describe('holding a command fills it and shows its flags', () => {
	const back = (page) => commands(page).locator('button[data-back]');
	const flags = async (page) => (await listed(commands(page), 'button[data-flag]')).map((b) => b.text);

	async function hold(page, name) {
		const box = await (await reach(commands(page), name)).boundingBox();
		await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
		await page.mouse.down();
		// Lifted once the hold has fired, by what it drew: a lift timed on
		// Node's clock can beat the page's own timer to it (da-mtsgy). Its
		// flags, as a submenu draws back too.
		await commands(page).locator('button[data-flag]').first().waitFor();
		await page.mouse.up();
	}

	/** What the line holds, whether the caret is in it, and whether a phone's keyboard would be up. */
	const typed = (page) =>
		page.evaluate(() => {
			const input = document.querySelector('.prompt-bar input');
			return { text: input.value, focused: document.activeElement === input, keyboard: input.inputMode !== 'none' };
		});

	test('moment fills `moment ` and its flags follow Back and Help, the curated ones first', async () => {
		await onPage(async ({ page }) => {
			await hold(page, 'moment');
			assert.equal((await typed(page)).text, 'moment ', 'the hold fills as the tap does');
			assert.deepEqual(await drawn(page), ['help'], 'back, Help, then flags: no command beside them');
			assert.deepEqual((await flags(page)).slice(0, 7), ['-t', '-d', '-y', '-q', '-c', '-m', '-k']);
			assert.ok((await flags(page)).includes('--id'), 'the rest follow, long-only ones included');
		});
	});

	test('-q appends with the keyboard shut; -t appends and opens it; the line composes', async () => {
		await onPage(async ({ page }) => {
			await hold(page, 'moment');
			await commands(page).locator('button[data-flag="moment -q"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment -q ');
			assert.deepEqual(await typed(page), { text: 'moment -q ', focused: true, keyboard: false });

			await commands(page).locator('button[data-flag="moment -t"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment -q -t ');
			assert.deepEqual(await typed(page), { text: 'moment -q -t ', focused: true, keyboard: true });
		});
	});

	// PR #216 (vremya): the flags outlive the line — a run spends it, a
	// reader types another command — and a flag tap then starts the line
	// afresh with the command whose flags are showing.
	test('after a run, or under another command, a flag tap starts `moment -q ` again', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['setup coffee --verb moment']);
			const q = commands(page).locator('button[data-flag="moment -q"]');
			const value = (v) => page.waitForFunction((v) => document.querySelector('.prompt-bar input').value === v, v);
			await hold(page, 'moment');
			await q.click();
			await value('moment -q ');
			await page.keyboard.type('coffee');
			await page.keyboard.press('Enter');
			await value('');
			assert.equal(await back(page).count(), 1, 'the flags stayed open');
			await q.click();
			await value('moment -q ');

			await page.locator('.prompt-bar input').fill('tail ');
			await q.click();
			await value('moment -q ');
			await app.idle();
		});
	});

	// da-bbmpx: the verb is still pending under `moment -q `, so the fields
	// narrow to moment's — and a tap on one runs the LINE with the field.
	test('a field tap under `moment -q ` runs `moment -q coffee` and spends the line', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['setup coffee --verb moment']);
			await hold(page, 'moment');
			await commands(page).locator('button[data-flag="moment -q"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment -q ');
			await echoed(app, () => fieldsBar(page).locator('[data-field="coffee"]').click());
			assert.equal((await app.echoes()).at(-1), 'moment -q coffee');
			assert.equal((await typed(page)).text, '', 'the line was not spent');
			await app.idle();
		});
	});

	test('back and Escape return to the commands, and leave the line as it is', async () => {
		await onPage(async ({ page }) => {
			// Back to the submenu the command was held in (da-fg4w4), with its own back.
			const flag = commands(page).locator('button[data-flag]').first();
			await hold(page, 'moment');
			await back(page).click();
			await flag.waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('moment'));

			await hold(page, 'moment');
			await page.keyboard.press('Escape');
			await flag.waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('moment'));
			assert.equal((await typed(page)).text, 'moment ', 'the same Escape peeled the line too');
		});
	});

	test('a command without flags is not held', async () => {
		await onPage(async ({ page }) => {
			await reach(commands(page), 'moment');
			assert.equal(await commands(page).locator('button[data-command="delete"][data-holds]').count(), 0);
			assert.equal(await commands(page).locator('button[data-command="moment"][data-holds]').count(), 1);
		});
	});
});

describe('a verb fills the line and filters the fields', () => {
	test('moment leaves the moment fields, and a tap on one runs at once', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);

			await tap(page, 'moment');
			assert.deepEqual(await line(page), { text: 'moment ', focused: true });
			assert.deepEqual(await showing(page), ['coffee']);

			// The fields alone: no `+` ends the filtered list (da-myet1).
			assert.deepEqual(await fieldsBar(page).locator('button').evaluateAll((all) => all.filter((b) => b.textContent.trim() === '+').length), 0);

			await echoed(app, () => fieldsBar(page).locator('[data-field="coffee"]').click());
			assert.equal((await app.echoes()).at(-1), 'moment coffee');
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

			// Every block, running or not (da-vpzbv: a start re-logged is a
			// forgotten one), and a switch with `true` among its states.
			await tap(page, 'on');
			assert.deepEqual(await showing(page), ['light', 'sleep', 'work']);

			// Every block again, and the switches that are on by `now`'s truth.
			await tap(page, 'off');
			assert.deepEqual(await showing(page), ['mood', 'sleep', 'work']);

			await tap(page, 'change');
			assert.deepEqual(await showing(page), ['light', 'mood']);

			await tap(page, 'record');
			assert.deepEqual(await showing(page), ['rec']);
		});
	});

	// da-vpzbv, vividn: «show both start and end commands … regardless of the
	// current state». A block's pill leaves both, running (`work`) or not
	// (`sleep`), and re-logging either runs: that is how a person records a
	// start or an end they forgot.
	test('a block field offers start and end whatever its state, and each runs', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const verbs = async () =>
				(await drawn(page)).filter((name) => ['moment', 'on', 'off', 'change', 'record'].includes(name));

			for (const field of ['work', 'sleep']) {
				await fieldsBar(page).locator(`[data-field="${field}"]`).click({ button: 'right' });
				// The pill's commands, flat (da-fg4w4), in place of the base row.
				await page.waitForFunction(() => document.querySelector('main .bar.commands [data-command="on"]'));
				assert.deepEqual(await verbs(), ['on', 'off'], field);
				await page.locator('.prompt-bar .pill.field button.remove').click();
				await page.waitForFunction(() => document.querySelector('main .bar.commands [data-menu="write"]'));
			}

			await tap(page, 'on');
			await echoed(app, () => fieldsBar(page).locator('[data-field="work"]').click());
			assert.equal((await app.echoes()).at(-1), 'on work');
			await tap(page, 'off');
			await echoed(app, () => fieldsBar(page).locator('[data-field="sleep"]').click());
			assert.equal((await app.echoes()).at(-1), 'off sleep');
			await app.idle();
			assert.deepEqual(
				(await app.entries(2)).map((e) => e.field).sort(),
				['sleep', 'work']
			);
			assert.deepEqual(app.errors, []);
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
			// The old word typed pends `change`, and runs as it was typed.
			assert.equal((await app.echoes()).at(-1), 'switch mood bad');
			assert.equal((await line(page)).text, '', 'the verb was not spent');

			// And a word that is not one of the five pends nothing.
			await input.fill('tail ');
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);
		});
	});
});

// da-lxqqz, vividn: «when a field is selected, then the start/end/moment
// buttons should become double lines and auto trigger entry (unless there
// are extra keys to be filled in, in which case the input line should
// automatically be selected for typing in those fields». Required keys
// only: a key with a default is filled from it and stops nothing.
describe('a verb under a field pill', () => {
	const KEYED = [
		...SEED,
		'setup tea --verb moment --data-key amount',
		'setup juice --verb moment --data-key cups=1',
		'setup shift --verb start --data-key project --data-key n=1',
		'on shift project=a n=1 -t -30'
	];
	const verb = (page, name) => commands(page).locator(`button[data-command="${name}"]`);
	const fieldPill = (page) => page.locator('.prompt-bar .pill.field');

	/** Mint `field`'s pill and wait for the verbs to narrow to it. */
	async function pillOf(page, field) {
		await fieldsBar(page).locator(`[data-field="${field}"]`).click({ button: 'right' });
		await fieldPill(page).waitFor();
		await page.waitForFunction(() => !document.querySelector('[data-command="change"]'));
	}

	test('a field that needs nothing typed: the verb is double-outlined and logs at once, spending the pill', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			await pillOf(page, 'coffee');
			assert.equal(await verb(page, 'moment').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'moment').click());
			assert.equal((await app.echoes()).at(-1), 'moment coffee');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal((await line(page)).text, '');
			await app.idle();
			assert.equal((await app.entries(1))[0].field, 'coffee');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a block field: start and end both run', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			for (const name of ['on', 'off']) {
				await pillOf(page, 'sleep');
				assert.equal(await verb(page, name).getAttribute('data-act'), 'run', name);
				await echoed(app, () => verb(page, name).click());
				assert.equal((await app.echoes()).at(-1), `${name} sleep`);
				await fieldPill(page).waitFor({ state: 'detached' });
				// The clock is held, and a second sleep at the same instant collides (da-ydao).
				await app.clock.forward(60_000);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	test('a required key with no default: single outline, the tap fills the line to the first key and opens the keyboard, nothing is logged', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			await app.idle();
			const before = (await app.allEntries()).length;
			await pillOf(page, 'tea');
			assert.equal(await verb(page, 'moment').getAttribute('data-act'), 'fill');

			await verb(page, 'moment').click();
			// Straight into the first key's frame (da-31d1d), which a bare word fills (da-p3hxn).
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment tea ');
			assert.deepEqual(await line(page), { text: 'moment tea ', focused: true });
			assert.notEqual(await page.locator('.prompt-bar input').getAttribute('inputmode'), 'none', 'the keyboard stayed shut');
			// The pill is on the line now, so it is spent rather than appended a second time.
			await fieldPill(page).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.allEntries()).length, before, 'the tap logged something');

			await echoed(app, () => app.submit('moment tea 2'));
			assert.equal((await app.echoes()).at(-1), 'moment tea 2');
			assert.deepEqual((await app.entries(1))[0].data, { amount: 2 });
			assert.deepEqual(app.errors, []);
		});
	});

	// da-7h73h: a field name may hold a space. The fill quotes it, and so
	// does the echo a press records, which Up recalls as a typed line.
	test('a spaced field under its pill: the fill and the echo quote it, and both log to it', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['setup "my coffee" --verb moment --data-key amount', 'setup "iced tea" --verb moment']);
			await pillOf(page, 'my coffee');
			await verb(page, 'moment').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment "my coffee" ');
			await fieldPill(page).waitFor({ state: 'detached' });
			const filled = (await line(page)).text;
			await echoed(app, () => app.submit(`${filled}2`));
			await app.idle();
			assert.deepEqual((await app.entries(1)).map((e) => [e.field, e.data]), [['my coffee', { amount: 2 }]]);

			await app.clock.forward(60_000);
			await pillOf(page, 'iced tea');
			await echoed(app, () => verb(page, 'moment').click());
			assert.equal((await app.echoes()).at(-1), 'moment "iced tea"');
			await app.idle();
			assert.equal((await app.entries(1))[0].field, 'iced tea');
			assert.deepEqual(app.errors, []);
		});
	});

	// «With the default» is the CLI's to fill, and it does not yet (da-9s98k):
	// the claim here is the web's half, that a default asks nothing of the tap.
	test('only optional keys: it logs at once, nothing typed', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			await pillOf(page, 'juice');
			assert.equal(await verb(page, 'moment').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'moment').click());
			assert.equal((await app.echoes()).at(-1), 'moment juice');
			await app.idle();
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal((await app.entries(1))[0].field, 'juice');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in
	// this one, so I think that should be the default».
	test('end runs at once whatever keys the field declares, and stores none; start still fills', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			await pillOf(page, 'shift');
			assert.equal(await verb(page, 'on').getAttribute('data-act'), 'fill');
			assert.equal(await verb(page, 'off').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'off').click());
			assert.equal((await app.echoes()).at(-1), 'off shift');
			await fieldPill(page).waitFor({ state: 'detached' });
			await app.idle();
			const [entry] = await app.entries(1);
			assert.equal(entry.field, 'shift');
			assert.deepEqual(entry.data ?? {}, {});
			assert.deepEqual(app.errors, []);
		});
	});
});

// da-8ru0m, vividn, 2026-09-27: «when the field pill or hid pills are
// present, the cursor should go automatically to the end after all the
// pills when any command requiring typing is clicked since these pills
// depend on positionality». A pill left loaded loses its slot to the first
// word typed, so `note ` under a coffee pill logged a field called `too`.
describe('a typing command under field and hid pills', () => {
	const input = (page) => page.locator('.prompt-bar input');
	const pill = (page, kind) => page.locator(`.prompt-bar .pill.${kind}`);
	/** The line, whether it has the caret, and where the caret is. */
	const caret = (page) =>
		input(page).evaluate((el) => ({ text: el.value, focused: document.activeElement === el, at: el.selectionStart }));

	test('remark under a coffee pill writes `remark coffee ` with the caret last; the words land after the field', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await fieldsBar(page).locator('[data-field="coffee"]').click({ button: 'right' });
			await pill(page, 'field').waitFor();

			await commands(page).locator('button[data-command="remark"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'remark coffee ');
			assert.deepEqual(await caret(page), { text: 'remark coffee ', focused: true, at: 'remark coffee '.length });
			await pill(page, 'field').waitFor({ state: 'detached' });

			await input(page).pressSequentially('too strong');
			await echoed(app, () => input(page).press('Enter'));
			assert.equal((await app.echoes()).at(-1), 'remark coffee too strong');
			await app.idle();
			const all = await app.allEntries();
			// A note is timeless (da-r17h), so it is found, not taken from the end.
			const notes = all.filter((e) => /too strong/.test(JSON.stringify(e.data ?? {})));
			assert.deepEqual(notes.map((e) => e.field), ['coffee']);
			assert.equal(all.filter((e) => e.field === 'too').length, 0, 'a field called too was logged');
			assert.deepEqual(app.errors, []);
		});
	});

	test('with no pills, a typing command writes its name alone, as it always did', async () => {
		await onPage(async ({ page }) => {
			await tap(page, 'remark');
			assert.deepEqual(await caret(page), { text: 'remark ', focused: true, at: 7 });
		});
	});

	test('update under an hid writes `update <hid> ` with the caret last; a field beside it is context and stays', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const [entry] = await app.entries(1);
			await load(page, [
				{ id: 'field', kind: 'field', text: 'mood' },
				{ id: 'hid', kind: 'hid', text: entry.hid }
			]);
			await commands(page).locator('button[data-command="update"]').click();
			const want = `update ${entry.hid} `;
			await page.waitForFunction((w) => document.querySelector('.prompt-bar input').value === w, want);
			assert.deepEqual(await caret(page), { text: want, focused: true, at: want.length });
			await pill(page, 'hid').waitFor({ state: 'detached' });
			assert.deepEqual(
				(await page.evaluate(() => window.datum.bar.pills())).map((p) => p.kind),
				['field'],
				'the context field was spent'
			);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the scroll rule', () => {
	// vividn, 2026-09-22 (da-vu7pi): «we should always maintain horizontal
	// scrolling in the commands widget, so when it is tall enough, do three
	// rows that scroll horizontally» — and each row packs on its own.
	// A submenu's: the base row is one row whatever the room (da-fg4w4), and
	// at a phone's width write's twelve are crowded in one.
	test('one, two and three rows high draw that many rows, each packed, all reached sideways; taller stays three', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await page.setViewportSize({ width: 390, height: page.viewportSize().height });
			await commands(page).locator('button[data-menu="write"]').click();

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
				packed(await gaps(commands(page), '.lane > *'), `${want} rows`);
				if (want === 2) {
					// Dealt by index: back and moment start the two rows, on
					// follows back and off follows moment.
					const at = Object.fromEntries((await boxes(commands(page), 'button.round')).map((b) => [b.name, b]));
					const back = at['back'];
					assert.deepEqual(
						[
							back.left === at.moment.left,
							back.line < at.moment.line,
							at.on.line === back.line && at.on.left > back.left,
							at.off.line === at.moment.line && at.off.left > at.moment.left
						],
						[true, true, true, true],
						'the rows are not back on … over moment off …'
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

				// The collect widget alike — the same component, the same rule.
				const short = await axes(fieldsBar(page));
				assert.deepEqual([short.x, short.y, short.display, short.fits], ['auto', 'hidden', 'flex', true]);
			}

			// Room for far more than three: still three, still sideways.
			const { width, height } = page.viewportSize();
			await page.setViewportSize({ width, height: height + 400 });
			const tall = await until(
				async () => ({
					...(await axes(commands(page))),
					room: await commands(page).evaluate((bar) => bar.clientHeight / bar.parentElement.querySelector('.probe').offsetHeight)
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
			await seedLines(app, ['setup coffee --verb moment', 'moment coffee']);
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
			await app.submit('moment coffee');
			await app.clock.set(AT + 11 * 60_000);
			await commands(page).locator('button[data-command="undo"]').click();
			await page.waitForFunction(() => window.__datumInflight === 0);
			assert.equal((await app.entries(1)).length, 0, 'the older change was refused');
			assert.match((await app.outputs()).join('\n'), /warning:[^\n]*\b11\b/, 'the warning names how old the change is');
		});
	});

	// da-z0zh3: the CLI's undo is an editor's — a second press walks back
	// past the first log instead of bringing the second one back.
	test('pressed twice after two logs, it takes both back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, ['moment coffee', 'moment tea']);
			assert.equal((await app.entries(2)).length, 2);
			const undo = commands(page).locator('button[data-command="undo"]');
			for (const left of [1, 0]) {
				await undo.click();
				await page.waitForFunction(() => window.__datumInflight === 0);
				assert.equal((await app.entries(2)).length, left);
			}
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
					(want) => !document.querySelector('main .bar.commands [data-command="redo"]').disabled === want,
					on
				);
				await app.idle();
				return { label: await redo.getAttribute('aria-label'), title: await redo.getAttribute('title') };
			};
			const press = async (button) => {
				await echoed(app, () => button.click());
				await app.idle();
			};

			await seedLines(app, ['setup coffee --verb moment', 'moment coffee']);
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
			await app.submit('moment coffee');
			await app.idle();
			await lit(false);

			// An undo from ANOTHER tab, on the seam and not the button, lights it here.
			const other = await app.newTab();
			assert.equal((await other.run(['undo'])).code, 0);
			await lit(true);
		});
	});
});
