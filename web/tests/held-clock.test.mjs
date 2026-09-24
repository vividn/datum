// The harness holds every app's clock (da-ydao) — and holds only its `Date`.
//
// Unpinned clocks cost this suite three reds, each a bead: a `-t 0700` seed
// that was in the future before 07:00, and a minute tick landing inside a
// spy's window twice. So `withApp` holds the clock by default. It holds
// `Date` and nothing else, because Playwright's fake clock also replaces
// `requestAnimationFrame` with a timer, and a test that waits on frames
// then waits on something else (PR 27: 14 of 80 wheel reads unmoved).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const MINUTE = 60_000;

/** When `field` was logged, newest first, as epoch ms. */
const logged = async (app, field) =>
	(await app.entries()).filter((e) => e.field === field).map((e) => Date.parse(e.occur_time.utc));

describe('the held clock', () => {
	test('holds the page and the CLI at one instant, and app.clock moves both', async () => {
		await withApp(async (app) => {
			assert.equal(await app.page.evaluate(() => Date.now()), HELD_AT);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);

			await app.clock.forward(MINUTE);
			assert.equal(await app.page.evaluate(() => new Date().getTime()), HELD_AT + MINUTE);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);

			await app.clock.set(HELD_AT - MINUTE);
			assert.equal(app.clock.at(), HELD_AT - MINUTE);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);

			assert.deepEqual(await logged(app, 'coffee'), [HELD_AT + MINUTE, HELD_AT, HELD_AT - MINUTE]);
		});
	});

	test('a reload does not wind a moved clock back, on the page or in the CLI', async () => {
		await withApp(async (app) => {
			await app.clock.forward(MINUTE);
			await app.reload();
			assert.equal(await app.page.evaluate(() => Date.now()), HELD_AT + MINUTE);
			assert.equal((await app.run(['occur', 'tea'])).code, 0);
			assert.deepEqual(await logged(app, 'tea'), [HELD_AT + MINUTE]);
		});
	});

	test('holds Date alone: frames and timers are the browser’s own', async () => {
		await withApp(async ({ page }) => {
			const seen = await page.evaluate(async () => {
				const native = (f) => /\[native code\]/.test(Function.prototype.toString.call(f));
				const from = { date: Date.now(), perf: performance.now() };
				await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
				await new Promise((r) => setTimeout(r, 50));
				return {
					date: Date.now() - from.date,
					perf: performance.now() - from.perf,
					native: [requestAnimationFrame, setTimeout, performance.now].map(native)
				};
			});
			assert.equal(seen.date, 0, 'Date moved on its own');
			assert.ok(seen.perf >= 50, `performance.now() stood still: ${seen.perf}ms`);
			assert.deepEqual(seen.native, [true, true, true], 'a frame or a timer is not the browser’s');
		});
	});

	test('clock: null is the wall clock', async () => {
		await withApp(
			async (app) => {
				const drift = (await app.page.evaluate(() => Date.now())) - Date.now();
				assert.ok(Math.abs(drift) < MINUTE, `the page is ${drift}ms off the wall clock`);
				assert.equal(app.clock, undefined);
			},
			{ clock: null }
		);
	});
});
