// The command pane, and the command the pills make ready leading it
// (da-h3jcu). The pane is the palette the `❯` opens, not a pane of every
// tab (da-jvwr8).
//
// vividn, 2026-11-03: «let's keep the command pane at the bottom with the
// circular commands like we just discussed, keep that design unified across
// the app», and when pills make a command ready, «it's the pane's suggested
// action: a larger circle, dashed while something's missing, filled in its
// field's color once it can run». Which command, and when it is ready, is
// `suggestionOf` (commands-pure.test.mjs); this file is what a reader sees
// and what a tap does.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, seedLines, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, along, pickOption, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'cli'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const shared = sharedApp(DESKTOP);
// The session a test before left (collect's time section open, da-iunoo) forgotten: a reload keeps it,
// and the section opening as the page loads moves the rows under a press measured before it did.
const onPage = (fn) =>
	shared.solo(async (app) => {
		await app.page.evaluate(() => window.datum.layout.forgetSession());
		await fn(app);
	}, PAGE);

const SEED = [
	"setup coffee --verb moment --color '#b87333'",
	"setup mood --verb switch --states good,bad --color '#336699' --state-color bad=#993366",
	'setup light --verb switch --states on,off'
];

const lead = (page) => page.locator('main section.pane[data-pane="cmd"] button.lead');
const load = (page, all) => page.evaluate((all) => window.datum.bar.setPills(all), all);
const field = (text) => ({ id: 'field', kind: 'field', text });
const state = (text) => ({ id: 'state', kind: 'state', text });

/** The lead circle as drawn: its name, whether it runs, its border style and fill. */
const drawn = (page) =>
	lead(page).evaluate((b) => {
		const css = getComputedStyle(b);
		return { name: b.textContent.trim(), ready: b.dataset.ready, border: css.borderTopStyle, fill: css.backgroundColor };
	});

/** The entries the database holds, newest last, as `field state`. */
const entries = async (app) =>
	JSON.parse((await app.runLine('tail --json -n 5')).stdout)
		.rows.map((r) => `${r.field} ${r.state ?? ''}`.trim());

const line = (page) => page.evaluate(() => document.querySelector('.prompt-bar input').value);

describe('the ready command leads the command pane', () => {
	test('with nothing loaded there is none', async () => {
		await onPage(async (app) => {
			await seedLines(app, SEED);
			assert.equal(await lead(app.page).count(), 0);
		});
	});

	test("a moment field's pill leads with a filled «moment coffee», and its tap logs it", async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await load(page, [field('coffee')]);
			await lead(page).waitFor();
			assert.deepEqual(await drawn(page), { name: 'moment coffee', ready: 'true', border: 'solid', fill: 'rgb(184, 115, 51)' });
			await lead(page).click();
			await page.waitForFunction(() => !window.datum.bar.pills().length);
			assert.equal((await entries(app)).at(-1), 'coffee');
			await lead(page).waitFor({ state: 'detached' });
		});
	});

	test("a state field's pill with no state is dashed, and a tap writes the line rather than running", async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const before = await entries(app);
			await load(page, [field('mood')]);
			await lead(page).waitFor();
			const { name, ready, border } = await drawn(page);
			assert.deepEqual({ name, ready, border }, { name: 'change → ?', ready: 'false', border: 'dashed' });
			await lead(page).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'change mood ');
			assert.deepEqual(await entries(app), before, 'the tap ran something');
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), [], 'the field pill was not spent onto the line');
		});
	});

	test('with its state pill too it fills in the state color and runs, spending both pills', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await load(page, [field('mood'), state('bad')]);
			await page.waitForFunction(() => document.querySelector('button.lead')?.dataset.ready === 'true');
			assert.deepEqual(await drawn(page), { name: 'change → bad', ready: 'true', border: 'solid', fill: 'rgb(153, 51, 102)' });
			await lead(page).click();
			await page.waitForFunction(() => !window.datum.bar.pills().length);
			assert.equal((await entries(app)).at(-1), 'mood bad');
			assert.equal(await line(page), '');
		});
	});
});

// vremya on #1636: a state pill left on the bar (a `switch` that failed keeps
// its pills) is its field's, so a field pill for another field, or none,
// takes it too; kept, the lead would read «switch → bad» on the wrong field.
test("another field's pill, or none, takes a state pill left on the bar, and the lead waits for a state again", async () => {
	await onPage(async (app) => {
		const { page } = app;
		await seedLines(app, SEED);
		const kinds = () => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));
		await load(page, [field('mood'), state('bad')]);
		await page.waitForFunction(() => document.querySelector('main button.lead')?.dataset.ready === 'true');

		// Mood's view is open (any field pill opens it, da-z45a6), so no other
		// field is there to hold: the pill moved from anywhere else, its state left.
		await load(page, [field('light'), state('bad')]);
		await until(kinds, (k) => k.join() === 'field:light');
		await page.waitForFunction(() => document.querySelector('main button.lead')?.dataset.ready === 'false');
		assert.equal((await drawn(page)).name, 'change → ?');

		await load(page, [field('mood'), state('bad')]);
		// Its field's button is not there to hold: its pill's × takes it.
		await page.locator('.prompt-bar .pill.field button.remove').click();
		await until(kinds, (k) => k.length === 0);
		await lead(page).waitFor({ state: 'detached' });
	});
});

/**
 * The command pane is the palette the `❯` slides up over the output, and no
 * first-visit tab holds one of its own (da-jvwr8), vividn, 2026-11-04: «The
 * command pallete comes out from the >, so a separate widget is redundant».
 * The ready command leads the palette as it leads any command pane.
 */
const TABS = { setup: 1, collect: 2, dayview: 3 };
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };

for (const [device, options] of [
	['a desktop', { ...DESKTOP, desk: null }],
	['a phone', PHONE]
]) {
	test(`on ${device}, setup, collect and dayview hold no command pane, and the ❯'s leads with the ready command`, async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const index of Object.values(TABS)) {
					await page.evaluate((i) => window.datum.shell.activate(i), index);
					await page.locator('main section.pane').first().waitFor();
					assert.equal(await page.locator('main section.pane[data-title="command"]').count(), 0);
				}
				await page.evaluate((i) => window.datum.shell.activate(i), TABS.collect);
				// The shipped tabs hold no CLI widget for `seedLines`' echo, and a
				// quiet line tells the page of no new field: a reload reads it.
				assert.equal((await app.runLine(SEED[0])).code, 0);
				await app.reload();
				await load(page, [field('coffee')]);
				const palette = page.locator('.prompt-bar #bar-commands button.lead');
				await page.waitForFunction(() => document.querySelector('.prompt-bar #bar-commands button.lead')?.dataset.ready === 'true');
				assert.equal((await palette.textContent()).trim(), 'moment coffee');
				await palette.click();
				await until(() => entries(app), (e) => e.includes('coffee'));
				assert.deepEqual(app.errors, []);
			},
			{ ...options, home: ON_COLLECT }
		);
	});
}

// vividn, 2026-11-03 (da-u017i): «1-click vs 2-click. So in 2-click mode you
// then hit the command again on the bottom», a mode «which reverses long
// press and short press operations». A tap on a state logs it (1-click), or
// chooses it, its pill placed for the ready command at the bottom to log.
describe('collect: 1-click and 2-click', () => {
	const VIEW = 'main section.pane[data-pane="bar"] .collect';
	const WHEN = 'main section.pane[data-pane="bar"] [data-when]';
	/** −5m pressed, the time section opened by its clock first where it is closed (da-iunoo). */
	const earlier = async (page) => {
		const clock = page.locator(`${WHEN} [data-when-clock][aria-expanded="false"]`);
		if ((await clock.count()) > 0) await clock.click();
		await page.locator(`${WHEN} [data-earlier="5"]`).click();
	};
	const open = (page, name) => page.locator(`${VIEW} [data-field="${name}"]`).click();
	const chip = (page, state) => page.locator(`${VIEW} [data-state="${state}"]`);
	const kinds = (page) => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));
	/** The `switch` lines run, dry runs (the nowview's preview) left out. */
	const switches = async (seen) => (await seen()).filter((a) => a[0] === 'change' && !a.includes('--dry-run'));
	/** Chosen, not logged: the chip ringed, its pill beside the field's and the time's, and the lead ready. */
	async function chosen(app, seen, state) {
		const { page } = app;
		await until(() => kinds(page), (k) => k.includes(`state:${state}`));
		assert.deepEqual((await kinds(page)).sort(), ['field:mood', `state:${state}`, 'time:-q'].sort());
		assert.equal(await chip(page, state).evaluate((b) => b.classList.contains('chosen')), true, 'the chip is not marked');
		await page.waitForFunction(() => document.querySelector('main button.lead')?.dataset.ready === 'true');
		assert.equal((await drawn(page)).name, `change → ${state}`);
		await app.idle();
		assert.deepEqual(await switches(seen), [], 'choosing logged');
	}
	/** Logged with the time set, and collect back at rest with no pills. */
	async function logged(app, seen, state) {
		const { page } = app;
		await until(() => switches(seen), (s) => s.length === 1);
		assert.deepEqual(await switches(seen), [['change', 'mood', state, '-q']]);
		await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
		await until(() => kinds(page), (k) => k.length === 0);
		await app.idle();
		const [row] = JSON.parse((await app.runLine('tail mood -n 1 --json')).stdout).rows;
		assert.equal(row.state, state);
	}

	test('1-click, the default: a tap logs at once, and a long press chooses', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, [...SEED, 'moment coffee']);
			let seen = await app.spyOnRunner();
			await open(page, 'mood');
			await earlier(page);
			await chip(page, 'bad').click();
			await logged(app, seen, 'bad');

			seen = await app.spyOnRunner();
			await open(page, 'mood');
			await earlier(page);
			await chip(page, 'good').click({ button: 'right' });
			await chosen(app, seen, 'good');
		});
	});

	// da-gra0z, vividn 2026-11-04: «you can still record with 1 tap unless
	// there is some extra data or whatever needed».
	test('1-click: a moment logs with one tap, and a block starts and ends with one each', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, [...SEED, 'setup walk --verb start', 'moment coffee', 'start walk -t -30', 'end walk -t -20']);
			// The clock is held: a minute on, so the tap's moment has an instant of its own.
			await app.clock.forward(60_000);
			const seen = await app.spyOnRunner();
			const runs = async () => (await seen()).filter((a) => ['moment', 'on', 'off'].includes(a[0]) && !a.includes('--dry-run'));
			for (const [name, line] of [
				['coffee', ['moment', 'coffee']],
				['walk', ['on', 'walk']],
				['walk', ['off', 'walk']]
			]) {
				const before = (await runs()).length;
				await open(page, name);
				await until(runs, (r) => r.length === before + 1);
				assert.deepEqual((await runs()).at(-1), line);
				await app.idle();
				assert.equal(await page.locator(`${VIEW} [data-open-field]`).count(), 0, `${name} opened`);
				// The clock is held: a minute on, so the next has an instant of its own.
				await app.clock.forward(60_000);
			}
			const rows = async (f) => JSON.parse((await app.runLine(`tail ${f} --json`)).stdout).rows;
			assert.equal((await rows('coffee')).length, 2, 'the moment is not logged');
			assert.deepEqual((await rows('walk')).map((r) => r.state), [true, false, true, false]);
		});
	});

	test('2-click: a tap chooses and the command at the bottom logs it, a long press logs, and a reload keeps the mode', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, [...SEED, 'moment coffee']);
				await pickOption(page, 'bar', 'a tap', 'opens and chooses, 2-click');
				await app.reload();
				await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();

				// A moment opens, to collect it, and logs nothing.
				let seen = await app.spyOnRunner();
				await open(page, 'coffee');
				await page.locator(`${VIEW} [data-open-field]`).waitFor();
				await app.idle();
				assert.deepEqual((await seen()).filter((a) => a[0] === 'moment' && !a.includes('--dry-run')), [], 'the tap logged');
				await page.locator(`${VIEW} [data-back]`).click();

				seen = await app.spyOnRunner();
				await open(page, 'mood');
				await earlier(page);
				await chip(page, 'bad').click();
				await chosen(app, seen, 'bad');
				await lead(page).click();
				await logged(app, seen, 'bad');

				// The clock is held: a minute on, so the next switch has an instant of its own.
				await app.clock.forward(60_000);
				seen = await app.spyOnRunner();
				await open(page, 'mood');
				await earlier(page);
				await chip(page, 'good').click({ button: 'right' });
				await logged(app, seen, 'good');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, desk: { ...JSON.parse(PAGE), designedFor: 'desktop' } }
		);
	});

	// da-z45a6, vividn 2026-11-04: «1-click mode: tap -> logs immediately (if
	// no extra data needed), long press/right click -> field pill and open the
	// next menu / 2-click mode: reversed».
	/** Hold a field at rest: a right click, or a mouse held down until `landed` says the hold did something. */
	async function hold(page, name, how, landed) {
		const at = page.locator(`${VIEW} [data-field="${name}"]`);
		if (how === 'a right click') return at.click({ button: 'right' });
		const box = await at.boundingBox();
		await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
		await page.mouse.down();
		await landed().catch(() => {});
		await page.mouse.up();
	}
	const runs = async (seen, verb) => (await seen()).filter((a) => a[0] === verb && !a.includes('--dry-run'));

	for (const how of ['a long press', 'a right click']) {
		test(`1-click: ${how} on a moment opens it with its pill and logs nothing, and Escape takes the pill`, async () => {
			await onPage(async (app) => {
				const { page } = app;
				await seedLines(app, [...SEED, 'moment coffee']);
				const seen = await app.spyOnRunner();
				const opened = () => page.locator(`${VIEW} [data-open-field]`).waitFor();
				await hold(page, 'coffee', how, opened);
				await opened();
				assert.deepEqual(await kinds(page), ['field:coffee']);
				await app.idle();
				assert.deepEqual(await runs(seen, 'moment'), [], 'the hold logged');

				await page.keyboard.press('Escape');
				await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
				assert.deepEqual(await kinds(page), []);
				assert.deepEqual(app.errors, []);
			});
		});

		test(`2-click: ${how} logs a moment, fills the line for a key to type, and opens a switch to its states`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seedLines(app, [...SEED, 'setup tea --verb moment --data-key cup', 'moment coffee']);
					await pickOption(page, 'bar', 'a tap', 'opens and chooses, 2-click');
					await page.locator(`${VIEW} [data-field="tea"]`).waitFor();
					const seen = await app.spyOnRunner();

					await hold(page, 'coffee', how, () => until(() => runs(seen, 'moment'), (r) => r.length === 1));
					await until(() => runs(seen, 'moment'), (r) => r.length === 1);
					assert.deepEqual(await runs(seen, 'moment'), [['moment', 'coffee']]);
					await app.idle();
					assert.equal(await page.locator(`${VIEW} [data-open-field]`).count(), 0, 'the hold opened coffee');

					await hold(page, 'tea', how, () => page.waitForFunction(() => document.querySelector('.prompt-bar input').value !== ''));
					await page.waitForFunction(() => document.querySelector('.prompt-bar input').value.startsWith('moment tea'));
					assert.deepEqual(await runs(seen, 'moment'), [['moment', 'coffee']], 'the hold ran a field with a key to type');
					await page.locator('.prompt-bar input').fill('');

					await hold(page, 'mood', how, () => chip(page, 'good').waitFor());
					await chip(page, 'good').waitFor();
					assert.deepEqual(await kinds(page), ['field:mood']);
					await app.idle();
					assert.deepEqual(await runs(seen, 'change'), [], 'the hold logged a state');
					assert.deepEqual(app.errors, []);
				},
				{ ...DESKTOP, desk: { ...JSON.parse(PAGE), designedFor: 'desktop' } }
			);
		});
	}

	// vividn, 16:10:35: «if anything elese opens a field pill, it hsould also advance to this secondary menu as well».
	test('a field pill put on the bar from anywhere opens that field, and another field’s pill opens that one', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await load(page, [field('mood')]);
			await chip(page, 'good').waitFor();
			await load(page, [field('light')]);
			await chip(page, 'on').waitFor();
			assert.equal(await chip(page, 'good').count(), 0, 'mood is still open');
			await load(page, []);
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
			assert.deepEqual(app.errors, []);
		});
	});
});
