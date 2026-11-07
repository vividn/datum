// The nowview widget's HANDLES (da-57vj4): the outermost label of each of
// its two time axes is one (da-b9glf) — the relative axis's far label, and the
// clock's first. Hover it, or press a finger on it, and a wheel of the
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
//
// One of 3 files split from nowview.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/nowview.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, activeTab, choose, chooseRoot, draft, pane, pastTheWindow, settled, swipe } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { PANE, handle, notch, asked, handled, beside, hover, farIs, soloNowview, TOUCH } from './support/nowview.mjs';

before(startApp);
after(stopApp);

// ONE APP for the two tests on the default screen (da-6b3dj, sharedApp in
// support/app.mjs); the one that steps the window does it in a room, the
// step being a session setting of `main`'s nowview. Every other test opens
// its own: a screen, clock, touch or seeded draft of its own, or a reload.
const shared = sharedApp();

/** The open wheel's notches, the longer (up) then the shorter, nearest first, and how clear each is. */
const wheelOf = (page) =>
	page.locator('.nowview-wheel').evaluate((w) => {
		const of = (sign) =>
			[...w.querySelectorAll('.notch')]
				.filter((n) => Math.sign(Number(n.style.getPropertyValue('--k'))) === sign)
				.sort((a, b) => Math.abs(a.style.getPropertyValue('--k')) - Math.abs(b.style.getPropertyValue('--k')))
				.map((n) => [n.dataset.step, Number(getComputedStyle(n).opacity)]);
		return { at: w.querySelector('.notch.held').dataset.at, up: of(1), down: of(-1) };
	});

/** Every nowview widget's settings in a layout tree. */
function nowviewSettings(node, out = []) {
	if (node?.kind === 'nowview') out.push(node.settings);
	for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) {
		nowviewSettings(child.content ?? child, out);
	}
	return out;
}

describe('the nowview widget', () => {
	test('a diamond on each axis, over no label, and no − / +', async () => {
		await shared.solo(async ({ page }) => {
			const where = await beside(page, PANE);
			assert.equal(where.far, '-15m');
			assert.deepEqual(where.wrong, []);
			assert.deepEqual(await handled(page), ['-15m']);
			const clock = await page.locator(`${PANE} .nowview-handles .handle[data-axis="abs"]`).getAttribute('data-at');
			assert.match(clock, /^\d\d:\d\d(:\d\d)?$/);
			assert.equal(await page.locator(`${PANE} .nowview-steps, ${PANE} .step`).count(), 0, 'the − / + are still drawn');
		});
	});

	// da-9hmqg, vividn: «make the outline of the handles a bit higher
	// contrast so that it's more clear they are clickable/draggable». The
	// ratio of what is painted, against the first ancestor that paints, beside
	// the axis rules' `--rule` against the same ground.
	test('a handle is drawn at a clearly higher contrast than a rule', async () => {
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
				const fill = parse(getComputedStyle(h, '::before').backgroundColor).slice(0, 3);
				return { border: ratio(fill, ground), rule: ratio(rule, ground), ground: `rgb(${ground})` };
			});
			assert.ok(m.border >= 2.5 && m.border >= 1.75 * m.rule, `handle ${m.border}:1, rule ${m.rule}:1 on ${m.ground}`);
		});
	});

	test('with the cursor set the far handle counts from it', async () => {
		await withApp(
			async ({ page }) => {
				await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), todayAt(9, 0));
				// Centred: the window's −15m to +15m, and its surroundings on to ±45m.
				await page.waitForFunction(
					(pane) => [...document.querySelectorAll(`${pane} svg.svg .tick-label`)].some((l) => l.textContent === 't+45m'),
					PANE
				);
				const where = await beside(page, PANE);
				assert.equal(where.far, 't-7m30s');
				assert.deepEqual(where.wrong, []);
				assert.deepEqual(await handled(page), ['t-7m30s']);
			},
			{ clock: todayAt(12, 0) }
		);
	});

	test('click the handle of a 30m window, at -15m: the numbers step it to -20m, a 40m window, then -30m, and stay open', async () => {
		await shared.room('steps', async (app) => {
			const { page } = app;
			await beside(page, PANE);
			await hover(page, '-30m');
			// The next step either way clear, those beyond it fading.
			const wheel = await wheelOf(page);
			assert.deepEqual(wheel.up.map(([s]) => s), ['20m', '30m', '45m', '60m']);
			assert.deepEqual(wheel.down.map(([s]) => s), ['10m', '5m', '2m', '1m']);
			for (const side of [wheel.up, wheel.down]) {
				assert.equal(side[0][1], 1, `the next step is not clear: ${JSON.stringify(side)}`);
				assert.ok(side[1][1] < 1 && side[2][1] < side[1][1] && side[3][1] < side[2][1], `the steps beyond do not fade: ${JSON.stringify(side)}`);
			}
			const seen = await app.spyOnRunner();

			await notch(page, '20m').click();
			await asked(page, '40m');
			assert.equal((await wheelOf(page)).at, '-20m', 'the wheel closed or did not move on');
			await notch(page, '30m').click();
			await asked(page, '60m');
			assert.deepEqual((await wheelOf(page)).up.map(([s]) => s), ['45m', '60m', '90m', '2h']);

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

	// vividn, on the clock's handle: «sensible round absolute times that you can
	// pick that scale in a similar fashion. does not have to be powers of two».
	test('on the clock’s 14:45 in a 30m window to 15:00, 14:40 makes the window 40m; the relative numbers end at 30s and 12h', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				assert.equal(await page.locator(`${PANE} .nowview-handles .handle[data-axis="abs"]`).getAttribute('data-at'), '14:45');
				await hover(page, '14:45');
				const wheel = await wheelOf(page);
				assert.deepEqual(wheel.up.map(([s]) => s), ['14:40', '14:30', '14:15', '14:00']);
				assert.deepEqual(wheel.down.map(([s]) => s), ['14:50', '14:55', '14:58', '14:59']);
				await notch(page, '14:40').click();
				await asked(page, '40m');
				await app.idle();
				assert.deepEqual(app.errors, [], 'datum did not take a window off the ladder');
			},
			{ viewport: { width: 460, height: 700 }, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
		// Past the ladder's ends a handle's wheel has no step that way (da-wd6h5).
		for (const [history, far, side] of [
			['24h', '-24h', 'up'],
			['1m', '-1m', 'down']
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
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 0, -68);
				await asked(page, '60m');
				// A swipe's lift takes its notch and puts the wheel away.
				await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-x9mzc, vividn: «Make these a bit bigger font so that you can see
	// them better … offset the times some to the left so they are not
	// obscured by the thumb … right now the transparency doesn't change, so
	// trying to change more than 3 steps becomes invisible».
	test('a wheel turned four steps shows the pick clear and fades from it, in words bigger than 11px', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				const box = await handle(page, '-30m').boundingBox();
				const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
				await hover(page, '-30m');
				// A step's turn is the notches' spacing, as drawn.
				const notchPx = await page.locator('.nowview-wheel').evaluate((w) => {
					const y = (k) => w.querySelector(`.notch[data-k="${k}"]`).getBoundingClientRect().y;
					return y(1) - y(2);
				});
				await page.mouse.down();
				await page.mouse.move(x, y - 4 * notchPx, { steps: 8 });
				await page.locator('.nowview-wheel .notch.picked[data-k="4"]').waitFor();
				const { up, down } = await wheelOf(page);
				const clear = Object.fromEntries([...up, ...down]);
				// -15m's longer steps, as many as show around the pick: picked 60m.
				assert.deepEqual(up.map(([s]) => s), ['20m', '30m', '45m', '60m', '90m', '2h', '3h', '6h']);
				assert.equal(clear['60m'], 1, `the pick is not clear: ${JSON.stringify(up)}`);
				assert.ok(clear['45m'] === 1 && clear['90m'] === 1, `its neighbours are not clear: ${JSON.stringify(up)}`);
				assert.ok(clear['30m'] < 1 && clear['20m'] < clear['30m'], `no fade below the pick: ${JSON.stringify(up)}`);
				assert.ok(clear['2h'] < 1 && clear['3h'] < clear['2h'], `no fade above the pick: ${JSON.stringify(up)}`);
				const size = await page.locator('.nowview-wheel .notch.picked').evaluate((n) => parseFloat(getComputedStyle(n).fontSize));
				assert.ok(size > 11, `the notches' words are ${size}px`);
				await page.mouse.up();
				await page.locator('.nowview-wheel .notch.held[data-at="-60m"]').waitFor();
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('the numbers stand off to the handle’s side, big: a tap opens them, a swipe on them takes one, and a tap on the handle puts them away', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await beside(page, PANE);
				await app.spyOnRunner();
				await handle(page, '-30m').tap();
				await page.locator('.nowview-wheel .notch.held[data-at="-15m"]').waitFor();
				const seen = await page.locator(PANE).evaluate((p) => {
					const h = p.querySelector('.nowview-handles .handle').getBoundingClientRect();
					const all = [...document.querySelectorAll('.nowview-wheel .notch')];
					return { off: all.map((n) => n.getBoundingClientRect().left - h.right), size: parseFloat(getComputedStyle(all[0]).fontSize), n: all.length };
				});
				assert.equal(seen.n, 9);
				assert.ok(seen.off.every((o) => o >= 0), `a number lies over the handle: ${seen.off}`);
				assert.ok(seen.size >= 20, `the numbers' words are ${seen.size}px`);
				// Four either side of the pick, fading slowly outward and none gone
				// (da-n22c5, vividn: «decrease the rate at which it becomes
				// transparent so that one more level in each direction can be seen»).
				for (const side of Object.values(await wheelOf(page)).slice(1)) {
					const clear = side.map(([, o]) => o);
					assert.equal(clear.length, 4, `not four a side: ${JSON.stringify(side)}`);
					assert.ok(clear.every((o, i) => o > 0 && (i === 0 || o < clear[i - 1])), `not fading outward: ${JSON.stringify(side)}`);
					assert.ok(clear[3] >= 0.25 && clear[2] > 0.45, `faded too fast: ${JSON.stringify(side)}`);
				}

				await handle(page, '-30m').tap();
				await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
				await handle(page, '-30m').tap();
				// Up two of them, from the one it stands on.
				const on = await page.locator('.nowview-wheel .notch.held').boundingBox();
				await swipe(page, on.x + on.width / 2, on.y + on.height / 2, 0, -68);
				await asked(page, '60m');
				await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('held without moving, the numbers open before the lift', async () => {
		await withApp(
			async ({ page }) => {
				await beside(page, PANE);
				const box = await handle(page, '-30m').boundingBox();
				const cdp = await page.context().newCDPSession(page);
				const at = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [at] });
				assert.equal(await page.locator('.nowview-wheel').count(), 0, 'open as the finger landed');
				await page.locator('.nowview-wheel .notch.held[data-at="-15m"]').waitFor();
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
				await page.locator('.nowview-wheel .notch.held').waitFor();
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
				await page.mouse.down();
				await page.mouse.move((x + now) / 2, y, { steps: 6 });
				// Under the finger, with no words, and the picture scaled.
				assert.equal(await page.locator('.handle.dragged').textContent(), '');
				await page.mouse.up();
				await asked(page, '60m');
				await handle(page, '-60m').waitFor();

				// And to anywhere: three quarters of the way back is 80m, off the
				// ladder, which datum is asked for as it is.
				const again = await handle(page, '-60m').boundingBox();
				const [x2, y2] = [again.x + again.width / 2, again.y + again.height / 2];
				const now2 = await anchorX(page);
				await page.mouse.move(x2, y2);
				await page.mouse.down();
				await page.mouse.move(now2 - (now2 - x2) * 0.75, y2, { steps: 6 });
				await page.mouse.up();
				await asked(page, '80m');
				await app.idle();
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-2x9eq, vividn: «Let's always keep them where they are when the
	// nowsection is visible. Let's have them scale from the right edge too, so
	// that when you are panned far in the past they still have a nice zoooming
	// effect».
	test('panned an hour back, the diamonds stand where they do on now, and a drag scales about the right edge', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const rel = page.locator(`${PANE} .nowview-handles .handle[data-axis="rel"]`);
				await rel.waitFor();
				const xOf = async () => {
					const b = await rel.boundingBox();
					return b.x + b.width / 2;
				};
				/** The instant at the picture's right edge, by its own axis, and that edge on screen. */
				const rightEdge = () =>
					page.locator(PANE).evaluate((p) => {
						const right = p.querySelector('svg.svg').closest('.picture').getBoundingClientRect().right;
						const g = p.querySelector('svg.svg [data-lo]');
						const [lo, hi] = ['lo', 'hi'].map((k) => new DOMPoint(Number(g.dataset[k]), 0).matrixTransform(g.getScreenCTM()).x);
						const [from, to] = ['from', 'to'].map((k) => Date.parse(g.dataset[k]));
						return { right, at: from + ((right - lo) / (hi - lo)) * (to - from) };
					});
				const panned = await xOf();
				const edge = await rightEdge();
				await app.spyOnRunner();

				// Halfway from the diamond to the right edge halves its distance: twice the window, the edge's instant kept.
				const y = await rel.boundingBox().then((b) => b.y + b.height / 2);
				await page.mouse.move(panned, y);
				await page.mouse.down();
				await page.mouse.move((panned + edge.right) / 2, y, { steps: 6 });
				await page.mouse.up();
				await asked(page, '60m');
				const argv = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
				assert.ok(argv.includes('--center'), `a drag panned back went home: ${argv.join(' ')}`);
				const kept = await until(rightEdge, (e) => Math.abs(e.at - edge.at) <= 15_000);
				assert.ok(Math.abs(kept.at - edge.at) <= 15_000, `the right edge moved ${(kept.at - edge.at) / 1000}s`);
				assert.ok(Math.abs((await xOf()) - panned) <= 1, 'the diamond moved with the window');

				// On now, the diamonds stand where they stood panned back.
				await page.locator(`${PANE} .nowview-now`).click();
				await until(
					() => page.locator(`${PANE} .nowview-now`).count(),
					(n) => n === 0
				);
				await app.idle();
				assert.ok(Math.abs((await xOf()) - panned) <= 1, `on now at ${await xOf()}, panned back at ${panned}`);
				assert.deepEqual(app.errors, []);
			},
			{
				...DESKTOP,
				clock: todayAt(12, 0),
				prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m', at: '11:00' }))
			}
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
				await asked(page, '40m');
				await page.keyboard.press('ArrowUp');
				await asked(page, '60m');
				await page.keyboard.press('ArrowDown');
				await asked(page, '40m');
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
			await notch(page, '30m').click();
			await asked(page, '60m');
			assert.equal(
				await page.locator('.root-bar [role="tab"].modified').count(),
				0,
				'the widget’s own step dirtied the layout'
			);

			await app.reload();
			await farIs(page, '-60m');
			assert.equal(
				await page.locator('.root-bar [role="tab"].modified').count(),
				0,
				'a kept session setting dirtied the layout'
			);

			await choose(page, 'nowview', 'Save options with layout');
			await page.waitForFunction(() => document.querySelector('.root-bar [role="tab"].modified'));

			// Now it is the draft's, and survives a reload on its own.
			await app.reload();
			await farIs(page, '-60m'); // the draft kept the handed-over window

			// The root bar's save icon, on the `main` tab (da-0a5o).
			await chooseRoot(page, 'Save layout');
			await page.waitForFunction(() => !document.querySelector('.root-bar.modified'));
			const other = await app.newTab();
			const saved = await other.page.evaluate(() => window.datum.layouts.loadLayout('main'));
			assert.deepEqual(
				nowviewSettings(saved.root).map((s) => s.history),
				['60m']
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

				// The full window stands BESIDE the collect widget now rather
				// than behind a tab of it (da-hkjp), so there is nothing to
				// tap: it is already drawn.
				const where = await beside(page, `section[data-pane="nowview"]`);
				assert.equal(where.far, '-15m');
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
				await notch(page, '30m').tap();
				await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'nowview' && a.includes('60m')));
				// A finger's wheel stays until a press lands elsewhere.
				assert.equal((await wheelOf(page)).at, '-30m');

				// A sideways drag on the far handle, either way, is the
				// handle's: the window scales, and the pane keeps its tab.
				await beside(page, 'section[data-pane="side"]');
				await pastTheWindow(page);
				const was = (await histories()).length;
				const box = await handle(page, '-60m', 'section[data-pane="side"]').boundingBox();
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 60, 0);
				await page.waitForFunction((n) => window.__datumSeen.filter((a) => a[0] === 'nowview').length > n, was);
				await settled(page);
				await app.idle();
				assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag on a handle moved the tab');
				const scaled = (await histories()).at(-1);
				assert.notEqual(scaled, '60m', 'a drag toward now did not rescale the window');
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
				const box = await notch(page, '20m').boundingBox();
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.mouse.down();
				await page.locator(`${PANE} svg.svg`).evaluate((svg) => (svg.dataset.old = '1'));
				await page.evaluate(() => window.datum.run(['occur', 'coffee']));
				await page.waitForFunction((p) => {
					const svg = document.querySelector(`${p} svg.svg`);
					return svg && !svg.dataset.old;
				}, PANE);
				await page.mouse.up();
				await asked(page, '40m');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP, clock: todayAt(12, 0) }
		);
	});
});
