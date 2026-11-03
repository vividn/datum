// Collect's pending entry on the nowview (da-axrdv). vividn, 2026-11-03:
// «the nowview could show a preview of what would actually be logged
// (somehow with dotted outline or something to show it is not yet logged.
// The nowview could even zoom out as necessary if duration goes beyond its
// scope.» The preview is the CLI's dry run of the line collect would run,
// drawn white and dotted in the field's lane; the length pill stands at its
// far edge; collect's time and length pills drag sideways.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';

before(startApp);
after(stopApp);

const COLLECT = 'section.pane[data-title="collect"] .bar .collect';
const NOWVIEW = 'section.pane[data-title="nowview"]';
const PREVIEW = `${NOWVIEW} svg.svg .preview`;

const SEED = ['setup mood --verb switch --states good,bad,meh', 'setup run --verb start --assign dur=20m', 'switch mood bad -t 14:00', 'start run -t 14:10', 'stop run -t 14:20'];

const shared = sharedApp({ clock: todayAt(15) });

const opened = (field, fn) =>
	shared.solo(async (app) => {
		for (const line of SEED) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		await app.page.locator(`${COLLECT} [data-field="${field}"]`).click();
		await app.page.locator(`${COLLECT} [data-time-row]`).waitFor();
		await fn(app);
	});

/** A box on screen, as the four edges: the first drawn, the picture kept on now for later being hidden. */
const box = (page, selector) =>
	page.locator(selector).evaluateAll((els) => {
		const r = els.map((el) => el.getBoundingClientRect()).find((r) => r.width || r.height);
		return r && { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
	});

/** Px along the nowview's time for one minute, read off its day group. */
const pxPerMinute = (page) =>
	page.locator(`${NOWVIEW} svg.svg [data-lo]`).evaluateAll((gs) => {
		const g = gs.find((g) => g.closest('svg').getBoundingClientRect().width);
		const r = g.closest('svg').getBoundingClientRect();
		const view = g.closest('svg').viewBox.baseVal;
		const units = (Number(g.dataset.hi) - Number(g.dataset.lo)) * (r.width / view.width);
		return units / ((Date.parse(g.dataset.to) - Date.parse(g.dataset.from)) / 60_000);
	});

const near = (a, b, slop, what) => assert.ok(Math.abs(a - b) <= slop, `${what}: ${a} against ${b}`);

/**
 * The preview as drawn on the one picture on screen: its length in minutes
 * by that picture's own axis, its two ends, and the selection's head there —
 * once it is `minutes` long, ending at the head where `atHead`, and ending
 * left of `before` where given. A widened window redraws the picture, so nothing is
 * read off two pictures (the one before and after it).
 */
async function settled(page, minutes, { atHead = false, before = Infinity } = {}) {
	await page.evaluate(() => {
		window.__previewMeasure = ([p, h]) => {
			const shown = (s) => [...document.querySelectorAll(s)].find((e) => e.getBoundingClientRect().width);
			const block = shown(p);
			const svg = block?.ownerSVGElement;
			const g = svg?.querySelector('[data-lo]');
			if (!g) return null;
			const head = [...svg.querySelectorAll(h)][0]?.getBoundingClientRect();
			const perMin = ((Number(g.dataset.hi) - Number(g.dataset.lo)) * (svg.getBoundingClientRect().width / svg.viewBox.baseVal.width)) / ((Date.parse(g.dataset.to) - Date.parse(g.dataset.from)) / 60_000);
			const b = block.getBoundingClientRect();
			return { long: (b.right - b.left) / perMin, left: b.left, right: b.right, head: head && (head.left + head.right) / 2 };
		};
	});
	const marks = [PREVIEW, '.cursor-head'];
	await page
		.waitForFunction(
			([m, want, head, was]) => {
				const r = window.__previewMeasure(m);
				return r && Math.abs(r.long - want) < 0.3 && (!head || Math.abs(r.right - r.head) < 1.5) && r.right < was;
			},
			[marks, minutes, atHead, before]
		)
		.catch(() => {});
	return page.evaluate((m) => window.__previewMeasure(m), marks);
}

describe('collect previews on the nowview', () => {
	test('−qq and ←10m: a white dotted block in the lane for the ten minutes ending at −qq, the pills on the relative axis', async () => {
		await opened('mood', async ({ page }) => {
			await page.locator(`${COLLECT} [data-q]`).click();
			await page.locator(`${COLLECT} .part-pill[data-pill="time"] .more`).click();
			await page.locator(`${COLLECT} [data-five]`).click();
			await page.locator(`${COLLECT} .part-pill[data-pill="length"] .more`).click();
			await page.waitForFunction((s) => document.querySelector(s), PREVIEW);
			const shown = await page.locator(PREVIEW).evaluate((el) => ({ tag: el.tagName, stroke: el.getAttribute('stroke'), dash: el.getAttribute('stroke-dasharray') }));
			assert.deepEqual(shown, { tag: 'rect', stroke: '#ffffff', dash: '3 3' });

			// Its time is the selection's: the ten minutes ending at −qq.
			const [preview, band, perMin] = [await box(page, PREVIEW), await box(page, `${NOWVIEW} svg.svg .cursor-span`), await pxPerMinute(page)];
			near(preview.right, band.right, 1.5, 'the block ends at −qq');
			near(preview.right - preview.left, 10 * perMin, 1.5, 'ten minutes long');
			// In mood's lane, as the CLI drew it.
			const lane = await box(page, `${NOWVIEW} svg.svg .block[data-field="mood"]`);
			near(preview.top, lane.top, 1, 'top');
			near(preview.bottom, lane.bottom, 1, 'bottom');

			// The pills: −qq at its line on the relative axis, the length at the block's far edge, beside it.
			const svg = await box(page, `${NOWVIEW} svg.svg`);
			const [time, dur] = [await box(page, `${NOWVIEW} .nowview-pills .pill.time`), await box(page, `${NOWVIEW} .nowview-pills .pill.dur`)];
			const middle = (b) => (b.top + b.bottom) / 2;
			assert.ok(middle(time) > (svg.top + svg.bottom) / 2 && middle(dur) > (svg.top + svg.bottom) / 2, 'both on the bottom axis');
			near((time.left + time.right) / 2, preview.right, 1.5, '−qq at its line');
			near((dur.left + dur.right) / 2, preview.left, 1.5, 'the length at the far edge');
		});
	});

	test('a length longer than the window zooms the window out to hold it', async () => {
		await opened('mood', async ({ page }) => {
			// The default window is 30m: 45m does not fit it.
			for (let i = 0; i < 9; i++) await page.locator(`${COLLECT} ${i ? '.part-pill[data-pill="length"] .more' : '[data-five]'}`).click();
			await page.waitForFunction((s) => document.querySelector(`${s} .part-pill[data-pill="length"] .more`)?.innerText === '45m', COLLECT);
			const seen = await settled(page, 45);
			// The picture is drawn past the window either side: the window is the band on screen.
			const band = await box(page, `${NOWVIEW} .nowview-band`);
			assert.ok(seen, 'a preview drawn');
			near(seen.long, 45, 0.3, 'forty-five minutes long');
			assert.ok(seen.left > band.left + 1, `the block starts inside the window: ${seen.left} past ${band.left}`);
		});
	});

	test('−qq dragged in collect turns into an absolute −t pill on the clock axis, and the block moves with it', async () => {
		await opened('mood', async ({ page }) => {
			for (let i = 0; i < 2; i++) await page.locator(`${COLLECT} ${i ? '.part-pill[data-pill="time"] .more' : '[data-q]'}`).click();
			await page.locator(`${COLLECT} [data-five]`).click();
			const before = await settled(page, 5, { atHead: true });
			const pill = await box(page, `${COLLECT} .part-pill[data-pill="time"] .more`);
			const [x, y] = [(pill.left + pill.right) / 2, (pill.top + pill.bottom) / 2];
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let dx = 4; dx <= 24; dx += 4) await page.mouse.move(x - dx, y);
			await page.mouse.up();

			const said = await page.locator(`${COLLECT} .part-pill[data-pill="time"] .more`).innerText();
			assert.match(said, /^-t \d\d:\d\d$/, 'plain: an absolute time');
			assert.equal(await page.locator(`${COLLECT} .part-pill[data-pill="time"] .more`).getAttribute('aria-disabled'), 'true', 'it adds no q');
			assert.ok((await page.locator('.prompt-bar .pill.time').innerText()).includes(said.slice(3)), 'the bar holds it');
			// Earlier, on the clock's axis now, over the line it names.
			const after = await settled(page, 5, { atHead: true, before: before.right - 2 });
			const [svg, time] = [await box(page, `${NOWVIEW} svg.svg`), await box(page, `${NOWVIEW} .nowview-pills .pill.time`)];
			assert.ok(after.right < before.right - 2, `earlier: ${after.right} left of ${before.right}`);
			near(after.long, 5, 0.3, 'the same length');
			assert.ok((time.top + time.bottom) / 2 < (svg.top + svg.bottom) / 2, 'on the top axis');
			near((time.left + time.right) / 2, after.right, 1.5, 'at its line');

			// The length dragged left grows, plain: it adds no five on a tap.
			const length = `${COLLECT} .part-pill[data-pill="length"] .more`;
			const grip = await box(page, length);
			const [lx, ly] = [(grip.left + grip.right) / 2, (grip.top + grip.bottom) / 2];
			await page.mouse.move(lx, ly);
			await page.mouse.down();
			for (let dx = 4; dx <= 16; dx += 4) await page.mouse.move(lx - dx, ly);
			await page.mouse.up();
			const grown = Number((await page.locator(length).innerText()).replace(/\D/g, ''));
			assert.ok(grown > 5, `longer than 5m: ${grown}m`);
			assert.equal(await page.locator(length).getAttribute('aria-disabled'), 'true', 'it adds no five');
			near((await settled(page, grown)).long, grown, 0.3, 'the block as long');
		});
	});

	test('the block is what the CLI says, a length the spec assigns included', async () => {
		await opened('run', async ({ page }) => {
			await page.locator(`${COLLECT} [data-q]`).click();
			// `start run -q`, and run's spec assigns dur=20m: twenty minutes, though collect set no length.
			const seen = await settled(page, 20, { atHead: true });
			assert.ok(seen, 'the block and the selection on one picture');
			near(seen.long, 20, 0.3, 'twenty minutes long');
			near(seen.right, seen.head, 1.5, 'ending at −q');
		});
	});

	test('back takes the preview away', async () => {
		await opened('mood', async ({ page }) => {
			await page.locator(`${COLLECT} [data-q]`).click();
			await page.waitForFunction((s) => document.querySelector(s), PREVIEW);
			await page.locator(`${COLLECT} [data-back]`).click();
			await page.waitForFunction((s) => !document.querySelector(s), PREVIEW);
		});
	});
});
