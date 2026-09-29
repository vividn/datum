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
 * a switch to show and `start` one to hide; `sleep` is a stopped block,
 * which both still show (da-vpzbv); `light` is a switch with `true` among
 * its states, which is the other half of `start`'s rule.
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

/**
 * The commands bar's sections as drawn: each one's name, its commands, the
 * x of the rule at its left edge (null where none is drawn), and how far
 * its buttons reach either side.
 */
const sectionsOf = (page) =>
	commands(page).evaluate((bar) =>
		[...bar.querySelectorAll(':scope > .section')].map((section) => {
			const style = getComputedStyle(section);
			const buttons = [...section.querySelectorAll('button[data-command]')];
			const boxes = buttons.map((b) => b.getBoundingClientRect());
			return {
				name: section.dataset.section,
				commands: buttons.map((b) => b.dataset.command),
				rule: style.borderLeftStyle === 'solid' && parseFloat(style.borderLeftWidth) > 0 ? section.getBoundingClientRect().left : null,
				left: Math.min(...boxes.map((b) => b.left)),
				right: Math.max(...boxes.map((b) => b.right))
			};
		})
	);

/** No empty section; a rule before every section but the first, standing between the two sections' buttons. */
function ruled(sections) {
	sections.forEach((s, i) => {
		assert.ok(s.commands.length > 0, `${s.name} is drawn empty`);
		if (i === 0) return assert.equal(s.rule, null, `a rule before ${s.name}, the first`);
		assert.notEqual(s.rule, null, `no rule between ${sections[i - 1].name} and ${s.name}`);
		assert.ok(sections[i - 1].right < s.rule && s.rule < s.left, `the rule before ${s.name} is not between the sections`);
	});
}

describe('the bar of commands', () => {
	test('Undo leads, the verbs follow, and note after them', async () => {
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
			assert.equal(buttons[8], 'note', 'note is on the bar, after the verbs (da-8ru0m)');
			assert.ok(buttons.includes('Sync'), 'the tail of the list is drawn too');
		});
	});

	// da-0mepr. vividn, 2026-09-26: «let's divide the command widget into
	// sections. I think that will visually differentiate different kinds of
	// actions» — undo and help, logging, setup, edits, analysis, db.
	test('the commands stand in sections, in order, a rule between each; a pill that hides the edits takes their section and its rule', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await atRows(page, 'cmd', 3);
			const whole = await sectionsOf(page);
			assert.deepEqual(
				whole.map((s) => s.name),
				['undo', 'log', 'setup', 'edit', 'analysis', 'db']
			);
			assert.deepEqual(whole.find((s) => s.name === 'edit').commands.sort(), ['delete', 'retime', 'update']);
			ruled(whole);

			// A field pill: coffee names no edit (fieldpill.test.mjs), so the whole section goes.
			await fieldsBar(page).locator('[data-field="coffee"]').click({ button: 'right' });
			await page.waitForFunction(() => !document.querySelector('[data-command="delete"]'));
			const under = await sectionsOf(page);
			assert.deepEqual(
				under.map((s) => s.name),
				whole.map((s) => s.name).filter((name) => under.some((s) => s.name === name))
			);
			assert.equal(under.some((s) => s.name === 'edit'), false, 'the edit section outlived its commands');
			ruled(under);
		});
	});
});

// da-803mk. vividn, 2026-09-25: «for commands with subcommands, like db,
// clicking on them should then show the subcommands in the commands widget
// instead.» Which are subcommands is `datum help --json`'s to say.
describe('a command with subcommands drills into them', () => {
	const back = (page) => commands(page).locator('button[data-back]');
	const DB = ['setup', 'purge', 'backup', 'restore', 'migrate', 'v2import'];

	test('db shows its subcommands under < Back, and setup fills `db setup `', async () => {
		await onPage(async ({ page }) => {
			await commands(page).locator('button[data-command="db"]').click();
			await back(page).waitFor();
			assert.equal((await back(page).innerText()).trim(), '< Back');
			assert.deepEqual(await drawn(page), ['Help', ...DB], 'Help stays, to explain a subcommand');
			// A drilled bar is one run: Help's section does not set it apart (da-0mepr).
			assert.deepEqual((await sectionsOf(page)).map((s) => s.name), [undefined]);
			assert.equal((await line(page)).text, '', '`db ` runs nothing, so a tap never writes it');

			await tap(page, 'db setup');
			assert.equal(await back(page).count(), 1, 'a fill stays among the subcommands');
		});
	});

	// da-9ow8m: vividn, «the "backup" command in the commands widget … should
	// that open this backups menu directly? It could». So `db backup` opens
	// the sheet, and `db restore` opens it at the backups to restore from;
	// neither writes the line, and both are drawn once-outlined.
	for (const sub of ['backup', 'restore']) {
		test(`db ${sub} opens «Backups», and writes nothing on the line`, async () => {
			await onPage(async ({ page }) => {
				await commands(page).locator('button[data-command="db"]').click();
				const button = commands(page).locator(`button[data-command="db ${sub}"]`);
				assert.match(await button.getAttribute('class'), /\bopens\b/);
				await button.click();
				const sheet = page.locator('dialog.sheet', { hasText: 'Backups' });
				await sheet.waitFor();
				assert.equal(
					await page.evaluate(() => document.querySelector('dialog.sheet .sheet-head span')?.textContent),
					'Backups'
				);
				assert.equal((await line(page)).text, '', `db ${sub} wrote the line`);
				await page.keyboard.press('Escape');
				await sheet.waitFor({ state: 'detached' });
			});
		});
	}

	test('< Back and Escape each return to the whole list, and Escape peels only that', async () => {
		await onPage(async ({ page }) => {
			await commands(page).locator('button[data-command="db"]').click();
			await back(page).click();
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('occur'), 'Back is the whole list');

			await commands(page).locator('button[data-command="db"]').click();
			await tap(page, 'db setup');
			await page.keyboard.press('Escape');
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('db'));
			assert.equal((await line(page)).text, 'db setup ', 'the same Escape peeled the line too');
		});
	});

	test('conflicts, which runs bare, is offered among its own subcommands', async () => {
		await onPage(async ({ page }) => {
			await commands(page).locator('button[data-command="conflicts"]').click();
			await back(page).waitFor();
			assert.deepEqual((await drawn(page)).slice(1, 3), ['conflicts', 'list']);
			await tap(page, 'conflicts');
		});
	});

	test('a command without subcommands still fills as before', async () => {
		await onPage(async ({ page }) => {
			await tap(page, 'tail');
			assert.equal(await back(page).count(), 0);
		});
	});
});

// da-st2xl. vividn, 2026-09-25: «long pressing on a command, like occur
// should add it to the input box as well, and then display various flags
// that you can add, like -q, -t, etc, sorted in a useful order … for -t
// keyboard should be opened on mobile». A REAL long press: the timers are
// the page's own, so the hold is held for as long as a finger would.
describe('holding a command fills it and shows its flags', () => {
	const back = (page) => commands(page).locator('button[data-back]');
	const flags = async (page) => (await listed(commands(page), 'button[data-flag]')).map((b) => b.text);

	async function hold(page, name) {
		const box = await commands(page).locator(`button[data-command="${name}"]`).boundingBox();
		await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
		await page.mouse.down();
		await page.waitForTimeout(600);
		await page.mouse.up();
		await back(page).waitFor();
	}

	/** What the line holds, whether the caret is in it, and whether a phone's keyboard would be up. */
	const typed = (page) =>
		page.evaluate(() => {
			const input = document.querySelector('.prompt-bar input');
			return { text: input.value, focused: document.activeElement === input, keyboard: input.inputMode !== 'none' };
		});

	test('occur fills `occur ` and its flags follow Back and Help, the curated ones first', async () => {
		await onPage(async ({ page }) => {
			await hold(page, 'occur');
			assert.equal((await typed(page)).text, 'occur ', 'the hold fills as the tap does');
			assert.deepEqual(await drawn(page), ['Help'], 'Back, Help, then flags: no command beside them');
			assert.deepEqual((await flags(page)).slice(0, 7), ['-t', '-d', '-y', '-q', '-c', '-m', '-k']);
			assert.ok((await flags(page)).includes('--id'), 'the rest follow, long-only ones included');
		});
	});

	test('-q appends with the keyboard shut; -t appends and opens it; the line composes', async () => {
		await onPage(async ({ page }) => {
			await hold(page, 'occur');
			await commands(page).locator('button[data-flag="occur -q"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q ');
			assert.deepEqual(await typed(page), { text: 'occur -q ', focused: true, keyboard: false });

			await commands(page).locator('button[data-flag="occur -t"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q -t ');
			assert.deepEqual(await typed(page), { text: 'occur -q -t ', focused: true, keyboard: true });
		});
	});

	// PR #216 (vremya): the flags outlive the line — a run spends it, a
	// reader types another command — and a flag tap then starts the line
	// afresh with the command whose flags are showing.
	test('after a run, or under another command, a flag tap starts `occur -q ` again', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur']);
			const q = commands(page).locator('button[data-flag="occur -q"]');
			const value = (v) => page.waitForFunction((v) => document.querySelector('.prompt-bar input').value === v, v);
			await hold(page, 'occur');
			await q.click();
			await value('occur -q ');
			await page.keyboard.type('coffee');
			await page.keyboard.press('Enter');
			await value('');
			assert.equal(await back(page).count(), 1, 'the flags stayed open');
			await q.click();
			await value('occur -q ');

			await page.locator('.prompt-bar input').fill('tail ');
			await q.click();
			await value('occur -q ');
			await app.idle();
		});
	});

	// da-bbmpx: the verb is still pending under `occur -q `, so the fields
	// narrow to occur's — and a tap on one runs the LINE with the field.
	test('a field tap under `occur -q ` runs `occur -q coffee` and spends the line', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur']);
			await hold(page, 'occur');
			await commands(page).locator('button[data-flag="occur -q"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q ');
			await echoed(app, () => fieldsBar(page).locator('[data-field="coffee"]').click());
			assert.equal((await app.echoes()).at(-1), 'occur -q coffee');
			assert.equal((await typed(page)).text, '', 'the line was not spent');
			await app.idle();
		});
	});

	test('< Back and Escape return to the commands, and leave the line as it is', async () => {
		await onPage(async ({ page }) => {
			await hold(page, 'occur');
			await back(page).click();
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('occur'));

			await hold(page, 'occur');
			await page.keyboard.press('Escape');
			await back(page).waitFor({ state: 'detached' });
			assert.ok((await drawn(page)).includes('occur'));
			assert.equal((await typed(page)).text, 'occur ', 'the same Escape peeled the line too');
		});
	});

	test('a command without flags is not held', async () => {
		await onPage(async ({ page }) => {
			assert.equal(await commands(page).locator('button[data-command="delete"][data-holds]').count(), 0);
			assert.equal(await commands(page).locator('button[data-command="occur"][data-holds]').count(), 1);
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

			// Every block, running or not (da-vpzbv: a start re-logged is a
			// forgotten one), and a switch with `true` among its states.
			await tap(page, 'start');
			assert.deepEqual(await showing(page), ['light', 'sleep', 'work']);

			// Every block again, and the switches that are on by `now`'s truth.
			await tap(page, 'end');
			assert.deepEqual(await showing(page), ['mood', 'sleep', 'work']);

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

	// da-vpzbv, vividn: «show both start and end commands … regardless of the
	// current state». A block's pill leaves both, running (`work`) or not
	// (`sleep`), and re-logging either runs: that is how a person records a
	// start or an end they forgot.
	test('a block field offers start and end whatever its state, and each runs', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const verbs = async () =>
				(await drawn(page)).filter((name) => ['occur', 'start', 'end', 'switch', 'add'].includes(name));

			for (const field of ['work', 'sleep']) {
				await fieldsBar(page).locator(`[data-field="${field}"]`).click({ button: 'right' });
				await page.waitForFunction(() => !document.querySelector('[data-command="occur"]'));
				assert.deepEqual(await verbs(), ['start', 'end'], field);
				await page.locator('.prompt-bar .pill.field button.remove').click();
				await page.waitForFunction(() => document.querySelector('[data-command="occur"]'));
			}

			await tap(page, 'start');
			await echoed(app, () => fieldsBar(page).locator('[data-field="work"]').click());
			assert.equal((await app.echoes()).at(-1), 'start work');
			await tap(page, 'end');
			await echoed(app, () => fieldsBar(page).locator('[data-field="sleep"]').click());
			assert.equal((await app.echoes()).at(-1), 'end sleep');
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
			assert.equal((await app.echoes()).at(-1), 'switch mood bad');
			assert.equal((await line(page)).text, '', 'the verb was not spent');

			// And a word that is not one of the five pends nothing.
			await input.fill('tail ');
			assert.deepEqual(await showing(page), ['coffee', 'light', 'mood', 'rec', 'sleep', 'work']);
		});
	});
});

// da-lxqqz, vividn: «when a field is selected, then the start/end/occur
// buttons should become double lines and auto trigger entry (unless there
// are extra keys to be filled in, in which case the input line should
// automatically be selected for typing in those fields». Required keys
// only: a key with a default is filled from it and stops nothing.
describe('a verb under a field pill', () => {
	const KEYED = [
		...SEED,
		'spec set tea --verb occur --data-key amount',
		'spec set juice --verb occur --data-key cups=1',
		'spec set shift --verb start --data-key project --data-key n=1',
		'start shift project=a n=1 -t -30'
	];
	const verb = (page, name) => commands(page).locator(`button[data-command="${name}"]`);
	const fieldPill = (page) => page.locator('.prompt-bar .pill.field');

	/** Mint `field`'s pill and wait for the verbs to narrow to it. */
	async function pillOf(page, field) {
		await fieldsBar(page).locator(`[data-field="${field}"]`).click({ button: 'right' });
		await fieldPill(page).waitFor();
		await page.waitForFunction(() => !document.querySelector('[data-command="switch"]'));
	}

	test('a field that needs nothing typed: the verb is double-outlined and logs at once, spending the pill', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, KEYED);
			await pillOf(page, 'coffee');
			assert.equal(await verb(page, 'occur').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'occur').click());
			assert.equal((await app.echoes()).at(-1), 'occur coffee');
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
			for (const name of ['start', 'end']) {
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
			assert.equal(await verb(page, 'occur').getAttribute('data-act'), 'fill');

			await verb(page, 'occur').click();
			// Straight into the first key's frame (da-31d1d), which a bare word fills (da-p3hxn).
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur tea ');
			assert.deepEqual(await line(page), { text: 'occur tea ', focused: true });
			assert.notEqual(await page.locator('.prompt-bar input').getAttribute('inputmode'), 'none', 'the keyboard stayed shut');
			// The pill is on the line now, so it is spent rather than appended a second time.
			await fieldPill(page).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.allEntries()).length, before, 'the tap logged something');

			await echoed(app, () => app.submit('occur tea 2'));
			assert.equal((await app.echoes()).at(-1), 'occur tea 2');
			assert.deepEqual((await app.entries(1))[0].data, { amount: 2 });
			assert.deepEqual(app.errors, []);
		});
	});

	// da-7h73h: a field name may hold a space. The fill quotes it, and so
	// does the echo a press records, which Up recalls as a typed line.
	test('a spaced field under its pill: the fill and the echo quote it, and both log to it', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set "my coffee" --verb occur --data-key amount', 'spec set "iced tea" --verb occur']);
			await pillOf(page, 'my coffee');
			await verb(page, 'occur').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur "my coffee" ');
			await fieldPill(page).waitFor({ state: 'detached' });
			const filled = (await line(page)).text;
			await echoed(app, () => app.submit(`${filled}2`));
			await app.idle();
			assert.deepEqual((await app.entries(1)).map((e) => [e.field, e.data]), [['my coffee', { amount: 2 }]]);

			await app.clock.forward(60_000);
			await pillOf(page, 'iced tea');
			await echoed(app, () => verb(page, 'occur').click());
			assert.equal((await app.echoes()).at(-1), 'occur "iced tea"');
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
			assert.equal(await verb(page, 'occur').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'occur').click());
			assert.equal((await app.echoes()).at(-1), 'occur juice');
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
			assert.equal(await verb(page, 'start').getAttribute('data-act'), 'fill');
			assert.equal(await verb(page, 'end').getAttribute('data-act'), 'run');

			await echoed(app, () => verb(page, 'end').click());
			assert.equal((await app.echoes()).at(-1), 'end shift');
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
	const load = (page, pills) => page.evaluate((all) => window.datum.bar.setPills(all), pills);
	/** The line, whether it has the caret, and where the caret is. */
	const caret = (page) =>
		input(page).evaluate((el) => ({ text: el.value, focused: document.activeElement === el, at: el.selectionStart }));

	test('note under a coffee pill writes `note coffee ` with the caret last; the words land after the field', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await fieldsBar(page).locator('[data-field="coffee"]').click({ button: 'right' });
			await pill(page, 'field').waitFor();

			await commands(page).locator('button[data-command="note"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'note coffee ');
			assert.deepEqual(await caret(page), { text: 'note coffee ', focused: true, at: 'note coffee '.length });
			await pill(page, 'field').waitFor({ state: 'detached' });

			await input(page).pressSequentially('too strong');
			await echoed(app, () => input(page).press('Enter'));
			assert.equal((await app.echoes()).at(-1), 'note coffee too strong');
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
			await tap(page, 'note');
			assert.deepEqual(await caret(page), { text: 'note ', focused: true, at: 5 });
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
