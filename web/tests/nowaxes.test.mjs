// The nowview's axes as drawn, and where a press takes a handle or puts a pill.
// Its story and helpers are support/nowaxes.mjs; one of four files, so each stays
// well inside test.mjs's 300 s file cap (da-9utsv).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { finger } from './support/press.mjs';
import { until } from './support/until.mjs';
import { bothWays, NOWVIEW, withAxes, read, pt, pillsOf, ticksOf, rightmost } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

bothWays((vertical) => {
	test('no pill row: the picture fills the widget, the clock written over the band and the relative times under it', async () => {
		await withAxes(vertical, async ({ page }) => {
			assert.equal(await page.locator(`${NOWVIEW} .nowview-margin`).count(), 0, 'the pill row is still drawn');
			const g = await read(page);
			// The band starts under the frame's own titles' 8px and runs to the
			// widget's foot, and the picture fills it: as little room over it
			// as under it, the pane's own inset.
			assert.ok(Math.abs(g.band.top - g.widget.top - 8) <= 1, `the band starts ${g.band.top - g.widget.top}px down the widget`);
			assert.ok(Math.abs(g.widget.bottom - g.band.bottom) <= 1, `${g.widget.bottom - g.band.bottom}px under the band`);
			const [over, under] = [g.picture.top - g.band.top, g.band.bottom - g.picture.bottom];
			assert.ok(over <= under + 1 && under <= 8, `${over}px over the picture, ${under}px under it`);

			const lines = await page.locator(NOWVIEW).evaluate((root) => {
				const svg = root.querySelector('svg.svg');
				const y = !!svg.querySelector('[data-lo][data-axis="y"]');
				const box = (e) => e.getBoundingClientRect();
				const across = (e) => (y ? box(e).left + box(e).width / 2 : box(e).top + box(e).height / 2);
				const shown = (e) => getComputedStyle(e).visibility !== 'hidden' && box(e).width > 0;
				const dash = (e) => getComputedStyle(e).strokeDasharray;
				const [rel, abs] = ['path.axis', 'path.clock-axis'].map((s) => across(svg.querySelector(s)));
				return {
					rel,
					abs,
					clock: [...svg.querySelectorAll('.clock-label')].filter(shown).map((l) => ({ text: l.textContent, at: across(l) })),
					ticks: [...svg.querySelectorAll('.tick-label')].filter(shown).map((l) => ({ text: l.textContent, at: across(l) })),
					clockDash: [...svg.querySelectorAll('.clock-mark')].map(dash),
					gridDash: [...svg.querySelectorAll('.gridline')].map(dash)
				};
			});
			// The clock above the band, or right of it; the relative times below it, or left.
			assert.ok(lines.clock.length >= 2, JSON.stringify(lines.clock));
			for (const l of lines.clock) {
				assert.match(l.text, /^(\d\d:\d\d|:\d\d)$/);
				assert.ok(vertical ? l.at > lines.abs : l.at < lines.abs, `${l.text} is not on the clock's side`);
			}
			assert.ok(lines.ticks.length >= 2, JSON.stringify(lines.ticks));
			for (const l of lines.ticks) {
				assert.match(l.text, /^[-+]\d/);
				assert.ok(vertical ? l.at < lines.rel : l.at > lines.rel, `${l.text} is not on the relative side`);
			}
			assert.ok(lines.clockDash.every((d) => d === 'none'), `a clock line is dashed: ${lines.clockDash}`);
			assert.ok(lines.gridDash.length && lines.gridDash.every((d) => d !== 'none'), `a relative line is solid: ${lines.gridDash}`);
		});
	});

	// vividn, on #1445: «small white diamonds, 2:1 ratio so they are longer
	// along the axis than sticking out», and 2026-11-13 (da-273ug): «on the
	// first tick mark of each axis … By first I mean furthest to the right».
	test('one white 2:1 diamond on each axis’s line, on its rightmost tick, a press on it no scrub, and no label under it', async () => {
		await withAxes(vertical, async ({ page }) => {
			const ticks = await ticksOf(page);
			const seen = await page.locator(NOWVIEW).evaluate((root) => {
				const svg = root.querySelector('svg.svg');
				const y = !!svg.querySelector('[data-lo][data-axis="y"]');
				const box = (e) => e.getBoundingClientRect();
				const frame = box(svg.closest('.picture'));
				const shown = [...svg.querySelectorAll('.tick-label, .clock-label')].filter((l) => getComputedStyle(l).visibility !== 'hidden');
				return {
					stroke: getComputedStyle(svg.querySelector('path.axis')).stroke,
					cut: shown.filter((l) => {
						const b = box(l);
						return y ? b.top < frame.top && b.bottom > frame.top : b.left < frame.left && b.right > frame.left;
					}).map((l) => l.textContent),
					handles: [...root.querySelectorAll('.nowview-handles .handle')].map((h) => {
						const [r, d] = [box(h), getComputedStyle(h, '::before')];
						const [cx, cy] = [r.left + r.width / 2, r.top + r.height / 2];
						const line = box(svg.querySelector(h.dataset.axis === 'rel' ? 'path.axis' : 'path.clock-axis'));
						const [w, t] = [parseFloat(d.width), parseFloat(d.height)];
						return {
							axis: h.dataset.axis,
							at: h.dataset.at,
							along: y ? t : w,
							out: y ? w : t,
							clip: d.clipPath,
							fill: d.backgroundColor,
							place: y ? cy : cx,
							off: h.dataset.axis === 'rel' ? (y ? cx - line.right : cy - line.top) : y ? cx - line.left : cy - line.bottom,
							// On a tick, its label stands beside it: the diamond may reach 2px
							// into the label's box across the axis, its descent, which no digit's ink reaches.
							under: shown.filter((l) => {
								const b = box(l);
								const [a, c] = [Math.min(cx + w / 2, b.right) - Math.max(cx - w / 2, b.left), Math.min(cy + t / 2, b.bottom) - Math.max(cy - t / 2, b.top)];
								return a > 0 && c > 0 && (y ? a : c) > 2;
							}).map((l) => l.textContent)
						};
					}).sort((a, b) => a.axis.localeCompare(b.axis))
				};
			});
			assert.deepEqual(seen.cut, [], 'the window\'s far end cuts a label in two');
			assert.deepEqual(seen.handles.map((h) => h.axis), ['abs', 'rel']);
			for (const h of seen.handles) {
				// da-6i27j: «a just a tiny bit bigger», 20 × 10 from 16 × 8.
				assert.deepEqual([h.along, h.out], [20, 10], `${h.axis}: ${h.along} along, ${h.out} out`);
				assert.match(h.clip, /^polygon/);
				assert.equal(h.fill, seen.stroke === 'white' ? 'rgb(255, 255, 255)' : seen.stroke);
				const tick = rightmost(ticks[h.axis]);
				assert.ok(Math.abs(h.place - tick.at) <= 1, `${h.axis} stands at ${h.place}, its rightmost tick at ${tick.at}: ${JSON.stringify(ticks[h.axis])}`);
				assert.equal(h.at, tick.word);
				assert.ok(Math.abs(h.off) <= 1, `${h.axis} is ${h.off}px off its line`);
				assert.deepEqual(h.under, []);
			}
			const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
			await finger(page, { x: h.x + h.width / 2, y: h.y + h.height / 2 });
			await page.locator('.nowview-wheel .notch.held').waitFor();
			assert.deepEqual(await pillsOf(page), [], 'a press on the handle made a pill');
		});
	});

	// «It should be possible to still put a time pill by clicking above the diamond».
	for (const kind of ['rel', 'abs'])
		test(`a press just out of the ${kind} diamond puts a time pill`, async () => {
			await withAxes(vertical, async ({ page }) => {
				const h = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
				const [x, y] = [h.x + h.width / 2, h.y + h.height / 2];
				// Out of the band: the relative axis stands under it, or left of it. A
				// touch within about 3px of the 12px target is the browser's to give to it.
				const by = kind === 'rel' ? 10 : -10;
				await finger(page, vertical ? { x: x - by, y } : { x, y: y + by });
				await until(() => pillsOf(page), (p) => p.length === 1);
				assert.equal(await page.locator('.nowview-wheel').count(), 0, 'the press opened the numbers');
			});
		});

	// vividn, 2026-11-05: «the hit box should extend into the data section a
	// bit more--this way it is easy to add a time pill on the axis side right
	// by it but still comfortably grab and drag the diamond» (da-6i27j).
	for (const kind of ['rel', 'abs'])
		test(`the ${kind} handle is grabbed well into the band, and a tap on the axis past its diamond's end is a pill`, async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const h = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
				const g = await read(page);
				const [along, across] = vertical ? [h.y + h.height / 2, h.x + h.width / 2] : [h.x + h.width / 2, h.y + h.height / 2];
				// Into the band: up from the relative axis and down from the clock's,
				// or right of the relative one and left of the clock's.
				const inward = (kind === 'rel') === vertical ? 1 : -1;
				// The diamond is 10 across: 14px from the line is 9px clear of it, and 8px past the button.
				const deep = pt(g, along, across + inward * 14);
				const taken = await page.evaluate(({ x, y }) => document.elementFromPoint(x, y)?.closest('.handle')?.dataset.axis ?? null, deep);
				assert.equal(taken, kind, `a press 14px into the band is not the ${kind} handle's`);
				// On the line, 10px past the diamond's end along it: the axis's.
				await finger(page, pt(g, along + 20, across));
				await until(() => pillsOf(page), (p) => p.length === 1 && p[0].startsWith('time:'));
				assert.equal(await page.locator('.nowview-wheel').count(), 0, 'the tap opened the numbers');
				await page.locator(`${NOWVIEW} .nowview-now`).click();
				await until(() => pillsOf(page), (p) => p.length === 0);
				await page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'detached' });
				await app.idle();
				await app.spyOnRunner();
				const at = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
				const from = pt(g, vertical ? at.y + at.height / 2 : at.x + at.width / 2, (vertical ? at.x + at.width / 2 : at.y + at.height / 2) + inward * 14);
				await finger(page, from, pt(g, (vertical ? from.y : from.x) + 37, vertical ? from.x : from.y));
				await until(
					() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
					(a) => a && a[a.indexOf('--history') + 1] !== '30m'
				);
				assert.deepEqual(await pillsOf(page), [], 'the drag made a pill');
			});
		});

	// vividn, 2026-11-09: «the white diamonds in the nowview still shift places
	// if you select a time. They should stay in the same place» (da-5whsm).
	for (const kind of ['rel', 'abs'])
		test(`a time tapped on the ${kind} axis leaves both diamonds where they stood`, async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const places = () =>
					page.locator(`${NOWVIEW} .nowview-handles .handle`).evaluateAll((hs) =>
						hs.map((h) => {
							const r = h.getBoundingClientRect();
							return { axis: h.dataset.axis, x: r.x + r.width / 2, y: r.y + r.height / 2 };
						})
					);
				const was = await places();
				assert.equal(was.length, 2, JSON.stringify(was));
				const g = await read(page);
				const h = was.find((p) => p.axis === kind);
				// On the axis's line, most of the way from the window's far end to the diamond: the old rule put it halfway to there.
				const [along, across] = vertical ? [h.y, h.x] : [h.x, h.y];
				const lo = vertical ? g.frame.top : g.frame.left;
				await finger(page, pt(g, lo + (along - lo) * 0.6, across));
				await until(() => pillsOf(page), (p) => p.length === 1 && p[0].startsWith('time:'));
				await page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'attached' });
				await app.idle();
				const now = await until(places, (p) => p.length === 2);
				for (const p of was) {
					const q = now.find((n) => n.axis === p.axis);
					assert.ok(Math.abs(q.x - p.x) <= 1 && Math.abs(q.y - p.y) <= 1, `the ${p.axis} diamond went from ${p.x},${p.y} to ${q.x},${q.y}`);
				}
			});
		});

	// vividn, 2026-11-09: «if you click immediately below it and start dragging
	// left and right, you probably want to be grabbing the white diamond and
	// rescaling rather than scrubbing the time pill» (da-5whsm).
	for (const kind of ['rel', 'abs'])
		test(`a press just out of the ${kind} diamond that drags along the axis scales the window; a tap there is a pill, and a drag from farther scrubs`, async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const g = await read(page);
				// Out of the band, 10px past the line on the axis's side, as `a press just out of the diamond puts a time pill`.
				const out = (kind === 'rel') !== vertical ? 10 : -10;
				const diamond = async () => {
					const b = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
					return vertical ? [b.y + b.height / 2, b.x + b.width / 2] : [b.x + b.width / 2, b.y + b.height / 2];
				};
				let [along, across] = await diamond();
				await app.spyOnRunner();
				// Toward the window's far end: the window grows, and nothing is selected.
				await finger(page, pt(g, along, across + out), pt(g, along - 30, across + out));
				await until(
					() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
					(a) => a && a[a.indexOf('--history') + 1] !== '30m'
				);
				assert.deepEqual(await pillsOf(page), [], 'the drag from under the diamond made a pill');
				await app.idle();

				// A tap there is still a pill, and opens no numbers.
				[along, across] = await diamond();
				await finger(page, pt(g, along, across + out));
				await until(() => pillsOf(page), (p) => p.length === 1 && p[0].startsWith('time:'));
				assert.equal(await page.locator('.nowview-wheel').count(), 0, 'the tap opened the numbers');
				await page.locator(`${NOWVIEW} .nowview-now`).click();
				await until(() => pillsOf(page), (p) => p.length === 0);
				await app.idle();

				// A drag along the axis that starts well away from the diamond scrubs a pill.
				const lo = vertical ? g.frame.top : g.frame.left;
				const far = lo + (along - lo) / 3;
				await finger(page, pt(g, far, across + out), pt(g, far + 20, across + out));
				await until(() => pillsOf(page), (p) => p.length === 1 && p[0].startsWith('time:'));
			});
		});
});
