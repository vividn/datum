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
// Timing is read off the page, never slept through: the ghost is found by
// waiting for it, and the box's value is read in the SAME evaluation that
// finds it, so a fade that ends early cannot turn an assertion into a race.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp, seedLines } from './support/app.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 800 } };

// ONE APP for the ghost's own tests (da-6b3dj, sharedApp in
// support/app.mjs). What slows the runner (`slowRunner` installs one, which
// `solo` does not take out) or asks for no motion opens an app of its own.
const shared = sharedApp(DESKTOP);

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
	await seedLines(app, ['spec set coffee --verb occur']);
	await app.page.locator('.bar [data-field="coffee"]').waitFor();
}

/**
 * The ghost's text and, read in the same breath, what is in the box.
 *
 * One evaluation: the ghost is gone in about a second, so two round trips
 * could see the box after it rather than during it, and the claim is
 * precisely that the two are true AT THE SAME TIME.
 */
const ghostAndValue = (page) =>
	page
		.waitForFunction(() => {
			const ghost = document.querySelector('.prompt-bar .ghost');
			if (!ghost) return null;
			return { ghost: ghost.textContent, value: document.querySelector('.prompt-bar input').value };
		})
		.then((handle) => handle.jsonValue());

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
		await shared.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);

			await page.locator('.bar [data-field="coffee"]').click();
			const seen = await ghostAndValue(page);
			assert.deepEqual(seen, { ghost: 'occur coffee', value: '' });

			// Dimmer than a typed line, and brighter than the placeholder it
			// stands in for: read off the page, both.
			const shades = await page.evaluate(() => {
				const bar = document.querySelector('.prompt-bar');
				const input = bar.querySelector('input');
				return {
					ghost: getComputedStyle(bar.querySelector('.ghost')).color,
					typed: getComputedStyle(input).color,
					placeholder: getComputedStyle(input, '::placeholder').color
				};
			});
			assert.notEqual(shades.ghost, shades.typed, 'the ghost is as bright as a typed line');
			assert.notEqual(shades.ghost, shades.placeholder, 'the ghost is as dim as the placeholder');

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
			// *finally* appears" (da-jrozq). Found while the command is still
			// in flight — no echo yet — in the same evaluation, so a command
			// landing between two reads cannot make it pass.
			await page.evaluate(() => void window.datum.run(['occur', 'slowly']));
			const seen = await page
				.waitForFunction((n) => {
					const ghost = document.querySelector('.prompt-bar .ghost');
					if (!ghost) return null;
					return { ghost: ghost.textContent, landed: document.querySelectorAll('.repl .echo').length > n };
				}, echoes)
				.then((h) => h.jsonValue());
			assert.deepEqual(seen, { ghost: 'occur slowly', landed: false });

			// The shape of the fade, read off the animation held at two
			// instants: fully opaque at 350ms ("stay fully opaque for 250-500
			// ms"), and going by 700ms, and all of it inside GHOST_MS.
			const opacity = await page.locator('.prompt-bar .ghost').evaluate((el) => {
				const [fade] = el.getAnimations();
				fade.pause();
				const at = (ms) => ((fade.currentTime = ms), Number(getComputedStyle(el).opacity));
				return { held: at(350), going: at(700), total: fade.effect.getComputedTiming().endTime };
			});
			assert.equal(opacity.held, 1, 'the ghost was already fading 350ms in');
			assert.ok(opacity.going < 1 && opacity.going > 0, `the ghost read ${opacity.going} at 700ms`);
			assert.equal(opacity.total, 1000, 'the fade is no longer GHOST_MS long');
		}, DESKTOP);
	});

	test('a second press replaces the first and restarts the fade — never two at once', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);

			await page.locator('.bar [data-field="coffee"]').click();
			assert.deepEqual(await ghostAndValue(page), { ghost: 'occur coffee', value: '' });
			// Mark the first one, so the second is known to be another
			// element and not the same one living on.
			await page.locator('.prompt-bar .ghost').evaluate((el) => (el.dataset.mark = 'first'));

			await page.locator('.bar [data-field="coffee"]').click();
			// A fresh element AND a fade starting over: the element's own
			// animation says how far through it is.
			await page.waitForFunction(() => {
				const g = document.querySelector('.prompt-bar .ghost');
				return !!g && g.dataset.mark !== 'first' && (g.getAnimations()[0]?.currentTime ?? 0) < 400;
			});
			assert.equal(await page.locator('.prompt-bar .ghost').count(), 1, 'two ghosts at once');
		});
	});

	test('a keystroke takes the ghost away at once — the box is the reader’s', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);
			const input = page.locator('.prompt-bar input');

			await page.locator('.bar [data-field="coffee"]').click();
			assert.deepEqual(await ghostAndValue(page), { ghost: 'occur coffee', value: '' });

			// How far into its second the ghost is, read before the key —
			// a ghost that had already run out would make what follows
			// true of nothing.
			const age = await page
				.locator('.prompt-bar .ghost')
				.evaluate((el) => el.getAnimations()[0]?.currentTime ?? 0);
			assert.ok(age < 500, `the ghost was already ${age}ms old before the keystroke`);

			// A key that types nothing, so what clears the ghost is the
			// keystroke itself and not the box turning non-empty. Gone well
			// inside the second it had left, so the key is what took it:
			// 500 above + 400 here is still short of the animation's ~1000ms,
			// and only a ghost that goes inside that window proves the KEY
			// took it rather than its own life running out.
			//
			// The 400 is an ASSERTION on the elapsed time and not a wait's
			// cap (§16). It cannot be a cap: raised to the suite's 30s tier
			// it would stop checking anything, and left at 400 a loaded box
			// reports a bare TimeoutError that names neither the number nor
			// what it was for. Asserted, it says both.
			const pressed = Date.now();
			await input.press('ArrowLeft');
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));
			const took = Date.now() - pressed;
			assert.ok(took < 400, `the ghost took ${took}ms to go, long enough that it could have simply expired`);
			assert.equal(await input.inputValue(), '');
		});
	});

	test('the console ghosts too: a command nobody typed says what it ran', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.run(['occur', 'tea']).then(() => {}));
			assert.deepEqual(await ghostAndValue(page), { ghost: 'occur tea', value: '' });
		});
	});

	test('no motion asked for, no fade: the ghost is shown and then removed', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await withCoffee(app);

				await page.locator('.bar [data-field="coffee"]').click();
				assert.deepEqual(await ghostAndValue(page), { ghost: 'occur coffee', value: '' });
				assert.deepEqual(
					await page.locator('.prompt-bar .ghost').evaluate((el) => el.getAnimations().length),
					0,
					'the ghost fades for a reader who asked for no motion'
				);
				await gone(page);
			},
			{ ...DESKTOP, reducedMotion: 'reduce' }
		);
	});
});
