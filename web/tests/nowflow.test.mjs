// The nowview flows (da-hklaz). vividn: «we'll need a 2 and 1 minute
// nowview, that updates very nicely in real time so that you can see the
// level of fine time detail that a game requires», and «it should already
// have all future and past data loaded, so it's just a matter of shifting it
// over (could this even be done in realtime without second jitters)?»
//
// So on now the picture is drawn AHEAD of now as well (`--ahead`), its lanes
// and whole-minute marks in `.flow` groups clipped at the now line, and the
// widget slides them along with the clock — no CLI run per tick.
//
// The page's timers are held (`holdTimers`), so a second of clock is a
// second of the slide's timer and nothing else runs: what moved is read off
// the flow group's own transform, and the runs are counted at the seam.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESK } from './support/desk.mjs';

before(startApp);
after(stopApp);

/** Today at 15:00 sharp: a whole minute, so the minute marks fall on known instants. */
const AT = todayAt(15);
/** Long enough for a box to settle ($lib/layout/resizing.ts). */
const SETTLE = 200;
const NOWVIEW = 'section.pane[data-title="nowview"]';

/** The suite's desk with its nowview at `settings`. */
function deskWith(settings) {
	const tree = structuredClone(DESK);
	const visit = (node) => {
		if (node?.kind === 'nowview') node.settings = settings;
		for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) visit(child.content ?? child);
	};
	visit(tree.root);
	return tree;
}

/**
 * How far the lanes' flow group is slid along x, in user units; where now is
 * on screen, by the now-line's own `data-now`; how many screen px a second
 * of the picture is, by its day's axis; and how many user units a second
 * and a device pixel are.
 */
const reading = (page) =>
	page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
		const day = svg.querySelector('[data-lo]');
		const ms = Date.parse(day.dataset.to) - Date.parse(day.dataset.from);
		const units = Number(day.dataset.hi) - Number(day.dataset.lo);
		const ctm = svg.getScreenCTM();
		const line = svg.querySelector('.current-time-line');
		return {
			x: new DOMMatrixReadOnly(getComputedStyle(svg.querySelector('.flow')).transform).e,
			now: new DOMPoint(Number(line.dataset.now), 0).matrixTransform(ctm).x,
			pxPerSecond: ((1000 * units) / ms) * ctm.a,
			unitsPerSecond: (1000 * units) / ms,
			unitsPerPixel: 1 / (ctm.a * devicePixelRatio)
		};
	});

/** Where the minute `iso` is marked on screen, and where its label stands. */
const minute = (page, iso, label) =>
	page.locator(`${NOWVIEW} svg.svg`).evaluate(
		(svg, [iso, label]) => {
			const centre = (el) => (el ? el.getBoundingClientRect().left + el.getBoundingClientRect().width / 2 : null);
			const mark = [...svg.querySelectorAll('.minute-mark')].find((m) => Date.parse(m.dataset.at) === Date.parse(iso));
			const text = [...svg.querySelectorAll('.minute-label')].find((l) => l.textContent === label);
			return { mark: centre(mark), label: centre(text), labels: [...svg.querySelectorAll('.minute-label')].map((l) => l.textContent) };
		},
		[iso, label]
	);

describe('the nowview flows', () => {
	test('a 1-minute window slides with the clock, a device pixel at a time, and asks datum for nothing', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:59'])).code, 0);
				await app.reload();
				await page.locator(`${NOWVIEW} svg.svg .flow rect.block[data-field="work"]`).waitFor();
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();
				const start = await reading(page);
				const { unitsPerSecond, unitsPerPixel } = start;
				const xs = [start.x];
				for (let i = 0; i < 20; i++) {
					await page.clock.runFor(50);
					xs.push((await reading(page)).x);
				}
				// Several moves a second, each further left by a device pixel:
				// a slide, neither a jump a second nor a restyle every frame
				// for a fraction of a pixel (da-k2088).
				const moves = xs.slice(1).map((x, i) => xs[i] - x).filter((d) => d !== 0);
				assert.ok(moves.length >= 5, `${moves.length} moves in a second: positions ${xs.join(', ')}`);
				for (const d of moves)
					assert.ok(d > unitsPerPixel * 0.99 && d < unitsPerPixel * 1.5, `a move of ${d} units, a device pixel is ${unitsPerPixel}`);
				// And a second of them is a second of the clock, to a pixel or two.
				const ran = xs[0] - xs.at(-1);
				assert.ok(Math.abs(ran - unitsPerSecond) < 2 * unitsPerPixel, `slid ${ran} units in a second of ${unitsPerSecond}`);
				// The now line stays where it is; the lanes move under it.
				const end = await reading(page);
				assert.ok(Math.abs(end.now - start.now) < 0.01, `the now line moved ${end.now - start.now}px`);
				assert.deepEqual(await seen(), [], 'datum was asked for something while the window slid');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: deskWith({ history: '1m' }) }
		);
	});

	test('whole-minute marks flow past with their clock labels, at the instants they name', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${NOWVIEW} svg.svg .minute-mark`).first().waitFor({ state: 'attached' });
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();

				await page.clock.runFor(20_000);
				const { now, pxPerSecond } = await reading(page);
				const at = await minute(page, new Date(AT).toISOString(), '15:00');
				assert.ok(at.labels.includes('15:00') && at.labels.includes('14:59'), `labels ${at.labels.join(', ')}`);
				// 15:00 is as far behind now as the clock has run since, and its
				// label is on its mark.
				const behind = ((await page.evaluate(() => Date.now())) - AT) / 1000;
				assert.ok(behind >= 20, `the clock ran ${behind}s`);
				assert.ok(
					Math.abs(at.mark - (now - behind * pxPerSecond)) < 1,
					`15:00 is at ${at.mark}, ${behind}s before now at ${now} is ${now - behind * pxPerSecond}`
				);
				assert.ok(Math.abs(at.label - at.mark) < 0.5, `the label is at ${at.label}, its mark at ${at.mark}`);
				assert.deepEqual(await seen(), [], 'datum was asked for something while the marks flowed');

				// The 2-minute window marks its minutes too; a 30-minute one does not.
				const two = await app.run(['nowview', '-o', '-', '--history', '2m', '--surround', '2m', '--ahead', '2m']);
				assert.match(two.stdout, />14:59<\/text>/);
				assert.match(two.stdout, />14:58<\/text>/);
				const wide = await app.run(['nowview', '-o', '-', '--history', '30m', '--ahead', '30m']);
				assert.ok(!wide.stdout.includes('class="minute-mark"'), 'a 30m window marks its minutes');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: deskWith({ history: '1m' }) }
		);
	});

	test('a clock time slid under the far label is hidden, and drawn again clear of it (da-to5n4)', async () => {
		/** Whether each clock time on the window is drawn, and the far label's words. */
		const shown = (page) =>
			page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
				const frame = svg.parentElement.getBoundingClientRect();
				const on = (l) => l.getBoundingClientRect().left < frame.right && l.getBoundingClientRect().right > frame.left;
				const ticks = [...svg.querySelectorAll('.tick-label')].filter(on);
				const far = ticks.reduce((a, b) => (b.getBoundingClientRect().left < a.getBoundingClientRect().left ? b : a));
				const labels = [...svg.querySelectorAll('.minute-label')].filter(on);
				return {
					far: far.textContent,
					drawn: Object.fromEntries(labels.map((l) => [l.textContent, getComputedStyle(l).visibility !== 'hidden']))
				};
			});
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${NOWVIEW} .nowview-handles .handle`).first().waitFor();
				await page.clock.runFor(SETTLE);
				await app.idle();
				// 14:59:10: 14:58 is 70 s back on a 2-minute window, well clear of «-2m».
				assert.deepEqual(await shown(page), { far: '-2m', drawn: { '14:58': true, '14:59': true } });
				const seen = await app.spyOnRunner();
				// 50 s on it has slid to the window's far end, under «-2m», with no new picture.
				await app.clock.forward(50_000);
				assert.deepEqual((await seen()).filter((a) => a[0] === 'nowview'), [], 'datum was asked for a new picture');
				assert.deepEqual(await shown(page), { far: '-2m', drawn: { '14:58': false, '14:59': true, '15:00': true } });
				assert.deepEqual(app.errors, []);
			},
			{ clock: todayAt(14, 59) + 10_000, holdTimers: true, desk: deskWith({ history: '2m' }) }
		);
	});

	test('a clock time still to come is not drawn beside «now», and is at its instant (da-xwe0h)', async () => {
		/** The 15:00 label: where its near edge is against the now line, and whether it is drawn. */
		const next = (page) =>
			page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
				const label = [...svg.querySelectorAll('.minute-label')].find((l) => l.textContent === '15:00');
				const line = svg.querySelector('.current-time-line');
				const now = new DOMPoint(Number(line.dataset.now), 0).matrixTransform(svg.getScreenCTM()).x;
				return { peeks: label.getBoundingClientRect().left < now, drawn: getComputedStyle(label).visibility !== 'hidden' };
			});
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${NOWVIEW} svg.svg .minute-mark`).first().waitFor({ state: 'attached' });
				await page.clock.runFor(SETTLE);
				await app.idle();
				// A second short of 15:00, half its label lies before the now line.
				assert.deepEqual(await next(page), { peeks: true, drawn: false });
				// Half a second on, the slide's last step has passed 15:00.
				await app.clock.forward(1_500);
				assert.equal((await next(page)).drawn, true);
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT - 1_000 - SETTLE, holdTimers: true, desk: deskWith({ history: '2m' }) }
		);
	});

	test('half of what was drawn ahead spent, datum is asked again, once', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${NOWVIEW} svg.svg .flow`).first().waitFor({ state: 'attached' });
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();
				await app.clock.forward(29_000);
				await app.idle();
				assert.deepEqual(await seen(), [], 'asked before half of a minute ahead was spent');
				await app.clock.forward(2_000);
				await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'nowview'));
				await app.idle();
				const asked = (await seen()).filter((a) => a[0] === 'nowview');
				assert.equal(asked.length, 1, `${asked.length} nowview runs`);
				assert.equal(asked[0][asked[0].indexOf('--ahead') + 1], '1m');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: deskWith({ history: '1m' }) }
		);
	});
});
