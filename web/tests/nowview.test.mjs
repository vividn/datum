// The nowview widget's HANDLES (da-57vj4): each major label of the time
// axis is one. Hover it, or press a finger on it, and a wheel of the
// ladder's steps opens around it (down to 30s and up to 12h, never to the
// snapshot, which is the widget's «show history» turned off, da-hklaz); a
// step keeps the handle's place and gives it the step's time, so the window
// rescales by their ratio, and a drag along the axis moves the handle's
// time under the finger, to a window of any length. They replaced the − / +
// beside the far label (da-ndzl). The window is the widget's setting, kept
// in the session and in a saved layout when asked. The snapshot, which
// draws no axis, has none: nothing lies over its chips.
//
// Delta-blind on purpose: each step is held to the `--history` the seam was
// handed and to the handles' words, never to "the picture changed".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, activeTab, choose, chooseRoot, draft, pane, pastTheWindow, settled, swipe } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// ONE APP for the two tests on the default screen (da-6b3dj, sharedApp in
// support/app.mjs); the one that steps the window does it in a room, the
// step being a session setting of `main`'s nowview. Every other test opens
// its own: a screen, clock, touch or seeded draft of its own, or a reload.
const shared = sharedApp();

const PANE = 'section.pane[data-title="nowview"]';
/** The handle over the label that reads `at`. */
const handle = (page, at, pane = PANE) => page.locator(`${pane} .nowview-handles .handle[data-at="${at}"]`);
/** A notch of the open wheel, by its step. */
const notch = (page, step) => page.locator(`.nowview-wheel .notch[data-step="${step}"]`);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to be `--history h`. */
const asked = (page, h) =>
	page.waitForFunction((h) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		return argv?.[argv.indexOf('--history') + 1] === h;
	}, h);

/** The handles' words, far first. */
const handled = (page, pane = PANE) =>
	page.locator(`${pane} .nowview-handles .handle`).evaluateAll((hs) => hs.map((h) => h.dataset.at));

/** The open wheel's notches, the longer (up) then the shorter, nearest first, and how clear each is. */
const wheelOf = (page) =>
	page.locator('.nowview-wheel').evaluate((w) => {
		const of = (sign) =>
			[...w.querySelectorAll('.notch')]
				.filter((n) => Math.sign(Number(n.style.getPropertyValue('--k'))) === sign)
				.sort((a, b) => Math.abs(a.style.getPropertyValue('--k')) - Math.abs(b.style.getPropertyValue('--k')))
				.map((n) => [n.dataset.step, Number(getComputedStyle(n).opacity)]);
		return { at: w.querySelector('.handle.held').dataset.at, up: of(1), down: of(-1) };
	});

/** Every nowview widget's settings in a layout tree. */
function nowviewSettings(node, out = []) {
	if (node?.kind === 'nowview') out.push(node.settings);
	for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) {
		nowviewSettings(child.content ?? child, out);
	}
	return out;
}

/**
 * Where the handles of the nowview in `pane` sit against its picture: the
 * far label's words, and what is wrong — each handle must lie over the
 * label it names, be a target a finger can take, stay under the axis line,
 * off every other handle; the far label, the first the WINDOW shows (the
 * picture is drawn past its box, `--surround`), whole inside it and drawn.
 * Waits for the handles to be drawn.
 */
async function beside(page, pane) {
	await page.locator(`${pane} .nowview-handles .handle`).first().waitFor();
	return page.locator(pane).evaluate((p) => {
		const r = (el) => el.getBoundingClientRect();
		const over = (a, b) => a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
		const svg = p.querySelector('svg.svg');
		const frame = r(svg.parentElement);
		const all = [...svg.querySelectorAll('.tick-label')].filter((l) => r(l).left < frame.right && r(l).right > frame.left);
		const far = all.reduce((a, b) => (r(b).x < r(a).x ? b : a));
		const labels = all.filter((l) => getComputedStyle(l).visibility !== 'hidden');
		const axis = r(svg.querySelector('.axis'));
		const handles = [...p.querySelectorAll('.nowview-handles .handle')];
		const wrong = [];
		if (r(far).left < frame.left - 0.5) wrong.push(`the far label starts at ${r(far).left}, the window at ${frame.left}`);
		if (!labels.includes(far)) wrong.push(`the far label ${far.textContent} is hidden`);
		for (const h of handles) {
			const box = r(h);
			const label = labels.find((l) => l.textContent === h.dataset.at);
			if (!label) wrong.push(`no label reads ${h.dataset.at}`);
			else {
				const l = r(label);
				if (l.left < box.left - 0.5 || l.right > box.right + 0.5) wrong.push(`${h.dataset.at}'s handle does not span its label`);
			}
			if (box.height < 18 || box.width < 20) wrong.push(`${h.dataset.at}'s handle is ${box.width}×${box.height}`);
			if (box.top < axis.top - 0.5) wrong.push(`${h.dataset.at}'s handle reaches above the axis`);
			for (const o of handles) if (o !== h && over(box, r(o))) wrong.push(`${h.dataset.at} is over ${o.dataset.at}`);
		}
		if (handles[0]?.dataset.at !== far.textContent) wrong.push(`the first handle is ${handles[0]?.dataset.at}`);
		return { far: far.textContent, wrong };
	});
}

/**
 * Hover the handle reading `at`, and wait for its wheel. By the mouse and
 * not `locator.hover`: the wheel opens OVER the handle, which is the point,
 * and Playwright reads that as the handle not taking the pointer.
 */
async function hover(page, at, pane = PANE) {
	const box = await handle(page, at, pane).boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.locator(`.nowview-wheel .handle.held[data-at="${at}"]`).waitFor();
}

/** Wait for the far handle to read `at`: the picture redrawn for the window asked. */
const farIs = (page, at, pane = PANE) =>
	page.waitForFunction(
		([pane, at]) => document.querySelector(`${pane} .nowview-handles .handle`)?.dataset.at === at,
		[pane, at]
	);

describe('the nowview widget', () => {
	test('a handle over every label the window shows, the far one first, and no − / +', async () => {
		await shared.solo(async ({ page }) => {
			const where = await beside(page, PANE);
			assert.equal(where.far, '-30m');
			assert.deepEqual(where.wrong, []);
			assert.ok((await handled(page)).length >= 2, `one handle: ${await handled(page)}`);
			assert.equal(await page.locator(`${PANE} .nowview-steps, ${PANE} .step`).count(), 0, 'the − / + are still drawn');
		});
	});

	// da-9hmqg, vividn: «make the outline of the handles a bit higher
	// contrast so that it's more clear they are clickable/draggable». The
	// ratio of what is painted, against the first ancestor that paints, beside
	// the axis rules' `--rule` against the same ground.
	test('a handle is outlined at a clearly higher contrast than a rule', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator(`${PANE} .nowview-handles .handle`).first().waitFor();
			const m = await page.locator(`${PANE} .nowview-handles .handle`).first().evaluate((h) => {
				const parse = (color) => (color.match(/[\d.]+/g) ?? []).map(Number);
				const channel = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
				const lum = ([r, g, b]) => 0.2126 * channel(r / 255) + 0.7152 * channel(g / 255) + 0.0722 * channel(b / 255);
				const ratio = (a, b) => {
					const [light, dark] = [lum(a), lum(b)].sort((x, y) => y - x);
					return Math.round(((light + 0.05) / (dark + 0.05)) * 100) / 100;
				};
				let ground = [0, 0, 0];
				for (let node = h.parentElement; node; node = node.parentElement) {
					const bg = parse(getComputedStyle(node).backgroundColor);
					if (bg[3] === undefined || bg[3] > 0) {
						ground = bg.slice(0, 3);
						break;
					}
				}
				const probe = document.createElement('span');
				probe.style.color = 'var(--rule)';
				h.parentElement.append(probe);
				const rule = parse(getComputedStyle(probe).color).slice(0, 3);
				probe.remove();
				const border = parse(getComputedStyle(h).borderTopColor).slice(0, 3);
				return { border: ratio(border, ground), rule: ratio(rule, ground), ground: `rgb(${ground})` };
			});
			assert.ok(m.border >= 2.5 && m.border >= 1.75 * m.rule, `handle ${m.border}:1, rule ${m.rule}:1 on ${m.ground}`);
		});
	});

	test('with the cursor set the far label is the leftmost, and a handle on each side of it', async () => {
		await withApp(
			async ({ page }) => {
				await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), todayAt(9, 0));
				// Centred: the window's −15m to +15m, and its surroundings on to ±45m.
				await page.waitForFunction(
					(pane) => [...document.querySelectorAll(`${pane} svg.svg .tick-label`)].some((l) => l.textContent === '+45m'),
					PANE
				);
				const where = await beside(page, PANE);
				assert.equal(where.far, '-15m');
				assert.deepEqual(where.wrong, []);
				assert.ok((await handled(page)).includes('+15m'), `${await handled(page)}`);
			},
			{ clock: todayAt(12, 0) }
		);
	});

	test('hover the far handle at 30m: the wheel steps it to 45m, then 60m, and stays open', async () => {
		await shared.room('steps', async (app) => {
			const { page } = app;
			await beside(page, PANE);
			await hover(page, '-30m');
			// The next step either way clear, those beyond it fading.
			const wheel = await wheelOf(page);
			assert.deepEqual(wheel.up.map(([s]) => s), ['45m', '60m', '90m']);
			assert.deepEqual(wheel.down.map(([s]) => s), ['20m', '15m', '10m']);
			for (const side of [wheel.up, wheel.down]) {
				assert.equal(side[0][1], 1, `the next step is not clear: ${JSON.stringify(side)}`);
				assert.ok(side[1][1] < 1 && side[2][1] < side[1][1], `the steps beyond do not fade: ${JSON.stringify(side)}`);
			}
			const seen = await app.spyOnRunner();

			await notch(page, '45m').click();
			await asked(page, '45m');
			assert.equal((await wheelOf(page)).at, '-45m', 'the wheel closed or did not move on');
			await notch(page, '60m').click();
			await asked(page, '60m');
			assert.deepEqual((await wheelOf(page)).up.map(([s]) => s), ['90m', '2h', '3h']);

			// Leaving the handle closes the wheel.
			await page.mouse.move(1, 1);
			await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
			assert.ok(
				!(await seen()).some((a) => a[0] === 'nowview' && a[a.indexOf('--history') + 1] === '0'),
				'a step reached the snapshot'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('on −15m in a 30m window, 20m makes the window 40m; the ladder ends at 30s and 12h', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				// A box narrow enough that the 30m window labels its −15m.
				assert.ok((await handled(page)).includes('-15m'), `${await handled(page)}`);
				await hover(page, '-15m');
				await notch(page, '20m').click();
				await asked(page, '40m');
				await app.idle();
				assert.deepEqual(app.errors, [], 'datum did not take a window off the ladder');
			},
			// 460, not 400: the now strip takes a tenth of the plot (da-6csum),
			// and at 400 five minutes of window were a pixel short of the room
			// two labels need, so the 30m window ruled tens and drew no −15m.
			{ viewport: { width: 460, height: 700 }, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
		// A window the ladder's ends would leave is not offered.
		for (const [history, far, side] of [
			['12h', '-720m', 'up'],
			['30s', '-30s', 'down']
		]) {
			await withApp(
				async ({ page }) => {
					await beside(page, PANE);
					await hover(page, far);
					assert.deepEqual((await wheelOf(page))[side], [], `${history}'s wheel goes on ${side}`);
				},
				{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history })) }
			);
		}
	});

	test('a finger: hold the far handle and swipe up two notches, and the window steps two', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				const box = await handle(page, '-30m').boundingBox();
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 0, -46);
				await asked(page, '60m');
				// A swipe's lift takes its notch and puts the wheel away.
				await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('a drag of the far handle along the axis to halfway to now doubles the window, and the ticks follow', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				const box = await handle(page, '-30m').boundingBox();
				const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
				const now = await anchorX(page);
				await page.mouse.move(x, y);
				await page.locator('.nowview-wheel .handle.held').waitFor();
				await page.mouse.down();
				await page.mouse.move((x + now) / 2, y, { steps: 6 });
				// Under the finger, the handle's words, and the picture scaled.
				await page.locator('.handle.dragged', { hasText: '-30m' }).waitFor();
				await page.mouse.up();
				await asked(page, '60m');
				await handle(page, '-60m').waitFor();

				// And to anywhere: four fifths of the way back is 75m, off
				// the ladder, which datum is asked for as it is.
				const again = await handle(page, '-60m').boundingBox();
				const [x2, y2] = [again.x + again.width / 2, again.y + again.height / 2];
				const now2 = await anchorX(page);
				// The wheel the redraw opened under the still pointer closes
				// as it leaves, never having been entered.
				await page.mouse.move(1, 1);
				await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
				await page.mouse.move(x2, y2);
				await page.mouse.down();
				await page.mouse.move(now2 - (now2 - x2) * 0.8, y2, { steps: 6 });
				await page.mouse.up();
				await asked(page, '75m');
				await app.idle();
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-byzc0, vividn: «the handles on the nowview should have their text
	// set to non selectable, right now when trying to drag it often just
	// selects the text which is annoying».
	for (const finger of [false, true])
		test(`a drag of a handle by ${finger ? 'a finger' : 'the mouse'} selects no text, and moves the window`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await beside(page, PANE);
					await app.spyOnRunner();
					const box = await handle(page, '-30m').boundingBox();
					const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
					const now = await anchorX(page);
					if (finger) await swipe(page, x, y, (now - x) / 2, 0);
					else {
						await page.mouse.move(x, y);
						await page.mouse.down();
						await page.mouse.move((x + now) / 2, y, { steps: 6 });
						await page.mouse.up();
					}
					await asked(page, '60m');
					await handle(page, '-60m').waitFor();
					assert.equal(await page.evaluate(() => window.getSelection().toString()), '');
					// And none of a handle's words can be taken, whatever the drag.
					// Read in one pass in the page, so no redraw lands between the
					// query and the look.
					const selectable = await page.evaluate(() =>
						[...document.querySelectorAll('.handle')]
							.filter((h) => getComputedStyle(h).userSelect !== 'none')
							.map((h) => h.dataset.at)
					);
					assert.deepEqual(selectable, []);
					assert.deepEqual(app.errors, []);
				},
				{ ...(finger ? TOUCH : DESKTOP), prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
			);
		});

	test('a focused handle steps with up and down', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				await handle(page, '-30m').focus();
				await page.keyboard.press('ArrowUp');
				await asked(page, '45m');
				await page.keyboard.press('ArrowUp');
				await asked(page, '60m');
				await page.keyboard.press('ArrowDown');
				await asked(page, '45m');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-vhwq: the window is a setting the widget moves ITSELF, so it is THIS
	// SESSION'S — it changes what this tab shows, the layout is not dirtied,
	// and a reload keeps it on this device (da-ray91). «Save options with
	// layout» is what hands it over; only then is it a draft edit, and only
	// then does `Save layout` write it down.
	test('the window is this session’s, and the draft and a save take it only when asked', async () => {
		await withApp(async (app) => { // reloads, and saves `main`
			const { page } = app;
			await app.spyOnRunner();
			await hover(page, '-30m');
			await notch(page, '45m').click();
			await asked(page, '45m');
			assert.equal(
				await page.locator('.root-bar [role="tab"].modified').count(),
				0,
				'the widget’s own step dirtied the layout'
			);

			await app.reload();
			await farIs(page, '-45m');
			assert.equal(
				await page.locator('.root-bar [role="tab"].modified').count(),
				0,
				'a kept session setting dirtied the layout'
			);

			await choose(page, 'nowview', 'Save options with layout');
			await page.waitForFunction(() => document.querySelector('.root-bar [role="tab"].modified'));

			// Now it is the draft's, and survives a reload on its own.
			await app.reload();
			await farIs(page, '-45m'); // the draft kept the handed-over window

			// The root bar's save icon, on the `main` tab (da-0a5o).
			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar.modified'));
			const other = await app.newTab();
			const saved = await other.page.evaluate(() => window.datum.layouts.loadLayout('main'));
			assert.deepEqual(
				nowviewSettings(saved.root).map((s) => s.history),
				['45m']
			);
		});
	});

	test('at 360px the handles do not widen the pane', async () => {
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
				assert.ok(await handle(page, '-30m').isVisible());
			},
			{ viewport: { width: 360, height: 640 } }
		);
	});
});

// ── the zoom (da-5goc) ───────────────────────────────────────────────
//
// Two fingers over the picture, or ctrl+wheel, scale the window and land
// on the ladder the handles' wheels step along.
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

/** A layout of one nowview pane, as the `main` tab's draft. */
const soloNowview = (settings) =>
	JSON.stringify({
		version: 1,
		name: 'main',
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
				await farIs(page, '-20m');

				// And back: fingers together by the same ratio is 20m × 1.5,
				// which rounds onto 30m.
				await pinch(page, box, 120, 80);
				await asked(page, '30m');
				await farIs(page, '-30m');
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('ctrl+wheel steps the same ladder, and the handles pick up from it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.keyboard.down('Control');

				await page.mouse.wheel(0, -100); // up: a shorter window
				await asked(page, '20m');

				await page.mouse.wheel(0, 100); // down: a longer one
				await asked(page, '30m');
				await page.keyboard.up('Control');

				// A bare wheel is the pane's own scroll, not a zoom.
				await page.mouse.wheel(0, -100);
				await app.idle();
				await farIs(page, '-30m');

				// The handles pick up from where the wheel left the window.
				await hover(page, '-30m');
				await notch(page, '45m').click();
				await asked(page, '45m');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP }
		);
	});

	// vividn, 2026-10-26: «when the view is panned away, scale about where
	// the "now" would actually be, not the center of the window» (da-zccjs).
	test('a window centred elsewhere scales about now, wherever now is', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				// Now is drawn, if at all, in the surroundings past the window (da-6fdaf).
				const lines = await page
					.locator(`${PANE} svg.svg .current-time-line`)
					.evaluateAll((ls) => ls.map((l) => l.getBoundingClientRect().left));
				assert.ok(lines.every((x) => x >= box.x + box.width), `the now line is in the window: ${lines} of ${JSON.stringify(box)}`);
				// Its ticks count from now, three hours on: the far one is -195m.
				await farIs(page, '-195m');

				await pinch(page, box, 80, 120);
				await asked(page, '20m');
				await app.idle();
				// 08:45–09:15 two thirds as long about 12:00 is 09:50–10:10.
				const centreOf = async () => {
					const argv = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
					return { argv, centre: Date.parse(argv[argv.indexOf('--center') + 1]) };
				};
				const { argv, centre } = await until(centreOf, ({ centre }) => Math.abs(centre - todayAt(10, 0)) <= 60_000);
				assert.ok(Math.abs(centre - todayAt(10, 0)) <= 60_000, `asked for ${argv.join(' ')}`);
				assert.equal(argv.includes('-t'), false, 'a pinch made an instant');
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
// axis label, and the snapshot, having no axis, has none. The handles that
// replaced them (da-57vj4) are the labels themselves.
//
// da-7q8r retired the chips from the webview altogether — they are the
// CLI's `--labels`, which nothing here passes — so the snapshot is its
// bands and nothing else, and there is nothing left for a handle to cover.

describe('the nowview handles stay off the picture', () => {
	// `mobile`'s snapshot pane went in da-2hd21; the full window is what is left.
	test('390×664 in `mobile`: the full window has its handles over its labels', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// A minute back: the clock is held, and a switch made AT now
				// is a band of no width yet.
				await app.runLine('switch music true -t -1');
				await app.runLine('switch mood happy -t -1');
				await page.evaluate(() => window.datum.run(['occur', 'coffee']));

				// The full window stands BESIDE the fields widget now rather
				// than behind a tab of it (da-hkjp), so there is nothing to
				// tap: it is already drawn.
				const where = await beside(page, `section[data-pane="nowview"]`);
				assert.equal(where.far, '-30m');
				assert.deepEqual(where.wrong, []);
				assert.deepEqual(app.errors, []);
			},
			{ viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT }
		);
	});

	// The tab stack is the phone's `main` tree (`[tail, nowview, custom]`)
	// rather than `mobile`, whose row 3 stopped being tabs (da-hkjp). What
	// is under test is a drag that begins on a handle inside a STACK, so any
	// shipped stack holding a nowview answers it.
	test('a tap on a handle opens its wheel and a tap on a notch steps it; a sideways drag on one scales, and never moves the tab', async () => {
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
				await handle(page, '-30m', 'section[data-pane="side"]').tap();
				await notch(page, '45m').tap();
				await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'nowview' && a.includes('45m')));
				// A finger's wheel stays until a press lands elsewhere.
				assert.equal((await wheelOf(page)).at, '-45m');

				// A sideways drag on the far handle, either way, is the
				// handle's: the window scales, and the pane keeps its tab.
				await beside(page, 'section[data-pane="side"]');
				await pastTheWindow(page);
				const was = (await histories()).length;
				const box = await handle(page, '-45m', 'section[data-pane="side"]').boundingBox();
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 60, 0);
				await page.waitForFunction((n) => window.__datumSeen.filter((a) => a[0] === 'nowview').length > n, was);
				await settled(page);
				await app.idle();
				assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag on a handle moved the tab');
				const scaled = (await histories()).at(-1);
				assert.ok(!['45m', '60m'].includes(scaled), `a drag toward now did not shorten the window: ${scaled}`);
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
				await hover(page, '-30m');
				const box = await notch(page, '45m').boundingBox();
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

// ── the surroundings (da-61nm8) ──────────────────────────────────────
//
// vividn: «predraw enough of the surrounding time so that I can click
// anywhere in the nowview and move my thumb mouse anywhere horizontally on
// the screen or be able to zoom out and be able to see the data without
// having to redraw». The picture is asked for a window's worth past its box
// either side (`--surround`), and a gesture moves what is drawn: a coffee
// forty minutes back, outside the thirty-minute window, is on screen
// mid-gesture with no nowview asked for since the press.

describe('the nowview draws past its box', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';

	/** Whether the coffee's mark is drawn inside the band, and how many nowviews were asked for. */
	const midGesture = (page) =>
		page.evaluate((band) => {
			const box = document.querySelector(band).getBoundingClientRect();
			const shown = [...document.querySelectorAll(`${band} svg.svg [data-field="coffee"]:not(g)`)].some((m) => {
				const r = m.getBoundingClientRect();
				return r.width > 0 && r.left >= box.left && r.right <= box.right;
			});
			return { shown, asked: (window.__datumSeen ?? []).filter((a) => a[0] === 'nowview').length };
		}, BAND);

	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };

	async function withCoffee(app) {
		await app.runLine('occur coffee -t -40');
		await app.redraw();
		await app.page.locator(`${BAND} svg.svg [data-field="coffee"]:not(g)`).first().waitFor({ state: 'attached' });
		await app.idle();
		const before = await midGesture(app.page);
		assert.equal(before.shown, false, 'the coffee is in the window before any gesture');
		await app.spyOnRunner();
		return app.page.locator(BAND).boundingBox();
	}

	test('a drag of a whole width shows what was before the window, and asks for nothing until the lift', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const box = await withCoffee(app);
			const y = box.y + box.height / 2;
			await page.mouse.move(box.x + 4, y);
			await page.mouse.down();
			await page.mouse.move(box.x + 4 + box.width, y, { steps: 8 });
			const during = await midGesture(page);
			await page.mouse.up();
			assert.deepEqual(during, { shown: true, asked: 0 });
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('a pinch out a step shows it too, with no nowview asked for under the fingers', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const box = await withCoffee(app);
			let during;
			await pinch(page, box, 160, 80, async () => (during = await midGesture(page)));
			assert.deepEqual(during, { shown: true, asked: 0 });
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('there is no clock: no HH:MM anywhere in the picture', async () => {
		await withApp(async ({ page }) => {
			await page.locator(`${BAND} svg.svg .tick-label`).first().waitFor({ state: 'attached' });
			const texts = await page.locator(`${BAND} svg.svg text`).allTextContents();
			assert.deepEqual(texts.filter((t) => /\d{1,2}:\d\d/.test(t)), []);
			assert.equal(await page.locator(`${BAND} svg.svg .current-time-text`).count(), 0);
		}, phone);
	});
});

// ── a stretch (da-6fdaf) ─────────────────────────────────────────────
//
// vividn, 2026-10-26: «Rescaling nowview should behave similarly to
// rescaling dayview, so occur dots and multi state stripes don't stretch
// with it … it would be good to load in more data into nowview, so that it
// can handle at least a x5 stretch». A pinch in to a fifth: the dot and the
// stripes' tile lie on the screen as they did at rest, and a coffee 130
// minutes back, five windows out, is drawn under the fingers with no
// nowview asked for since the press.

describe('a nowview stretch moves marks, never sizes them', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';
	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };

	/** The tea dot's box, the stripes' tile on the screen as a 2×2, whether the coffee is in the band, and the nowviews asked for. */
	const marks = (page) =>
		page.evaluate((band) => {
			const box = document.querySelector(band).getBoundingClientRect();
			const dot = document.querySelector(`${band} svg.svg [data-field="tea"] .point-fg`).getBoundingClientRect();
			const block = document.querySelector(`${band} svg.svg rect.block.multi-state`);
			const pattern = document.getElementById(block.getAttribute('fill').slice('url(#'.length, -1));
			const m = block.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
			const coffee = [...document.querySelectorAll(`${band} svg.svg [data-field="coffee"]:not(g)`)].some((c) => {
				const r = c.getBoundingClientRect();
				return r.width > 0 && r.left >= box.left && r.right <= box.right;
			});
			return {
				dot: [dot.width, dot.height],
				tile: [m.a, m.b, m.c, m.d],
				block: block.getBoundingClientRect().width,
				coffee,
				asked: (window.__datumSeen ?? []).filter((a) => a[0] === 'nowview').length
			};
		}, BAND);

	const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-3 * Math.max(1, Math.abs(b[i])));

	test('a pinch in to a fifth keeps the dot and the stripes, and shows the coffee five windows back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -130', 'switch posture vertical,sit -t -20', 'occur tea -t -10'])
				await app.runLine(line);
			await app.redraw();
			await page.locator(`${BAND} svg.svg rect.block.multi-state`).waitFor({ state: 'attached' });
			await app.idle();
			const rest = await marks(page);
			assert.equal(rest.coffee, false, 'the coffee is in the window before any gesture');
			await app.spyOnRunner();
			const box = await page.locator(BAND).boundingBox();
			let during;
			// Fingers inside the band, a fifth as far apart at the end.
			const apart = box.width - 20;
			await pinch(page, box, apart, apart / 5, async () => {
				during = await marks(page);
				// The dash under the now strip runs along time: it stretches with the strip.
				assert.equal(
					await page.locator(`${BAND} svg.svg line.current-time-line-far`).evaluate((l) => getComputedStyle(l).scale),
					'none'
				);
			});
			// The control: the stretch happened, and a duration is as long as its minutes.
			assert.ok(Math.abs(during.block / rest.block - 0.2) < 0.01, `the block is ${during.block}px, was ${rest.block}`);
			assert.ok(near(during.dot, rest.dot), `the dot is ${during.dot}, was ${rest.dot}`);
			assert.ok(near(during.tile, rest.tile), `the stripes' tile is ${during.tile}, was ${rest.tile}`);
			assert.deepEqual([during.coffee, during.asked], [true, 0]);
			assert.deepEqual(app.errors, []);
		}, phone);
	});
});

// ── a crop of the day's data (da-0e06p) ──────────────────────────────
//
// vividn, 2026-10-26: «the cached part could just be the data part, and then
// dayview and nowview draw their respective axes and ui elements overtop
// cheaply and quickly without needing any database calls». The picture is
// the CLI's overlay (`nowview --overlay`, no data) with the day's data layer
// (`dayview --data-layer`) laid under it: a stretch asks for the overlay of
// its window on the release, and for no data at all.

describe('the nowview crops the day’s data under its overlay', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';
	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };
	const seen = (page) => page.evaluate(() => (window.__datumSeen ?? []).map((a) => a.join(' ')));

	test('a pinch in to a fifth asks for the new window’s overlay on the release, and for no data', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -130', 'switch mood calm -t -20', 'occur tea -t -10']) await app.runLine(line);
			await app.redraw();
			await page.locator(`${BAND} svg.svg rect.block[data-state="calm"]`).first().waitFor({ state: 'attached' });
			await app.idle();
			const rest = await handled(page, 'section[data-pane="nowview"]');
			await app.spyOnRunner();
			const box = await page.locator(BAND).boundingBox();
			const apart = box.width - 20;
			await pinch(page, box, apart, apart / 5);
			await app.idle();
			const asked = await seen(page);
			assert.deepEqual(asked.filter((a) => a.startsWith('dayview')), [], 'the stretch asked for data');
			assert.equal(asked.filter((a) => a.startsWith('nowview')).length, 1, `asked: ${asked.join(' | ')}`);
			assert.match(asked.find((a) => a.startsWith('nowview')), /--overlay .*--history (?!30m)/);
			// The ticks follow the window: its handles say the new one.
			await until(() => handled(page, 'section[data-pane="nowview"]'), (h) => h.join() !== rest.join());
			// The coffee, 130 minutes back, is drawn from the data on screen.
			assert.ok(
				await page.locator(`${BAND} svg.svg [data-field="coffee"]:not(g)`).first().evaluate((c, band) => {
					const [r, b] = [c.getBoundingClientRect(), document.querySelector(band).getBoundingClientRect()];
					return r.width > 0 && r.left >= b.left && r.right <= b.right;
				}, BAND),
				'the coffee is not in the window'
			);
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('a window across midnight shows the end of yesterday stitched on', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				for (const line of ['switch mood late -t -20', 'switch mood early -t -15']) await app.runLine(line);
				await app.redraw();
				const late = page.locator(`${BAND} svg.svg rect.block[data-state="late"]`).first();
				await late.waitFor({ state: 'attached' });
				const [r, b] = await late.evaluate((l, band) => [l.getBoundingClientRect(), document.querySelector(band).getBoundingClientRect()].map((x) => x.toJSON()), BAND);
				assert.ok(r.width > 1 && r.left >= b.left && r.right <= b.right, `yesterday's block is at ${JSON.stringify(r)} in ${JSON.stringify(b)}`);
				assert.ok((await seen(page)).some((a) => /^dayview --data-layer .*--end-date -1$/.test(a)), 'yesterday was not asked');
				assert.deepEqual(app.errors, []);
			},
			{ ...phone, clock: todayAt(0, 10) }
		);
	});
});
