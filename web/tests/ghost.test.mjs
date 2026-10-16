// What the bottom bar's box does after Enter (da-jprg).
//
// vividn: "after hitting enter on a command the command persists in the input
// box for a few seconds, bad UX". It did: the box emptied on Enter and the
// bar put the line BACK when the command landed — a wait that grew with the
// command, so the line reappeared after the reader had looked away and went
// again 1.5s later. Worse, it came back as the input's VALUE, which reads as
// text the reader typed and may edit.
//
// So: a TYPED line never returns to the box. A command the reader did NOT
// type — a fields-widget press, the console — says what it ran as a ghost
// where the placeholder goes, dim and fading within about a second, and is
// never the value. The assertion that carries this file is `inputValue() ===
// ''` WHILE the ghost is on screen; it is what fails against the old code.
//
// Timing is read off the page, never slept through: the ghost is recorded by
// `app.watchGhosts()`, installed before the press, in the page as it is
// inserted — with the box's value beside it in the same record — so a ghost
// gone before the test's first round trip is read all the same (da-vp0qc).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { along, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 800 } };

// ONE APP for the ghost's own tests (da-6b3dj, sharedApp in
// support/app.mjs). What slows the runner (`slowRunner` installs one, which
// `solo` does not take out) or asks for no motion opens an app of its own.
const shared = sharedApp(DESKTOP);
// `main` as a draft of the REPL and the widget bar alone: nothing here
// reads the rest of the day frame, and drawing it costs what a boot did
// (da-m1m9a).
const BAR = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));
const onBar = (fn) => shared.solo(fn, BAR);

/**
 * Make any command carrying `slowly` take `ms`, through the seam's own
 * extension point — so a test can type into the box while a command it
 * started is still in flight. Everything else runs at its own speed: the
 * frame re-runs every pane on each command, and slowing those too would
 * cost the suite minutes to prove nothing.
 */
async function slowRunner(page, ms) {
	await page.evaluate(
		async ([ms]) => {
			await window.datum.runQuiet(['tail']);
			const previous = window.datum.setRunner(async (argv, ...rest) => {
				if (argv.includes('slowly')) await new Promise((r) => setTimeout(r, ms));
				return previous(argv, ...rest);
			});
		},
		[ms]
	);
}

/** Track `coffee` and get its button onto the widget bar. */
async function withCoffee(app) {
	// Typed, not quiet: a write is what refreshes the bar, a read does not (da-fprk0).
	await seedLines(app, ['setup coffee --verb occur']);
	await app.page.locator('.bar [data-field="coffee"]').waitFor();
}

/**
 * What `saw` (from `app.watchGhosts()`) recorded of the ghost reading
 * `line`: every value the box held while it was up — the claim is that the
 * two are true AT THE SAME TIME, so each record pairs them — and its `look`
 * as it was inserted.
 */
const ghostOf = async (saw, line) => {
	const ghosts = (await saw(line)).filter((g) => g.line === line);
	return { values: [...new Set(ghosts.map((g) => g.value))], look: ghosts[0].look };
};

const gone = (page) => page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));

describe('the bar after Enter', () => {
	test('a typed line is gone on Enter and does not come back when the command lands', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await slowRunner(page, 700);
			const input = page.locator('.prompt-bar input');
			const echoes = await page.locator('.repl .echo').count();

			await input.fill('occur slowly');
			await input.press('Enter');
			// Empty at once — a shell consumes the line it is given.
			assert.equal(await input.inputValue(), '');

			// And still empty when the command finally lands, which is the
			// moment the old code refilled it.
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);
			await app.idle();
			assert.equal(await input.inputValue(), '');
			assert.equal(await page.locator('.prompt-bar .ghost').count(), 0, 'a typed line ghosted');

			// Recall is untouched: what was typed is still an Up away.
			await input.press('ArrowUp');
			assert.equal(await input.inputValue(), 'occur slowly');
		}, DESKTOP);
	});

	test('a line typed while a command is in flight is not overwritten, by the command or by a press', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await withCoffee(app);
			await slowRunner(page, 700);
			const input = page.locator('.prompt-bar input');
			const echoes = await page.locator('.repl .echo').count();

			await input.fill('occur slowly');
			await input.press('Enter');
			// The reader starts the next line before the last one is done.
			await input.fill('occur half-typed');
			// A press lands in the middle of it, too.
			await page.locator('.bar [data-field="coffee"]').click();

			await page.waitForFunction(
				(n) => document.querySelectorAll('.repl .echo').length > n + 1,
				echoes
			);
			await app.idle();
			assert.equal(await input.inputValue(), 'occur half-typed');
			// And the ghost keeps out of the way of a box with text in it.
			assert.equal(await page.locator('.prompt-bar .ghost').count(), 0);
		}, DESKTOP);
	});
});

describe('the ghost', () => {
	test('a press shows the argv it ran without it becoming the input’s value, and fades within a second', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await withCoffee(app);

			const saw = await app.watchGhosts();
			await page.locator('.bar [data-field="coffee"]').click();
			const { values, look } = await ghostOf(saw, 'occur coffee');
			assert.deepEqual(values, [''], 'the press became the input’s value');

			// Dimmer than a typed line, and brighter than the placeholder it
			// stands in for: read off the page, both.
			assert.notEqual(look.shade, look.typed, 'the ghost is as bright as a typed line');
			assert.notEqual(look.shade, look.placeholder, 'the ghost is as dim as the placeholder');

			const from = Date.now();
			await gone(page);
			const took = Date.now() - from;
			assert.ok(took < 2500, `the ghost was still there ${took}ms after it was seen`);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '');
		});
	});

	test('the ghost is up at the press, before the command lands, and holds before it fades', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await slowRunner(page, 700);
			const echoes = await page.locator('.repl .echo').count();

			// vividn: "~1.5 s later, finally the ghost text of the command
			// *finally* appears" (da-jrozq). The REPL's echoes counted as the
			// ghost is inserted: none new, so the command was still in flight.
			//
			// And the shape of the fade, its animation sought to two instants
			// as it was inserted: fully opaque at 350ms ("stay fully opaque
			// for 250-500 ms"), and going by 700ms, and all of it inside
			// GHOST_MS.
			const saw = await app.watchGhosts();
			await page.evaluate(() => void window.datum.run(['occur', 'slowly']));
			const { look } = await ghostOf(saw, 'occur slowly');
			assert.equal(look.echoes, echoes, 'the ghost waited for the command to land');
			assert.equal(look.fade[350], 1, 'the ghost was already fading 350ms in');
			assert.ok(look.fade[700] < 1 && look.fade[700] > 0, `the ghost read ${look.fade[700]} at 700ms`);
			assert.equal(look.fade.total, 1000, 'the fade is no longer GHOST_MS long');
		}, DESKTOP);
	});

	test('a second press replaces the first and restarts the fade — never two at once', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await withCoffee(app);

			// Each element the watch sees is numbered (`nth`), so the second
			// is known to be another element and not the same one living on.
			const saw = await app.watchGhosts();
			await page.locator('.bar [data-field="coffee"]').click();
			assert.deepEqual((await ghostOf(saw, 'occur coffee')).values, ['']);

			await page.locator('.bar [data-field="coffee"]').click();
			// A fresh element AND a fade starting over: the element's own
			// animation says how far through it is.
			const ghosts = await page
				.waitForFunction(() => window.__datumGhosts.some((g) => g.look.nth === 2) && window.__datumGhosts)
				.then((h) => h.jsonValue());
			const second = ghosts.find((g) => g.look.nth === 2);
			assert.equal(second.line, 'occur coffee');
			assert.ok(second.look.age < 400, `the second ghost's fade began ${second.look.age}ms in`);
			assert.deepEqual([...new Set(ghosts.map((g) => g.count))], [1], 'two ghosts at once');
		});
	});

	test('a keystroke takes the ghost away at once — the box is the reader’s', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await withCoffee(app);
			const input = page.locator('.prompt-bar input');

			const saw = await app.watchGhosts();
			await page.locator('.bar [data-field="coffee"]').click();
			assert.deepEqual((await ghostOf(saw, 'occur coffee')).values, ['']);

			// A key that types nothing, so what clears the ghost is the
			// keystroke itself and not the box turning non-empty. Read in the
			// page, on either side of the key: the ghost's age as the key
			// arrives, and whether it is still there one task later — the
			// key's own update lands before that, and the ghost's timer, due
			// at GHOST_MS, is not due yet at an age short of it. So a ghost
			// gone then is one the KEY took, not its own life running out,
			// however slow the runner is between the reads.
			await page.evaluate(() => {
				window.__datumKey = new Promise((resolve) =>
					addEventListener(
						'keydown',
						() => {
							const age = document.querySelector('.prompt-bar .ghost')?.getAnimations()[0]?.currentTime;
							setTimeout(() => resolve({ age, after: !!document.querySelector('.prompt-bar .ghost') }));
						},
						{ capture: true, once: true }
					)
				);
			});
			await input.press('ArrowLeft');
			const key = await page.evaluate(() => window.__datumKey);
			assert.ok(key.age != null, 'the ghost was gone before the key');
			assert.ok(key.age < 800, `the ghost was ${key.age}ms old at the key, too near its own end to tell`);
			assert.equal(key.after, false, 'the ghost outlived the keystroke');
			assert.equal(await input.inputValue(), '');
		});
	});

	test('the console ghosts too: a command nobody typed says what it ran', async () => {
		await onBar(async (app) => {
			const { page } = app;
			// Watched from before the call: the ghost is up from it for
			// GHOST_MS, and a wait begun after it — even one round trip
			// after — can begin after the ghost has gone (da-pzfdz, da-ohoei).
			const saw = await app.watchGhosts();
			await page.evaluate(() => void window.datum.run(['occur', 'tea']));
			assert.deepEqual((await ghostOf(saw, 'occur tea')).values, [''], 'the console\'s command became the input\'s value');
			await app.idle();
		});
	});

	test('no motion asked for, no fade: the ghost is shown and then removed', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await withCoffee(app);

				const saw = await app.watchGhosts();
				await page.locator('.bar [data-field="coffee"]').click();
				const { values, look } = await ghostOf(saw, 'occur coffee');
				assert.deepEqual(values, ['']);
				assert.equal(look.animations, 0, 'the ghost fades for a reader who asked for no motion');
				await gone(page);
			},
			{ ...DESKTOP, reducedMotion: 'reduce' }
		);
	});
});
