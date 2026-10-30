// A finger's drag on the day strip follows it from the first move, every
// frame (da-uovtn). vividn, 2026-10-29: «When panning vertically, there is
// probably around ~200 ms delay until it starts tracking the finger after
// which it is smooth. Panning horizontally never gets smooth … These both
// absolutely must be always smooth and reactive», and «There should be no
// redrawing at all happening when panning around».
//
// So both gestures are asserted by what the renderer DID, from a trace of
// the drag (`support/pans.mjs`), on a phone with a strip full of marks:
// - vertical: the stack's own scroll, which the browser starts once the
//   page has answered the first `touchmove`. A press restyled every mark
//   in the strip before it could (a `:has()` whose subject was any element,
//   CliEntry.svelte), so the scroll waited for thousands of elements;
// - horizontal: the pan of a zoomed window, which moves the pictures on
//   layers of their own and nothing else: no paint, no restyle of a mark,
//   no layout, per step; and the band stays under the finger every frame;
// - the nowview's pan, the same recognizer's, which slid its picture by a
//   custom property on the band that every mark inherited: each step
//   restyled the whole picture.
// The probe that measured both on a real history is `support/measure-pans.mjs`.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, ON_COLLECT } from './support/app.mjs';
import { FRAME_MS, framesOf, liftFinger, readFrames, seedMarks, touchDrag, traceOf } from './support/pans.mjs';

before(startApp);
after(stopApp);

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };
// The strip is the dayview tab's on a phone (da-uzi5x), the nowview collect's.
const ON_DAYVIEW = { ...PHONE, home: { ...ON_COLLECT, active: 3 } };
const TODAY = '.band[data-days-ago="0"]';
const MOVES = 20;

/** Where a finger lands on today's band: the middle of what is on screen of it. */
const middleOf = (page) =>
	page.locator(TODAY).evaluate((el) => {
		const box = el.getBoundingClientRect();
		return { x: Math.round(box.left + box.width / 2), y: Math.round((Math.max(box.top, 0) + Math.min(box.bottom, innerHeight)) / 2) };
	});

/** How many elements the strip holds: what a restyle of all of it would count. */
const stripSize = (page) => page.locator('.strip').evaluate((strip) => strip.querySelectorAll('*').length);

test('a vertical drag on the strip scrolls the stack from the first move past slop, restyling none of the marks', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await seedMarks(app);
		const size = await stripSize(page);
		assert.ok(size > 2000, `the strip holds ${size} elements: too few to tell a restyle of it from none`);
		const cdp = await page.context().newCDPSession(page);
		const at = await middleOf(page);
		const seen = await framesOf(page, TODAY, () => traceOf(page, cdp, () => touchDrag(cdp, at, { x: 0, y: 8 }, MOVES)));
		const trace = seen.result;
		const frames = readFrames(seen);
		assert.ok(
			trace.restyled.most < size / 10,
			`a restyle during the drag took ${trace.restyled.most} of the strip's ${size} elements`
		);
		assert.ok(seen.scrolls.length > 0, 'the stack never scrolled');
		// Chrome's touch slop is a few moves of 8px; the scroll is under way
		// by the move after it, and not a wait later.
		assert.ok(frames.movesBeforeScroll <= 4, `${frames.movesBeforeScroll} moves were heard before the stack scrolled`);
	}, ON_DAYVIEW);
});

test('a horizontal pan moves the pictures and redraws none of them, under the finger every frame', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await seedMarks(app);
		const at = await middleOf(page);
		await page.locator('.strip').evaluate((strip, at) => {
			strip.dispatchEvent(new WheelEvent('wheel', { ctrlKey: true, deltaY: -300, clientX: at.x, clientY: at.y, bubbles: true, cancelable: true }));
		}, at);
		// The wheel's window is a preview until its pause commits it, 120ms
		// on, and a press before that pans nothing: the band says what the
		// committed window cannot pan past, the whole day's `start end`
		// until then. Then idle, for the pictures the commit asks for.
		await page.locator(`${TODAY}[data-pan-stop=""]`).waitFor();
		await app.idle();
		// The commit lays each band out again at the window (da-syaz8): then it is at rest.
		await page.waitForFunction(() => !document.querySelector('.band.stretched'));
		const cdp = await page.context().newCDPSession(page);
		const from = { x: at.x + 60, y: at.y };
		// The trace is of the drag with the finger still down: the release
		// commits the window, and that is a redraw the pan is owed.
		const seen = await framesOf(page, TODAY, () => traceOf(page, cdp, () => touchDrag(cdp, from, { x: -6, y: 0 }, MOVES, { lift: false })));
		await liftFinger(cdp);
		const trace = seen.result;
		const frames = readFrames(seen);
		assert.ok(frames.strays.length > MOVES / 2, `the band moved on ${frames.strays.length} frames of ${MOVES} moves`);
		assert.ok(frames.mostStray <= 1, `the band strayed ${frames.mostStray.toFixed(1)}px from the finger`);
		// The press and the pan's first step lay out and paint what they
		// show and hide — the wait's mark, the ruler naming the whole day —
		// and no step after them does, and no band ever.
		const bands = trace.painted.filter((name) => name.includes('dayview band '));
		assert.deepEqual(bands, [], `the pan painted a band's picture ${bands.length} times`);
		assert.ok(trace.paints <= 8, `the pan painted ${trace.paints} times: ${trace.painted}`);
		assert.ok(trace.restyled.most <= 50, `a step of the pan restyled ${trace.restyled.most} elements`);
		assert.ok(trace.layouts <= 2, `the pan laid the page out ${trace.layouts} times`);
		// In the thread's own time: a wall clock's counts the runner's other
		// files too, 18–20ms a move with four at once, 12 alone.
		assert.ok(trace.threadMs / MOVES < FRAME_MS, `the main thread took ${Math.round(trace.threadMs / MOVES)}ms a move`);
	}, ON_DAYVIEW);
});

test("the nowview's pan restyles none of its marks", async () => {
	await withApp(async (app) => {
		const { page } = app;
		await seedMarks(app);
		const band = 'section.pane[data-title="nowview"] .nowview-band';
		const size = await page.locator(band).evaluate((el) => el.querySelectorAll('*').length);
		assert.ok(size > 2000, `the nowview holds ${size} elements: too few to tell a restyle of it from none`);
		const box = await page.locator(band).boundingBox();
		const from = { x: Math.round(box.x + box.width / 2 - 60), y: Math.round(box.y + box.height / 2) };
		const cdp = await page.context().newCDPSession(page);
		const seen = await framesOf(page, band, () => traceOf(page, cdp, () => touchDrag(cdp, from, { x: 6, y: 0 }, MOVES, { lift: false })));
		await liftFinger(cdp);
		const trace = seen.result;
		assert.ok(readFrames(seen).strays.length > MOVES / 2, 'the nowview did not pan');
		assert.ok(trace.restyled.most <= 50, `a step of the pan restyled ${trace.restyled.most} of the nowview's ${size} elements`);
	}, PHONE);
});
