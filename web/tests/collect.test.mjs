// The collect widget (da-w6437): the fields widget retitled and built for
// logging. vividn, 2026-11-03: «the primary interaction widget on the
// collect pane … this widget really adapts to whatever you're doing».
//
// At rest its fields stand in a cluster, the most used in the middle. A tap
// opens a field: back, its name and chosen state on the first row; its time
// (`−q`, `−m`, `5m`, which become pills); a switch field's states, the first
// of its spec in the middle; its keys and `+ data` at the bottom. Every pill
// it makes is on the input bar too, and nothing here runs a command.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';

before(startApp);
after(stopApp);

const WIDGET = 'section.pane[data-title="collect"]';
const VIEW = `${WIDGET} .bar .collect`;
const BAR_PILLS = '.prompt-bar .pill';

const SEED = [
	'setup mood --verb switch --states good,bad,meh -k why= -k with=',
	'setup coffee --verb occur',
	'setup work --verb start',
	'occur coffee -t 09:00',
	'occur coffee -t 10:00',
	'occur coffee -t 11:00',
	'occur tea -t 12:00',
	'switch mood bad -t 13:00'
];

const shared = sharedApp({ clock: todayAt(15) });

/** The app seeded, its fields sorted again as on a load, coffee drawn. */
const collect = (fn) =>
	shared.solo(async (app) => {
		for (const line of SEED) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		// What a load does, and a return to the foreground: `resort`.
		await app.page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		await app.page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
		await fn(app);
	});

/** The center of what `selector` names, on screen. */
const center = (page, selector) =>
	page.locator(selector).first().evaluate((el) => {
		const r = el.getBoundingClientRect();
		return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
	});

/** The pills on the input bar, by kind and what each says. */
const barPills = (page) =>
	page.locator(BAR_PILLS).evaluateAll((ps) => ps.map((p) => `${[...p.classList].find((c) => c !== 'pill')}:${p.innerText.replace(/[×\s]+$/, '').trim()}`));

/** The view's own pill of `kind`: what its left part says. */
const partPill = (page, kind) => page.locator(`${VIEW} .part-pill[data-pill="${kind}"] .more`);

describe('collect', () => {
	test('at rest the widget is titled collect, and the most used field stands in its middle', async () => {
		await collect(async ({ page }) => {
			// coffee, logged three times, is the most used.
			await page.waitForFunction((view) => {
				const room = document.querySelector(`${view} .room`)?.getBoundingClientRect();
				const first = document.querySelector(`${view} .spot`);
				return room && first?.querySelector('[data-field="coffee"]');
			}, VIEW);
			const room = await center(page, `${VIEW} .room`);
			const coffee = await center(page, `${VIEW} [data-field="coffee"]`);
			assert.ok(Math.abs(room.x - coffee.x) <= 2 && Math.abs(room.y - coffee.y) <= 2, `coffee at ${JSON.stringify(coffee)}, the middle at ${JSON.stringify(room)}`);
		});
	});

	test('a switch field tapped: the time row, its states with the first in the middle, its keys and + data at the bottom', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${VIEW} [data-state="good"]`).waitFor();
			assert.equal(await page.locator(`${VIEW} [data-field]`).count(), 0, 'the fields are still drawn under the view');
			assert.deepEqual(await page.locator(`${VIEW} [data-state]`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['good', 'bad', 'meh']);
			const room = await center(page, `${VIEW} .room`);
			const good = await center(page, `${VIEW} [data-state="good"]`);
			assert.ok(Math.abs(room.x - good.x) <= 2 && Math.abs(room.y - good.y) <= 2, `good at ${JSON.stringify(good)}, the middle at ${JSON.stringify(room)}`);

			const tops = await page.locator(VIEW).evaluate((view) =>
				Object.fromEntries(['.head', '[data-time-row]', '.room', '[data-data-row]'].map((s) => [s, view.querySelector(s).getBoundingClientRect().top]))
			);
			// Back and the time above the states (on one row in a short pane), the keys below.
			assert.ok(tops['.head'] < tops['.room'] && tops['[data-time-row]'] < tops['.room'] && tops['.room'] < tops['[data-data-row]'], JSON.stringify(tops));
			assert.deepEqual(await page.locator(`${VIEW} [data-data-row] button`).allInnerTexts(), ['why=', 'with=', '+ data']);
			assert.deepEqual(await barPills(page), ['field:mood'], 'the field is a pill on the bar');

			// mood is in `bad`: its chip says so with the CLI's yellow `!`.
			assert.deepEqual(
				await page.locator(`${VIEW} [data-state]`).evaluateAll((bs) => bs.filter((b) => getComputedStyle(b, '::after').content === '"!"').map((b) => b.dataset.state)),
				['bad']
			);

			// A key's chip writes `key=` on the line.
			await page.locator(`${VIEW} [data-key="why"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input')?.value === 'why=');
		});
	});

	// vividn, 16:07: «1-click»: a state logs at once with the time pills set,
	// and collect is back at rest, the pills spent.
	test('a state tapped logs at once with the time set, and collect is back at rest with no pills', async () => {
		await collect(async (app) => {
			const { page } = app;
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${VIEW} [data-q]`).click();
			await partPill(page, 'time').waitFor();
			const seen = await app.spyOnRunner();
			await page.locator(`${VIEW} [data-state="meh"]`).click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'switch'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'switch'), ['switch', 'mood', 'meh', '-q']);
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
			await app.idle();
			const [row] = JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows;
			assert.equal(row.state, 'meh');
		});
	});

	test('−q tapped twice is one −qq pill, on the bar too', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await page.locator(`${VIEW} [data-q]`).click();
			await partPill(page, 'time').waitFor();
			assert.equal(await partPill(page, 'time').innerText(), '-q');
			await partPill(page, 'time').click();
			await page.waitForFunction((v) => document.querySelector(`${v} .part-pill[data-pill="time"] .more`)?.innerText === '-qq', VIEW);
			assert.equal(await page.locator(`${VIEW} .part-pill`).count(), 1, 'a second pill');
			assert.deepEqual(await barPills(page), ['field:coffee', 'time:-qq']);
		});
	});

	test('5m tapped twice is ←10m, −m after it replaces the length, and 5m −m', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="work"]`).click();
			await page.locator(`${VIEW} [data-five]`).click();
			await partPill(page, 'length').waitFor();
			assert.equal(await page.locator(`${VIEW} [data-five]`).count(), 0, 'its button stood beside the length');
			await partPill(page, 'length').click();
			await page.waitForFunction((v) => document.querySelector(`${v} .part-pill[data-pill="length"] .more`)?.innerText === '10m', VIEW);
			assert.equal(await page.locator(`${BAR_PILLS}.dur`).count(), 1, 'the bar has no length');

			await page.locator(`${VIEW} [data-m]`).click();
			await partPill(page, 'moment').waitFor();
			assert.equal(await page.locator(`${VIEW} .part-pill[data-pill="length"]`).count(), 0, 'the length stood beside −m');
			assert.deepEqual(await barPills(page), ['field:work', 'moment:-m']);

			await page.locator(`${VIEW} [data-five]`).click();
			await partPill(page, 'length').waitFor();
			assert.equal(await page.locator(`${VIEW} .part-pill[data-pill="moment"]`).count(), 0, '−m stood beside the length');
			assert.equal(await partPill(page, 'length').innerText(), '5m');
			assert.equal(await partPill(page, 'length').locator('svg.length-arrow').count(), 1, 'the length wears no arrow');
			assert.equal(await page.locator(`${BAR_PILLS}.moment`).count(), 0);
			assert.equal(await page.locator(`${BAR_PILLS}.dur`).count(), 1);
		});
	});

	test('× clears its own pill and no other', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="work"]`).click();
			await page.locator(`${VIEW} [data-q]`).click();
			await page.locator(`${VIEW} [data-five]`).click();
			await partPill(page, 'length').waitFor();
			await page.locator(`${VIEW} .part-pill[data-pill="length"] .clear`).click();
			await page.locator(`${VIEW} [data-five]`).waitFor();
			assert.equal(await partPill(page, 'time').innerText(), '-q');
			assert.deepEqual(await barPills(page), ['field:work', 'time:-q']);

			await page.locator(`${VIEW} .part-pill[data-pill="time"] .clear`).click();
			await page.locator(`${VIEW} [data-q]`).waitFor();
			assert.deepEqual(await barPills(page), ['field:work']);
		});
	});

	test('back takes the field and the pills made since the tap, and keeps a time pill already there', async () => {
		await collect(async ({ page }) => {
			// A −q made in one field's view, its field's pill then taken off the
			// bar: the view closes and the −q stays, made before the next tap.
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await page.locator(`${VIEW} [data-q]`).click();
			await partPill(page, 'time').waitFor();
			await page.locator(`${BAR_PILLS}.field button.remove`).click();
			await page.locator(`${VIEW} [data-field="mood"]`).waitFor();
			assert.deepEqual(await barPills(page), ['time:-q']);

			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await partPill(page, 'time').click();
			await page.locator(`${VIEW} [data-m]`).click();
			await page.waitForFunction((v) => document.querySelector(`${v} .part-pill[data-pill="time"] .more`)?.innerText === '-qq', VIEW);
			assert.deepEqual((await barPills(page)).sort(), ['field:mood', 'moment:-m', 'time:-qq']);

			await page.locator(`${VIEW} [data-back]`).click();
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
			assert.deepEqual(await barPills(page), ['time:-q']);
		});
	});

	test('a moment field shows no states, and of its time −q alone', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await page.locator(`${VIEW} [data-time-row]`).waitFor();
			assert.equal(await page.locator(`${VIEW} [data-state]`).count(), 0);
			assert.deepEqual(await page.locator(`${VIEW} [data-time-row] button`).allInnerTexts(), ['−q']);
			assert.deepEqual(await page.locator(`${VIEW} [data-data-row] button`).allInnerTexts(), ['+ data']);
			assert.match(await page.locator(`${VIEW} .head`).innerText(), /back\s*coffee/);
		});
	});
});
