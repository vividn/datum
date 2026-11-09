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
import { pickOption } from './support/layout.mjs';

before(startApp);
after(stopApp);

const COLLECT = 'section.pane[data-title="collect"] .bar .collect';
/** The time and duration section beside it (da-us1n1), and its pill of `kind`. */
const WHEN = 'section.pane[data-title="collect"] .bar [data-when]';
const EARLIER = `${WHEN} [data-earlier="5"]`;
const LONGER = `${WHEN} [data-longer="5"]`;
const pillOf = (kind) => `${WHEN} .pill[data-pill="${kind}"] .said`;
/** The time section opened by its clock, where it is closed, as a reader would (da-iunoo). */
const openWhen = async (page, when = WHEN) => {
	const clock = page.locator(`${when} [data-when-clock][aria-expanded="false"]`);
	if ((await clock.count()) > 0) await clock.click();
};
/** A button of the section pressed, the section opened first. */
const press = async (page, at) => (await openWhen(page), page.locator(at).click());
const NOWVIEW = 'section.pane[data-title="nowview"]';
const PREVIEW = `${NOWVIEW} svg.svg .preview`;

const SEED = ['setup mood --verb switch --states good,bad,meh', 'setup run --verb start --assign dur=20m', 'switch mood bad -t 14:00', 'start run -t 14:10', 'stop run -t 14:20'];

const shared = sharedApp({ clock: todayAt(15) });

/** `field` opened in collect: a switch by one tap, a block in 2-click mode, since one click logs it (da-gra0z). */
const opened = (field, fn) =>
	shared.solo(async (app) => {
		// The session a test before left forgotten (da-v4b43): `solo` wipes its stored copy and not the page's, so the
		// time section opened before stood open here, and closed under a press once a write read the wiped copy back.
		await app.page.evaluate(() => window.datum.layout.forgetSession());
		if (field !== 'mood') await pickOption(app.page, 'bar', 'a tap', 'opens and chooses, 2-click');
		for (const line of SEED) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		await app.page.locator(`${COLLECT} [data-field="${field}"]`).click();
		await app.page.locator(`${COLLECT} [data-open-field]`).waitFor();
		await fn(app);
	});

/** A box on screen, as the four edges: the first drawn, the picture kept on now for later being hidden. */
const box = (page, selector) =>
	page.locator(selector).evaluateAll((els) => {
		const r = els.map((el) => el.getBoundingClientRect()).find((r) => r.width || r.height);
		return r && { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
	});

const near = (a, b, slop, what) => assert.ok(Math.abs(a - b) <= slop, `${what}: ${a} against ${b}`);

/**
 * The preview as drawn on the one picture on screen: its length in minutes
 * by that picture's own axis, its two ends, and the selection's head there —
 * once it is `minutes` long, ending at the head where `atHead`, and ending
 * left of `before` where given. A widened window redraws the picture, so nothing is
 * read off two pictures (the one before and after it): the time pill and the
 * picture's middle across are read in the same frame, since a selection made
 * in collect asks for a window about it, and the one asked for a step of a
 * drag before can still be on screen, the head two minutes off where the
 * next draws it (da-uthkn).
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
			const frame = svg.getBoundingClientRect();
			const at = (s, root = svg) => root.querySelector(s)?.getBoundingClientRect();
			const mid = (r) => r && { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
			const lane = at('.block[data-field="mood"]');
			return {
				long: (b.right - b.left) / perMin,
				left: b.left,
				right: b.right,
				top: b.top,
				bottom: b.bottom,
				head: head && (head.left + head.right) / 2,
				band: at('.cursor-span')?.right,
				lane: lane && { top: lane.top, bottom: lane.bottom },
				middle: (frame.top + frame.bottom) / 2,
				pill: mid(at('.nowview-pills .pill.time', svg.closest('section.pane'))),
				dur: mid(at('.nowview-pills .pill.dur', svg.closest('section.pane')))
			};
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
			for (const at of [EARLIER, EARLIER, LONGER, LONGER]) await press(page, at);
			// Each press draws a picture of its own: read the one of ten minutes, all in one frame (da-yozs7).
			const seen = await settled(page, 10, { atHead: true });
			assert.ok(seen, 'a preview drawn');
			const shown = await page.locator(PREVIEW).evaluate((el) => ({ tag: el.tagName, stroke: el.getAttribute('stroke'), dash: el.getAttribute('stroke-dasharray') }));
			assert.deepEqual(shown, { tag: 'rect', stroke: '#ffffff', dash: '3 3' });

			// Its time is the selection's: the ten minutes ending at −qq.
			near(seen.right, seen.band, 1.5, 'the block ends at −qq');
			near(seen.long, 10, 0.3, 'ten minutes long');
			// In mood's lane, as the CLI drew it.
			near(seen.top, seen.lane.top, 1, 'top');
			near(seen.bottom, seen.lane.bottom, 1, 'bottom');

			// The pills: −qq at its line on the relative axis, the length at the block's far edge, beside it.
			assert.ok(seen.pill.y > seen.middle && seen.dur.y > seen.middle, 'both on the bottom axis');
			near(seen.pill.x, seen.right, 1.5, '−qq at its line');
			near(seen.dur.x, seen.left, 1.5, 'the length at the far edge');
		});
	});

	test('a length longer than the window zooms the window out to hold it', async () => {
		await opened('mood', async ({ page }) => {
			// The default window is 30m: 45m does not fit it.
			for (let i = 0; i < 9; i++) await press(page, LONGER);
			await page.waitForFunction((s) => document.querySelector(s)?.innerText === '45m', pillOf('dur'));
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
			for (const at of [EARLIER, EARLIER, LONGER]) await press(page, at);
			const before = await settled(page, 5, { atHead: true });
			const pill = await box(page, pillOf('time'));
			const [x, y] = [(pill.left + pill.right) / 2, (pill.top + pill.bottom) / 2];
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let dx = 4; dx <= 24; dx += 4) await page.mouse.move(x - dx, y);
			await page.mouse.up();

			const said = await page.locator(pillOf('time')).innerText();
			assert.match(said, /^\d\d:\d\d$/, 'plain: an absolute time');
			assert.ok((await page.locator('.prompt-bar .pill.time').innerText()).includes(said), 'the bar holds it');
			// Earlier, on the clock's axis now, over the line it names.
			const after = await settled(page, 5, { atHead: true, before: before.right - 2 });
			assert.ok(after.right < before.right - 2, `earlier: ${after.right} left of ${before.right}`);
			near(after.long, 5, 0.3, 'the same length');
			assert.ok(after.pill.y < after.middle, 'on the top axis');
			near(after.pill.x, after.right, 1.5, 'at its line');

			// The length dragged left grows.
			const length = pillOf('dur');
			const grip = await box(page, length);
			const [lx, ly] = [(grip.left + grip.right) / 2, (grip.top + grip.bottom) / 2];
			await page.mouse.move(lx, ly);
			await page.mouse.down();
			for (let dx = 4; dx <= 16; dx += 4) await page.mouse.move(lx - dx, ly);
			await page.mouse.up();
			const grown = Number((await page.locator(length).innerText()).replace(/\D/g, ''));
			assert.ok(grown > 5, `longer than 5m: ${grown}m`);
			near((await settled(page, grown)).long, grown, 0.3, 'the block as long');
		});
	});

	test('the block is what the CLI says, a length the spec assigns included', async () => {
		await opened('run', async ({ page }) => {
			await press(page, EARLIER);
			// `start run -q`, and run's spec assigns dur=20m: twenty minutes, though collect set no length.
			const seen = await settled(page, 20, { atHead: true });
			assert.ok(seen, 'the block and the selection on one picture');
			near(seen.long, 20, 0.3, 'twenty minutes long');
			near(seen.right, seen.head, 1.5, 'ending at −q');
		});
	});

	test('back takes the preview away', async () => {
		await opened('mood', async ({ page }) => {
			await press(page, EARLIER);
			await page.waitForFunction((s) => document.querySelector(s), PREVIEW);
			await page.locator(`${COLLECT} [data-back]`).click();
			await page.waitForFunction((s) => !document.querySelector(s), PREVIEW);
		});
	});
});
