// The input line's key frames (da-31d1d), in a browser: a reader typing
// `occur tea` for a field with a key of each kind (da-p3hxn), and what the
// bar draws and runs. The rules are `keyframes-pure.test.mjs`.
//
// vividn, 09-26: «a small frame with the name of the key, space advances
// (unless there are quotes), show the default value in ghost text for
// optional keys, maybe somehow visually differentiate optional vs required
// keys through frame color».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines } from './support/app.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp({ viewport: { width: 1280, height: 800 } });

const TEA = 'spec set tea --verb occur --data-key amount --data-key size= --data-key cups=1 --assign brand=acme';
const WORK = 'spec set work --verb start --data-key project --data-key n=1';

/** The bar's line, and the keys it frames: written ones over the text, waiting ones after it. */
const drawn = (page) =>
	page.evaluate(() => {
		const bar = document.querySelector('.prompt-bar');
		const style = (el) => ({ key: el.dataset.key, outline: getComputedStyle(el).outlineStyle, border: getComputedStyle(el).borderStyle });
		return {
			text: bar.querySelector('input').value,
			written: [...bar.querySelectorAll('.frame')].map(style),
			waiting: [...bar.querySelectorAll('.key')].map((el) => ({ ...style(el), label: el.textContent })),
			ghost: [...bar.querySelectorAll('.frame .default')].map((el) => el.textContent)
		};
	});

/** Type into the bar as a keyboard does, one key at a time. */
async function type(page, keys) {
	await page.locator('.prompt-bar input').focus();
	await page.keyboard.type(keys);
}

/** Enter, and the line gone: what ran is the last echo. */
async function enter(app) {
	await app.page.keyboard.press('Enter');
	await app.page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
	await app.idle();
}

describe('key frames', () => {
	// da-p3hxn: the CLI binds a spec's keys to positionals, so a frame holds a bare value.
	test('frames in order, required solid and the rest dashed, an assign none; `3 space 2` runs `occur tea 3 2`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);

			await type(page, 'occur tea');
			let now = await drawn(page);
			assert.deepEqual(now.written, []);
			assert.deepEqual(now.waiting.map((k) => [k.key, k.label, k.border]), [
				['amount', 'amount', 'solid'],
				['size', 'size', 'dashed'],
				['cups', 'cups1', 'dashed']
			], 'brand is assigned: fixed, and not typed');

			await type(page, ' 3 2');
			now = await drawn(page);
			assert.equal(now.text, 'occur tea 3 2', 'a space is only a space');
			assert.deepEqual(now.written.map((f) => [f.key, f.outline]), [['amount', 'solid'], ['size', 'dashed']]);
			assert.deepEqual(now.waiting.map((k) => k.key), ['cups']);

			// Each frame stands where its token does in the box: measured on
			// the mirror, which is the box's text in the box's font.
			const drift = await page.evaluate(() => {
				const bar = document.querySelector('.prompt-bar');
				const text = bar.querySelector('.mirror').firstChild;
				return [...bar.querySelectorAll('.frame')].map((frame, i) => {
					const range = document.createRange();
					const from = 'occur tea '.length + 2 * i;
					range.setStart(text, from);
					range.setEnd(text, from + 1);
					return Math.abs(range.getBoundingClientRect().left - frame.getBoundingClientRect().left);
				});
			});
			assert.ok(drift.length === 2 && drift.every((px) => px < 1), `frames off their tokens by ${drift} px`);

			await enter(app);
			assert.equal((await app.echoes()).at(-1), 'occur tea 3 2');
			assert.deepEqual((await app.entries(1))[0].data, { amount: 3, size: 2, cups: 1, brand: 'acme' });
			assert.deepEqual(app.errors, []);
		});
	});

	test('inside quotes a space is a space', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);
			await type(page, 'occur tea "a b" ');
			const now = await drawn(page);
			assert.equal(now.text, 'occur tea "a b" ');
			assert.deepEqual(now.written.map((f) => f.key), ['amount']);
		});
	});

	test('a space in an empty frame skips it; Enter leaves an empty optional absent and the default to the CLI', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);
			await type(page, 'occur tea 3  ');
			const now = await drawn(page);
			assert.equal(now.text, 'occur tea 3 cups=', 'size skipped: cups is named, as no bare word reaches it');
			assert.deepEqual(now.ghost, ['1'], 'the empty default ghosts its value');
			await enter(app);
			assert.equal((await app.echoes()).at(-1), 'occur tea 3', 'the default is not written onto the line');
			assert.deepEqual((await app.entries(1))[0].data, { amount: 3, cups: 1, brand: 'acme' });
			assert.deepEqual(app.errors, []);
		});
	});

	// da-cej7q: a state may declare keys of its own.
	test("a switch frames its state's own keys: sad asks for reason, happy for nothing", async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set mood --verb switch --state happy --state sad --state-key sad=reason']);
			await type(page, 'switch mood happy');
			assert.deepEqual((await drawn(page)).waiting, []);
			await page.locator('.prompt-bar input').fill('');
			await type(page, 'switch mood sad');
			assert.deepEqual((await drawn(page)).waiting.map((k) => [k.key, k.border]), [['reason', 'solid']]);
			await type(page, ' tired');
			await enter(app);
			assert.deepEqual((await app.entries(1))[0].data, { reason: 'tired' });
			assert.deepEqual(app.errors, []);
		});
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in
	// this one, so I think that should be the default. But allowing it might
	// also be nice??» — not prompted, and typed still runs.
	test('end draws no frames and Enter writes no defaults; a key typed on it runs as typed; start keeps its frames', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [WORK, 'start work project=a -t -30']);

			await type(page, 'start work');
			assert.deepEqual((await drawn(page)).waiting.map((k) => k.key), ['project', 'n']);
			await page.locator('.prompt-bar input').fill('');

			await type(page, 'end work ');
			const now = await drawn(page);
			assert.equal(now.text, 'end work ', 'a space after end is only a space');
			assert.deepEqual([now.written, now.waiting], [[], []]);

			await type(page, 'project=x');
			await page.keyboard.press('Enter');
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
			await app.idle();
			assert.equal((await app.echoes()).at(-1), 'end work project=x');
			assert.deepEqual((await app.entries(1))[0].data, { project: 'x' });
			assert.deepEqual(app.errors, []);
		});
	});
});
