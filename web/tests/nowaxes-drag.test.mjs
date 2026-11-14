// The nowview's axes: a drag of a handle scales the window.
// Its story and helpers are support/nowaxes.mjs; one of four files, so each stays
// well inside test.mjs's 300 s file cap (da-9utsv).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { finger } from './support/press.mjs';
import { placeBar } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { bothWays, NOW, NOWVIEW, withAxes, read, pt, timePill, drewPill, minutesOf, ticksOf, rightmost } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

bothWays((vertical) => {
	// da-5whsm, vividn: «They should stay in the same place»: no longer halfway to the pill.
	test('with an absolute pill near the middle, the handle stands where it did and still scales', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			const before = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
			const l = await page.locator(`${NOWVIEW} svg.svg .clock-label`, { hasText: /^14:50$|^14:45$/ }).first();
			const b = await l.boundingBox();
			const word = await l.textContent();
			await finger(page, { x: b.x + b.width / 2, y: b.y + b.height / 2 });
			await timePill(page, `-t ${word}`);
			await drewPill(app);
			await app.spyOnRunner();
			const handle = page.locator(`${NOWVIEW} .handle[data-axis="rel"]`);
			const at = await until(() => handle.boundingBox(), (r) => r !== null);
			const g = await read(page);
			const [start, length] = g.y ? [g.frame.top, g.frame.height] : [g.frame.left, g.frame.width];
			const [from, across] = g.y ? [at.y + at.height / 2, at.x + at.width / 2] : [at.x + at.width / 2, at.y + at.height / 2];
			// Where it stood on now with no pill, on its tick (da-273ug): the pill moves the window's picture, not the handle.
			assert.ok((from - start) / length > 0.3 && (from - start) / length < 1, `it stands ${(from - start) / length} along`);
			assert.ok(Math.abs(at.x - before.x) <= 0.5 && Math.abs(at.y - before.y) <= 0.5, `it went from ${before.x},${before.y} to ${at.x},${at.y}`);
			await finger(page, pt(g, from, across), pt(g, from + 30, across));
			await until(
				() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
				(a) => a && a[a.indexOf('--history') + 1] !== '30m'
			);
		});
	});

	// vividn, on #1445: «Don't show the new extant over the dragged handle … adjust
	// the axis labels as you are dragging … the axis labels that you will have as
	// soon as you let go, so that it is seamless».
	test('mid-drag the axis wears the labels the release will, and the handle no word', async () => {
		await withAxes(vertical, async ({ page }) => {
			const labels = (sel) =>
				page.locator(NOWVIEW).evaluate((root, sel) => {
					const svg = root.querySelector('svg.svg');
					const y = !!svg.querySelector('[data-lo][data-axis="y"]');
					const f = svg.closest('.picture').getBoundingClientRect();
					return [...svg.querySelectorAll(sel)]
						.filter((l) => {
							const b = l.getBoundingClientRect();
							const [a, z] = y ? [b.top, f.bottom] : [b.left, f.right];
							return getComputedStyle(l).visibility !== 'hidden' && a >= (y ? f.top : f.left) && a < z;
						})
						.map((l) => l.textContent)
						.sort();
				}, sel);
			const before = await labels('.plot > .tick-label');
			const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
			const [x, y] = [h.x + h.width / 2, h.y + h.height / 2];
			await page.mouse.move(x, y);
			await page.mouse.down();
			// Toward now: a longer window.
			const g = await read(page);
			const from = g.y ? y : x;
			const to = from + ((g.y ? g.frame.bottom : g.frame.right) - from) * 0.35;
			await page.mouse.move(g.y ? x : to, g.y ? to : y, { steps: 8 });
			await page.locator(`${NOWVIEW} svg.svg.axis-live .live-axis .tick-label`).first().waitFor({ state: 'attached' });
			assert.equal(await page.locator('.handle.dragged').textContent(), '', 'the dragged handle says something');
			const during = await until(() => labels('.live-axis .tick-label'), (l) => l.length > 0);
			assert.notDeepEqual(during, before, 'the labels did not follow the drag');
			await page.mouse.up();
			await page.waitForFunction((sel) => !document.querySelector(`${sel} svg.svg.axis-live`), NOWVIEW);
			const after = await until(() => labels('.plot > .tick-label'), (l) => l.length > 0);
			assert.deepEqual(during, after, 'the release moved the labels');
		});
	});

	// Tabs hang 8px into the widget from a bar beside them (CI on #1445).
	for (const edge of ['top', 'left'])
		test(`with the bar on the ${edge}, every tab keeps its taps`, async () => {
			await withAxes(vertical, async ({ page }) => {
				await placeBar(page, 'side', edge);
				const seen = await until(
					() =>
						page.locator('section[data-pane="side"]').evaluate((p) => {
							const box = (e) => e.getBoundingClientRect();
							const taken = [...p.querySelectorAll('[role="tab"]')].flatMap((tab) => {
								const r = box(tab);
								return [1, 3, 5, 7, 9].flatMap((i) =>
									[1, 3, 5, 7, 9].map((j) => document.elementFromPoint(r.left + (r.width * i) / 10, r.top + (r.height * j) / 10)?.closest('.handle') && tab.textContent.trim())
								);
							});
							return { tabs: p.querySelectorAll('[role="tab"]').length, handles: p.querySelectorAll('.nowview-handles .handle').length, taken: taken.filter(Boolean) };
						}),
					(s) => s.handles === 2
				);
				assert.ok(seen.tabs > 1, 'no tabs to tap');
				assert.deepEqual(seen.taken, [], 'a handle took a tap on a tab');
			});
		});

	// vividn, on #1445: «make it smooth and continuous rather than having jumps».
	for (const kind of ['rel', 'abs'])
		test(`a drag of the ${kind} handle scales the window by where the finger goes, unrounded`, async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const h = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
				const g = await read(page);
				const from = pt(g, vertical ? h.y + h.height / 2 : h.x + h.width / 2, vertical ? h.x + h.width / 2 : h.y + h.height / 2);
				const start = vertical ? from.y : from.x;
				const anchor = g.pos.dot + ((NOW - g.instants.dot) * (g.pos.dot - g.pos.change)) / (g.instants.dot - g.instants.change);
				await finger(page, from, pt(g, start + 37, vertical ? from.x : from.y));
				const seen = await until(
					() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
					(a) => a?.[a.indexOf('--history') + 1] !== '30m'
				);
				const want = (30 * (anchor - start)) / (anchor - start - 37);
				const got = minutesOf(seen[seen.indexOf('--history') + 1]);
				assert.ok(Math.abs(got - want) / want < 0.03, `the window came to ${got}m, the finger to ${want}m`);
			});
		});

	/** Each diamond's place along time and its words, by axis. */
	const diamonds = (page) =>
		page.locator(`${NOWVIEW} .nowview-handles .handle`).evaluateAll((hs) =>
			Object.fromEntries(
				hs.map((h) => {
					const r = h.getBoundingClientRect();
					const y = !!h.closest('.vertical');
					return [h.dataset.axis, { at: y ? r.top + r.height / 2 : r.left + r.width / 2, word: h.dataset.at }];
				})
			)
		);
	/** Wait until each diamond is on its axis's rightmost tick, within a pixel, saying its time. */
	const onTicks = (page) =>
		until(
			async () => [await diamonds(page), await ticksOf(page)],
			([d, t]) => ['rel', 'abs'].every((k) => d[k] && t[k].length && Math.abs(d[k].at - rightmost(t[k]).at) <= 1 && d[k].word === rightmost(t[k]).word)
		);

	// vividn, 2026-11-13 (da-273ug): on the rightmost tick, a diamond «let's you
	// zoom in more easily using it, right now it's much easier to zoom out than in».
	test('the relative diamond dragged to the window\'s far end zooms in past the middle place\'s 2×, and lands on a tick', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const [d] = await onTicks(page);
			const g = await read(page);
			const anchor = g.pos.dot + ((NOW - g.instants.dot) * (g.pos.dot - g.pos.change)) / (g.instants.dot - g.instants.change);
			const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
			const across = vertical ? h.x + h.width / 2 : h.y + h.height / 2;
			const lo = (vertical ? g.frame.top : g.frame.left) + 2;
			await finger(page, pt(g, d.rel.at, across), pt(g, lo, across));
			const seen = await until(
				() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
				(a) => a?.[a.indexOf('--history') + 1] !== '30m'
			);
			const got = minutesOf(seen[seen.indexOf('--history') + 1]);
			const want = (30 * (anchor - d.rel.at)) / (anchor - lo);
			assert.ok(Math.abs(got - want) / want < 0.03, `the window came to ${got}m, the finger to ${want}m`);
			// Halfway to the anchor, the far end was twice in: from the tick, more.
			assert.ok(30 / got > 2.5, `the drag zoomed in ${30 / got}×`);
			await app.idle();
			await onTicks(page);
		});
	});

	test('after a step of the numbers, each diamond is on its axis\'s rightmost tick again', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
			await finger(page, { x: h.x + h.width / 2, y: h.y + h.height / 2 });
			await page.locator('.nowview-wheel .notch.held').waitFor();
			await page.locator('.nowview-wheel .notch[data-k="1"]').click();
			await until(
				() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
				(a) => a && a[a.indexOf('--history') + 1] !== '30m'
			);
			await app.idle();
			await onTicks(page);
		});
	});
});
