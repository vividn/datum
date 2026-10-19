// An entry's time is the moment the person acted (da-szzgy): the CLI's wall
// clock is frozen at the tap, Enter or Save that ran it, by the event's own
// stamp, not read when the process got to run.
//
// vividn: «freeze the embeded wasm cli's idea of what time it is at the exact
// moment that a person taps or presses enter … on slower phones».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const FROZEN_AT = 'DATUM_WEB_FROZEN_AT=';

/** `field`'s newest entry: its time and its create time, as epoch ms. */
const stamped = async (app, field) => {
	const row = (await app.entries()).find((e) => e.field === field);
	return { occur: Date.parse(row.occur_time.utc), create: Date.parse(row.create_time) };
};

/**
 * Keep the page busy for `ms`, as a slow phone's main thread is, so the next
 * input waits for it; and keep when the `type` event happened, on the page's
 * clock, and how long it waited.
 */
const slowPhone = async (page, type, ms) => {
	await page.evaluate(
		([type, ms]) => {
			const seen = (e) => {
				const age = performance.now() - e.timeStamp;
				window.__gesture = { at: Date.now() - age, age };
			};
			window.addEventListener(type, seen, { capture: true, once: true });
			setTimeout(() => {
				const until = performance.now() + ms;
				while (performance.now() < until);
			}, 50);
		},
		[type, ms]
	);
	await new Promise((resolve) => setTimeout(resolve, 100));
};

/** Wait for the REPL's `n`th echo. */
const echoed = (page, n) => page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length >= n, n);

describe('the clock frozen at the gesture', () => {
	// On the wall clock: under a held one the harness gives the CLI the held
	// instant, a tap's too (`holdDate`), and nothing would be late.
	test('a command run on a frozen instant seconds back is stamped there, to the millisecond', async () => {
		await withApp(
			async (app) => {
				const at = Math.floor(await app.page.evaluate(() => Date.now())) - 7_000 + 123;
				const env = [`${FROZEN_AT}${at}`];
				const r = await app.page.evaluate((env) => window.datum.runQuiet(['occur', 'coffee'], undefined, undefined, undefined, undefined, env), env);
				assert.equal(r.code, 0, r.stderr);
				assert.deepEqual(await stamped(app, 'coffee'), { occur: at, create: at });
				// One process's: the next run is on the clock again.
				const before = await app.page.evaluate(() => Date.now());
				assert.equal((await app.run(['occur', 'tea'])).code, 0);
				assert.ok((await stamped(app, 'tea')).create >= before);
			},
			{ clock: null }
		);
	});

	test('the bar’s Enter and a widget tap carry the instant of the event, not of the process', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const LATE = 400;
				await page.locator('.prompt-bar input').fill('occur water');
				await slowPhone(page, 'keydown', LATE);
				await page.keyboard.press('Enter');
				await echoed(page, 1);
				const typed = await page.evaluate(() => window.__gesture);
				assert.ok(typed.age > LATE / 2, `the Enter waited ${typed.age} ms`);
				const water = await stamped(app, 'water');
				assert.ok(Math.abs(water.create - typed.at) <= 1, `Enter at ${typed.at}, stamped ${water.create}`);
				assert.equal(water.occur, water.create);

				const box = await page.locator('.bar button.widget', { hasText: 'water' }).first().boundingBox();
				await slowPhone(page, 'click', LATE);
				await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
				await echoed(page, 2);
				const tapped = await page.evaluate(() => window.__gesture);
				assert.ok(tapped.age > LATE / 2, `the tap waited ${tapped.age} ms`);
				const again = await stamped(app, 'water');
				assert.ok(Math.abs(again.create - tapped.at) <= 1, `tap at ${tapped.at}, stamped ${again.create}`);
				assert.equal(again.occur, again.create);

				// A key and then a tap, both waiting on the busy page: the browser
				// runs the tap before the keydown's timer, and the tap is its own.
				await slowPhone(page, 'click', LATE);
				const key = page.keyboard.press('ArrowRight');
				await new Promise((resolve) => setTimeout(resolve, 100));
				await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
				await key;
				await echoed(page, 3);
				const after = await page.evaluate(() => window.__gesture);
				const third = await stamped(app, 'water');
				assert.ok(Math.abs(third.create - after.at) <= 1, `tap at ${after.at}, stamped ${third.create}`);
			},
			{ clock: null }
		);
	});
});
