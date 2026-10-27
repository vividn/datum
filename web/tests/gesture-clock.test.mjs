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
 * Keep the page busy for `ms`, as a slow phone's main thread is, in the
 * dispatch of the next `busy` event, so the handler that runs the command
 * waits for it; and keep when the next `type` event happened, on the page's
 * clock, and how long it waited. In the dispatch and not on a page timer: a
 * timer raced the input on a loaded runner, fired after it, and the input
 * did not wait at all (da-zibfx).
 */
const slowPhone = (page, type, ms, busy = type) =>
	page.evaluate(
		([type, ms, busy]) => {
			const stall = () => {
				const until = performance.now() + ms;
				while (performance.now() < until);
			};
			const seen = (e) => {
				const age = performance.now() - e.timeStamp;
				// Rounded as the app's stamp is (`gesture` in engine.ts, da-ku6zc).
				window.__gesture = { at: Math.round(Date.now() - age), age };
			};
			// The stall first: on one event, `seen` reads the wait it made.
			window.addEventListener(busy, stall, { capture: true, once: true });
			window.addEventListener(type, seen, { capture: true, once: true });
		},
		[type, ms, busy]
	);

/** How long a busy handler keeps the page, and so how late a process runs. */
const LATE = 400;

/** How long after a key the tests tap, so a tap stamped at the key is off by this. */
const APART = 100;

/**
 * The app's stamp against the test's: each is `Date.now() - age` read at its
 * own moment, so they differ by what the wall clock moved against
 * `performance.now()` in between — 2 ms apart once rounded (da-zibfx), 5 on a
 * loaded runner (da-2wp71). A wrong stamp is off by `APART` at least (the
 * key's, not the tap's) or `LATE` (the process's), so the tolerance is a
 * fraction of what it tells apart, and not the last skew seen plus a little.
 */
const near = (a, b) => Math.abs(a - b) <= APART / 4;

/** Wait for the REPL's `n`th echo. */
const echoed = (page, n) => page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length >= n, n);

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
				await page.locator('.prompt-bar input').fill('occur water');
				await slowPhone(page, 'keydown', LATE);
				await page.keyboard.press('Enter');
				await echoed(page, 1);
				const typed = await page.evaluate(() => window.__gesture);
				assert.ok(typed.age > LATE / 2, `the Enter waited ${typed.age} ms`);
				const water = await stamped(app, 'water');
				assert.ok(near(water.create, typed.at), `Enter at ${typed.at}, stamped ${water.create}`);
				assert.equal(water.occur, water.create);

				const box = await page.locator('.bar button.widget', { hasText: 'water' }).first().boundingBox();
				await slowPhone(page, 'click', LATE);
				await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
				await echoed(page, 2);
				const tapped = await page.evaluate(() => window.__gesture);
				assert.ok(tapped.age > LATE / 2, `the tap waited ${tapped.age} ms`);
				const again = await stamped(app, 'water');
				assert.ok(near(again.create, tapped.at), `tap at ${tapped.at}, stamped ${again.create}`);
				assert.equal(again.occur, again.create);

				// A key and then a tap, the tap waiting on the key's busy handler:
				// the browser runs the tap before the keydown's timer, and the tap
				// is its own.
				await slowPhone(page, 'click', LATE, 'keydown');
				const key = page.keyboard.press('ArrowRight');
				await new Promise((resolve) => setTimeout(resolve, APART));
				await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
				await key;
				await echoed(page, 3);
				const after = await page.evaluate(() => window.__gesture);
				const third = await stamped(app, 'water');
				assert.ok(near(third.create, after.at), `tap at ${after.at}, stamped ${third.create}`);

				// The key between the tap's down and up, as a loaded runner
				// delivered them (da-j0l7r): the pointerdown went before the key
				// was kept, and the tap is still its own.
				await slowPhone(page, 'click', LATE, 'keydown');
				await page.mouse.down();
				const mid = page.keyboard.press('ArrowRight');
				await new Promise((resolve) => setTimeout(resolve, APART));
				await page.mouse.up();
				await mid;
				await echoed(page, 4);
				const up = await page.evaluate(() => window.__gesture);
				const fourth = await stamped(app, 'water');
				assert.ok(near(fourth.create, up.at), `tap at ${up.at}, stamped ${fourth.create}`);

				// Enter on the focused button: its click is the key's, made after
				// the busy handler, and the entry is stamped at the keydown.
				await page.locator('.bar button.widget', { hasText: 'water' }).first().focus();
				await slowPhone(page, 'keydown', LATE);
				await page.keyboard.press('Enter');
				await echoed(page, 5);
				const entered = await page.evaluate(() => window.__gesture);
				const fifth = await stamped(app, 'water');
				assert.ok(near(fifth.create, entered.at), `Enter at ${entered.at}, stamped ${fifth.create}`);
			},
			{ clock: null }
		);
	});
});
