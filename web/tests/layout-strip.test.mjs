// The day strip in a layout (da-zh2o, da-13bc, da-2a5y): dragged taller,
// dragged shut, and snapping to one whole day — driven through the built
// app in a real browser, measured on the preview with the button down.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	CHROME,
	DESKTOP,
	along,
	bar,
	cellExtents,
	dragFor,
	drawnRuns,
	draft,
	grab,
	pane,
	settled,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The strip body's padding as the strip's snap counts it (registry.ts). */
const PADDING = 16;
const todayBox = (dayHeight) => dayHeight + 2 * 2 + 15; // panes.ts: the day, datum's margins and the hour axis

/**
 * The days of the past the strip is holding. The past is one picture of
 * several days (da-hnyb) — a fortnight at most each — so the days are what
 * its pictures claim, not how many there are.
 */
const pastDays = (page) =>
	pane(page, 'strip')
		.locator('.stack .band')
		.evaluateAll((els) => els.reduce((n, b) => n + Number(b.dataset.days), 0));

describe('resizing', () => {
	// At 1000px tall, not 900: the row under the strip sits near its floor
	// (three panes stacked in the side column), so a drag can only take the
	// strip up to that floor — and whether that is one more day or none
	// is a threshold a few pixels of chrome move (the shell's bars, da-0a5o,
	// moved it). A taller screen leaves the strip room for a third day.
	test('the strip, dragged taller over a second, reaches further back as it goes and draws each depth once', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const before = await pastDays(page);
			// The root's column border, under the strip: down is a taller strip.
			const border = page.locator('section.root > .content > .split.column > [role="separator"]').first();
			const took = await dragFor(page, border, 0, 200, 1000);
			const during = drawnRuns(await seen());
			const mid = await pastDays(page);
			console.log(`  drag: ${took}ms, dayview ran ${during.dayview}x, nowview ${during.nowview}x; days ${before} → ${mid}`);
			assert.ok(mid > before, `the strip held ${before} days before and ${mid} during the drag: it did not grow live`);
			assert.ok(during.nowview >= 2 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			await page.mouse.up();
			await app.idle();
			const after = drawnRuns(await seen());
			const days = await pastDays(page);
			assert.ok(days > before, `a 200px taller strip holds no more days (${before} → ${days})`);
			// Only the past is re-drawn, one picture per depth the drag passed
			// through and never the same one twice: the width did not change,
			// so today's picture, and each asked for at an earlier tick, is kept.
			const views = (await seen()).filter((a) => a[0] === 'dayview').map((a) => a.join(' '));
			assert.ok(views.every((v) => v.includes('--end-date')), `today was re-drawn: ${views.join(' | ')}`);
			assert.equal(new Set(views).size, views.length, `a picture was drawn twice: ${views.join(' | ')}`);
			assert.ok(
				after.dayview >= 1 && after.dayview <= days - before,
				`${after.dayview} pictures for ${days - before} more days`
			);
			assert.ok(after.nowview - during.nowview <= 1, 'nowview, now shorter, redraws at most once at the drop');
		}, { viewport: { width: 1280, height: 1000 } });
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
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(twoStrips(85))) }
		);
	});
});

/** A root column of [a left-bar strip / a top-bar strip / the REPL] at `dayHeight`. */
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
	test('with 85px days the border above the strip lands on one day within ±8px, and passes 12px beyond', async () => {
		await withApp(
			async ({ page }) => {
				const snap = todayBox(85) + PADDING + CHROME.height;
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
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip', { dayHeight: 85 })))) }
		);
	});

	test('two strips in one column each snap to their own day height, 85px and 120px', async () => {
		await withApp(
			async ({ page }) => {
				const [top] = await cellExtents(page, 'column');
				// The border between the two strips; the REPL below them is what makes the app ready.
				const at = await grab(page, page.locator('.split.column > [role="separator"]').first());
				const snapA = todayBox(85) + PADDING + CHROME.height;
				await page.mouse.move(at.x, at.y - (top - snapA) + 6);
				const [a] = await cellExtents(page, 'column');
				assert.ok(Math.abs(a - snapA) <= 0.5, `the 85px strip showed ${a}px, not ${snapA}`);
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
					context.addInitScript(draft, JSON.stringify(along('column', widgetPane('a', 'daystrip', { dayHeight: 85 }), widgetPane('b', 'daystrip', { dayHeight: 120 }), widgetPane('repl', 'repl'))))
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
				await pane(page, 'strip').locator('.band.today svg').waitFor();
				const seen = await pane(page, 'strip').evaluate((section) => {
					const box = (s) => section.querySelector(s).getBoundingClientRect();
					const body = section.querySelector('.body');
					return {
						scrolls: body.scrollHeight - body.clientHeight,
						strip: box('.strip'),
						today: box('.band.today'),
						grip: box('.day-grip')
					};
				});
				assert.ok(seen.scrolls <= 0, `the body scrolls by ${seen.scrolls}px`);
				assert.ok(seen.today.top < seen.strip.top - 1, 'today\'s band is not clipped at the top');
				assert.ok(Math.abs(seen.today.bottom - seen.strip.bottom) <= 1, 'today\'s band, and its axis, left the bottom of the strip');
				assert.ok(seen.grip.top >= seen.strip.top - 0.5 && seen.grip.bottom <= seen.strip.bottom, 'the day-height notch is outside the strip');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('strip', 'daystrip')))) }
		);
	});
});
