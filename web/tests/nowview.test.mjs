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
import { noWait } from './support/press.mjs';
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
			// 460, not 400: the now strip takes its share of the plot (da-6csum),
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
// Two fingers over the picture scale the window, and the release asks for
// the window they left, to the second (da-ygpto); ctrl+wheel lands on the
// ladder the handles' wheels step along.
//
// A pinch scales about the FINGERS (da-3zpdx, vividn, 2026-10-27: «if my
// fingers are on the time of two different occur dots moving the two
// fingers should track with the occur dots regardless if panning or
// zooming, letting go should then keep those two dots exactly where they
// are»): the instant under each finger stays under it, spread, squeezed or
// slid, and the window the release asks for keeps them there. That the
// frame after the release is the frame under the fingers is
// nowrelease.test.mjs.

/**
 * The x of the picture's now — the anchor a handle scales about: the
 * instant its now-line stands for (`data-now`, in the picture's own units),
 * which datum draws a gap past it (da-ygpto).
 */
const anchorX = (page) =>
	page.locator(`${PANE} svg.svg .current-time-line`).evaluate((line) => {
		const at = new DOMPoint(Number(line.getAttribute('data-now')), 0).matrixTransform(line.ownerSVGElement.getScreenCTM());
		return at.x;
	});

/**
 * Each coffee's centre on screen, by its instant: the copy drawn, inside its
 * stretch. A picture the now strip cuts in two lays the day's data into each
 * stretch, clipped to it (`compose`), so the copy past its stretch is hidden.
 */
const coffees = (page) =>
	page.locator(`${PANE} svg.svg [data-field="coffee"][data-at]`).evaluateAll((ms) =>
		Object.fromEntries(
			ms.flatMap((m) => {
				const r = m.getBoundingClientRect();
				const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
				const g = m.closest('[data-lo]');
				const [lo, hi] = ['lo', 'hi'].map((k) => new DOMPoint(Number(g.dataset[k]), 0).matrixTransform(g.getScreenCTM()).x);
				return x >= Math.min(lo, hi) - 1 && x <= Math.max(lo, hi) + 1 ? [[m.dataset.at, { x, y }]] : [];
			})
		)
	);

/** Two fingers through CDP, landing one then the other at `from`, moved in six steps to `to`; `midway` with them still down. */
async function fingers(page, from, to, midway) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	const step = (t) => from.map((f, i) => ({ id: i + 1, x: f.x + (to[i].x - f.x) * t, y: f.y + (to[i].y - f.y) * t }));
	try {
		await at('touchStart', [step(0)[0]]);
		await at('touchStart', step(0));
		for (let i = 1; i <= 6; i++) await at('touchMove', step(i / 6));
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

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
	test('a pinch keeps the instant under each finger under it: apart, together and sideways, and after the release', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (let i = 0; i < 40; i++) await app.runLine(`occur coffee -t -${1 + 3 * i}`);
				await app.redraw();
				await page.locator(`${PANE} svg.svg [data-field="coffee"][data-at]`).first().waitFor({ state: 'attached' });
				await app.idle();
				await app.spyOnRunner();
				const band = await page.locator(`${PANE} .nowview-band`).boundingBox();
				for (const [name, to] of [
					// Back first: together, about the fingers, would otherwise
					// bring the time past now in, and the window stops at now
					// (da-v8gzq).
					['back', (a, b) => [a.x + 150, b.x + 150]],
					['apart', (a, b) => [a.x - 30, b.x + 30]],
					['together', (a, b) => [a.x + (b.x - a.x) / 5, b.x - (b.x - a.x) / 5]],
					['sideways', (a, b) => [a.x + 40, b.x + 40]]
				]) {
					// Two coffees in the window, clear of the now strip and of the band's ends.
					const shown = Object.entries(await coffees(page)).filter(([, c]) => c.x > band.x + 40 && c.x < band.x + band.width * 0.6);
					const pick = (x) => shown.reduce((a, b) => (Math.abs(b[1].x - x) < Math.abs(a[1].x - x) ? b : a));
					const [a, b] = [pick(band.x + 40), pick(band.x + band.width * 0.55)];
					assert.ok(b[1].x - a[1].x > 60, `${name}: no two coffees apart in ${JSON.stringify(shown)}`);
					const xs = to(a[1], b[1]);
					const ends = xs.map((x) => ({ x, y: a[1].y }));
					const off = (when, at) =>
						[a, b].flatMap(([key], i) =>
							Math.abs(at[key]?.x - xs[i]) <= 1 ? [] : [`${name}, ${when}: the coffee at ${key} is at ${at[key]?.x}, its finger at ${xs[i]}`]
						);
					let during;
					await fingers(page, [a[1], b[1]], ends, async () => (during = await coffees(page)));
					assert.deepEqual(off('under the fingers', during), []);
					// Let go: datum's picture of the window replaces the one held.
					await page.waitForFunction((band) => document.querySelector(band).style.getPropertyValue('--zoom') === '1', `${PANE} .nowview-band`);
					await app.idle();
					assert.deepEqual(off('after the release', await coffees(page)), []);
				}
				const asked = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview');
				assert.ok(asked.length >= 2, `the pinches asked for ${asked.length} windows`);
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

	// Off now too: the window keeps the instant under the fingers where it
	// is (da-3zpdx), where a handle scales about now (da-zccjs).
	test('a window centred elsewhere scales about the fingers', async () => {
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
				// The fingers at the box's middle, a tenth of the box past the
				// window's (the box's last fifth is the strip's room): at 09:03:45.
				// 08:45–09:15 two thirds as long, that instant kept, is centred 09:01:15.
				const centreOf = async () => {
					const argv = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
					return { argv, centre: Date.parse(argv[argv.indexOf('--center') + 1]) };
				};
				const expected = todayAt(9, 1) + 15_000;
				const { argv, centre } = await until(centreOf, ({ centre }) => Math.abs(centre - expected) <= 5_000);
				assert.ok(Math.abs(centre - expected) <= 5_000, `asked for ${argv.join(' ')}`);
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
// stripes' tile lie on the screen as they did at rest, and a coffee 90
// minutes back, three windows out, is drawn under the fingers with no
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

	test('a pinch in to a fifth keeps the dot and the stripes, and shows the coffee three windows back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -90', 'switch posture vertical,sit -t -20', 'occur tea -t -10'])
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
				// The dash under the now strip is the strip's: scaled back with it, about now, never on its own.
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
	/** The dates a data layer run drew, by its `--end-date` and `--pictures` (#lib/dayLayers.ts), or checked too, by `--digests`. */
	const layerDates = (argv, flags = ['--pictures']) => {
		const words = argv.split(' ');
		const end = words[0] === 'dayview' && words.includes('--data-layer') && words[words.indexOf('--end-date') + 1];
		const flag = flags.find((f) => words.includes(f));
		const backs = end && flag ? words[words.indexOf(flag) + 1].split(',') : [];
		return backs.map((back) => new Date(Date.parse(end) - Number(back) * 86_400_000).toISOString().slice(0, 10));
	};

	test('a pinch in to a fifth asks for the new window’s overlay on the release, and for no data', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -90', 'switch mood calm -t -20', 'occur tea -t -10']) await app.runLine(line);
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
			// Its surroundings reach further, and a day held there from before may be
			// checked (da-vnfr3); never a day the window it shows draws.
			const today = await page.evaluate(() => [new Date()].map((d) => [d.getFullYear(), d.getMonth() + 1, d.getDate()].map((n) => String(n).padStart(2, '0')).join('-'))[0]);
			assert.deepEqual(asked.filter((a) => layerDates(a, ['--pictures', '--digests']).includes(today)), [], 'the stretch asked for its window’s data');
			assert.equal(asked.filter((a) => a.startsWith('nowview')).length, 1, `asked: ${asked.join(' | ')}`);
			assert.match(asked.find((a) => a.startsWith('nowview')), /--overlay .*--history (?!30m)/);
			// The ticks follow the window: its handles say the new one.
			await until(() => handled(page, 'section[data-pane="nowview"]'), (h) => h.join() !== rest.join());
			// The coffee, 90 minutes back, is drawn from the data on screen.
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
				const yesterday = await page.evaluate(() => {
					const d = new Date();
					d.setDate(d.getDate() - 1);
					return [d.getFullYear(), d.getMonth() + 1, d.getDate()].map((n) => String(n).padStart(2, '0')).join('-');
				});
				assert.ok((await seen(page)).some((a) => layerDates(a).includes(yesterday)), 'yesterday was not asked');
				assert.deepEqual(app.errors, []);
			},
			{ ...phone, clock: todayAt(0, 10) }
		);
	});
});

// ── a pan stops at now, and coasts (da-v8gzq) ────────────────────────
//
// vividn, 2026-10-27: «let's stop letting you pan into the future on
// nowview, it should just elastically stop at now» and «maybe add some
// acceleration so that you can pan with velocity/inertia». The picture is
// read frame by frame: where each coffee stands along the band, and how
// many nowviews were asked for by then.

describe('a nowview pan stops at now, and a flick coasts', () => {
	const BAND = `${PANE} .nowview-band`;

	/** A coffee every 20 minutes for eight hours back, so the band always holds marks to follow. */
	async function seeded(app) {
		for (let i = 0; i < 24; i++) await app.runLine(`occur coffee -t -${5 + 20 * i}`);
		await app.redraw();
		await app.page.locator(`${BAND} svg.svg [data-field="coffee"][data-at]`).first().waitFor({ state: 'attached' });
		await app.idle();
		await app.spyOnRunner();
		return app.page.locator(BAND).boundingBox();
	}

	/** Record each frame until stopped: whether a pointer is down, each coffee's x inside its stretch, and the nowviews asked. */
	const record = (page) =>
		page.evaluate((band) => {
			const frames = (window.__frames = []);
			let down = false;
			addEventListener('pointerdown', () => (down = true), { capture: true });
			addEventListener('pointerup', () => (down = false), { capture: true });
			const tick = () => {
				const at = {};
				for (const m of document.querySelectorAll(`${band} svg.svg [data-field="coffee"][data-at]`)) {
					const r = m.getBoundingClientRect();
					const x = r.left + r.width / 2;
					const g = m.closest('[data-lo]');
					const [lo, hi] = ['lo', 'hi'].map((k) => new DOMPoint(Number(g.dataset[k]), 0).matrixTransform(g.getScreenCTM()).x);
					if (r.width > 0 && x >= Math.min(lo, hi) - 1 && x <= Math.max(lo, hi) + 1) at[m.dataset.at] = x;
				}
				frames.push({ down, at, asks: (window.__datumSeen ?? []).filter((a) => a[0] === 'nowview').length });
				if (!window.__stopFrames) requestAnimationFrame(tick);
			};
			window.__stopFrames = false;
			return new Promise((resolve) => requestAnimationFrame(() => resolve(tick())));
		}, BAND);

	/** Wait for `n` still frames after the last lift and after every picture asked, then stop recording. */
	const settle = (page, n = 20) =>
		page
			.waitForFunction((n) => {
				const f = window.__frames;
				const up = f.findLastIndex((x) => x.down) + 1;
				const asked = f.findIndex((x) => x.asks === f.at(-1).asks);
				const from = Math.max(up, asked);
				const still = f.slice(-n).every((x, i, all) => i === 0 || Math.abs(shift(all[i - 1], x)) <= 0.5);
				function shift(a, b) {
					const keys = Object.keys(a.at).filter((k) => k in b.at);
					return keys.length ? keys.reduce((s, k) => s + b.at[k] - a.at[k], 0) / keys.length : 0;
				}
				return up > 0 && f.length - from >= n && still;
			}, n)
			.then(() =>
				page.evaluate(() => {
					window.__stopFrames = true;
					return window.__frames;
				})
			);

	/** How far the coffees moved between two frames, along the band: their mean, over those both show. */
	const shift = (a, b) => {
		const keys = Object.keys(a.at).filter((k) => k in b.at);
		return keys.length ? keys.reduce((s, k) => s + b.at[k] - a.at[k], 0) / keys.length : NaN;
	};

	/**
	 * One finger or the mouse through CDP: down at `from`, moved `by` px in
	 * `steps`, lifted `still` ms after the last move. The events carry their
	 * own clock, a frame apart, so the flick is as fast however loaded the
	 * runner is: left to the round trips, a loaded CI box spaced them past
	 * what the page reads as a pointer held still, and nothing coasted
	 * (da-couf7, as da-bwvuu was for the day strip's taps).
	 */
	async function flick(page, from, by, { steps = 6, still = 0, mouse = false } = {}) {
		const cdp = await page.context().newCDPSession(page);
		const t0 = Date.now() / 1000;
		const when = (i) => t0 + (i * 16 + (i > steps ? still : 0)) / 1000;
		const x = (i) => from.x + (by * Math.min(i, steps)) / steps;
		const touch = (type, i) =>
			cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ id: 1, x: x(i), y: from.y }], timestamp: when(i) });
		const mice = { touchStart: 'mousePressed', touchMove: 'mouseMoved', touchEnd: 'mouseReleased' };
		const press = (type, i) =>
			cdp.send('Input.dispatchMouseEvent', { type: mice[type], x: x(i), y: from.y, button: 'left', buttons: type === 'touchEnd' ? 0 : 1, clickCount: 1, timestamp: when(i) });
		const at = mouse ? press : touch;
		try {
			if (mouse) await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: from.x, y: from.y, timestamp: t0 - 0.016 });
			await page.evaluate(noWait);
			await at('touchStart', 0);
			for (let i = 1; i <= steps; i++) await at('touchMove', i);
			await at('touchEnd', steps + 1);
		} finally {
			await cdp.detach();
		}
	}

	// da-w57do, vividn: «yes, it should feel smooth for both»: the mouse's drag coasts as the finger's flick does.
	for (const mouse of [false, true])
		test(`a flick into the past by ${mouse ? 'the mouse' : 'a finger'} keeps the window moving after the lift, then settles, with one nowview asked at the lift`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					const box = await seeded(app);
					// A flick at a time: the first several coast inside what the
					// picture drew past its box (`--surround`) and ask for nothing,
					// and then one would leave it, and asks at its lift for where
					// it comes to rest, which lands as it coasts (da-vnfr3).
					for (let flicks = 1; ; flicks++) {
						await page.evaluate(() => (window.__datumSeen = []));
						await record(page);
						await flick(page, { x: box.x + 40, y: box.y + box.height / 2 }, 160, { mouse });
						const frames = await settle(page);
						const up = frames.findLastIndex((f) => f.down) + 1;
						const after = frames.slice(up - 1);
						const coasted = shift(after[0], after.at(-1));
						assert.ok(coasted > 40, `flick ${flicks}: the window moved on ${coasted}px after the lift`);
						// Over several frames, slowing: a coast, not a jump.
						const steps = after.slice(1).map((f, i) => shift(after[i], f));
						const moving = steps.filter((d) => d > 0.5);
						assert.ok(moving.length >= 5, `flick ${flicks}: the coast moved in ${moving.length} frames: ${steps}`);
						assert.ok(moving[0] > moving.at(-1), `flick ${flicks}: the coast did not slow: ${moving}`);
						// Asked as it is let go, if at all: nothing while it moved or once it stopped.
						const lift = after[Math.min(2, after.length - 1)].asks;
						assert.deepEqual(
							after.slice(2).filter((f) => f.asks !== lift).length,
							0,
							`flick ${flicks}: a nowview asked mid-coast or at rest`
						);
						const asked = frames.at(-1).asks;
						assert.ok(asked <= 1, `flick ${flicks}: ${asked} nowviews asked`);
						if (asked) break;
						assert.ok(flicks < 12, 'twelve flicks back and nothing asked');
					}
					assert.deepEqual(app.errors, []);
				},
				{ ...(mouse ? { viewport: TOUCH.viewport } : TOUCH), prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
			);
		});

	// da-vnfr3, vividn: «it should be able to handle unlimited drag pretty
	// well. Or at least far enough where any amount of flicking and panning
	// and zooming never goes off the edge of visible data». A mood calm for a
	// week: every frame, the band up to now is that block, never a black one.
	test('hard flicks back and forth, pinches out to 12h and flicks there: every frame is drawn data up to now', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.runLine('switch mood calm -t -10080');
				await app.redraw();
				await page.locator(`${BAND} svg.svg rect.block[data-state="calm"]`).first().waitFor({ state: 'attached' });
				await app.idle();
				await app.spyOnRunner();
				const box = await page.locator(BAND).boundingBox();
				const y = box.y + box.height / 2;
				// Each frame: the px of the band before now that no calm covers, and where the band is slid.
				await page.evaluate((band) => {
					const frames = (window.__gaps = []);
					const tick = () => {
						const b = document.querySelector(band);
						const box = b.getBoundingClientRect();
						// Now, where the data's own axis puts it (the stretch it ends or starts,
						// on screen as scaled and slid): the running block stops there. Not the
						// yellow line, which datum draws a gap past it.
						const now = Date.now();
						const xs = [...b.querySelectorAll('svg.svg [data-lo]')]
							.filter((g) => Date.parse(g.dataset.from) <= now && now <= Date.parse(g.dataset.to))
							.map((g) => {
								const [from, to, lo, hi] = [Date.parse(g.dataset.from), Date.parse(g.dataset.to), Number(g.dataset.lo), Number(g.dataset.hi)];
								return new DOMPoint(lo + ((now - from) * (hi - lo)) / (to - from), 0).matrixTransform(g.getScreenCTM()).x;
							});
						const end = Math.max(box.left, Math.min(box.right, ...xs));
						const spans = [...b.querySelectorAll('svg.svg rect.block[data-state="calm"]')]
							.map((r) => r.getBoundingClientRect())
							.map((r) => [Math.max(box.left, r.left), Math.min(end, r.right)])
							.filter(([l, r]) => r > l)
							.sort((p, q) => p[0] - q[0]);
						let [gap, at] = [0, box.left];
						for (const [l, r] of spans) {
							gap += Math.max(0, l - at);
							at = Math.max(at, r);
						}
						gap += Math.max(0, end - at);
						frames.push({ gap, pan: b.style.getPropertyValue('--pan') + b.style.getPropertyValue('--zoom') });
						requestAnimationFrame(tick);
					};
					requestAnimationFrame(tick);
				}, BAND);
				/** Twenty frames unmoved, every command answered, and twenty more. */
				const still = () =>
					page.waitForFunction(() => {
						const f = window.__gaps.slice(-20);
						return f.length === 20 && f.every((x) => x.pan === f[0].pan);
					});
				const hard = (by) => () => flick(page, { x: by > 0 ? box.x + 30 : box.x + box.width - 30, y }, by, { steps: 4 });
				const out = () => pinch(page, box, 300, 100);
				const gestures = [
					...[1, 2, 3, 4].map(() => ['back', hard(300)]),
					...[1, 2, 3].map(() => ['on', hard(-300)]),
					...[1, 2, 3].map(() => ['out', out]),
					...[1, 2].map(() => ['back at 12h', hard(300)]),
					['on at 12h', hard(-300)]
				];
				for (const [i, [name, gesture]] of gestures.entries()) {
					const from = await page.evaluate(() => window.__gaps.length);
					await gesture();
					await still();
					await app.idle();
					await still();
					const gaps = await page.evaluate((from) => window.__gaps.slice(from).map((f) => f.gap), from);
					const worst = Math.max(...gaps);
					assert.ok(worst <= 3, `gesture ${i + 1} (${name}): a frame shows ${worst.toFixed(0)}px of the band before now with no data, ${gaps.filter((g) => g > 3).length} of ${gaps.length} frames`);
				}
				// The control: the gestures went where they say, out to the longest window.
				const last = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
				assert.equal(last[last.indexOf('--history') + 1], '12h');
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('a mouse drag held still before its release stops where it is let go', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				await record(page);
				await flick(page, { x: box.x + 40, y: box.y + box.height / 2 }, 160, { mouse: true, still: 100 });
				const frames = await settle(page);
				const up = frames.findLastIndex((f) => f.down) + 1;
				assert.ok(shift(frames[0], frames[up - 1]) > 100, 'the drag did not move the band');
				const later = frames.slice(up).map((f) => shift(frames[up - 1], f));
				assert.ok(
					later.every((d) => Math.abs(d) <= 0.5),
					`the band moved after the release: ${later.filter((d) => Math.abs(d) > 0.5)}`
				);
				assert.deepEqual(app.errors, []);
			},
			{ viewport: TOUCH.viewport, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-aicu3, vividn: «when panning on the nowview, the boxes surrounding
	// the time axis labels don't travel with the labels, fix that please».
	test('mid-drag, each handle stands on its label as at rest', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				await page.locator(`${PANE} .nowview-handles .handle`).first().waitFor();
				/** Each handle's centre less its label's, and where the labels are along the band. */
				const where = () =>
					page.locator(PANE).evaluate((p) => {
						const mid = (el) => {
							const r = el.getBoundingClientRect();
							return [r.left + r.width / 2, r.top + r.height / 2];
						};
						const labels = [...p.querySelectorAll('svg.svg .tick-label')].filter((l) => getComputedStyle(l).visibility !== 'hidden');
						const off = {};
						for (const h of p.querySelectorAll('.nowview-handles .handle')) {
							const [hx, hy] = mid(h);
							const [lx, ly] = mid(labels.find((l) => l.textContent === h.dataset.at));
							off[h.dataset.at] = [hx - lx, hy - ly, lx];
						}
						return off;
					});
				const rest = await where();
				const y = box.y + box.height / 2;
				await page.mouse.move(box.x + 40, y);
				await page.mouse.down();
				await page.mouse.move(box.x + 160, y, { steps: 6 });
				const during = await where();
				await page.mouse.up();
				const at = Object.keys(rest).filter((k) => k in during);
				assert.ok(at.length >= 2, `handles at rest ${Object.keys(rest)}, mid-drag ${Object.keys(during)}`);
				for (const k of at) {
					assert.ok(during[k][2] - rest[k][2] > 60, `${k}'s label moved ${during[k][2] - rest[k][2]}px`);
					const [dx, dy] = [0, 1].map((i) => Math.abs(during[k][i] - rest[k][i]));
					assert.ok(dx <= 1 && dy <= 1, `${k}'s handle is ${dx}px, ${dy}px off where it stands at rest`);
				}
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('a drag toward the future stops at now, elastically, and springs back to it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const y = box.y + box.height / 2;
				await record(page);
				await page.mouse.move(box.x + box.width - 60, y);
				await page.evaluate(noWait);
				await page.mouse.down();
				await page.mouse.move(box.x + box.width - 160, y, { steps: 8 });
				await page.waitForTimeout(100);
				await page.mouse.up();
				const frames = await settle(page);
				const up = frames.findLastIndex((f) => f.down) + 1;
				// The finger went 100px on; the band gave less than half of it.
				const held = shift(frames[0], frames[up - 1]);
				assert.ok(held < -5 && held > -48, `the band gave ${held}px to a 100px drag past now`);
				// And sprang back: through frames between, to where it was.
				const back = frames.slice(up).map((f) => shift(frames[0], f));
				assert.ok(Math.abs(back.at(-1)) <= 0.5, `it came to rest ${back.at(-1)}px off now`);
				assert.ok(back.some((d) => d > held + 2 && d < -2), `no frame between: ${back}`);
				// On now: no window centred elsewhere asked, and no «now» button.
				const centred = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview' && a.includes('--center'));
				assert.deepEqual(centred, []);
				assert.equal(await page.locator(`${PANE} .nowview-now`).count(), 0);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('a touch mid-coast stops it where it is, and selects nothing', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const y = box.y + box.height / 2;
				const cdp = await page.context().newCDPSession(page);
				const at = (type) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ id: 2, x: box.x + box.width / 2, y }] });
				await record(page);
				await flick(page, { x: box.x + 40, y }, 160);
				// While the coast is measured moving, a finger lands and stays.
				await page.waitForFunction(() => {
					const f = window.__frames;
					const up = f.findLastIndex((x) => x.down) + 1;
					if (!(up > 0 && f.length - up >= 2)) return false;
					const [a, b] = f.slice(-2);
					const keys = Object.keys(a.at).filter((k) => k in b.at);
					return keys.length > 0 && keys.reduce((s, k) => s + b.at[k] - a.at[k], 0) / keys.length > 1;
				});
				await at('touchStart');
				await page.waitForFunction(() => window.__frames.at(-1).down && window.__frames.length >= window.__frames.findLastIndex((x) => !x.down) + 20);
				await at('touchEnd');
				await cdp.detach();
				const frames = await settle(page);
				// The frame the finger landed in, as the page saw it, not when the test heard back.
				const lift = frames.findIndex((f, i) => i > 0 && frames[i - 1].down && !f.down);
				const caught = frames.findIndex((f, i) => i > lift && f.down);
				// Moving when the finger landed, and from the next frame on, still.
				assert.ok(shift(frames[caught - 3], frames[caught - 1]) > 1, 'the coast had stopped before the touch');
				const later = frames.slice(caught + 1).map((f) => shift(frames[caught], f));
				assert.ok(
					later.every((d) => Math.abs(d) <= 0.5),
					`the band moved after the touch: ${later.filter((d) => Math.abs(d) > 0.5)}`
				);
				assert.equal(await page.locator(`${PANE} .nowview-margin .margin-pill`).count(), 0, 'the touch that stopped the coast selected a time');
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-y6jzs, vividn: «It should appear immediately when the now section is
	// off the screen. Pressing the "now->" button at any point, even during
	// acceleration or data redraw should immediately snap back to now and have
	// the data be ready to go».
	test('mid-drag, «now →» is there in every frame now is off the band, and goes in the frame the drag brings it back', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const y = box.y + box.height / 2;
				const cdp = await page.context().newCDPSession(page);
				const at = (type, x) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ id: 3, x, y }] });
				/** After a frame: whether the now line is past the band's end, and the button's words. */
				const look = () =>
					page.evaluate(
						(pane) =>
							new Promise((resolve) =>
								requestAnimationFrame(() => {
									const p = document.querySelector(pane);
									const band = p.querySelector('.nowview-band').getBoundingClientRect();
									const line = p.querySelector('svg.svg .current-time-line').getBoundingClientRect();
									resolve({ off: line.left > band.right, button: p.querySelector('.nowview-now')?.textContent ?? null });
								})
							),
						PANE
					);
				await page.evaluate(noWait);
				const x0 = box.x + 40;
				await at('touchStart', x0);
				const out = [];
				for (let d = 12; d <= 240; d += 12) {
					await at('touchMove', x0 + d);
					out.push({ d, ...(await look()) });
				}
				const back = [];
				for (let d = 228; d >= 0; d -= 12) {
					await at('touchMove', x0 + d);
					back.push({ d, ...(await look()) });
				}
				await at('touchEnd');
				await cdp.detach();
				// Off the band, and the button there in that frame and every one after it, finger still down.
				const left = out.findIndex((f) => f.off);
				assert.ok(left >= 0, `the drag never took now off the band: ${JSON.stringify(out)}`);
				assert.deepEqual(
					out.slice(left).filter((f) => f.button !== 'now →'),
					[],
					'a frame with now off the band and no «now →»'
				);
				// Brought back to where it started, the button goes in that frame.
				assert.equal(back.at(-1).off, false);
				assert.equal(back.at(-1).button, null, `«now» still there with the drag back on now: ${JSON.stringify(back)}`);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('«now →» pressed mid-coast, with the panned window’s picture still out: on now in the next frame, nothing asked, no picture after it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				// Every picture off now is held until the test lets it go: the run «now» leaves behind.
				await page.evaluate(() => {
					window.__held = [];
					window.__centred = 0;
					const previous = window.datum.setRunner((argv, ...rest) => {
						if (argv[0] !== 'nowview' || !argv.includes('--center')) return previous(argv, ...rest);
						window.__centred++;
						return new Promise((resolve) => window.__held.push(() => resolve(previous(argv, ...rest))));
					});
				});
				// Flicks into the past, each let come to rest, until one asks for a window as it lets go; that picture
				// is let land, so the one on screen is off now, and the flicks go on until another is asked.
				await page.locator(`${BAND} svg.svg`).evaluate((svg) => (svg.dataset.first = ''));
				for (let flicks = 1, landed = false; ; flicks++) {
					assert.ok(flicks < 24, 'flick after flick back and nothing asked');
					await flick(page, { x: box.x + 40, y: box.y + box.height / 2 }, 160);
					if (await page.evaluate(() => window.__held.length)) {
						if (landed) break;
						await page.evaluate(() => window.__held.splice(0).forEach((go) => go()));
						await page.waitForFunction((band) => document.querySelector(`${band} svg.svg`)?.dataset.first === undefined, BAND);
						landed = true;
					}
					await page.waitForFunction(
						(band) => {
							const pan = document.querySelector(band).style.getPropertyValue('--pan');
							const s = (window.__rest ??= { pan, n: 0 });
							s.n = s.pan === pan ? s.n + 1 : 0;
							s.pan = pan;
							return s.n >= 10 && !(window.__rest = undefined);
						},
						BAND,
						{ polling: 'raf' }
					);
				}
				// In a frame the coast moves the band, the button is pressed; each frame after it is read, and the held
				// picture let go ten frames on. Not in the first frame after the lift: a picture redraws only a task
				// after a finger leaves it (da-hn2w6, #lib/hover.ts), and no finger is on the button that soon.
				const frames = await page.evaluate(
					(pane) =>
						new Promise((resolve) => {
							const band = document.querySelector(`${pane} .nowview-band`);
							let [was, n, pressed, tag] = [band.style.getPropertyValue('--pan'), 0, -1, 0];
							const read = () => {
								const svg = band.querySelector('svg.svg');
								svg.dataset.seen ??= String(++tag);
								const [b, line] = [band.getBoundingClientRect(), svg.querySelector('.current-time-line')?.getBoundingClientRect()];
								return {
									svg: svg.dataset.seen,
									line: line ? (line.left - b.left) / b.width : null,
									button: !!document.querySelector(`${pane} .nowview-now`),
									centred: window.__centred,
									held: window.__held.length
								};
							};
							const frame = () => {
								n++;
								const pan = band.style.getPropertyValue('--pan');
								if (pressed < 0 && n > 3 && pan !== was) {
									pressed = n;
									frames.push({ ...read(), button: document.querySelector(`${pane} .nowview-now`)?.textContent });
									document.querySelector(`${pane} .nowview-now`).click();
								} else if (pressed > 0) frames.push(read());
								if (n - pressed === 10) window.__held.splice(0).forEach((go) => go());
								was = pan;
								if (pressed > 0 && n - pressed >= 70) resolve(frames);
								else requestAnimationFrame(frame);
							};
							const frames = [];
							requestAnimationFrame(frame);
						}),
					PANE
				);
				await app.idle();
				const [press, next, ...after] = frames;
				assert.equal(press.button, 'now →', 'pressed mid-coast, with now off the band');
				assert.ok(press.held > 0, 'no picture of the panned window was out when «now» was pressed');
				// The next frame: on now, the now line at the band's end, the button gone.
				assert.ok(next.line > 0.6 && next.line < 1, `the now line ${next.line} of the band along, the frame after «now»`);
				assert.equal(next.button, false);
				assert.notEqual(next.svg, press.svg, 'the picture the coast was on');
				// And it stays: no other picture, the held one let go included, and no window off now asked.
				assert.deepEqual(
					after.filter((f) => f.svg !== next.svg || f.line !== next.line),
					[],
					'the picture changed after «now»'
				);
				assert.equal(after.at(-1).centred, press.centred, 'a window off now asked after «now»');
				assert.equal(await page.locator(`${BAND} svg.svg`).getAttribute('data-seen'), next.svg);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});
});

// ── one band of time across a clock change (da-31hhd) ───────────────
//
// vividn, 2026-10-28: «don't show timezone changes in the nowview. Right now
// scrolling back I can see the DST change rendered in the nowview. It should
// just be one continuous band of time». Its axis is instants, so a window over
// 2026-10-25, when Berlin's clock went back at 01:00 UTC, draws the hour once.

describe('the nowview is one band of time across a clock change', () => {
	const BAND = `${PANE} .nowview-band`;

	test('panned back over Berlin’s fall-back: marks spaced by their instants, the state one block, no seam or zone', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// A coffee every 20 minutes either side of the change, and a
				// state running through it and through Berlin's midnight (22:00Z).
				const lines = ['switch mood calm -t 2026-10-24T21:30:00+00:00', 'switch mood ok -t 2026-10-25T02:30:00+00:00'];
				for (let m = 21 * 60 + 10; m < 29 * 60; m += 20) {
					const t = new Date(Date.UTC(2026, 9, 24, 0, m)).toISOString().replace('.000Z', '+00:00');
					lines.push(`occur coffee -t ${t}`);
				}
				for (const line of lines) assert.equal((await app.runLine(line)).code, 0, line);
				await app.redraw();
				await page.locator(`${BAND} svg.svg rect.block[data-state="calm"]`).first().waitFor({ state: 'attached' });
				await app.idle();

				const band = await page.locator(BAND).evaluate((el) => {
					const svg = el.querySelector('svg.svg');
					const box = el.getBoundingClientRect();
					const mid = (r) => (r.left + r.right) / 2;
					const inBox = (r) => r.width > 0 && r.left >= box.left - 0.5 && r.right <= box.right + 0.5;
					const axis = [...svg.querySelectorAll('[data-lo]')].find((g) => !g.closest('.crop'));
					const x = (name) => new DOMPoint(Number(axis.getAttribute(name)), 0).matrixTransform(axis.getScreenCTM()).x;
					const pxPerMs = (x('data-hi') - x('data-lo')) / (Date.parse(axis.getAttribute('data-to')) - Date.parse(axis.getAttribute('data-from')));
					const coffees = {};
					for (const m of svg.querySelectorAll('[data-field="coffee"][data-at]')) {
						const r = m.getBoundingClientRect();
						if (inBox(r)) coffees[m.dataset.at] = mid(r);
					}
					const calm = [...svg.querySelectorAll('.block[data-state="calm"]')]
						.map((b) => b.getBoundingClientRect())
						.filter((r) => r.width > 0)
						.map((r) => [r.left, r.right, Math.round(r.height)]);
					return {
						pxPerMs,
						box: [box.left, box.right],
						coffees: Object.entries(coffees).map(([at, x]) => [Date.parse(at), x]).sort((a, b) => a[0] - b[0]),
						calm: calm.sort((a, b) => a[0] - b[0]),
						zones: svg.querySelectorAll('.zone-label, .dead-zone, .lane').length,
						ticks: [...svg.querySelectorAll('.tick-label')].map((t) => t.textContent)
					};
				});

				// Marks either side of 01:00Z, each the next 20 minutes on by the axis's own scale.
				const change = Date.UTC(2026, 9, 25, 1);
				assert.ok(band.coffees.some(([t]) => t < change) && band.coffees.some(([t]) => t > change), JSON.stringify(band));
				assert.ok(band.coffees.length >= 6, JSON.stringify(band));
				for (let i = 1; i < band.coffees.length; i++) {
					const [[t0, x0], [t1, x1]] = [band.coffees[i - 1], band.coffees[i]];
					const want = (t1 - t0) * band.pxPerMs;
					assert.ok(Math.abs(x1 - x0 - want) < 1, `${new Date(t0).toISOString()} → ${new Date(t1).toISOString()}: ${x1 - x0}px, not ${want}px`);
				}
				// The state is one band, a lane's height, unbroken where the days meet.
				assert.equal(new Set(band.calm.map((c) => c[2])).size, 1, `the state narrows: ${JSON.stringify(band.calm)}`);
				for (let i = 1; i < band.calm.length; i++)
					assert.ok(Math.abs(band.calm[i][0] - band.calm[i - 1][1]) < 1, `a seam in the state: ${JSON.stringify(band.calm)}`);
				// No offset label, dead zone or lane, and no tick said twice.
				assert.equal(band.zones, 0);
				assert.deepEqual(band.ticks, [...new Set(band.ticks)]);
				assert.deepEqual(app.errors, []);
			},
			{
				...DESKTOP,
				timezone: 'Europe/Berlin',
				clock: Date.UTC(2026, 9, 27, 12),
				prepare: (context) => context.addInitScript(draft, soloNowview({ history: '8h', at: '2026-10-25T01:00:00+00:00' }))
			}
		);
	});
});
