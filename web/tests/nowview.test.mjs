// The nowview widget's + / − (da-pcis): they step how far back the picture
// reaches, down to 0 — the snapshot — and the step is the widget's setting,
// kept in the draft and in a saved layout. They sit at the picture's
// top-left, + first, with no label: each one's title names where it goes
// (da-rex1).
//
// Delta-blind on purpose: each step is held to the `--history` the seam was
// handed and to the buttons' titles, never to "the picture changed".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, draft } from './support/layout.mjs';

before(startApp);
after(stopApp);

const PANE = 'section.pane[data-title="nowview"]';
const shorter = (page) => page.locator(`${PANE} button[aria-label="shorter window"]`);
const longer = (page) => page.locator(`${PANE} button[aria-label="longer window"]`);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to be `--history h`. */
const asked = (page, h) =>
	page.waitForFunction((h) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		return argv?.[argv.indexOf('--history') + 1] === h;
	}, h);

/** The two buttons' titles, + then −. */
const titles = (page) =>
	Promise.all([longer(page).getAttribute('title'), shorter(page).getAttribute('title')]);

/** Every nowview widget's settings in a layout tree. */
function nowviewSettings(node, out = []) {
	if (node?.kind === 'nowview') out.push(node.settings);
	for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) {
		nowviewSettings(child.content ?? child, out);
	}
	return out;
}

describe('the nowview widget', () => {
	test('sit at the top-left, + then −, with no label', async () => {
		await withApp(async ({ page }) => {
			const steps = page.locator(`${PANE} .nowview-steps`);
			const [box, content] = await Promise.all([
				steps.boundingBox(),
				page.locator(`${PANE} .nowview`).boundingBox()
			]);
			assert.ok(box.x - content.x <= 8, `steps at ${box.x}, content at ${content.x}`);
			// The title lies over the content's top strip (da-13bc): the steps
			// sit below it, not under it.
			const title = await page.locator(`${PANE} h2`).boundingBox();
			assert.ok(box.y >= title.y + title.height, `steps at ${box.y}, the title ends at ${title.y + title.height}`);
			assert.equal(await steps.locator('span').count(), 0);
			assert.deepEqual(
				await steps.locator('button').evaluateAll((bs) => bs.map((b) => [b.textContent, b.ariaLabel])),
				[
					['+', 'longer window'],
					['−', 'shorter window']
				]
			);
		});
	});

	test('steps its window with + and −, through the seam, down to the snapshot and up to 12h', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await titles(page), ['longer window (45m)', 'shorter window (20m)']);
			await app.spyOnRunner();

			await longer(page).click();
			await asked(page, '45m');
			assert.deepEqual(await titles(page), ['longer window (60m)', 'shorter window (30m)']);

			// 45m → 30m, 20m, 15m, 10m, 5m, 0.
			for (let i = 0; i < 5; i++) await shorter(page).click();
			await asked(page, '5m');
			assert.deepEqual(await titles(page), ['longer window (10m)', 'shorter window (now)']);
			await shorter(page).click();
			await asked(page, '0');
			await app.idle();
			assert.ok(await shorter(page).isDisabled(), '− is live at 0');
			assert.deepEqual(await titles(page), ['longer window (5m)', 'shorter window']);
			const snapshot = await app.paneSvg('nowview');
			assert.ok(!snapshot.includes('tick-label') && !snapshot.includes('class="axis"'), snapshot);

			for (let i = 0; i < 12; i++) await longer(page).click();
			await asked(page, '12h');
			assert.ok(await longer(page).isDisabled(), '+ is live at 12h');
			assert.deepEqual(await titles(page), ['longer window', 'shorter window (6h)']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the step is kept in the draft and in a saved layout', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await longer(page).click();
			await asked(page, '45m');

			await app.reload();
			assert.deepEqual(
				await titles(page),
				['longer window (60m)', 'shorter window (30m)'],
				'the step did not survive a reload'
			);

			// The root bar's save icon, on the `day` tab (da-0a5o).
			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar.modified'));
			const other = await app.newTab();
			const saved = await other.page.evaluate(() => window.datum.layouts.loadLayout('day'));
			assert.deepEqual(
				nowviewSettings(saved.root).map((s) => s.history),
				['45m']
			);
		});
	});

	test('at 360px the steps do not widen the pane', async () => {
		await withApp(
			async ({ page }) => {
				const tab = page.locator('[role="tab"]', { hasText: 'nowview' });
				if (await tab.count()) await tab.click();
				await page.locator(`${PANE} svg.svg`).waitFor();
				const box = await page.locator(`${PANE} .body`).evaluate((b) => ({
					scroll: b.scrollWidth,
					client: b.clientWidth
				}));
				assert.ok(box.scroll <= box.client, `nowview scrolls sideways: ${JSON.stringify(box)}`);
				assert.ok(await longer(page).isVisible());
			},
			{ viewport: { width: 360, height: 640 } }
		);
	});
});

// ── the zoom (da-5goc) ───────────────────────────────────────────────
//
// Two fingers over the picture, or ctrl+wheel, scale the same window the
// + / − step — and land on the same ladder, so the two controls never
// disagree about where the window can be.
//
// The anchor is the instant the picture is about: now, or `--at`'s when a
// tree carries one. What is held here is that the anchor does not move
// under the gesture — the band scales about the now-line's own x, so the
// column the fingers are anchored to is the one column that stays.
//
// After the release the anchor's x is datum's to place, and for a window
// that ENDS at now it moves: nowview reserves a now strip of
// `NOW_WIDTH_MINUTES` at the window's own scale (nowview.rs, the original
// datum's proportions), so a shorter window spends proportionally more of
// the band on it and the now-line slides left. A CENTRED window — `--at` —
// has no strip and its anchor is the middle of the band whatever the
// window, which is the case asserted to the pixel below.

/** The x of the picture's now-line — the anchor datum drew. */
const anchorX = (page) =>
	page.locator(`${PANE} svg.svg .current-time-line`).evaluate((line) => {
		const box = line.getBoundingClientRect();
		return box.x + box.width / 2;
	});

/** The picture's own box. */
const pictureBox = (page) => page.locator(`${PANE} svg.svg`).boundingBox();

/** A layout of one nowview pane, as the `day` tab's draft. */
const soloNowview = (settings) =>
	JSON.stringify({
		version: 1,
		name: 'day',
		root: {
			id: 'only',
			tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'nowview', settings } }],
			active: 0,
			bar: { edge: 'top', align: 'start' }
		}
	});

/**
 * Two fingers over `box`, `from` px apart, drawn to `to` px apart.
 *
 * Through CDP, as every other touch in this suite: Playwright's
 * touchscreen only taps, and a pinch made of mouse events would be a test
 * of the wrong handler. `midway` is called with the fingers still down —
 * the gesture's preview is only on screen until they lift.
 */
async function pinch(page, box, from, to, midway) {
	const y = box.y + box.height / 2;
	const x = box.x + box.width / 2;
	const cdp = await page.context().newCDPSession(page);
	const both = (apart) => [
		{ id: 1, x: x - apart / 2, y },
		{ id: 2, x: x + apart / 2, y }
	];
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	try {
		// One finger, then the second: the fingers land as fingers do, and
		// the widget's count of them is what starts the pinch.
		await at('touchStart', [both(from)[0]]);
		await at('touchStart', both(from));
		for (let i = 1; i <= 6; i++) await at('touchMove', both(from + ((to - from) * i) / 6));
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

const TOUCH = { device: { hasTouch: true, isMobile: true }, viewport: { width: 390, height: 640 } };

describe('the nowview widget zooms', () => {
	test('a pinch steps the window on the ladder, about the anchor', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				const before = await anchorX(page);

				// Fingers apart by half again: the picture magnified 1.5×,
				// which is 30m ÷ 1.5 — the ladder's own next rung down.
				await pinch(page, box, 80, 120, async () => {
					const during = await anchorX(page);
					assert.ok(
						Math.abs(during - before) <= 2,
						`the anchor moved under the pinch: ${before} → ${during}`
					);
				});
				await asked(page, '20m');
				assert.deepEqual(await titles(page), ['longer window (30m)', 'shorter window (15m)']);

				// And back: fingers together by the same ratio is 20m × 1.5,
				// which rounds onto 30m.
				await pinch(page, box, 120, 80);
				await asked(page, '30m');
				assert.deepEqual(await titles(page), ['longer window (45m)', 'shorter window (20m)']);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('ctrl+wheel steps the same ladder, and the buttons still read it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.keyboard.down('Control');

				await page.mouse.wheel(0, -100); // up: a shorter window
				await asked(page, '20m');
				assert.deepEqual(await titles(page), ['longer window (30m)', 'shorter window (15m)']);

				await page.mouse.wheel(0, 100); // down: a longer one
				await asked(page, '30m');
				await page.keyboard.up('Control');

				// A bare wheel is the pane's own scroll, not a zoom.
				await page.mouse.wheel(0, -100);
				await app.idle();
				assert.deepEqual(await titles(page), ['longer window (45m)', 'shorter window (20m)']);

				// The buttons pick up from where the wheel left the window.
				await longer(page).click();
				await asked(page, '45m');
				assert.deepEqual(await titles(page), ['longer window (60m)', 'shorter window (30m)']);
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP }
		);
	});

	test('with --at set the pinch keeps that instant, not now', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				const before = await anchorX(page);
				// A centred window: the anchor is the middle of the band, and
				// nowhere near the right edge where a window ending at now
				// would draw it.
				assert.ok(
					Math.abs(before - (box.x + box.width / 2)) <= 4,
					`09:00 is not the middle of the band: ${before} of ${JSON.stringify(box)}`
				);

				await pinch(page, box, 80, 120);
				await asked(page, '20m');
				await app.idle();
				assert.deepEqual(await titles(page), ['longer window (30m)', 'shorter window (15m)']);
				const after = await anchorX(page);
				assert.ok(Math.abs(after - before) <= 2, `09:00 moved: ${before} → ${after}`);
				// And the window is still that instant's, not the clock's.
				const argv = (await page.evaluate(() => window.__datumSeen))
					.filter((a) => a[0] === 'nowview')
					.at(-1);
				assert.equal(argv[argv.indexOf('--at') + 1], '09:00');
				assert.deepEqual(app.errors, []);
			},
			{
				...TOUCH,
				clock: todayAt(12, 0),
				prepare: (context) =>
					context.addInitScript(draft, soloNowview({ history: '30m', at: '09:00' }))
			}
		);
	});
});
