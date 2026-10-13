// The borders (da-jdop, da-jgrn, da-2a5y, da-zh2o): dragged, nudged,
// throttled, and down to a pane's frame and no further — driven through
// the built app in a real browser, the redraws counted through the seam.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { SETTLE_MS } from '../src/lib/layout/resizing.ts';
import {
	CHROME,
	DESKTOP,
	along,
	boxOf,
	canvasWidth,
	dragFor,
	drawnRuns,
	finalAsks,
	grab,
	pane,
	settled,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP on the desktop (da-6b3dj, sharedApp in support/app.mjs): a drag
// moves `main`'s ratios, which `solo` restores, and the panes that shrink to
// their frame open `main` from a draft of their own (`solo(fn, draft)`).
// The resizing tests that COUNT redraws keep an app each: after `solo`
// restores a dragged `main`, nowview redraws once more at its restored size
// AFTER `solo` has returned, and that run lands in their count.
const shared = sharedApp(DESKTOP);

describe('a border', () => {
	test('dragged 100px moves the ratio and redraws the picture at the new width', async () => {
		await shared.solo(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const repl = pane(page, 'repl');
			const before = await repl.boundingBox();
			const drawn = await canvasWidth(page, 'nowview');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x + i * 20, y);
			await page.mouse.up();
			await page.waitForFunction(
				([id, was]) =>
					Math.abs(document.querySelector(`section[data-pane="${id}"]`).getBoundingClientRect().width - was - 100) <= 2,
				['repl', before.width]
			);
			// The picture is re-asked at the narrower box, not squeezed into it.
			await page.waitForFunction(
				(was) => document.querySelector('section[data-pane="nowview"] svg.svg')?.width.baseVal.value < was,
				drawn
			);
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		});
	});

	// The kinds have no floor (da-zh2o): the side column, a holder over
	// nowview and the tail, drags down to its panes' outlines and no further.
	test('drags the side column down to its panes\' outlines and no further', async () => {
		await shared.solo(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 20; i++) await page.mouse.move(x + i * 80, y);
			await page.mouse.up();
			await settled(page);
			const side = await pane(page, 'side').boundingBox();
			assert.ok(Math.abs(side.width - CHROME.width) <= 1, `the side column was dragged to ${side.width}px`);
		});
	});
});

/** A drawn pane's content box, whole pixels down, as `paneArgv` asks for it. */
const contentWidth = (page, id) =>
	pane(page, id)
		.locator('.body')
		.evaluate((el) => {
			const s = getComputedStyle(el);
			return Math.floor(el.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
		});

/**
 * Wait for the page's next rendering step to have reported its resizes. A
 * new observer's first report comes at that step, after the observers made
 * before it — the panes' — have had theirs.
 */
const rendered = (page) =>
	page.evaluate(
		() =>
			new Promise((resolve) => {
				const observer = new ResizeObserver(() => {
					observer.disconnect();
					resolve();
				});
				observer.observe(document.body);
			})
	);

/** Wait until `id`'s picture is drawn at its pane's content box, `contentWidth`'s. */
const fits = (page, id) =>
	page.waitForFunction((of) => {
		const body = document.querySelector(`section[data-pane="${of}"] .body`);
		const s = getComputedStyle(body);
		const box = Math.floor(body.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
		return body.querySelector('svg.svg')?.width.baseVal.value === box;
	}, id);

// vividn's first look (da-jgrn): dragging a border was jittery, because every
// drawn pane re-ran its CLI command at every size it passed through. His
// second (da-2a5y): "have the widgets update some when dragging things
// around, rather than only when you let go, maybe just have a debounce
// baked in". So a drag redraws a drawn pane at most every quarter second
// (`THROTTLE_MS`, resizing.ts) — never at pointer rate — the drop flushes
// the newest box at once, and a burst of nudges is one redraw. Counted
// through the seam, never eyeballed, with the denominator printed.
// The clock is HELD in here (`clock`, da-r61i): a wall-clock minute re-runs
// the panes whose picture is of now, and a test that counts a drawn pane's
// runs over a second of real time would now and then count that tick as one
// of the drag's. A fixed clock never turns a minute.
const STILL = { ...DESKTOP, clock: todayAt(12) };

/**
 * Wait for a drop's redraw of `id`, and give up 400ms later.
 *
 * The drop may have nothing to ask for — the last throttle tick can
 * already have had the final box — so the wait needs a bound, and the
 * bound has to be measured on the same clock as the waiting. Playwright's
 * `timeout` is node's own; a `Date.now() - <node timestamp>` INSIDE the
 * page is not, because `STILL` freezes the page's clock (da-r61i). That
 * subtraction is a constant there: large and positive before the pinned
 * hour, negative for the rest of the day after it — so for most of the
 * day it never passed 200 and the wait hung on a redraw the assertion
 * below says may never come (da-ru9t).
 */
const waitForRedraw = (page, id, before) =>
	page
		.waitForFunction(([of, was]) => window.__datumSeen.filter((a) => a[0] === of).length > was, [id, before], { timeout: 400 })
		.catch((e) => {
			if (e.name !== 'TimeoutError') throw e;
		});

describe('resizing', () => {
	test('a border dragged 200px over a second redraws a drawn pane 1–6 times, and the final box once', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			const took = await dragFor(page, border, 200, 0, 1000);
			const during = drawnRuns(await seen());
			console.log(`  drag: ${took}ms, nowview ran ${during.nowview}x during it, dayview ${during.dayview}x`);
			// At least once: a drag redraws as it goes, not only at the drop. Not
			// twice — that is the throttle's (resizing.test.mjs): this counts the
			// CLI's runs, and a pane keeps one out, so on a loaded box one run
			// can take the whole second (da-i1ok).
			assert.ok(during.nowview >= 1 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			// The strip spans the page above this split: its box did not move,
			// so it has nothing to redraw — and redraws nothing.
			assert.equal(during.dayview, 0, `the strip redrew ${during.dayview}x for a border that did not touch it`);
			// Meanwhile the picture is still up, scaled to its box — never blank.
			assert.ok((await canvasWidth(page, 'nowview')) > 0, 'the picture went away during the drag');

			await page.mouse.up();
			const dropped = Date.now();
			// The drop is the leading edge: the newest box is asked for at
			// once, not a throttle period later — unless the last tick had it
			// already, in which case there is nothing left to ask.
			await waitForRedraw(page, 'nowview', during.nowview);
			await app.idle();
			const after = drawnRuns(await seen());
			const atDrop = after.nowview - during.nowview;
			console.log(`  drop: nowview ran ${atDrop}x, asked ${Date.now() - dropped}ms after it`);
			assert.equal(finalAsks(await seen(), 'nowview'), 1, `nowview ran ${atDrop} times at the drop, the final box more than once`);
			assert.equal(after.dayview, 0);
			// And the picture is the box's size: asked for at the final width,
			// not the old one stretched to it.
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, STILL);
	});

	// The case above's other branch, made certain rather than waited for
	// (da-ru9t): hold the pointer still until the throttle's tick has taken
	// the final box, and the drop's flush finds nothing pending — so no
	// redraw is coming and the wait can only end on its bound. This is the
	// test of the test. Under a bound read from the page's held clock it
	// ended nowhere, and the file died on the 30s timeout instead.
	test('a drop whose box the last tick already had asks for nothing, and the wait still ends', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			const at = await grab(page, border);
			for (let i = 1; i <= 5; i++) await page.mouse.move(at.x + i * 20, at.y);
			// Within a throttle period of the last of those moves, the trailing
			// tick draws the newest box — the one the pointer came to rest on.
			// Waited for as that box drawn, not as any run: under load a tick
			// can fire between two moves, and the first run is then of a box
			// the pointer has already left (da-2q30w, 321 !== 301).
			await fits(page, 'nowview');
			await app.idle();
			const before = drawnRuns(await seen()).nowview;
			// It is the FINAL box: the picture already fits the pane it is in,
			// which is what leaves the drop nothing to ask for.
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));

			const from = Date.now();
			await page.mouse.up();
			await waitForRedraw(page, 'nowview', before);
			const waited = Date.now() - from;
			await app.idle();
			const atDrop = drawnRuns(await seen()).nowview - before;
			console.log(`  drop: nowview ran ${atDrop}x, waited ${waited}ms for it`);
			assert.equal(atDrop, 0, `the drop asked ${atDrop}x for a box the tick already had`);
			assert.ok(waited < 5000, `the wait for a redraw that was never coming took ${waited}ms`);
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
		}, STILL);
	});

	test('ten arrow nudges in 300ms are one redraw', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			await border.focus();
			// 30ms apart, inside one `SETTLE_MS` burst: the pacing is the
			// input, so it is measured on the page's clock, held. On the wall
			// clock a loaded runner stretched a gap past `SETTLE_MS` and the
			// burst came out as two, the first after seven nudges (da-2q30w,
			// 226 !== 151). The one redraw is the burst's trailing edge, and
			// nothing is pushed after it, so once it is asked for and has
			// answered there is no second one coming.
			// Each press's box reaches `settle` by the pane's ResizeObserver,
			// at a rendering step and not on the held clock, so the clock waits
			// for it: moved first, it fired the flush before the last boxes
			// came. Of 20 idle runs, 6 asked for no run at all, a hang in
			// `fits` below, and 3 asked twice, the box at press 8 and then the
			// last.
			for (let i = 0; i < 10; i++) {
				await page.keyboard.press('ArrowRight');
				await rendered(page);
				await page.clock.runFor(30);
			}
			await page.clock.runFor(SETTLE_MS);
			await fits(page, 'nowview');
			await app.idle();
			const runs = drawnRuns(await seen());
			assert.equal(runs.nowview, 1, `nowview ran ${runs.nowview} times for ten nudges`);
			assert.equal(runs.dayview, 0, `the strip, untouched by this border, ran ${runs.dayview} bands`);
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
		}, { ...STILL, holdTimers: true });
	});

	test('a border is grabbed right beside the badges, which hang half over the gap', async () => {
		await shared.solo(async ({ page }) => {
			// The tail's badges are centred on its top outline, so half of each
			// hangs into the gap above — the column border between nowview and
			// tail — and no further: 8px of a 16px badge in an 8px gap (da-om9v).
			const border = pane(page, 'side').locator('.split.column > [role="separator"]').first();
			const badge = pane(page, 'tail').locator('.titlebar button.add');
			const [gap, more, tailBefore] = await Promise.all([boxOf(border), boxOf(badge), boxOf(pane(page, 'tail'))]);
			const middle = more.y + more.height / 2;
			assert.ok(Math.abs(middle - (tailBefore.y + 0.5)) <= 1, `the badge's middle (${middle}) is not on the outline (${tailBefore.y})`);
			assert.ok(Math.abs(more.y - (gap.y + gap.height - 8)) <= 1, `the badge's top (${more.y}) is not 8px above the gap's end (${gap.y + gap.height})`);
			// The gap the border is grabbed by is the one past ALL the bar's
			// badges — the maximize before the title (da-bzhq), then the `+`
			// and the `▾` after it. Read them, take the far edge of the run,
			// and check the point is clear of every one before pressing it.
			const [past, clear] = await pane(page, 'tail').evaluate((section) => {
				const badges = [...section.querySelectorAll(':scope > .titlebar .along button.badge')];
				const beside = Math.max(...badges.map((b) => b.getBoundingClientRect().right));
				const on = (at) => badges.some((b) => b.getBoundingClientRect().left <= at && at <= b.getBoundingClientRect().right);
				return [beside, !on(beside + 6)];
			});
			assert.ok(clear, `the point past the bar's badges (${past + 6}) is on a badge`);
			const x = past + 6;
			const y = gap.y + gap.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x, y - i * 6);
			await page.mouse.up();
			// The tail grows upward — by less than the 30px if nowview is at
			// its floor, but it grows: this was a drag, not a press on the badge.
			await page.waitForFunction(
				([was]) => document.querySelector('section[data-pane="tail"]').getBoundingClientRect().height > was + 8,
				[tailBefore.height]
			);
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the drag opened the menu instead');
		});
	});
});

// vividn's third look (da-zh2o): "let widgets get a lot smaller, until the
// display is gone and just a frame bar remains". Every kind a border can
// be dragged beside (the widget bar sizes itself, and the border beside
// it is no handle) shrinks to its frame, down a column AND across a row:
// the section is exactly the frame, and nothing of the widget is painted
// past it. The title lies OVER the content (da-13bc), so what is left of
// the content box is the inside half of the title's row — 8px down a
// column under a top bar, 0px across a row.
describe('a widget pane shrinks to its frame', () => {
	for (const kind of ['daystrip', 'repl', 'nowview', 'tail', 'custom', 'stats']) {
		for (const direction of ['column', 'row']) {
			const axis = direction === 'row' ? 'width' : 'height';
			test(`${kind}, across a ${direction}: only its frame is left, ${CHROME[axis]}px ${axis}`, async () => {
				await shared.solo(
					async ({ page }) => {
						const at = await grab(page, page.locator(`.split.${direction} > [role="separator"]`));
						for (let i = 1; i <= 20; i++) {
							await page.mouse.move(direction === 'row' ? at.x - i * 80 : at.x, direction === 'row' ? at.y : at.y - i * 60);
						}
						await page.mouse.up();
						await settled(page);
						const seen = await pane(page, 'a').evaluate((section, row) => {
							const content = section.querySelector(':scope > .content');
							const s = getComputedStyle(content);
							const b = section.getBoundingClientRect();
							// Halfway into the border gap past the frame, at every layer: nothing of the widget is there.
							const past = row ? [b.right + 4, b.top + b.height / 2] : [b.left + b.width / 2, b.bottom + 4];
							return {
								spills: document.elementsFromPoint(...past).some((el) => section.contains(el)),
								section: b[row ? 'width' : 'height'],
								content: row
									? content.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight)
									: content.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom)
							};
						}, direction === 'row');
						assert.ok(Math.abs(seen.content - (CHROME[axis] - 2)) <= 1, `${kind}: the content box is ${seen.content}px ${axis}, not the title's inside half`);
						assert.ok(!seen.spills, `${kind}: the widget paints outside its frame`);
						assert.ok(Math.abs(seen.section - CHROME[axis]) <= 1, `${kind}: the frame is ${seen.section}px ${axis}, not ${CHROME[axis]}`);
					},
					JSON.stringify(along(direction, widgetPane('a', kind), widgetPane('b', 'repl')))
				);
			});
		}
	}
});

// da-kpjn: a tabbed holder's strip is as tall as its badges, which are a
// finger's 24px under a coarse pointer, and the room kept under it for its
// children's titles is half that. Its floor counts the same, so a holder
// dragged down to it leaves its panes their frames and cuts none — under
// either pointer, and a fine one's floor where it always was.
describe('a tabbed holder shrinks to its strip and its panes\' frames', () => {
	const tabbed = {
		id: 'tabbed',
		tabs: [
			{
				id: 'tabbed-split',
				content: { id: 'inner', direction: 'row', ratios: [0.5, 0.5], children: [widgetPane('x', 'tail'), widgetPane('y', 'repl')] }
			},
			{ id: 'tabbed-aliases', content: { id: 'tabbed-aliases-widget', kind: 'aliases', settings: {} } }
		],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	};
	for (const [pointer, strip, options] of [
		['fine', 16, DESKTOP],
		['coarse', 24, { viewport: { width: 390, height: 844 }, device: { hasTouch: true, isMobile: true } }]
	]) {
		test(`under a ${pointer} pointer: a ${strip}px strip, ${strip / 2}px under it, and a pane's frame`, async () => {
			await withApp(async (app) => {
				const { page } = await app.solo(JSON.stringify(along('column', tabbed, widgetPane('b', 'repl'))));
				const at = await grab(page, page.locator('.split.column > [role="separator"]').first());
				for (let i = 1; i <= 20; i++) await page.mouse.move(at.x, at.y - i * 60);
				await page.mouse.up();
				await settled(page);
				const seen = await pane(page, 'tabbed').evaluate((section) => ({
					holder: section.getBoundingClientRect().height,
					strip: section.querySelector(':scope > .titlebar').getBoundingClientRect().height,
					under: parseFloat(getComputedStyle(section.querySelector(':scope > .content')).paddingTop),
					inner: document.querySelector('section[data-pane="x"]').getBoundingClientRect().height
				}));
				assert.deepEqual(
					{ strip: seen.strip, under: seen.under, inner: Math.round(seen.inner) },
					{ strip, under: strip / 2, inner: CHROME.height },
					`what is drawn: ${JSON.stringify(seen)}`
				);
				assert.ok(Math.abs(seen.holder - (strip + strip / 2 + CHROME.height)) <= 1, `the holder's floor is ${seen.holder}px: ${JSON.stringify(seen)}`);
			}, options);
		});
	}
});
