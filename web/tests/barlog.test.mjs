// The log under the bar, and the two gestures that reach it (da-ckw3).
//
// vividn, 2026-09-17: "clicking and dragging the input bar up, or hovering
// the mouse over the input bar and scrolling down scrolls the page down to
// reveal the reversed REPL, and the ability to go back in time to view
// previous commands and their outputs. Scrolling or dragging anywhere else
// on the page should not go down into the repl section. When scrolled down
// into the REPL, put a (return to top) at the bottom, which restores back
// to the dashboard with the input section down in its normal place. When
// scrolling deep into the repl, the input section still stays at the
// top(!)".
//
// So there are four claims here and they are all measured on the page:
//
// 1. the bar's own gestures move it — a touch drag on a phone, a wheel on
//    a desktop — and when they have, the input section is at the top of
//    the page with the newest entry directly under it and older ones
//    below;
// 2. no other gesture moves it: a wheel over a pane, a drag that starts on
//    a pane, and a drag on the bar too small to be one;
// 3. `close history` — `return to top` when he named it — is pinned to the
//    BOTTOM of the screen, not to the end
//    of a history that can be thousands of entries long — and Escape does
//    the same thing;
// 4. the bar is still the bar down there: a tap on the box still focuses
//    it, the mark's menu still opens, and a command run from inside the
//    log lands at the TOP of it.
//
// What the log shows is `Repl.svelte` with `reversed` (da-gvdc), the same
// component a layout can hold, reading the same scrollback store. So this
// file does not re-assert what a REPL draws — repl.test.mjs does that. It
// asserts where it is and how you get to it. A bare `Pane` has no `.repl`
// class (that is the layout frame's), so the log is read through `.log`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { swipe } from './support/layout.mjs';
// Synthesised rather than driven through `page.touchscreen`, because what is
// asserted is the POINTER contract the bar listens on — down, a run of
// moves, up — and a test that sent two events could not tell a threshold
// that works from one that never fires. With one stamp, so no speed.
import { dragOn as dragBy } from './support/bar.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 }, query: 'repl' };
const PHONE = {
	viewport: { width: 390, height: 664 },
	device: { isMobile: true, hasTouch: true },
	query: 'repl'
};

const BAR = '.prompt-bar';

// ONE desktop app for the three tests below that begin and end with the
// bar at rest and read the log from its newest row (da-6b3dj, sharedApp in
// support/app.mjs) — tidied by hand, not by `solo`, which would close the
// repl tab under them. Everything else opens its own: a phone starts
// FOLDED and the device remembers every fold a test leaves it in, a drag's
// swallow window outlives the test that armed it, and a count of the log's
// or the pane's rows would count the tests before.
const shared = sharedApp(DESKTOP);
const desktop = async (fn) => {
	const app = await shared();
	await app.home();
	await app.empty();
	return fn(app);
};

/** How far the bar is open, and where the things that move with it are. */
const where = (page) =>
	page.evaluate(() => {
		const box = (s) => {
			const el = document.querySelector(s);
			if (!el) return null;
			const { top, bottom, left, right, height } = el.getBoundingClientRect();
			return { top, bottom, left, right, height };
		};
		const px = (name) =>
			Math.round(parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue(name)) || 0);
		return {
			pages: box('.pages'),
			bar: box('.prompt-bar'),
			input: box('.prompt-bar .section.input'),
			out: box('.prompt-bar .section.output'),
			log: box('.log'),
			top: box('.log .top'),
			main: box('main'),
			shown: px('--shown'),
			height: px('--log-height'),
			// How far the list has scrolled, in its two halves: the bar's own
			// row riding off the top, and then the log's box (da-tj6m).
			rolled: px('--rolled'),
			scroll: Math.round(document.querySelector('.log .body')?.scrollTop ?? 0)
		};
	});

/**
 * Wait until the bar has stopped moving.
 *
 * It settles on an animation frame curve now ($lib/log.ts) rather than by a
 * scroll, so what a test waits for is the number holding still — two reads
 * the same, a few frames apart. ALL of the number: past the ceiling the
 * bar's height stops changing and the list scrolls instead (da-tj6m), so a
 * watch on `--log-height` alone would call a glide through the history
 * finished before it began.
 */
const settled = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				let last = '';
				let same = 0;
				const tick = () => {
					const style = getComputedStyle(document.querySelector('.shell'));
					const px = (name) => Math.round(parseFloat(style.getPropertyValue(name)) || 0);
					const now = `${px('--log-height')}/${px('--rolled')}/${Math.round(
						document.querySelector('.log .body')?.scrollTop ?? 0
					)}`;
					same = now === last ? same + 1 : 0;
					last = now;
					if (same > 4) return done();
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/** The log's echoed command lines, as it draws them: newest first. */
const logEchoes = async (page) =>
	(await page.locator('.log .echo').allInnerTexts()).map((t) => t.replace(/^\s*❯\s*/, '').trim());

/** A wheel of `dy` with the pointer over `selector`'s middle. */
async function wheelOver(page, selector, dy) {
	const box = await page.locator(selector).first().boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.wheel(0, dy);
}

describe('the bar is the way in', () => {
	test('a finger dragging the bar up puts the input at the top with the newest entry under it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await app.submit(line);
			await app.idle();

			const rest = await where(page);
			assert.equal(rest.height, 0, 'the log was open before anything pulled it');
			assert.equal(rest.top, null, '`close history` shows while the dashboard does');

			// Far enough to pass the notch and open the log to its tallest.
			await dragBy(page, BAR, -900);
			await settled(page);

			const deep = await where(page);
			// The top of the PAGE — the root tab bar is the app's own chrome
			// and stands above it however far the bar is opened.
			assert.ok(deep.height > 0, 'the drag opened no log at all');
			// The BAR's top, which is the input row plus the hairline above it.
			assert.ok(
				Math.abs(deep.bar.top - deep.pages.top) <= 1,
				`the bar is at ${deep.bar.top}, the page starts at ${deep.pages.top}`
			);
			assert.ok(deep.main.height <= 1, `the dashboard kept ${deep.main.height}px at the log's tallest`);
			assert.ok(
				deep.log.top >= deep.input.bottom - 1,
				`the log starts at ${deep.log.top}, above the input's ${deep.input.bottom}`
			);
			// ONE LIST, and the bar's output section is row one of it (vividn,
			// 2026-09-21: "it shouldn't be repeated when you drag it up").
			// So the newest is in the BAR, and the log carries on from the
			// second — newest first, directly under the input row.
			assert.deepEqual(
				await page.locator(`${BAR} .section.output .echo`).allInnerTexts(),
				['❯ occur tea']
			);
			assert.deepEqual(await logEchoes(page), ['occur water', 'occur coffee']);

			// A command run from in here becomes row one, and the one it
			// displaces moves to the top of the log — still no repeat.
			await app.submit('occur toast');
			await app.idle();
			assert.deepEqual(
				await page.locator(`${BAR} .section.output .echo`).allInnerTexts(),
				['❯ occur toast']
			);
			assert.deepEqual((await logEchoes(page))[0], 'occur tea');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// UNDER THE THRESHOLD, NOTHING IS PREVENTED. A drag on the bar takes the
	// gesture — no text selection, no caret dragged through the line — but
	// only once the finger has said it means one; until then the move must
	// pass through untouched, or a tap on the box loses its caret and its
	// keyboard (the bead's trap, and keyboard.test.mjs's contract).
	//
	// Asserted on the EVENT rather than on what the box looks like
	// afterwards: `dispatchEvent` answers false exactly when something
	// called `preventDefault`, so this is the claim itself and not a proxy
	// for it. Two canary rounds read the code as preventing before the
	// threshold; this is the measurement that settles it either way.
	test('a move under the threshold is not prevented; past it, every move is', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const prevented = await page.evaluate(() => {
				const el = document.querySelector('.prompt-bar');
				const r = el.getBoundingClientRect();
				const x = r.left + r.width / 2;
				const y = r.top + r.height / 2;
				// `dispatchEvent` is false when a listener prevented the default.
				const send = (type, at) =>
					!el.dispatchEvent(
						new PointerEvent(type, {
							pointerId: 11,
							pointerType: 'touch',
							isPrimary: true,
							clientX: x,
							clientY: at,
							bubbles: true,
							cancelable: true
						})
					);
				send('pointerdown', y);
				const tiny = send('pointermove', y - 4);
				const nearly = send('pointermove', y - 7);
				const past = send('pointermove', y - 40);
				const after = send('pointermove', y - 80);
				send('pointerup', y - 80);
				return { tiny, nearly, past, after };
			});
			assert.deepEqual(prevented, { tiny: false, nearly: false, past: true, after: true });
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// A drag ends in a click the browser dispatches from the pointerup it
	// came from, and that one click is the drag's tail, not a press. It is
	// swallowed — and ONLY it: the window is a safety net for a drag that
	// produced no click at all, and the first click inside it closes the
	// window. A canary on PR #46 read it as "any click on the bar for 300ms",
	// which would take the caret away from a finger that dragged and then
	// pressed it quickly. This is that claim, at the event level.
	test('a drag swallows the one click it ends in, and no other', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await page.evaluate(() => {
				const el = document.querySelector('.prompt-bar');
				const r = el.getBoundingClientRect();
				const x = r.left + r.width / 2;
				const y = r.top + r.height / 2;
				const pointer = (type, at) =>
					el.dispatchEvent(
						new PointerEvent(type, {
							pointerId: 12,
							pointerType: 'touch',
							isPrimary: true,
							clientX: x,
							clientY: at,
							bubbles: true,
							cancelable: true
						})
					);
				// `dispatchEvent` is false when a listener prevented the default,
				// which is what the swallow does.
				const click = () =>
					!el.dispatchEvent(new MouseEvent('click', { clientX: x, clientY: y, bubbles: true, cancelable: true }));

				const before = click();
				pointer('pointerdown', y);
				pointer('pointermove', y - 40);
				pointer('pointermove', y - 80);
				pointer('pointerup', y - 80);
				const tail = click();
				const next = click();
				return { before, tail, next };
			});
			// Nothing before the drag, the drag's own click, and then nothing
			// again — inside the same 300ms, because none of this waits.
			assert.deepEqual(seen, { before: false, tail: true, next: false });
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('a wheel over the bar goes down and comes back; over a pane it moves nothing', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await app.idle();

			// A desktop starts with the output already open, so what is asked
			// of a wheel over a pane is that it changes NOTHING, not that it
			// leaves the bar shut.
			const before = await where(page);
			await wheelOver(page, 'section.pane', 400);
			await page.waitForTimeout(250);
			const after = await where(page);
			assert.deepEqual(
				[after.shown, after.height],
				[before.shown, before.height],
				'a wheel over a pane moved the bar'
			);

			// A notch per wheel — from a desktop's open output, one opens the log.
			await wheelOver(page, BAR, 120);
			await settled(page);
			assert.ok((await where(page)).height > 0, 'a wheel over the bar opened no log');

			await wheelOver(page, BAR, -120);
			await settled(page);
			assert.equal((await where(page)).height, 0, 'wheeling back left the log open');
			assert.deepEqual(app.errors, []);
		});
	});

	// A pointercancel BEFORE the threshold: the system took a gesture that was
	// still a tap. Nothing was armed, nothing moved, and nothing settles — a
	// canary on PR #46 asked twice whether a cancel between `arm()` and
	// `dragging` could settle a bar nobody dragged. It cannot: `dragging` is
	// set BEFORE `arm()`, so there is no such moment, and this holds it that
	// way if the two are ever reordered.
	test('a cancel before the threshold moves nothing and arms nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const before = await where(page);
			const seen = await page.evaluate(() => {
				const el = document.querySelector('.prompt-bar');
				const r = el.getBoundingClientRect();
				const x = r.left + r.width / 2;
				const y = r.top + r.height / 2;
				const send = (type, at) =>
					el.dispatchEvent(
						new PointerEvent(type, {
							pointerId: 13,
							pointerType: 'touch',
							isPrimary: true,
							clientX: x,
							clientY: at,
							bubbles: true,
							cancelable: true
						})
					);
				send('pointerdown', y);
				send('pointermove', y - 4);
				send('pointercancel', y - 4);
				return { log: !!document.querySelector('.log .repl, .log section.pane') };
			});
			await settled(page);
			assert.equal(seen.log, false, 'a cancelled tap mounted the log');
			assert.deepEqual(await where(page), before, 'a cancelled tap moved the bar');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// `inert` is not only a Tab-order rule: an inert subtree is not focusable
	// at all, so a `.focus()` on something inside it does nothing either. A
	// canary on PR #46 said twice that programmatic focus gets through. It
	// does not — measured here, both ways round, so the claim is settled by
	// the browser rather than by either of us. (`Repl.svelte` calls `.focus()`
	// nowhere, which is the other half of the answer.)
	test('inert on the log refuses a programmatic focus too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await page.evaluate(() => {
				const log = document.querySelector('.log');
				const probe = document.createElement('button');
				probe.textContent = 'probe';
				log.append(probe);
				probe.focus();
				const withInert = { inert: log.hasAttribute('inert'), took: document.activeElement === probe };
				log.removeAttribute('inert');
				probe.focus();
				const without = { inert: log.hasAttribute('inert'), took: document.activeElement === probe };
				probe.remove();
				return { withInert, without };
			});
			assert.deepEqual(seen, {
				withInert: { inert: true, took: false },
				without: { inert: false, took: true }
			});
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// A REAL FINGER, on the BOX. Everything else in this file dispatches
	// synthetic `PointerEvent`s, which a browser never cancels — so a
	// gesture the browser would have taken for itself looks fine to them.
	// This one goes through CDP's `Input.dispatchTouchEvent`, which is the
	// only way to find out what `touch-action` does.
	//
	// It is here because making `.field` a scroll container (the pills ride
	// its run) silently took the bar's `touch-action: pan-x` out of the
	// resolution for any touch landing on the box — the value is resolved up
	// to the NEAREST SCROLL CONTAINER and no further — so the browser claimed
	// every upward drag from the box, cancelled the pointer after one move,
	// and the bar came back shut. vremya measured it on PR #46 with seven
	// drags; this is that measurement, kept.
	test('a real finger dragging up FROM THE BOX opens the log', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			const box = await page.locator(`${BAR} input`).boundingBox();
			await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 0, -200, 10);
			await settled(page);
			const seen = await where(page);
			assert.ok(
				seen.height > 0,
				`a drag from the box left the log at ${seen.height}px — the browser took the gesture`
			);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// The other half of the same declaration: sideways on the box is still
	// the RUN's, so a pill off the right edge is still reachable and the bar
	// does not move for it.
	test('a real finger dragging sideways on the box scrolls the run, not the bar', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.evaluate(() =>
				window.datum.bar.setPills([
					{ id: 'when', text: '-t 09:00', kind: 'time' },
					{ id: 'day', text: '-d yesterday', kind: 'time' }
				])
			);
			await page.locator(`${BAR} .pill`).nth(1).waitFor();
			const before = await where(page);
			// The FIELD's box, not the input's: the input's width is the
			// field's measured back through a resize observer, so for a frame
			// after the pills land it is still the wider pill-less one, and a
			// finger placed from it lands past the field's edge (da-xix5m).
			const box = await page.locator(`${BAR} .field`).boundingBox();
			await swipe(page, box.x + box.width - 8, box.y + box.height / 2, -120, 0, 10);
			await settled(page);
			const scrolled = await page.locator(`${BAR} .field`).evaluate((el) => el.scrollLeft);
			assert.ok(scrolled > 0, `the run did not scroll: scrollLeft ${scrolled}`);
			assert.deepEqual(await where(page), before, 'a sideways drag moved the bar');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('a drag that starts on a pane never opens the bar', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await app.idle();

			const before = await where(page);
			await dragBy(page, 'section.pane', -240);
			await page.waitForTimeout(250);
			const after = await where(page);
			assert.deepEqual(
				[after.shown, after.height],
				[before.shown, before.height],
				'a drag on a pane moved the bar'
			);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

describe('a tap during a settle', () => {
	// A press STOPS a glide, because a finger must win over an animation —
	// and then it owes the bar an ending. Two claims, and the second is the
	// one a canary on PR #46 caught me getting wrong: the ending is where the
	// glide was GOING, not "shut". `settle(false)` below the notch is 0, so a
	// tap on the box inside the 220ms after a tap on ERROR closed the output
	// that first tap had just asked for.

	/** Tap `selector` without moving: a press the bar must read as a tap. */
	const tap = (page, selector) =>
		page.evaluate((sel) => {
			const el = document.querySelector(sel);
			const r = el.getBoundingClientRect();
			const x = r.left + r.width / 2;
			const y = r.top + r.height / 2;
			for (const type of ['pointerdown', 'pointerup'])
				el.dispatchEvent(
					new PointerEvent(type, {
						pointerId: 21,
						pointerType: 'touch',
						isPrimary: true,
						clientX: x,
						clientY: y,
						bubbles: true,
						cancelable: true
					})
				);
		}, selector);

	test('it lands where the glide was going, and never part-way', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// A failure, so the ERROR chip is there to open the output with.
			await page.locator(`${BAR} input`).fill('occur nope --nonsense');
			await page.locator(`${BAR} input`).press('Enter');
			await page.locator(`${BAR} .failed`).waitFor();
			assert.equal(await page.locator(`${BAR} .section.output`).count(), 0, 'the phone did not start folded');

			// Open it, and tap the box while it is still opening.
			await page.locator(`${BAR} .failed`).click();
			await tap(page, `${BAR} input`);
			await settled(page);

			const seen = await where(page);
			assert.ok(seen.shown > 0, 'the tap closed the output the ERROR chip had just opened');
			// And it is at the notch, not frozen at whatever fraction the tap
			// caught it at — the freeze is the other half of this claim.
			assert.ok(
				Math.abs(seen.shown - seen.out.height) <= 1,
				`the output came to rest at ${seen.shown}px of ${seen.out.height}px`
			);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

describe('a fling keeps going', () => {
	// vividn, 2026-09-18: "There should be acceleration on the REPL drawer
	// you're pulling out. So if you let go with velocity, it should keep
	// moving. So a big swipe will fully close or open the bar. When closing
	// snag at the notch so that the output section stays open, but you can
	// drag over this notch if you like."
	//
	// A fling needs REAL TIME between its moves, which synthetic
	// `PointerEvent`s do not have — they all carry the same stamp, and a
	// gesture with no time in it has no speed. So these go through CDP, the
	// same real touch `swipe` sends. The let-go rules are asserted above with
	// synthetic drags, which is what a drag WITHOUT a flick is.

	/**
	 * A flick: far enough and few enough steps that the box's own speed
	 * cannot decide the answer.
	 *
	 * `swipe` awaits every CDP round trip, so the gesture's DURATION — and so
	 * the speed `settle` measures — is whatever this machine's round trips
	 * cost. With the floor at 1.2 px/ms that margin is the test's to earn: a
	 * canary on PR #46 worked out that a 260px swipe in 6 steps needs to land
	 * inside ~215ms, which a loaded box cannot promise. 400px in 4 steps needs
	 * ~330ms and takes five round trips instead of eight, so the same loaded
	 * box has three times the room.
	 *
	 * And it reports what it actually achieved, so a failure here says whether
	 * the code was wrong or the box was slow.
	 */
	const flick = async (page, dy) => {
		const r = await page.locator(BAR).boundingBox();
		const began = Date.now();
		await swipe(page, r.x + r.width / 2, r.y + r.height / 2, 0, dy, 4);
		const ms = Date.now() - began;
		await settled(page);
		return `${Math.abs(dy)}px in ${ms}ms = ${(Math.abs(dy) / ms).toFixed(2)} px/ms, against a 1.2 floor`;
	};

	test('a big swipe up opens the bar the whole way', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			const threw = await flick(page, -400);
			const seen = await where(page);
			// The whole way: the bar's top is the top of the page, and the
			// dashboard is gone. It went much further than the finger did.
			assert.ok(seen.log.height > 400, `the log is ${seen.log.height}px — ${threw}`);
			assert.ok(seen.main.height <= 1, `the dashboard is still ${seen.main.height}px — ${threw}`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('a big swipe down SNAGS at the notch, and a second one shuts it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			const up = await flick(page, -400);
			assert.ok((await where(page)).log.height > 0, `the fling up did not open the log — ${up}`);

			// Down: it stops with the OUTPUT still open, not shut.
			const down = await flick(page, 400);
			const snagged = await where(page);
			assert.equal(snagged.height, 0, `the log is still ${snagged.height}px — ${down}`);
			assert.ok(snagged.out, `the fling down closed the output as well — it did not snag — ${down}`);
			assert.ok(snagged.shown > 0, `the output section is ${snagged.shown}px — ${down}`);

			// And from the notch, the next one shuts it. So a fling can still
			// close the bar; it takes the two the snag asks for.
			const again = await flick(page, 400);
			const shut = await where(page);
			assert.equal(shut.shown, 0, `the output section is still ${shut.shown}px — ${again}`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// "but you can drag over this notch if you like" — a deliberate drag that
	// ENDS below the notch closes, however it got there. Only a fling snags.
	test('a slow drag through the notch closes it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await dragBy(page, BAR, -200);
			await settled(page);
			assert.ok((await where(page)).log.height > 0, 'the drag up did not open the log');

			// Far enough down to end below the notch, with no flick in it.
			await dragBy(page, BAR, 400);
			await settled(page);
			const shut = await where(page);
			assert.equal(shut.height, 0, `the log is ${shut.height}px`);
			assert.equal(shut.shown, 0, `the output section is ${shut.shown}px — the drag did not pass the notch`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

describe('the way back', () => {
	test('`close history` is pinned to the bottom of the screen, and restores the bar’s place', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// A history long enough that its end is nowhere near the screen.
			for (let i = 0; i < 12; i += 1) await app.runLine(`occur field${i}`);
			await app.submit('occur coffee');
			await app.idle();

			await dragBy(page, BAR, -240);
			await settled(page);

			const deep = await where(page);
			assert.ok(deep.top, '`close history` is not on the page');
			// ITS NAME, not only its box. vividn, 2026-09-21: "let's change
			// 'return to top' with 'close history'". Every other claim in this
			// file reaches the button by its class, so the words themselves
			// were pinned nowhere and a rename back would have passed in
			// silence — vremya, re-driving the head, could only confirm them by
			// reading the source and asked for a test instead.
			assert.equal(await page.locator('.log .top').innerText(), 'close history');
			assert.ok(
				deep.pages.bottom - deep.top.bottom < 40,
				`the button's foot is ${deep.pages.bottom - deep.top.bottom}px off the bottom of the screen`
			);
			assert.ok(deep.top.height >= 24, `the button is ${deep.top.height}px tall`);

			await page.locator('.log .top').click();
			await settled(page);
			const back = await where(page);
			// Its normal place: the foot of the bar on the foot of the page.
			assert.ok(
				Math.abs(back.bar.bottom - back.pages.bottom) <= 1,
				`the bar's foot came back to ${back.bar.bottom}, the page ends at ${back.pages.bottom}`
			);
			assert.equal(back.top, null, 'the button stayed after the return');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('Escape returns too', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await app.idle();
			await wheelOver(page, BAR, 120);
			await settled(page);

			await page.keyboard.press('Escape');
			await settled(page);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('down there it is still the bar', () => {
	test('a tap on the box focuses it, and the mark’s menu still opens', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await app.idle();
			await dragBy(page, BAR, -240);
			await settled(page);

			await page.locator(`${BAR} input`).click();
			assert.ok(
				await page.locator(`${BAR} input`).evaluate((el) => el === document.activeElement),
				'a tap on the box did not focus it'
			);

			await page.locator(`${BAR} .brand`).click();
			await page.locator('[role="menu"][aria-label="databases"]').waitFor();
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"][aria-label="databases"]').waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// THE NEWEST ENTRY IS IN THE BAR AND NOWHERE ELSE, and the one before it
	// is the log's first row. vividn, 2026-09-21: "the output section … should
	// actually *be* the first line of the repl. That is, it shouldn't be
	// repeated when you drag it up."
	//
	// This used to read "what the output section cut reads whole in the log",
	// and it cannot: the log no longer holds the newest entry at all. What
	// the bar's row cuts is still reachable — the `+N` opens a sheet with
	// every key on it, which is `bar.test.mjs`'s claim — so the data is not
	// lost, only somewhere else. The claim that moves here is the one that
	// replaced it: two surfaces, one list, and no entry on both.
	test('the newest entry is the bar’s row, and the log carries on from the second', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await app.submit('occur water');
			await page.locator(`${BAR} .section.output .echo`).waitFor();

			// Row one is the newest, in the bar.
			assert.deepEqual(
				await page.locator(`${BAR} .section.output .echo`).allInnerTexts(),
				['❯ occur water']
			);

			await wheelOver(page, BAR, 120);
			await settled(page);

			// And the log starts at the one before it, whole: the brace it
			// draws has every key, where the bar's row would have cut to four
			// and a `+2`.
			const echoes = await logEchoes(page);
			assert.equal(echoes[0], 'occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			assert.ok(!echoes.includes('occur water'), `the log repeated the bar's row: ${echoes.join(' | ')}`);

			const doc = page.locator('.log .doc').first();
			assert.deepEqual(await doc.locator('.brace .row').allInnerTexts(), [
				'a: 1',
				'b: 2',
				'c: 3',
				'd: 4',
				'e: 5',
				'f: 6'
			]);
			assert.equal(await doc.locator('.brace .more').count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});
});

// ONE LIST, TWO RENDERERS, AND A HAIRLINE PER SEAM BETWEEN ITS ROWS.
//
// vividn, 2026-09-21, of the deployed build: "there is a missing line between
// the first entry in the repl and the second." `Repl.svelte` drew its rule
// before every row but its own first, which was right while the repl was
// one pane and wrong the moment the bar's output section became row one of
// the same list: the log's `i === 0` is the SECOND row overall, and the one
// seam neither renderer owns went undrawn.
//
// So this counts rules against rows ACROSS BOTH SURFACES, which is the
// count no earlier assertion made — every one of them counted rows.
//
// Since da-5iyo the log is COLUMNS, which settles the count for good: the
// row above every row of the log is a row the log draws or the bar does,
// so EVERY cell in it is ruled, and there is no arithmetic left to get
// wrong. In a pane, where the first row is the first row, the first row
// is the one that is not.
describe('the seam between the bar’s row and the log', () => {
	/** Rows and rules, over the output section and the log together. */
	const drawn = (page) =>
		page.evaluate(() => ({
			barRows: document.querySelectorAll('.prompt-bar .section.output .head').length,
			logRows: document.querySelectorAll('.log .head').length,
			logUnruled: [...document.querySelectorAll('.log .entry')].filter(
				(el) => getComputedStyle(el).borderTopWidth === '0px'
			).length
		}));

	test('every seam between two rows has a hairline, the bar’s own included', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await app.submit(line);
			await app.idle();

			await dragBy(page, BAR, -900);
			await settled(page);

			const { barRows, logRows, logUnruled } = await drawn(page);
			assert.equal(barRows, 1, 'the bar drew no row of its own');
			assert.equal(logRows, 2, `the log drew ${logRows} rows of three entries`);
			// The log owns every seam it has, because the row above its
			// first is the bar's: not one of its entries goes unruled.
			assert.equal(logUnruled, 0, `${logUnruled} of the log's entries drew no hairline`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// AND NOT IN A PANE. A repl WIDGET is the same component with `skip` 0,
	// and there the first row is the first row: a hairline above it would be
	// a rule against the top of the pane's own body.
	test('a repl widget still draws no rule above its first row', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water']) await app.submit(line);
			await app.idle();

			const pane = page.locator('.pane.repl, [data-title="repl"]').first();
			await pane.waitFor();
			assert.equal(await pane.locator('.head').count(), 2, 'the widget drew the wrong rows');
			// Whatever the columns do with those two entries, the ones on
			// the pane's first row carry no rule: there is nothing above
			// them but the top of the pane's own body.
			const firstRow = await pane.evaluate((el) => {
				const cells = [...el.querySelectorAll('.entry')];
				const top = (c) => Math.round(c.getBoundingClientRect().top);
				const first = top(cells[0]);
				return cells
					.filter((c) => top(c) === first)
					.map((c) => getComputedStyle(c).borderTopWidth);
			});
			assert.ok(firstRow.length > 0, 'the widget drew no entries');
			assert.deepEqual(
				firstRow,
				firstRow.map(() => '0px'),
				'the widget ruled its first row'
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

// ONE GESTURE, THREE PHASES, IN SEQUENCE (da-tj6m).
//
// vividn, 2026-09-21, of the deployed bar: "2-N in the repl seem to scroll
// independently, so if I pull up the input section a bit, I can then scroll
// through the old history independently. This behavior is incorrect,
// instead, you should be able to grab from anywhere in the repl and it
// behaves as one unit, so it will just pull the input section up more. When
// and only when the input bar reaches the top of the screen, does the REPL
// then scroll all together (including the most recent command). Then I can
// scroll around. When I get back to the top, then trying to scroll up
// further starts closing the input section".
//
//     phase 1   drag anywhere in the repl  ->  the input section GROWS
//     phase 2   at full height             ->  the list scrolls, ALL ROWS
//                                              AS ONE, the newest included
//     phase 3   back at the list's top,    ->  the section starts CLOSING
//               still pulling
//
// The handoffs are inside ONE drag, so what these assert is the SEQUENCE —
// sampled at every step of the gesture — and not merely where it ends up.
describe('the bar and the log are one handle', () => {
	/**
	 * A history taller than the screen, so there is something to scroll.
	 *
	 * The SCROLLBACK is what the log draws, and only a command that went
	 * through the bar is in it — `runLine` writes to the database and to
	 * nothing else. So the rows are seeded with entries (cheap) and then
	 * three commands whose ANSWER is tall are submitted through the bar,
	 * which buys a list twice the screen's height for four round trips
	 * instead of forty.
	 */
	const seed = async (app, lines = ['tail -n 25', 'tail -n 25', 'tail -n 25', 'occur coffee']) => {
		for (let i = 0; i < 25; i += 1) await app.runLine(`occur field${i}`);
		for (const line of lines) await app.submit(line);
		await app.idle();
	};

	/**
	 * Drag `selector` by `dy`, reading the bar and the list at EVERY step.
	 *
	 * A frame between the moves, because Svelte writes the DOM on a
	 * microtask: a sample taken in a tight loop would report the state
	 * before the move that caused it, which is the one thing a sequence
	 * cannot be asserted from. It also gives the moves real stamps, so this
	 * gesture has a speed in it like a thumb's — where it SETTLES is
	 * `detent.test.mjs`'s business and not this one's.
	 */
	const dragWatching = (page, selector, dy, steps = 14) =>
		page.evaluate(
			async ([selector, dy, steps]) => {
				const el = document.querySelector(selector);
				const r = el.getBoundingClientRect();
				const x = r.left + r.width / 2;
				const y = r.top + r.height / 2;
				const send = (type, at) =>
					el.dispatchEvent(
						new PointerEvent(type, {
							pointerId: 31,
							pointerType: 'touch',
							isPrimary: true,
							clientX: x,
							clientY: at,
							bubbles: true,
							cancelable: true
						})
					);
				const px = (name) =>
					Math.round(
						parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue(name)) || 0
					);
				const read = () => ({
					log: px('--log-height'),
					shown: px('--shown'),
					rolled: px('--rolled'),
					scroll: Math.round(document.querySelector('.log .body')?.scrollTop ?? 0),
					main: Math.round(document.querySelector('main').getBoundingClientRect().height)
				});
				const frame = () => new Promise((done) => requestAnimationFrame(done));
				send('pointerdown', y);
				const seen = [];
				for (let i = 1; i <= steps; i += 1) {
					send('pointermove', y + (dy * i) / steps);
					await frame();
					seen.push(read());
				}
				send('pointerup', y + dy);
				return seen;
			},
			[selector, dy, steps]
		);

	// THE COMPLAINT ITSELF, and it is about the rows and not about the bar:
	// a finger on the history used to move the history. The box's own
	// vertical scroll is what did that, and it is inert now — asserted as
	// the RULE (the computed value) and as what the rule is for (the drag).
	test('a drag on the rows pulls the bar up; it does not scroll them', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app);

			// Part-way open, so there is a log to put a finger on and room
			// above it for the bar still to grow.
			await dragBy(page, BAR, -300);
			await settled(page);
			const part = await where(page);
			assert.ok(part.height > 40, `the log is ${part.height}px — nothing to grab`);
			assert.ok(part.main.height > 40, `the bar is already at the top: ${part.main.height}px of dashboard`);

			assert.equal(
				await page.locator('.log .body').evaluate((el) => getComputedStyle(el).overflowY),
				'hidden',
				'the rows still scroll on their own'
			);

			await dragBy(page, '.log', -120);
			await settled(page);
			const pulled = await where(page);
			assert.ok(
				pulled.height > part.height + 60,
				`the drag on the rows opened the bar by ${pulled.height - part.height}px`
			);
			assert.equal(pulled.scroll, 0, `the rows scrolled ${pulled.scroll}px under the finger`);
			assert.equal(pulled.rolled, 0, "the bar's own row moved before the bar was at the top");
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('the list starts scrolling when, and only when, the input row reaches the top', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app);
			await dragBy(page, BAR, -200);
			await settled(page);
			// Open and unrolled, so this IS the output section's whole height.
			const whole = (await where(page)).shown;

			const seen = await dragWatching(page, '.log', -1400);
			for (const step of seen)
				if (step.rolled + step.scroll > 0)
					assert.ok(
						step.main <= 1,
						`the list scrolled with ${step.main}px of dashboard still showing: ${JSON.stringify(step)}`
					);
			const end = seen.at(-1);
			assert.ok(end.scroll > 0, `the pull never reached the list: ${JSON.stringify(end)}`);
			// AND THE NEWEST COMMAND WENT WITH IT. The bar's own row is the
			// first thing the list scrolls, and the output section gives the
			// log exactly the pixels the row takes with it — one list, drawn
			// by two renderers, and no row handed between them.
			const rolled = seen.find((step) => step.rolled > 0 && step.rolled < whole);
			assert.ok(rolled, `the bar's row never rolled: ${JSON.stringify(seen.map((s) => s.rolled))}`);
			assert.ok(
				Math.abs(rolled.shown + rolled.rolled - whole) <= 1,
				`the row rolled ${rolled.rolled}px and the section kept ${rolled.shown}px of ${whole}px`
			);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('and pulling the other way scrolls back to the top before it closes anything', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app);
			// TWO DRAGS, and the first one is what MOUNTS the log: the
			// scrollback down there is drawn on the first gesture and not
			// before ($lib/log.ts's `armed`), and `dragBy` dispatches its
			// whole run of moves synchronously — so a drag that arms the log
			// cannot also reach into it, there being no frame in the middle
			// for it to be drawn in. A finger has frames.
			await dragBy(page, BAR, -200);
			await settled(page);
			await dragBy(page, '.log', -900);
			await settled(page);
			const deep = await where(page);
			assert.ok(deep.scroll > 0, `the list is at ${deep.scroll}px — there is nothing to come back from`);

			const seen = await dragWatching(page, '.log', 900, 20);
			for (const step of seen)
				if (step.main > 1)
					assert.deepEqual(
						[step.rolled, step.scroll],
						[0, 0],
						`the bar began closing with the list at ${step.scroll}px: ${JSON.stringify(step)}`
					);
			const end = seen.at(-1);
			assert.ok(end.main > 1, `the pull never started closing: ${JSON.stringify(end)}`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// "it should allow little bits of acceleration sliding the section up
	// and down, and just have a nice notch at closed or open" — the rule is
	// `detent.test.mjs`; what is asserted HERE is that a real thumb's push
	// reaches the bar at all, which is the half no pure test can make.
	test('a real finger’s push keeps the bar moving after the finger has gone', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, ['occur coffee']);
			const r = await page.locator(BAR).boundingBox();
			const middle = [r.x + r.width / 2, r.y + r.height / 2];

			// THE SAME DISTANCE TWICE, and the only difference is the TIME in
			// it. A synthetic drag's moves all carry one stamp, so it has no
			// speed and comes to rest exactly where the finger stopped; a real
			// touch through CDP has stamps a frame apart and so has a push in
			// it. Comparing them is what makes this claim independent of how
			// fast this machine happens to be — the numbers themselves are
			// `detent.test.mjs`'s, and where a thumb lands is nobody's but a
			// thumb's.
			await dragBy(page, BAR, -220);
			await settled(page);
			// Both numbers are the LOG's height, which is the bar's own travel
			// less the notch — the same yardstick for both arms, which is all
			// the comparison needs (canary on PR #59: the message used to read
			// as though it were quoting the travel itself).
			const placedLog = (await where(page)).height;
			await dragBy(page, BAR, 900);
			await settled(page);
			assert.equal((await where(page)).height, 0, 'the bar did not come back shut');

			await swipe(page, ...middle, 0, -220, 6);
			await settled(page);
			const thrownLog = (await where(page)).height;
			assert.ok(
				thrownLog > placedLog,
				`a 220px push left the log ${thrownLog}px tall; the same 220px placed left it ${placedLog}px`
			);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});
