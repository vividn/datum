// The input line's key frames (da-31d1d), in a browser: a reader typing
// `occur tea` for a field with one required key and one optional, and
// what the bar draws and runs. The rules are `keyframes-pure.test.mjs`.
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

const TEA = 'spec set tea --verb occur --data-key amount --data-key cups=1';
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

describe('key frames', () => {
	test('two frames in order, required solid and optional dashed with its default ghosted; space fills them in turn', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);

			await type(page, 'occur tea');
			let now = await drawn(page);
			assert.deepEqual(now.written, []);
			assert.deepEqual(now.waiting.map((k) => [k.key, k.label, k.border]), [['amount', 'amount', 'solid'], ['cups', 'cups1', 'dashed']]);

			await type(page, ' ');
			now = await drawn(page);
			assert.equal(now.text, 'occur tea amount=');
			assert.deepEqual(now.written.map((f) => [f.key, f.outline]), [['amount', 'solid']]);
			assert.deepEqual(now.waiting.map((k) => k.key), ['cups']);

			await type(page, '3 ');
			now = await drawn(page);
			assert.equal(now.text, 'occur tea amount=3 cups=');
			assert.deepEqual(now.written.map((f) => [f.key, f.outline]), [['amount', 'solid'], ['cups', 'dashed']]);
			assert.deepEqual(now.ghost, ['1'], 'the empty optional ghosts its default');
			assert.deepEqual(now.waiting, []);

			// Each frame stands where its token does in the box: measured on
			// the mirror, which is the box's text in the box's font.
			const drift = await page.evaluate(() => {
				const bar = document.querySelector('.prompt-bar');
				const text = bar.querySelector('.mirror').firstChild;
				const value = bar.querySelector('input').value;
				return [...bar.querySelectorAll('.frame')].map((frame) => {
					const range = document.createRange();
					const from = value.indexOf(`${frame.dataset.key}=`);
					range.setStart(text, from);
					range.setEnd(text, from + 1);
					return Math.abs(range.getBoundingClientRect().left - frame.getBoundingClientRect().left);
				});
			});
			assert.ok(drift.every((px) => px < 1), `frames off their tokens by ${drift} px`);
		});
	});

	test('inside quotes a space is a space', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);
			await type(page, 'occur tea "a b" ');
			assert.equal((await drawn(page)).text, 'occur tea amount="a b" cups=');
		});
	});

	test('Enter writes an empty optional key its default, and runs what the line says', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, [TEA]);
			await type(page, 'occur tea 3 ');
			await page.keyboard.press('Enter');
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
			await app.idle();
			assert.equal((await app.echoes()).at(-1), 'occur tea amount=3 cups=1');
			assert.deepEqual((await app.entries(1))[0].data, { amount: 3, cups: 1 });
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
