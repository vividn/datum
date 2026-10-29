// The log over the bar, and the two gestures that reach it (da-ckw3,
// da-mwp4m).
//
// vividn, 2026-09-17, of the first: "clicking and dragging the input bar up,
// or hovering the mouse over the input bar and scrolling down scrolls the
// page down to reveal the reversed REPL … Scrolling or dragging anywhere
// else on the page should not go down into the repl section." And,
// 2026-10-27: «keep the input bar on the bottom, and put the output/return
// bar above it again / make the CLI chronological order again».
//
// So there are four claims here and they are all measured on the page:
//
// 1. the bar's own gestures move it — a touch drag on a phone, a wheel on
//    a desktop — and when they have, the history is ABOVE the bar, oldest
//    first, the newest in the bar's output section, and the input row
//    still at the foot of the page;
// 2. no other gesture moves it: a wheel over a pane, a drag that starts on
//    a pane, and a drag on the bar too small to be one;
// 3. `close history` is pinned to the foot of the log, on screen however
//    long the history is, and Escape does the same thing;
// 4. the bar is still the bar with the log open: a tap on the box still
//    focuses it, the mark's menu still opens, and a command run then lands
//    at the FOOT of the history.
//
// What the log shows is `CliScrollback.svelte`, the same component a layout can
// hold, reading the same scrollback store. So this file does not re-assert
// what the CLI draws — cli.test.mjs does that. It
// asserts where it is and how you get to it. A bare `Pane` has no `.cli`
// class (that is the layout frame's), so the log is read through `.log`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { swipe, withBar } from './support/layout.mjs';
// Synthesised rather than driven through `page.touchscreen`, because what is
// asserted is the POINTER contract the bar listens on — down, a run of
// moves, up — and a test that sent two events could not tell a threshold
// that works from one that never fires. With one stamp, so no speed.
import { dragOn as dragBy } from './support/bar.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 }, prepare: withBar };
const PHONE = {
	viewport: { width: 390, height: 664 },
	device: { isMobile: true, hasTouch: true },
	prepare: withBar
};

const BAR = '.prompt-bar';

// ONE desktop app for the four tests below that begin and end with the bar
// at rest (da-6b3dj, sharedApp in support/app.mjs) — tidied by hand, not by
// `solo`, which would close the repl tab under them. Its scrollback keeps
// every test's commands, so a test on it reads the log from its newest row
// and counts the rows it drew itself.
const shared = sharedApp(DESKTOP);
const desktop = async (fn) => {
	const app = await shared();
	await app.home();
	await app.empty();
	return fn(app);
};

/**
 * ONE phone for a describe, where a describe has several tests on one
 * (da-87x74): an open is a second, and `sharedApp` inside a describe is
 * taken down with it. Each test is handed the phone as a fresh one opens —
 * the page tidied, the bar shut and folded, the database empty.
 *
 * Two things a test leaves are NOT put back, and they decide the order of
 * the tests that care: the log stays MOUNTED once a drag has pulled the bar
 * (`armed`, #lib/log.ts), and the scrollback keeps every command. So the
 * test that a tap mounts no log comes before any drag, and the test that
 * reads the log's rows before any other that submits. A describe with one
 * test on a phone still opens its own.
 */
function onePhone() {
	const phone = sharedApp(PHONE);
	return async (fn) => {
		const app = await phone();
		await app.home();
		await shut(app.page);
		await app.empty();
		return fn(app);
	};
}

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
			// How far the log's rows are scrolled, and how far they could be.
			scroll: Math.round(document.querySelector('.log .body')?.scrollTop ?? 0),
			scrollMax: (() => {
				const body = document.querySelector('.log .body');
				return body ? Math.round(body.scrollHeight - body.clientHeight) : 0;
			})()
		};
	});

/**
 * Wait until the bar has stopped moving.
 *
 * It settles on an animation frame curve (#lib/log.ts) rather than by a
 * scroll, so what a test waits for is the number holding still — two reads
 * the same, a few frames apart: the output's height, the log's, and where
 * the log's rows are scrolled to.
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
					const now = `${px('--shown')}/${px('--log-height')}/${Math.round(
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

/**
 * The bar shut: a drag with no speed in it, down further than any list is
 * long. A drag down mounts no log: the log mounts the first time the bar
 * opens past the notch (#lib/log.ts's `armed`).
 *
 * And the click a drag ends in, spent: the drag before this — a test's, or
 * the one above — may have armed the 300ms swallow (#lib/grip.ts), which
 * would otherwise take the next test's first click. On the bar's own box,
 * which nothing listens to, so a click the swallow lets through does nothing.
 */
async function shut(page) {
	const { shown, height } = await where(page);
	if (height || shown) await dragBy(page, BAR, 100_000);
	await page.locator(BAR).dispatchEvent('click');
	await settled(page);
}

/** The log's echoed command lines, as it draws them: oldest first. */
const logEchoes = async (page) =>
	(await page.locator('.log .echo').allInnerTexts()).map((t) => t.replace(/^\s*❯\s*/, '').trim());

/** A wheel of `dy` with the pointer over `selector`'s middle. */
async function wheelOver(page, selector, dy) {
	const box = await page.locator(selector).first().boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.wheel(0, dy);
}

describe('the bar is the way in', () => {
	const phone = onePhone();

	// FIRST, and that is its place (da-87x74): the phone is the describe's
	// one, the log stays mounted once any drag has pulled the
	// bar, and this is the test that says a cancelled tap mounts none.
	//
	// A pointercancel BEFORE the threshold: the system took a gesture that was
	// still a tap. Nothing was armed, nothing moved, and nothing settles — a
	// canary on PR #46 asked twice whether a cancel between `arm()` and
	// `dragging` could settle a bar nobody dragged. It cannot: `dragging` is
	// set BEFORE `arm()`, so there is no such moment, and this holds it that
	// way if the two are ever reordered.
	test('a cancel before the threshold moves nothing and arms nothing', async () => {
		await phone(async (app) => {
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
				return { log: !!document.querySelector('.log .cli, .log section.pane') };
			});
			await settled(page);
			assert.equal(seen.log, false, 'a cancelled tap mounted the log');
			assert.deepEqual(await where(page), before, 'a cancelled tap moved the bar');
			assert.deepEqual(app.errors, []);
		});
	});

	// THE INPUT STAYS AT THE FOOT (da-mwp4m). vividn, 2026-10-27: «keep the
	// input bar on the bottom, and put the output/return bar above it again
	// … make the CLI chronological order again». So pulling the bar up opens
	// the history ABOVE it, oldest at the top, and the input never moves.
	test('a finger dragging the bar up opens the history above it, oldest first, the input staying at the foot', async () => {
		await phone(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea']) await app.submit(line);
			await app.idle();

			const rest = await where(page);
			assert.equal(rest.height, 0, 'the log was open before anything pulled it');
			assert.equal(rest.top, null, '`close history` shows while the dashboard does');

			// Far enough to pass the notch: up past it is the full screen.
			await dragBy(page, BAR, -900);
			await settled(page);

			const deep = await where(page);
			assert.ok(deep.height > 0, 'the drag opened no log at all');
			// The input on the foot of the page, as at rest.
			assert.ok(
				Math.abs(deep.input.bottom - deep.pages.bottom) <= 1,
				`the input's foot is at ${deep.input.bottom}, the page ends at ${deep.pages.bottom}`
			);
			assert.ok(Math.abs(deep.input.top - rest.input.top) <= 1, 'the input row moved');
			// And the log over the bar, up to the top of the PAGE — the root
			// tab bar is the app's own chrome and stands above it.
			assert.ok(Math.abs(deep.log.top - deep.pages.top) <= 1, `the log starts at ${deep.log.top}`);
			assert.ok(deep.log.bottom <= deep.bar.top + 1, `the log ends at ${deep.log.bottom}, under the bar's ${deep.bar.top}`);
			assert.ok(deep.main.height <= 1, `the dashboard kept ${deep.main.height}px at the log's tallest`);
			// ONE LIST, and full it is all in the log (da-0b2vy): the strip is
			// gone and the newest is the log's last line, just over the input.
			assert.equal(deep.out, null, 'the strip stayed under the full screen');
			assert.deepEqual(await logEchoes(page), ['occur coffee', 'occur water', 'occur tea']);

			// A command run from in here is the log's new last line.
			await app.submit('occur toast');
			await app.idle();
			assert.deepEqual((await logEchoes(page)).at(-1), 'occur toast');
			assert.deepEqual(app.errors, []);
		});
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
		await phone(async (app) => {
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
		});
	});

	// A drag ends in a click the browser dispatches from the pointerup it
	// came from, and that one click is the drag's tail, not a press. It is
	// swallowed — and ONLY it: the window is a safety net for a drag that
	// produced no click at all, and the first click inside it closes the
	// window. A canary on PR #46 read it as "any click on the bar for 300ms",
	// which would take the caret away from a finger that dragged and then
	// pressed it quickly. This is that claim, at the event level.
	test('a drag swallows the one click it ends in, and no other', async () => {
		await phone(async (app) => {
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
		});
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
			// Read once the bar holds still over the page's own frames: a
			// fixed wait on Node's clock could end before a late page had
			// handled the wheel, and read «nothing moved» for that reason.
			await settled(page);
			const after = await where(page);
			assert.deepEqual(
				[after.shown, after.height],
				[before.shown, before.height],
				'a wheel over a pane moved the bar'
			);

			// A notch per wheel — from a desktop's open output, one opens the log.
			// Sent past `INERTIA_MS` (150, #lib/grip.ts) after the pane's, or
			// the bar takes it for that wheel's inertia: a gap between the
			// events' timestamps, stamped as they are sent, so Node's clock.
			await page.waitForTimeout(200);
			await wheelOver(page, BAR, 120);
			await settled(page);
			assert.ok((await where(page)).height > 0, 'a wheel over the bar opened no log');

			await wheelOver(page, BAR, -120);
			await settled(page);
			assert.equal((await where(page)).height, 0, 'wheeling back left the log open');
			assert.deepEqual(app.errors, []);
		});
	});

	// `inert` is not only a Tab-order rule: an inert subtree is not focusable
	// at all, so a `.focus()` on something inside it does nothing either. A
	// canary on PR #46 said twice that programmatic focus gets through. It
	// does not — measured here, both ways round, so the claim is settled by
	// the browser rather than by either of us. (`CliScrollback.svelte` calls `.focus()`
	// nowhere, which is the other half of the answer.)
	test('inert on the log refuses a programmatic focus too', async () => {
		await phone(async (app) => {
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
		});
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
		await phone(async (app) => {
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
		});
	});

	// The other half of the same declaration: sideways on the box is still
	// the RUN's, so a pill off the right edge is still reachable and the bar
	// does not move for it.
	test('a real finger dragging sideways on the box scrolls the run, not the bar', async () => {
		await phone(async (app) => {
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
		});
	});

	test('a drag that starts on a pane never opens the bar', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await app.idle();

			const before = await where(page);
			await dragBy(page, 'section.pane', -240);
			await settled(page);
			const after = await where(page);
			assert.deepEqual(
				[after.shown, after.height],
				[before.shown, before.height],
				'a drag on a pane moved the bar'
			);
			assert.deepEqual(app.errors, []);
		});
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
			// Folded first: every screen starts open (da-mwp4m), and the chip
			// is what a folded bar says.
			await dragBy(page, BAR, 100_000);
			await page.locator(`${BAR} .section.output`).waitFor({ state: 'detached' });
			// The drag's click swallow spent, or it takes the chip's click.
			await page.locator(BAR).dispatchEvent('click');
			await page.locator(`${BAR} input`).fill('occur nope --nonsense');
			await page.locator(`${BAR} input`).press('Enter');
			await page.locator(`${BAR} .failed`).waitFor();

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
	const phone = onePhone();
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
	 * A flick: 400px in four moves 20ms apart, by the gesture's OWN clock.
	 *
	 * The clock is the point (da-wucn6). `swipe` awaits every CDP round trip,
	 * and unstamped, each event is timed as the browser receives it — while a
	 * release reads its speed off the moves of its last 80ms (`WINDOW_MS`,
	 * #lib/grip.ts). At a runner's pace, some 60ms a move, the moves sit at
	 * that window's edge: one round trip over 80ms before the last move left
	 * it the window's only sample, the lift is where that move was, and the
	 * release read a speed of exactly 0 — the bar stayed where the finger put
	 * it, 342px of log, after a gesture that was 1.37 px/ms end to end (CI
	 * run 2772). Stamped, the release reads the same on any box.
	 *
	 * 20ms and not 40 (da-5rr8f): the window is the 80ms before the LIFT, so
	 * one stamp sits on its edge, in or out by the browser's rounding. At 20
	 * that is 300px over 80ms or 200px over 60, a throw either way. At 40 it
	 * was 100px over 80ms or the last move alone, a speed of 0: one run of
	 * three failed.
	 *
	 * `hold` is how long the finger then rests before it lifts.
	 */
	const flick = async (page, dy, hold) => {
		const r = await page.locator(BAR).boundingBox();
		await swipe(page, r.x + r.width / 2, r.y + r.height / 2, 0, dy, 4, 20, hold);
		await settled(page);
	};

	test('a big swipe up opens the bar the whole way', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await flick(page, -400);
			const seen = await where(page);
			// The whole way: the bar's top is the top of the page, and the
			// dashboard is gone. It went much further than the finger did.
			assert.ok(seen.log.height > 400, `the log is ${seen.log.height}px`);
			assert.ok(seen.main.height <= 1, `the dashboard is still ${seen.main.height}px`);
			assert.deepEqual(app.errors, []);
		});
	});

	// The speed a release carries is the speed at the LIFT (da-5rr8f): a
	// finger at rest sends no moves, so the race's were still the window
	// half a second later, and the bar glided on from where it was held.
	// From full, a fling down snags at the row; held before the lift, the
	// same swipe ends where the finger left it, past the notch — shut.
	test('a big swipe down that stops and holds before the lift goes where it was put', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await flick(page, -400);
			const deep = await where(page);
			assert.equal(deep.shown, 0, 'the fling up did not go full');

			await flick(page, deep.height - 10, 520);
			const held = await where(page);
			assert.equal(held.height, 0, `the log is still ${held.height}px`);
			assert.equal(held.shown, 0, `the output section is ${held.shown}px — the held swipe snagged`);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a big swipe down SNAGS at the notch, and a second one shuts it', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await flick(page, -400);
			assert.ok((await where(page)).log.height > 0, 'the fling up did not open the log');

			// Down: it stops with the OUTPUT still open, not shut.
			await flick(page, 400);
			const snagged = await where(page);
			assert.equal(snagged.height, 0, `the log is still ${snagged.height}px`);
			assert.ok(snagged.out, 'the fling down closed the output as well — it did not snag');
			assert.ok(snagged.shown > 0, `the output section is ${snagged.shown}px`);

			// And from the notch, the next one shuts it. So a fling can still
			// close the bar; it takes the two the snag asks for.
			await flick(page, 400);
			const shut = await where(page);
			assert.equal(shut.shown, 0, `the output section is still ${shut.shown}px`);
			assert.deepEqual(app.errors, []);
		});
	});

	// "but you can drag over this notch if you like" — a deliberate drag that
	// ENDS below the notch closes, however it got there. Only a fling snags.
	test('a slow drag through the notch closes it', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await dragBy(page, BAR, -200);
			await settled(page);
			assert.ok((await where(page)).log.height > 0, 'the drag up did not open the log');

			// Far enough down to end below the notch, with no flick in it.
			await dragBy(page, BAR, 900);
			await settled(page);
			const shut = await where(page);
			assert.equal(shut.height, 0, `the log is ${shut.height}px`);
			assert.equal(shut.shown, 0, `the output section is ${shut.shown}px — the drag did not pass the notch`);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the way back', () => {
	// At the foot of the LOG, just over the output section: the log is above
	// the bar now (da-mwp4m), and its foot is the nearest the history comes
	// to the input — on screen however long the history is.
	test('`close history` is pinned to the foot of the log, and restores the bar’s place', async () => {
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
				deep.log.bottom - deep.top.bottom < 40 && deep.top.bottom <= deep.log.bottom,
				`the button's foot is ${deep.log.bottom - deep.top.bottom}px over the log's`
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

	// THE NEWEST ENTRY IS THE ROW'S, and the one before it the log's last
	// row while the row is open. vividn, 2026-09-21: "the output section …
	// should actually *be* the first line of the repl. That is, it shouldn't
	// be repeated when you drag it up." Full, the strip is gone and the log
	// draws them all (da-0b2vy) — each WHOLE: what the row cut on one line is
	// every key there, one to a row (da-hxjrk).
	test('the newest entry is the row’s, and full the log draws it whole as its last line', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await app.declare('brew');
			await app.submit('occur water');
			await app.submit('occur brew a=1 b=2 c=3 d=4 e=5 f=6');
			await page.locator(`${BAR} .group:last-child .echo`).waitFor();

			assert.deepEqual(
				await page.locator(`${BAR} .group:last-child .echo`).allInnerTexts(),
				['❯ occur brew a=1 b=2 c=3 d=4 e=5 f=6']
			);
			assert.ok(!(await logEchoes(page)).includes('occur brew a=1 b=2 c=3 d=4 e=5 f=6'), "the log repeated the row's newest");

			await wheelOver(page, BAR, 120);
			await settled(page);

			const echoes = await logEchoes(page);
			assert.deepEqual(echoes.slice(-2), ['occur water', 'occur brew a=1 b=2 c=3 d=4 e=5 f=6']);
			const doc = page.locator('.log .doc').last();
			assert.deepEqual(await doc.locator('.rows .row').allInnerTexts(), [
				'a: 1',
				'b: 2',
				'c: 3',
				'd: 4',
				'e: 5',
				'f: 6'
			]);
			await page.keyboard.press('Escape');
			await settled(page);
			assert.deepEqual(app.errors, []);
		});
	});
});

// ONE LIST, TWO RENDERERS, AND A LINE AT EVERY SEAM BETWEEN ITS ROWS.
//
// vividn, 2026-09-21, of the deployed build: "there is a missing line between
// the first entry in the repl and the second." The one seam neither renderer
// owns is the one between the log's rows and the bar's own row, so this
// counts rules against rows ACROSS BOTH SURFACES.
//
// In time order (da-mwp4m) that seam is the bar's TOP RULE: the log ends
// over it. Within the log, every row is a frame of its own since da-hxjrk,
// so a seam is the edges of two frames, and no row is ruled besides.
describe('the seam between the bar’s row and the log', () => {
	/** Rows and rules, over the output section and the log together. */
	const drawn = (page) =>
		page.evaluate(() => {
			const cells = [...document.querySelectorAll('.log .entry')];
			const top = (c) => Math.round(c.getBoundingClientRect().top);
			const first = cells.length ? top(cells[0]) : 0;
			const ruled = (c) => getComputedStyle(c).borderTopWidth !== '0px';
			const framed = (c) => parseFloat(getComputedStyle(c.querySelector('.frame')).borderTopWidth) >= 1;
			return {
				barRows: document.querySelectorAll('.prompt-bar .group:last-child .head').length,
				logRows: document.querySelectorAll('.log .head').length,
				firstRuled: cells.filter((c) => top(c) === first && ruled(c)).length,
				unframed: cells.filter((c) => !framed(c)).length,
				barRule: parseFloat(getComputedStyle(document.querySelector('.prompt-bar')).borderTopWidth)
			};
		});

	test('every seam between two rows is drawn, the bar’s own included', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur water', 'occur tea', 'occur toast']) await app.submit(line);
			await app.idle();

			await dragBy(page, BAR, -900);
			await settled(page);

			// Full: the strip is gone, and the log draws all four (da-0b2vy).
			const { barRows, logRows, firstRuled, unframed, barRule } = await drawn(page);
			assert.equal(barRows, 0, 'the strip stayed under the full screen');
			assert.equal(logRows, 4, `the log drew ${logRows} rows of four entries`);
			assert.equal(firstRuled, 0, "the log ruled its first row against the top of its own box");
			assert.equal(unframed, 0, `${unframed} of the log's entries drew no frame`);
			assert.ok(barRule >= 1, "no rule between the log's last row and the bar's");
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// AND NOT IN A PANE. A CLI WIDGET is the same component with `skip` 0,
	// and there the first row is the first row: a hairline above it would be
	// a rule against the top of the pane's own body.
	test('a CLI widget still draws no rule above its first row', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const pane = page.locator('.pane.cli, [data-title="cli"]').first();
			await pane.waitFor();
			// The desktop's CLI keeps the tests before this one: counted from there.
			const had = await pane.locator('.head').count();
			for (const line of ['occur coffee', 'occur water']) await app.submit(line);
			await app.idle();

			assert.equal((await pane.locator('.head').count()) - had, 2, 'the widget drew the wrong rows');
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
		});
	});
});

// THE BAR OPENS THE LOG, AND THE LOG'S ROWS SCROLL AS A TERMINAL'S DO.
//
// da-tj6m made the bar and the log ONE handle — past the ceiling the pull
// scrolled the list — for a history that hung below an input at the top of
// the screen. In time order, with the input at the foot (da-mwp4m), the
// history reads upward from the bar, and its rows scroll on their own,
// pinned to the newest.
describe('the bar and the log are one handle', () => {
	const phone = onePhone();
	/**
	 * A history taller than the screen, so there is something to scroll.
	 *
	 * The SCROLLBACK is what the log draws, and only a command that went
	 * through the bar is in it — `runLine` writes to the database and to
	 * nothing else. So the rows are seeded with entries (cheap) and then
	 * three commands whose ANSWER is tall are submitted through the bar,
	 * which buys a list twice the screen's height for four round trips
	 * instead of forty.
	 *
	 * ONCE for the describe (da-87x74): the phone is its one, and nothing puts
	 * a scrollback back, so the list the first test seeds is the list the
	 * rest read.
	 */
	let seeded = false;
	const seed = async (app) => {
		if (seeded) return;
		for (let i = 0; i < 25; i += 1) await app.runLine(`occur field${i}`);
		for (const line of ['tail -n 25', 'tail -n 25', 'tail -n 25', 'occur coffee']) await app.submit(line);
		await app.idle();
		seeded = true;
	};

	test('the log opens on its newest row, and its rows scroll without moving the bar', async () => {
		await phone(async (app) => {
			const { page } = app;
			await seed(app);
			await dragBy(page, BAR, -900);
			await settled(page);
			const open = await where(page);
			assert.ok(open.scrollMax > 0, `the history fits in ${open.height}px — nothing to scroll`);
			assert.ok(open.scrollMax - open.scroll <= 1, `the log opened ${open.scrollMax - open.scroll}px above its newest row`);

			await wheelOver(page, '.log', -400);
			await settled(page);
			const back = await where(page);
			assert.ok(back.scroll < open.scroll, 'a wheel over the rows did not scroll them');
			assert.equal(back.height, open.height, 'the wheel over the rows moved the bar');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a command that lands brings the log back to its newest row', async () => {
		await phone(async (app) => {
			const { page } = app;
			await seed(app);
			await dragBy(page, BAR, -900);
			await settled(page);
			await wheelOver(page, '.log', -400);
			await settled(page);
			assert.ok((await where(page)).scroll < (await where(page)).scrollMax - 1, 'the rows did not scroll up');

			await app.submit('occur water');
			await app.idle();
			await settled(page);
			const after = await where(page);
			assert.ok(after.scrollMax - after.scroll <= 1, `the log stayed ${after.scrollMax - after.scroll}px above its newest row`);
			assert.equal((await logEchoes(page)).at(-1), 'occur water');
			assert.deepEqual(app.errors, []);
		});
	});

	// "it should allow little bits of acceleration" — the rule is
	// `detent.test.mjs`; what is asserted HERE is that a real thumb's push
	// reaches the bar at all, which is the half no pure test can make.
	test('a real finger’s push keeps the bar moving after the finger has gone', async () => {
		await phone(async (app) => {
			const { page } = app;
			await seed(app);
			await dragBy(page, BAR, -40);
			await settled(page);
			const r = await page.locator(BAR).boundingBox();
			const middle = [r.x + r.width / 2, r.y + r.height / 2];

			// THE SAME DISTANCE TWICE, and the only difference is the TIME in
			// it. A synthetic drag's moves all carry one stamp, so it has no
			// speed: 20px up from the row is inside the notch's catch, and the
			// bar comes back to the row. A real touch through CDP has stamps a
			// frame apart — its own, not the round trips' (`flick`, da-wucn6) —
			// and its push carries it past the catch, up to the full screen.
			await dragBy(page, BAR, -20);
			await settled(page);
			const placed = await where(page);
			assert.equal(placed.height, 0, `20px placed opened ${placed.height}px of log`);
			assert.ok(placed.shown > 0, '20px placed closed the row');

			await swipe(page, ...middle, 0, -20, 6, 16);
			await settled(page);
			const thrown = await where(page);
			assert.equal(thrown.shown, 0, `a 20px push left the row open: ${thrown.shown}px`);
			assert.ok(thrown.height > 0, 'a 20px push opened no log');
			assert.deepEqual(app.errors, []);
		});
	});
});

// THE HANDLE AND THE FULL SCREEN (da-0b2vy). vividn, 2026-10-27: «the top of
// the output bar should show a very clear handle toggle. Clicking or swiping
// up on this makes it full screen. Entries animate and flow to be vertically
// chronological … each command get's it's own line … when the cli is
// fullscreen, the handle is at the top of the screen and clicking or
// swiping there closes the cli again. Also put a button between the input
// bar and the output bar when the cli is full screen that collapses the cli
// … Otherwise scrolling scrolls through the history of the CLI until you
// reach the end of the history at which point scrolling up/swiping down
// again closes the cli». And his answer 2: «clicking on the handle and
// swiping down could close it still».
describe('the three states and the full screen', () => {
	const phone = onePhone();
	const INPUT = `${BAR} .section.input`;
	const OUTPUT = `${BAR} .section.output`;

	/** Which of the three states the bar rests in. */
	const state = async (page) => {
		await settled(page);
		const { shown, height, out } = await where(page);
		if (height > 0 && shown === 0 && !out) return 'full';
		if (height === 0 && shown > 0) return 'row';
		if (height === 0 && shown === 0) return 'closed';
		return `between: shown ${shown}, log ${height}`;
	};

	// The handle is gone (da-pnxug), vividn, 2026-10-28: «the swiping up and
	// down on the output section/input section is sufficient here».
	test('a swipe on the input or the output: up from closed is the row, up from the row full, down from full the row, down from the row closed', async () => {
		await phone(async (app) => {
			const { page } = app;
			// A control of the bar's that is in neither section is a handle.
			const handles = () =>
				page.locator(`${BAR} button`).evaluateAll((all) => all.filter((b) => !b.closest('.section')).length);
			await app.submit('occur coffee');
			assert.equal(await state(page), 'closed');
			for (const [on, dy, want] of [
				[INPUT, -60, 'row'],
				[OUTPUT, -60, 'full'],
				[INPUT, 60, 'row'],
				[OUTPUT, 60, 'closed'],
				[INPUT, -60, 'row'],
				[INPUT, -60, 'full'],
				[INPUT, 60, 'row'],
				[INPUT, 60, 'closed']
			]) {
				assert.equal(await handles(), 0, 'a handle is drawn');
				await dragBy(page, on, dy);
				assert.equal(await state(page), want, `a ${dy}px swipe on ${on} did not leave the bar ${want}`);
			}
			assert.equal(await handles(), 0, 'a handle is drawn');
			assert.deepEqual(app.errors, []);
		});
	});

	// And the keyboard's way to the same three states, which the handle was.
	test('on the box, PageUp goes closed → row → full and PageDown back, and the box keeps the caret and its line', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			const box = page.locator(`${BAR} input`);
			await box.click({ position: { x: 4, y: 12 } });
			await box.type('occur cof');
			assert.equal(await state(page), 'closed');
			for (const [key, want] of [
				['PageUp', 'row'],
				['PageUp', 'full'],
				['PageUp', 'full'],
				['PageDown', 'row'],
				['PageDown', 'closed'],
				['PageDown', 'closed']
			]) {
				await page.keyboard.press(key);
				assert.equal(await state(page), want, `${key} did not leave the bar ${want}`);
				assert.equal(await page.evaluate(() => document.activeElement?.tagName), 'INPUT', `${key} took the caret`);
			}
			assert.equal(await box.inputValue(), 'occur cof');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the button between the history and the input collapses full to the row', async () => {
		await phone(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await dragBy(page, BAR, -900);
			assert.equal(await state(page), 'full');
			const { top, input } = await where(page);
			assert.ok(top.bottom <= input.top && input.top - top.bottom < 40, `the button ends ${input.top - top.bottom}px over the input`);
			await page.locator('.log .top').click();
			assert.equal(await state(page), 'row');
			assert.deepEqual(app.errors, []);
		});
	});

	test('at the top of the history one more scroll up collapses it, a wheel or a finger; a scroll short of the top does not', async () => {
		await phone(async (app) => {
			const { page } = app;
			for (let i = 0; i < 25; i += 1) await app.runLine(`occur field${i}`);
			for (const line of ['tail -n 25', 'tail -n 25', 'occur coffee']) await app.submit(line);
			await app.idle();
			await dragBy(page, BAR, -900);
			assert.equal(await state(page), 'full');
			const open = await where(page);
			assert.ok(open.scrollMax > 0, 'nothing to scroll');

			// Short of the top, a wheel up scrolls the history and nothing else.
			await wheelOver(page, '.log', -120);
			assert.equal(await state(page), 'full', 'a wheel short of the top collapsed it');
			assert.ok((await where(page)).scroll < open.scroll, 'the wheel did not scroll the history');

			// At the top, a wheel up of its own collapses it — after the last
			// wheel's gesture is over (`SAME_MS`, BarLog.svelte).
			await page.evaluate(() => (document.querySelector('.log .body').scrollTop = 0));
			await page.waitForTimeout(200);
			await wheelOver(page, '.log', -120);
			assert.equal(await state(page), 'row', 'a wheel up at the top did not collapse it');

			// And a finger swiping down from the top.
			await dragBy(page, BAR, -900);
			assert.equal(await state(page), 'full');
			await page.evaluate(() => (document.querySelector('.log .body').scrollTop = 0));
			const box = await page.locator('.log').boundingBox();
			await swipe(page, box.x + box.width / 2, box.y + box.height / 3, 0, 120, 6, 16);
			assert.equal(await state(page), 'row', 'a swipe down at the top did not collapse it');
			assert.deepEqual(app.errors, []);
		});
	});

	test('full, each command is a line of its own, and a command’s documents share one', async () => {
		await desktop(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee', 'occur tea -t 9:00 , switch mood happy -t 9:10', 'occur water']) await app.submit(line);
			await app.idle();
			await wheelOver(page, BAR, 120);
			await settled(page);
			assert.equal((await where(page)).shown, 0, 'the wheel did not go full');
			// Measured where the commands rest, not mid-flow.
			await page.waitForFunction(() => ![...document.querySelectorAll('.log [data-ran]')].some((el) => el.getAnimations().length));

			const lines = await page.evaluate(() =>
				[...document.querySelectorAll('.log .entry')].slice(-3).map((entry) => {
					const r = entry.getBoundingClientRect();
					return {
						top: r.top,
						bottom: r.bottom,
						docs: [...entry.querySelectorAll('.doc')].map((d) => Math.round(d.getBoundingClientRect().top))
					};
				})
			);
			assert.equal(lines.length, 3);
			// One under another: no two commands share a line.
			for (let i = 1; i < lines.length; i += 1)
				assert.ok(lines[i].top >= lines[i - 1].bottom - 1, `command ${i} starts at ${lines[i].top}, over the end of the one before (${lines[i - 1].bottom})`);
			// The two-document command's documents, side by side.
			assert.equal(lines[1].docs.length, 2);
			assert.equal(lines[1].docs[0], lines[1].docs[1], `its documents start at ${lines[1].docs}`);
			await page.keyboard.press('Escape');
			await settled(page);
			assert.deepEqual(app.errors, []);
		});
	});

	// «Entries animate and flow to be vertically chronological». Counted at
	// `animate` on a command's own element, going full and coming back; under
	// reduced motion the same change and no flow.
	for (const reduced of [false, true])
		test(`the commands flow to their new places${reduced ? ' — not under reduced motion' : ''}`, async () => {
			await phone(async (app) => {
				const { page } = app;
				if (reduced) await page.emulateMedia({ reducedMotion: 'reduce' });
				for (const line of ['occur coffee', 'occur water']) await app.submit(line);
				await app.idle();
				await dragBy(page, BAR, -40);
				assert.equal(await state(page), 'row');
				await page.locator(BAR).dispatchEvent('click');
				await page.evaluate(() => {
					window.__flows = 0;
					const animate = (window.__animate = Element.prototype.animate);
					Element.prototype.animate = function (...args) {
						if (this.dataset?.ran) window.__flows += 1;
						return animate.apply(this, args);
					};
				});
				await dragBy(page, OUTPUT, -60);
				assert.equal(await state(page), 'full');
				const into = await page.evaluate(() => window.__flows);
				await dragBy(page, INPUT, 60);
				assert.equal(await state(page), 'row');
				const back = await page.evaluate(() => {
					Element.prototype.animate = window.__animate;
					return window.__flows;
				}) - into;
				if (reduced) assert.deepEqual([into, back], [0, 0], 'a reader who asked for no motion got the flow');
				else assert.ok(into > 0 && back > 0, `going full flowed ${into} commands, coming back ${back}`);
				await page.emulateMedia({ reducedMotion: null });
				assert.deepEqual(app.errors, []);
			});
		});
});
