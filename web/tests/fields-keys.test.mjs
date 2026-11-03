// A field tap when the field has a REQUIRED key (da-chml5); since da-w6437
// a bare tap opens it, and the fill is a pending verb's. vividn, 09-27:
// «moment/start/end run at once with a double outline, unless there are
// required keys only, in which case fill the line and focus it». The rule
// is `typesKeys` in #lib/commands.ts; the verb under a field PILL, which
// had it first (da-lxqqz), is `commands.test.mjs`'s.
//
// Its own file, as fields-press.test.mjs's budget asks of a new group.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { changeCounter, sharedApp, startApp, stopApp } from './support/app.mjs';
import { type } from './support/fields.mjs';

const SEED = [
	'track tea --verb moment -k amount -k cups=1',
	'track juice --verb moment -k cups=1',
	'track shift --verb start -k project',
	'track mood --verb switch --states good,bad -k why'
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

	// A bare tap opens the field (da-w6437): its keys are chips at the
	// bottom, the required one drawn solid, and nothing is written. Under a
	// pending verb a tap still fills its line.
	test('a bare tap opens it with its keys; under a verb a tap fills its first key frame, focused, nothing written', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			await app.idle();
			assert.equal(await chip(page, 'tea').getAttribute('data-act'), 'show');
			const counter = await changeCounter(page);

			await chip(page, 'tea').click();
			await page.locator('.bar [data-key="amount"]').waitFor();
			assert.deepEqual(
				await page.locator('.bar [data-key]').evaluateAll((ks) => ks.map((k) => [k.dataset.key, k.classList.contains('required')])),
				[['amount', true], ['cups', false]]
			);
			await page.locator('.bar [data-back]').click();

			// Under a pending verb its flags come too, after the field, so
			// the frame is still written (canary finding on #273).
			await page.locator('.prompt-bar input').fill('moment -q ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="tea"]')?.dataset.act === 'fill');
			await chip(page, 'tea').click();
			await lineIs(page, 'moment tea -q ');
			await app.idle();
			assert.equal(await changeCounter(page), counter, 'the tap wrote');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a field whose keys all have defaults still runs at once under a verb, double-outlined', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			await page.locator('.prompt-bar input').fill('moment ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="juice"]')?.dataset.act === 'run');
			await app.pressWidget('juice');
			assert.equal((await app.echoes()).at(-1), 'moment juice');
		});
	});

	test('a block fills its start under `start`, and ends at once under `end`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of SEED) await type(page, line);
			await page.locator('.prompt-bar input').fill('start ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="shift"]')?.dataset.act === 'fill');
			await chip(page, 'shift').click();
			await lineIs(page, 'start shift ');

			// Not backdated: `track` wrote its TRACKED marker now, which would end it.
			await app.clock.forward(60_000);
			await type(page, 'start shift project=a');
			await app.pressWidget('shift', 'end');
			assert.equal((await app.echoes()).at(-1), 'end shift');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-7h73h: a field name may hold a space, so every line the web writes
	// with a field in it quotes the field (`shellJoin`), or `my coffee`
	// splits and the line logs to a field called `my`. Under its PILL is
	// `commands.test.mjs`'s.
	test('a spaced field: its fill quotes it under a pending verb, with its flags too, and Enter logs to it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await type(page, 'track "my coffee" --verb moment -k amount');
			// The clock is held: a log at the declaration's instant would share its slug.
			await app.clock.forward(1000);
			await app.idle();
			await page.locator('.prompt-bar input').fill('moment ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="my coffee"]')?.dataset.act === 'fill');
			await chip(page, 'my coffee').click();
			await lineIs(page, 'moment "my coffee" ');
			await page.keyboard.type('3');
			await page.keyboard.press('Enter');
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
			await app.idle();
			const [entry] = await app.entries(1);
			assert.deepEqual([entry.field, entry.data], ['my coffee', { amount: 3 }]);

			await page.locator('.prompt-bar input').fill('moment -q ');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="my coffee"]')?.dataset.act === 'fill');
			await chip(page, 'my coffee').click();
			await lineIs(page, 'moment "my coffee" -q ');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-pqbo2: a title that names a command names one that works when typed.
	test('a spaced field: the add chip’s title and a state’s title quote it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await type(page, 'setup "my notes" --verb add');
			await type(page, 'setup "my mood" --verb switch --states false,good');
			await app.idle();
			assert.match(await chip(page, 'my notes').getAttribute('title'), /add "my notes"/);
			await chip(page, 'my mood').click();
			const good = page.locator('.bar [data-state="good"]');
			assert.match(await good.getAttribute('title'), /^switch "my mood" good\b/);
			assert.deepEqual(app.errors, []);
		});
	});
});
