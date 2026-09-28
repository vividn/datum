// A field tap when the field has a REQUIRED key (da-chml5). vividn, 09-27:
// «occur/start/end run at once with a double outline, unless there are
// required keys only, in which case fill the line and focus it». The rule
// is `typesKeys` in $lib/commands.ts; the verb under a field PILL, which
// had it first (da-lxqqz), is `commands.test.mjs`'s.
//
// Its own file, as fields-press.test.mjs's budget asks of a new group.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { changeCounter, sharedApp, startApp, stopApp } from './support/app.mjs';
import { type } from './support/fields.mjs';

const SEED = [
	'track tea --verb occur --required-key amount --key cups=1',
	'track juice --verb occur --key cups=1',
	'track shift --verb start --required-key project',
	'track mood --verb switch --state good --state bad --required-key why'
];

const chip = (page, field) => page.locator(`.bar [data-field="${field}"]`);
const lineIs = (page, text) =>
	page.waitForFunction((t) => {
		const input = document.querySelector('.prompt-bar input');
		return input.value === t && document.activeElement === input;
	}, text);

describe('a field with a required key', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();

	test('taps to fill: single outline, the line in its first key frame, focused, nothing written', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			await app.idle();
			assert.equal(await chip(page, 'tea').getAttribute('data-act'), 'fill');
			const counter = await changeCounter(page);

			await chip(page, 'tea').click();
			// The first key's frame, as a Space after the field writes it (da-31d1d).
			await lineIs(page, 'occur tea amount=');
			await app.idle();
			assert.equal(await changeCounter(page), counter, 'the tap wrote');

			// Under a pending verb its flags come too, after the field, so
			// the frame is still written (canary finding on #273).
			await page.locator('.prompt-bar input').fill('occur -q ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="tea"]')?.dataset.act === 'fill');
			await chip(page, 'tea').click();
			await lineIs(page, 'occur tea -q amount=');
			await app.idle();
			assert.equal(await changeCounter(page), counter, 'the tap wrote');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a field whose keys all have defaults still runs at once, double-outlined', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			assert.equal(await chip(page, 'juice').getAttribute('data-act'), 'run');
			await app.pressWidget('juice');
			assert.equal((await app.echoes()).at(-1), 'occur juice');
		});
	});

	test('a block fills its start, and ends at once; a state from the ring fills', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			assert.equal(await chip(page, 'shift').getAttribute('data-act'), 'fill');
			await chip(page, 'shift').click();
			await lineIs(page, 'start shift project=');

			// Not backdated: `track` wrote its TRACKED marker now, which would end it.
			await app.clock.forward(60_000);
			await type(page, 'start shift project=a');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="shift"]')?.dataset.act === 'run');
			await app.pressWidget('shift');
			assert.equal((await app.echoes()).at(-1), 'end shift');

			// The ring still opens; the state chosen in it is the line, unrun.
			assert.equal(await chip(page, 'mood').getAttribute('data-act'), 'open');
			await chip(page, 'mood').click();
			const good = page.locator('.bar button[data-state="good"]');
			assert.equal(await good.getAttribute('data-act'), 'fill');
			const counter = await changeCounter(page);
			await good.click();
			await lineIs(page, 'switch mood good ');
			await app.idle();
			assert.equal(await changeCounter(page), counter, 'the state wrote');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-7h73h: a field name may hold a space, so every line the web writes
	// with a field in it quotes the field (`shellJoin`), or `my coffee`
	// splits and the line logs to a field called `my`. Under its PILL is
	// `commands.test.mjs`'s.
	test('a spaced field: its fill quotes it, from the chip and under a pending verb, and Enter logs to it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await type(page, 'track "my coffee" --verb occur --required-key amount');
			// The clock is held: a log at the declaration's instant would share its slug.
			await app.clock.forward(1000);
			await app.idle();
			await chip(page, 'my coffee').click();
			await lineIs(page, 'occur "my coffee" amount=');
			await page.keyboard.type('3');
			await page.keyboard.press('Enter');
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
			await app.idle();
			const [entry] = await app.entries(1);
			assert.deepEqual([entry.field, entry.data], ['my coffee', { amount: 3 }]);

			await page.locator('.prompt-bar input').fill('occur -q ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="my coffee"]')?.dataset.act === 'fill');
			await chip(page, 'my coffee').click();
			await lineIs(page, 'occur "my coffee" -q amount=');
			assert.deepEqual(app.errors, []);
		});
	});
});
