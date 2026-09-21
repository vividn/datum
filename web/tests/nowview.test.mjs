// The nowview widget's − / + (da-pcis): they step how far back the picture
// reaches, down to 0 — the snapshot — and the step is the widget's setting,
// kept in the draft and in a saved layout. They have no label: each one's
// title names where it goes (da-rex1). They sit, tiny, just right of the
// axis's far label — how far back the window reaches — and the snapshot,
// which draws no axis, has none: nothing lies over its chips (da-ndzl).
//
// Delta-blind on purpose: each step is held to the `--history` the seam was
// handed and to the buttons' titles, never to "the picture changed".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, activeTab, choose, chooseRoot, draft, pane, settled, swipe } from './support/layout.mjs';

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

/**
 * Where the steps of the nowview in `pane` sit against its picture: the far
 * label's words, and what is wrong — each step must be whole, right of the
 * far label, on its row, below the axis, off every band, and not over the
 * other step. Waits for the steps to be drawn.
 */
async function beside(page, pane) {
	await page.locator(`${pane} .nowview-steps button`).first().waitFor();
	return page.locator(pane).evaluate((p) => {
		const r = (el) => el.getBoundingClientRect();
		const over = (a, b) => a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
		const all = [...p.querySelectorAll('svg.svg .tick-label')];
		const far = all.reduce((a, b) => (r(b).x < r(a).x ? b : a));
		// A label the steps take the place of is not drawn; the far one is.
		const labels = all.filter((l) => getComputedStyle(l).visibility !== 'hidden');
		const label = r(far);
		const axis = r(p.querySelector('svg.svg line.axis'));
		const bands = [...p.querySelectorAll('svg.svg .day-data rect')].map(r);
		const steps = [...p.querySelectorAll('.nowview-steps button')].map(r);
		const wrong = [];
		steps.forEach((s, i) => {
			if (s.width < 20 || s.height < 20) wrong.push(`step ${i} is ${s.width}×${s.height}`);
			if (s.left < label.right - 0.5) wrong.push(`step ${i} starts at ${s.left}, the label ends at ${label.right}`);
			if (s.top > label.bottom || s.bottom < label.top) wrong.push(`step ${i} is off the label's row`);
			if (s.top < axis.top - 0.5) wrong.push(`step ${i} reaches above the axis`);
			for (const b of bands) if (over(s, b)) wrong.push(`step ${i} is over a band`);
			for (const l of labels) if (over(s, r(l))) wrong.push(`step ${i} is over ${l.textContent}`);
		});
		if (!labels.includes(far)) wrong.push(`the far label ${far.textContent} is hidden`);
		if (steps.length !== 2) wrong.push(`${steps.length} steps`);
		else if (over(steps[0], steps[1])) wrong.push('the steps overlap');
		return { far: far.textContent, wrong };
	});
}

describe('the nowview widget', () => {
	test('sit just right of the far axis label, − then +, with no label', async () => {
		await withApp(async ({ page }) => {
			const where = await beside(page, PANE);
			assert.equal(where.far, '-30m');
			assert.deepEqual(where.wrong, []);
			assert.deepEqual(
				await page.locator(`${PANE} .nowview-steps button`).evaluateAll((bs) => bs.map((b) => [b.textContent, b.ariaLabel])),
				[
					['−', 'shorter window'],
					['+', 'longer window']
				]
			);
		});
	});

	test('with the cursor set the far label is the leftmost, and the steps follow it', async () => {
		await withApp(
			async ({ page }) => {
				await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), todayAt(9, 0));
				await page.waitForFunction(
					(pane) => document.querySelector(`${pane} svg.svg .tick-label`)?.textContent === '-15m',
					PANE
				);
				const where = await beside(page, PANE);
				assert.equal(where.far, '-15m');
				assert.deepEqual(where.wrong, []);
			},
			{ clock: todayAt(12, 0) }
		);
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
			const snapshot = await app.paneSvg('nowview');
			assert.ok(!snapshot.includes('tick-label') && !snapshot.includes('class="axis"'), snapshot);
			// No axis, no steps: the way out of the snapshot is the zoom.
			await page.locator(`${PANE} .nowview-steps`).waitFor({ state: 'detached' });
			const box = await pictureBox(page);
			await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, 100);
			await page.keyboard.up('Control');
			await asked(page, '5m');

			for (let i = 0; i < 11; i++) await longer(page).click();
			await asked(page, '12h');
			assert.ok(await longer(page).isDisabled(), '+ is live at 12h');
			assert.deepEqual(await titles(page), ['longer window', 'shorter window (6h)']);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-vhwq: the step is a setting the widget moves ITSELF, so it is THIS
	// SESSION'S — it changes what this tab shows, the layout is not dirtied,
	// and a reload starts back at what the layout says. «Save options with
	// layout» is what hands it over; only then is it a draft edit, and only
	// then does `Save layout` write it down.
	test('the step is this session’s, and the draft and a save take it only when asked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await longer(page).click();
			await asked(page, '45m');
			assert.equal(
				await page.locator('.root-bar [role="tab"].modified').count(),
				0,
				'the widget’s own + dirtied the layout'
			);

			await app.reload();
			assert.deepEqual(
				await titles(page),
				['longer window (45m)', 'shorter window (20m)'],
				'a session setting survived a reload'
			);

			// The spy is a page hook, so the reload took it: arm it again.
			await app.spyOnRunner();
			await longer(page).click();
			await asked(page, '45m');
			await choose(page, 'nowview', 'Save options with layout');
			await page.waitForFunction(() => document.querySelector('.root-bar [role="tab"].modified'));

			// Now it is the draft's, and survives a reload on its own.
			await app.reload();
			assert.deepEqual(
				await titles(page),
				['longer window (60m)', 'shorter window (30m)'],
				'the draft did not keep the handed-over step'
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
				const box = await page.locator(`${PANE} .body:not(.behind .body)`).evaluate((b) => ({
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
// The anchor is the instant the picture is about: now, or `-t`'s when a
// tree carries one. What is held here is that the anchor does not move
// under the gesture — the band scales about the now-line's own x, so the
// column the fingers are anchored to is the one column that stays.
//
// After the release the anchor's x is datum's to place, and for a window
// that ENDS at now it moves: nowview reserves a now strip of
// `NOW_WIDTH_MINUTES` at the window's own scale (nowview.rs, the original
// datum's proportions), so a shorter window spends proportionally more of
// the band on it and the now-line slides left. A CENTRED window — `-t` —
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

	test('with -t set the pinch keeps that instant, not now', async () => {
		// The instant is the axis label under the middle of the band; the
		// yellow line is now (da-5flt), and 12:00 is not in 08:45–09:15.
		const instantX = (page) =>
			page
				.locator(`${PANE} svg.svg text.tick-label`, { hasText: '09:00' })
				.evaluate((label) => {
					const box = label.getBoundingClientRect();
					return box.x + box.width / 2;
				});
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				assert.equal(await page.locator(`${PANE} svg.svg .current-time-line`).count(), 0);
				const before = await instantX(page);
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
				const after = await instantX(page);
				assert.ok(Math.abs(after - before) <= 2, `09:00 moved: ${before} → ${after}`);
				// And the window is still that instant's, not the clock's.
				const argv = (await page.evaluate(() => window.__datumSeen))
					.filter((a) => a[0] === 'nowview')
					.at(-1);
				assert.equal(argv[argv.indexOf('-t') + 1], '09:00');
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

// ── off the picture (da-ndzl) ────────────────────────────────────────
//
// vividn, 2026-09-17: the corner + / − lay over the tiny snapshot's first
// chip — «music: true» read «sic: true». The steps now sit beside the far
// axis label, and the snapshot, having no axis, has none.
//
// da-7q8r retired the chips from the webview altogether — they are the
// CLI's `--labels`, which nothing here passes — so the snapshot is its
// bands and nothing else, and there is nothing left for a step to cover.

describe('the nowview steps stay off the picture', () => {
	test('390×664 in `mobile`: the snapshot is bands with no chips and no steps; the full window has them beside its far label', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.runLine('switch music true');
				await app.runLine('switch mood happy');
				await page.evaluate(() => window.datum.run(['occur', 'coffee']));
				const now = pane(page, 'now');
				await now.locator('svg.svg .now-chunk').nth(1).waitFor();
				const width = (await now.boundingBox()).width;
				assert.ok(width < 130, `the snapshot's pane is ${width}px wide`);
				assert.equal(await now.locator('.nowview-steps, button[aria-label$="window"]').count(), 0);
				// Nothing is left for a step to lie over: da-7q8r put the
				// chips behind the CLI's `--labels`, which nothing here passes.
				assert.equal(await now.locator('svg.svg .state-label').count(), 0, 'the webview asks for no labels');

				// The full window stands BESIDE the fields widget now rather
				// than behind a tab of it (da-hkjp), so there is nothing to
				// tap: it is already drawn.
				const where = await beside(page, `section[data-pane="nowview"]`);
				assert.equal(where.far, '-30m');
				assert.deepEqual(where.wrong, []);
				assert.deepEqual(app.errors, []);
			},
			{ viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: null }
		);
	});

	// The tab stack is the phone's `day` tree (`[tail, nowview, custom]`)
	// rather than `mobile`, whose row 3 stopped being tabs (da-hkjp). What
	// is under test is a pan that begins on a step inside a STACK, so any
	// shipped stack holding a nowview answers it.
	test('a tap on a step fires it; a pan that begins on one neither fires it nor moves the tab', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const fields = pane(page, 'side');
				await fields.locator('[role="tab"]', { hasText: 'nowview' }).click();
				await beside(page, 'section[data-pane="side"]');
				await app.spyOnRunner();
				/** Every `--history` a nowview was asked for; the snapshot beside it asks too. */
				const histories = () =>
					page.evaluate(() =>
						window.__datumSeen.filter((a) => a[0] === 'nowview').map((a) => a[a.indexOf('--history') + 1])
					);
				const step = fields.locator('button[aria-label="longer window"]');
				await step.tap();
				await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'nowview' && a.includes('45m')));

				// Now 60m is the step: a pan from it either way asks for nothing.
				await beside(page, 'section[data-pane="side"]');
				const box = await step.boundingBox();
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 120, 0);
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, -120, 0);
				await settled(page);
				await app.idle();
				assert.equal(await activeTab(page, 'side'), 'nowview', 'a pan on a step moved the tab');
				assert.ok(!(await histories()).includes('60m'), `a pan fired the step: ${await histories()}`);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, viewport: { width: 390, height: 664 } }
		);
	});

	test('a redraw mid-press does not lose the press', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				const box = await longer(page).boundingBox();
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.mouse.down();
				await page.locator(`${PANE} svg.svg`).evaluate((svg) => (svg.dataset.old = '1'));
				await page.evaluate(() => window.datum.run(['occur', 'coffee']));
				await page.waitForFunction((p) => {
					const svg = document.querySelector(`${p} svg.svg`);
					return svg && !svg.dataset.old;
				}, PANE);
				await page.mouse.up();
				await asked(page, '45m');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, clock: todayAt(12, 0) }
		);
	});
});
