// The day strip in a layout (da-zh2o, da-13bc, da-2a5y): dragged taller,
// dragged shut, and snapping to one whole day — driven through the built
// app in a real browser, measured on the preview with the button down.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import {
	CHROME,
	DESKTOP,
	along,
	bar,
	cellExtents,
	dragFor,
	drawnRuns,
	draft,
	finalAsks,
	grab,
	pane,
	settled,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// Every test here opens an app of its own (da-6b3dj): the four on a draft
// measured no faster through a shared app's `solo(fn, draft)` — a boot into
// a small tree is mostly its strips' drawing, which `solo` pays too, and
// the shared app's first boot draws the whole `main` — and the drag over a
// second is alone on a taller screen and a held clock.

/** The strip body's padding as the strip's snap counts it (registry.ts). */
const PADDING = 16;
const todayBox = (dayHeight) => dayHeight + 15; // panes.ts: the day, and the ruler row the strip draws under it

/**
 * The days of the past the strip is holding — one picture each since
 * da-iu9i, so the days are what its pictures claim, not how many there are.
 */
const pastDays = (page) =>
	pane(page, 'strip')
		.locator('.stack .band')
		.evaluateAll((els) => els.reduce((n, b) => n + Number(b.dataset.days), 0));

/**
 * The days of the past the strip is SHOWING: those whose whole date stamp
 * is inside the stack's box.
 *
 * Since da-bewc the strip holds a fortnight however short its box, so
 * that there is a past above the box to scroll back through — which means
 * the days it holds no longer answer "how far back does this reach", and
 * the days on screen do.
 */
const shownDays = (page) =>
	pane(page, 'strip')
		.locator('.stack')
		.evaluate((stack) => {
			const box = stack.getBoundingClientRect();
			let n = 0;
			// The date stamps are the strip's own column now (da-iu9i), one
			// per row, beside the picture rather than inside it.
			for (const row of stack.querySelectorAll('.row')) {
				const mark = row.querySelector('.stamp').getBoundingClientRect();
				if (mark.top >= box.top - 1 && mark.bottom <= box.bottom + 1) n += 1;
			}
			return n;
		});

/**
 * Hold the strip's day height at `px`, as a saved layout would.
 *
 * ⚑ THE BOX ONLY SAYS HOW MUCH PAST A STRIP HOLDS ONCE THE DAY IS HELD
 * (da-r0ydu). Untouched, the day FOLLOWS the widget — one day fills it at
 * any height — so a border dragged taller draws a taller DAY and never
 * more days, and a strip at rest has no more past above today than the
 * sliver of yesterday a pinned today leaves (da-2hd21). The
 * tests below are about the box: what a drag shows as it goes, and what
 * the notch lands on. So they enter the second state first, the way a
 * reader does with the `+` handle. The drafts further down set
 * `dayHeight` in the tree for the same reason.
 */
const holdDay = (page, px) =>
	page.evaluate((px) => {
		window.datum.layout.apply((tree) => {
			const copy = structuredClone(tree);
			const walk = (pane) => {
				for (const tab of pane.tabs)
					if ('children' in tab.content) tab.content.children.forEach(walk);
					else if (tab.content.kind === 'daystrip')
						tab.content.settings = { ...tab.content.settings, dayHeight: px };
			};
			walk(copy.root);
			return copy;
		});
	}, px);

describe('resizing', () => {
	// At 1000px tall, not 900: the row under the strip sits near its floor
	// (three panes stacked in the side column), so a drag can only take the
	// strip up to that floor — and whether that is one more day or none
	// is a threshold a few pixels of chrome move (the shell's bars, da-0a5o,
	// moved it). A taller screen leaves the strip room for a third day.
	test('the strip, dragged taller over a second, shows further back as it goes and re-draws nothing it holds', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// 60, which is short enough to leave room above today at this
			// height: the drag has to have somewhere further back to GO,
			// and a strip showing exactly one day has nowhere.
			await holdDay(page, 60);
			await pane(page, 'strip').locator('.stack .band').first().waitFor();
			const seen = await app.spyOnRunner();
			const held = await pastDays(page);
			const before = await shownDays(page);
			// The root's column border, under the strip: down is a taller strip.
			const border = page.locator('section.root > .content > .tab-body > .split.column > [role="separator"]').first();
			const took = await dragFor(page, border, 0, 200, 1000);
			const during = drawnRuns(await seen());
			const mid = await shownDays(page);
			console.log(`  drag: ${took}ms, dayview ran ${during.dayview}x, nowview ${during.nowview}x; shown ${before} → ${mid} of ${held}`);
			assert.ok(mid > before, `the strip showed ${before} days before and ${mid} during the drag: it did not grow live`);
			// Once at least, not twice: see layout-floors' drag (da-i1ok).
			assert.ok(during.nowview >= 1 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			await page.mouse.up();
			await app.idle();
			const all = await seen();
			const after = drawnRuns(all);
			const days = await shownDays(page);
			assert.ok(days > before, `a 200px taller strip shows no more days (${before} → ${days})`);
			// And it cost nothing: this drag stays inside the fortnight the
			// stack already holds (da-bewc), and 200px of height changes
			// neither the width nor the day height, so no picture's argv
			// changed and datum was not asked again — the days that came
			// into view were drawn before the drag began.
			assert.equal(await pastDays(page), held, `the drag changed the days held: ${held} → ${await pastDays(page)}`);
			assert.equal(during.dayview, 0, `${during.dayview} pictures drawn during a drag inside the fortnight`);
			assert.equal(after.dayview, 0, `${after.dayview} pictures drawn at the drop of a drag inside the fortnight`);
			// The drop is counted by the box it asks for, not by when a run
			// starts (da-eghi9, da-i1ok): a run the drag asked for can start
			// after the button is let go.
			const final = finalAsks(all, 'nowview');
			assert.equal(final, 1, `nowview, now shorter, was drawn at its final box ${final} times`);
			// The clock is HELD (`clock`, da-r61i): a wall-clock minute re-draws
			// today's band, and this counts the bands a drag draws over a
			// second of real time. A fixed clock never turns a minute.
		}, { viewport: { width: 1280, height: 1000 }, clock: todayAt(12) });
	});
});

describe('the title on the line', () => {
	// Its kind has no floor (da-zh2o), so what is left of a strip dragged
	// shut is its frame, and the frame follows the bar's edge: a left-hand
	// title adds width, not height, so that strip keeps only its outline.
	test('dragged shut, a left-bar strip keeps its outline and a top-bar one 8px more', async () => {
		await withApp(
			async ({ page }) => {
				const borders = page.locator('.split.column > [role="separator"]');
				for (const i of [0, 1]) {
					const at = await grab(page, borders.nth(i));
					for (let step = 1; step <= 10; step++) await page.mouse.move(at.x, at.y - step * 60);
					await page.mouse.up();
					await settled(page);
				}
				const [left, top] = await cellExtents(page, 'column');
				assert.ok(Math.abs(left - 2) <= 0.5, `the left-bar strip stopped at ${left}px, its outline is 2px`);
				assert.ok(Math.abs(top - CHROME.height) <= 0.5, `the top-bar strip stopped at ${top}px, its frame is ${CHROME.height}px`);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(twoStrips(HELD))) }
		);
	});
});

/** A held day height that is not the retired literal — see below. */
const HELD = 100;

/**
 * A root column of [a left-bar strip / a top-bar strip / the REPL] at
 * `dayHeight`.
 *
 * ⚑ NEVER AT 85 (da-r0ydu). These trees go in as a DRAFT, so they are read
 * through `inline` — and `RETIRED_SETTINGS` (shell.ts) drops a saved
 * `dayHeight: 85` there, because that is the literal `defaults.ts` wrote
 * into every tree before the day began following its widget. A fixture at
 * 85 would therefore arrive FOLLOWING its box, with no height snap at all,
 * and these tests are about what a strip does when its day height is
 * HELD. Any other number says that; 85 says «saved before da-r0ydu».
 */
const twoStrips = (dayHeight) => ({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [
			{
				id: 'root-tab',
				content: {
					id: 'col',
					direction: 'column',
					ratios: [0.34, 0.33, 0.33],
					children: [
						widgetPane('left', 'daystrip', { dayHeight }, 'left'),
						widgetPane('top', 'daystrip', { dayHeight }),
						widgetPane('repl', 'repl')
					]
				}
			}
		],
		active: 0,
		bar
	}
});

// "let the height of the dayview get smaller than 1 day, but put a
// snapping notch at exactly one day" (da-zh2o). The strip's snap is one
// whole day at its day height, plus its padding and its frame: a pointer
// within 8px of it lands the border ON it, 12px beyond goes past, and the
// drop keeps what the drag showed. Measured on the PREVIEW, button down.
describe("the strip's notch", () => {
	test('with a HELD day height the border above the strip lands on one day within ±8px, and passes 12px beyond', async () => {
		await withApp(
			async ({ page }) => {
				const snap = todayBox(HELD) + PADDING + CHROME.height;
				const [, strip] = await cellExtents(page, 'column');
				const at = await grab(page, page.locator('.split.column > [role="separator"]'));
				// Moving the border down by d shrinks the strip below it by d.
				const toward = async (extent) => {
					await page.mouse.move(at.x, at.y + strip - extent);
					return (await cellExtents(page, 'column'))[1];
				};
				for (const off of [8, 4, 0, -4, -8]) {
					const shown = await toward(snap + off);
					assert.ok(Math.abs(shown - snap) <= 0.5, `a pointer ${off}px off the notch showed the strip at ${shown}px, not ${snap}`);
				}
				for (const off of [12, -12]) {
					const shown = await toward(snap + off);
					assert.ok(Math.abs(shown - (snap + off)) <= 1, `a pointer ${off}px off the notch showed the strip at ${shown}px, not ${snap + off}`);
				}
				await toward(snap + 5);
				await page.mouse.up();
				await settled(page);
				const dropped = (await cellExtents(page, 'column'))[1];
				assert.ok(Math.abs(dropped - snap) <= 0.5, `the drop kept ${dropped}px, not the ${snap}px the drag showed`);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip', { dayHeight: HELD })))) }
		);
	});

	test('two strips in one column each snap to their own day height, 100px and 120px', async () => {
		await withApp(
			async ({ page }) => {
				const [top] = await cellExtents(page, 'column');
				// The border between the two strips; the REPL below them is what makes the app ready.
				const at = await grab(page, page.locator('.split.column > [role="separator"]').first());
				const snapA = todayBox(HELD) + PADDING + CHROME.height;
				await page.mouse.move(at.x, at.y - (top - snapA) + 6);
				const [a] = await cellExtents(page, 'column');
				assert.ok(Math.abs(a - snapA) <= 0.5, `the ${HELD}px strip showed ${a}px, not ${snapA}`);
				const snapB = todayBox(120) + PADDING + CHROME.height;
				const [, bottom] = await cellExtents(page, 'column');
				// The pointer now: at the notch A snapped to, and B is what is left.
				await page.mouse.move(at.x, at.y - (top - snapA) + (bottom - snapB) - 6);
				const [, b] = await cellExtents(page, 'column');
				assert.ok(Math.abs(b - snapB) <= 0.5, `the 120px strip showed ${b}px, not ${snapB}`);
				await page.mouse.up();
			},
			{
				...DESKTOP,
				prepare: (context) =>
					context.addInitScript(draft, JSON.stringify(along('column', widgetPane('a', 'daystrip', { dayHeight: HELD }), widgetPane('b', 'daystrip', { dayHeight: 120 }), widgetPane('repl', 'repl'))))
			}
		);
	});

	// Shorter than one day the strip clips today's band from the top and
	// keeps its hour axis on screen, and the pane's body does not scroll;
	// the day-height notch is clamped into the box, so it can still be had.
	test('in a pane 60px tall the strip clips today\'s band, keeps its axis, and the body does not scroll', async () => {
		await withApp(
			async ({ page }) => {
				const [, strip] = await cellExtents(page, 'column');
				const at = await grab(page, page.locator('.split.column > [role="separator"]'));
				await page.mouse.move(at.x, at.y + strip - 60);
				await page.mouse.up();
				await settled(page);
				assert.ok(Math.abs((await cellExtents(page, 'column'))[1] - 60) <= 1);
				await pane(page, 'strip').locator('.row.pinned svg').waitFor();
				const seen = await pane(page, 'strip').evaluate((section) => {
					const box = (s) => section.querySelector(s).getBoundingClientRect();
					const body = section.querySelector('.body');
					return {
						scrolls: body.scrollHeight - body.clientHeight,
						strip: box('.strip'),
						// The band, not its row: the row gives way to the stack's
						// sliver (da-2hd21) and the band runs out past its top.
						today: box('.row.pinned svg'),
						axis: box('.axis')
					};
				});
				assert.ok(seen.scrolls <= 0, `the body scrolls by ${seen.scrolls}px`);
				assert.ok(seen.today.top < seen.strip.top - 1, 'today\'s band is not clipped at the top');
				assert.ok(Math.abs(seen.axis.bottom - seen.strip.bottom) <= 1, 'the hour ruler left the bottom of the strip');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip', { freezeToday: true })))) }
		);
	});
});
