// Collect moves nothing up and down at rest, so a vertical scroll over it is the page's (da-tsomc, vividn,
// 2026-11-09: «if you scroll up and down on the collect widget you should still be able to scroll around in the
// page, since collect doesn't have any up and down type movement»): a finger's drag and the wheel. What it does
// move stays its own: a sideways drag pans the rows, a pinch zooms them (collect-zoom.test.mjs), and the picture
// layout zoomed in pans up and down under a finger.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { pickOption, rested } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// The second SE, 375×667: on the first, 320×568, the tutorial's collect is a third shorter since da-je8ju, two rows at
// the smallest zoom, and the café's five overrun it however small, so «rows that fit» could not be played there.
const PHONE = { device: devices['iPhone SE (3rd gen)'], query: '?db=tutorial' };
const TAB = 'moment';
const FIELDS = `[data-pane="${TAB}-fields"]`;
const SCROLLER = 'section.holder.root > .content';
const FIRST = `${FIELDS} .fields button[data-field]`;

/** The tutorial's moment tab, scrolled so collect's middle is the screen's: room to scroll both ways. */
async function onCollect(page) {
	await page.locator('.root-bar [role="tab"]', { hasText: TAB }).click();
	const button = page.locator(FIRST).first();
	await button.waitFor();
	await page.evaluate(
		async ([fields, scroller]) => {
			const s = document.querySelector(scroller);
			const box = document.querySelector(fields).getBoundingClientRect();
			const screen = s.getBoundingClientRect();
			s.scrollTop += box.top + box.height / 2 - (screen.top + screen.height / 2);
			await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
		},
		[FIELDS, SCROLLER]
	);
	return buttonAt(page);
}

const scrollTop = (page) => page.locator(SCROLLER).evaluate((s) => s.scrollTop);

/** The middle of collect's first field button, once the page's scroll has come to rest (a swipe's fling). */
const buttonAt = (page) => middleOf(page, FIRST);

/** The middle of the now strip at rest, beside the rows (da-90nlf). */
const stripAt = (page) => middleOf(page, `${FIELDS} .bar > [data-strip].pans`);

async function middleOf(page, selector) {
	// The offset read, then read again 150 ms on: still once the two agree.
	const twice = () => page.locator(SCROLLER).evaluate(async (s) => [s.scrollTop, await new Promise((r) => setTimeout(() => r(s.scrollTop), 150))]);
	await until(twice, ([was, now]) => was === now);
	const at = await page.locator(selector).first().boundingBox();
	return { x: Math.round(at.x + at.width / 2), y: Math.round(at.y + at.height / 2) };
}

const count = (app) => app.entries(200).then((rows) => rows.length);

/**
 * One finger drawn `by` px from `from` in ten moves, through CDP, as a hand does, and `settles` at rest after the
 * lift: what the drag set going (the page's fling, the rows' glide) has stopped where the next read looks.
 */
async function drag(page, from, by, settles) {
	const cdp = await page.context().newCDPSession(page);
	const at = (k) => [{ id: 7, x: from.x + by.x * k, y: from.y + by.y * k }];
	try {
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: at(0) });
		for (let i = 1; i <= 10; i++) {
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: at(i / 10) });
			// 16 ms a move, a frame each, is the drag's speed, which the rows' momentum is read from
			// (`speedOver`, #lib/grip.ts) and the page's fling from: the gesture's shape, not a wait for the page.
			await page.waitForTimeout(16);
		}
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	} finally {
		await cdp.detach();
	}
	// The page's fling and the rows' glide (`glideOn`, decaying by TAU) have stopped.
	await rested(page, settles);
}

/**
 * Collect's rows zoomed to their largest (`by` < 0) or smallest, by ctrl+wheel over them: the lesson's five then
 * run past the room's width, or fit inside it.
 */
async function zoom(page, by) {
	const at = await onCollect(page);
	// The click on the tab opened the window after a switch (da-d9p5m), in which a sideways drag is the frame's
	// swipe: it cancelled the rows' pan at its first 10px, 20 of 100 (da-72c7z). Waited out from the switch.
	await rested(page, page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: TAB }), { tapped: true });
	await page.mouse.move(at.x, at.y);
	await page.keyboard.down('Control');
	for (let i = 0; i < 5; i++) await page.mouse.wheel(0, by);
	await page.keyboard.up('Control');
	// The rows have glided to the new zoom (`.spot.shelf.glide`'s transitions) and been measured again.
	await rested(page, page.locator(`${FIELDS} .room`));
}

// On rows that fit, and on rows that overrun the room, where a sideways drag is collect's pan and so a vertical one
// must be let go before it becomes one; and from the now strip at rest beside them, where a sideways drag pans the
// rows too (da-90nlf).
for (const [where, by, pans, from] of [
	['collect rows that fit', 200, null, buttonAt],
	['collect rows that overrun the room', -200, 'x', buttonAt],
	['the now strip beside rows that overrun the room', -200, 'x', stripAt]
])
	test(`a finger dragged up or down from ${where} scrolls the tutorial page, and logs nothing`, async () => {
		await withApp(async (app) => {
			const { page } = app;
			await zoom(page, by);
			await onCollect(page);
			const at = await from(page);
			assert.equal(await page.locator(`${FIELDS} .room`).getAttribute('data-pans'), pans, `${where}: the case is not tested`);
			const [top, logged] = [await scrollTop(page), await count(app)];
			const first = page.locator(FIRST).first();
			await drag(page, at, { x: 0, y: -120 }, first);
			const next = await from(page);
			const up = await scrollTop(page);
			assert.ok(up - top > 60, `a drag up of 120px over collect scrolled the page ${up - top}px`);
			await drag(page, next, { x: 0, y: 120 }, first);
			await from(page);
			const down = up - (await scrollTop(page));
			assert.ok(down > 60, `a drag down of 120px over collect scrolled the page back ${down}px`);
			assert.equal(await count(app), logged, 'neither drag logged anything');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

test('a sideways finger from the now strip at rest still pans the rows', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await zoom(page, -200);
		const first = page.locator(FIRST).first();
		const x = async () => (await first.boundingBox()).x;
		const was = await x();
		// The rows panned along on the room, then back from the strip.
		await drag(page, await onCollect(page), { x: -100, y: 0 }, first);
		const panned = await x();
		assert.ok(panned < was - 30, `a drag left on the rows moved them from ${was} to ${panned}`);
		await drag(page, await stripAt(page), { x: 100, y: 0 }, first);
		assert.ok((await x()) > panned + 30, `a drag right from the strip moved the rows from ${panned} to ${await x()}`);
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('the wheel over collect scrolls the tutorial page', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const at = await onCollect(page);
		const top = await scrollTop(page);
		await page.mouse.move(at.x, at.y);
		await page.mouse.wheel(0, 120);
		await page.waitForFunction(([s, top]) => document.querySelector(s).scrollTop - top > 60, [SCROLLER, top]);
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('the picture layout zoomed in still pans under a vertical finger, and the page stays where it is', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (let i = 0; i < 12; i++) assert.equal((await app.runLine(`setup f${i} --from-json '${JSON.stringify({ field: `f${i}`, verb: 'occur', y: (i + 0.5) / 12, height: 0.02 })}'`)).code, 0);
			await app.reload();
			const pane = await page.locator('section.pane[data-title="collect"]').getAttribute('data-pane');
			await page.locator('section.pane[data-title="collect"] .bar [data-field]').first().waitFor();
			await pickOption(page, pane, 'the buttons stand', 'aligned to the picture');
			const bar = page.locator('section.pane[data-title="collect"] .fields[data-align="picture"] .bar');
			await bar.waitFor();
			const box = await bar.boundingBox();
			const mid = { x: Math.round(box.x + 20), y: Math.round(box.y + box.height / 2) };

			// Zoomed in by ctrl+wheel, a trackpad's pinch: the marker says which part is in view.
			const marker = page.locator('section.pane[data-title="collect"] button.range');
			await page.mouse.move(mid.x, mid.y);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -400);
			await page.keyboard.up('Control');
			await marker.waitFor();
			const from = () => marker.evaluate((el) => Number(el.dataset.from));
			const [was, top] = [await from(), await scrollTop(page)];
			await drag(page, mid, { x: 0, y: 80 }, bar);
			assert.ok((await from()) < was, `a drag down over the zoomed picture moved the view from ${was} to ${await from()}`);
			assert.equal(await scrollTop(page), top, 'and left the page where it was');
			assert.deepEqual(app.errors, []);
		},
		{ device: { ...devices['iPhone 12'] }, home: ON_COLLECT }
	);
});
