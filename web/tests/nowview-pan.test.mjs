// The nowview's pan: it stops at now, and a flick coasts.
// One of 3 files split from nowview.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/nowview.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, draft, pane } from './support/layout.mjs';
import { noWait } from './support/press.mjs';
import { PANE, asked, soloNowview, pinch, TOUCH } from './support/nowview.mjs';

before(startApp);
after(stopApp);

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
	test('hard flicks back and forth, pinches out past 12h and flicks there: every frame is drawn data up to now', async () => {
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
						frames.push({ gap, pan: b.querySelector('.slide')?.style.translate + b.style.getPropertyValue('--zoom') });
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
					...[1, 2].map(() => ['back past 12h', hard(300)]),
					['on past 12h', hard(-300)]
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
				// The control: the gestures went where they say, out past the ladder's 12h (da-wd6h5).
				const last = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
				const window = /^(\d+)m$/.exec(last[last.indexOf('--history') + 1]);
				assert.ok(window && Number(window[1]) > 720, `the last window asked was ${last[last.indexOf('--history') + 1]}`);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// da-4gwp5: the speed a lift carries is read over the 80ms before the LIFT, as the bar's
	// and collect's are (`speedOver`, #lib/grip.ts, da-5rr8f), not before the last move: a
	// finger at rest sends no moves, so one that stopped and held let go at the speed it had
	// stopped from. Over that window a 40ms hold reads about a quarter of the flick's speed
	// (16ms moves: 111px of coast to 416), and a hold past 50ms none; the last move's window
	// read it whole, 522px either way.
	test('a finger flick that stops and holds before the lift coasts far less than one that does not', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const coast = async (still) => {
					await record(page);
					await flick(page, { x: box.x + 40, y: box.y + box.height / 2 }, 160, { still });
					const frames = await settle(page);
					const up = frames.findLastIndex((f) => f.down) + 1;
					return shift(frames[up - 1], frames.at(-1));
				};
				const plain = await coast(0);
				const held = await coast(40);
				assert.ok(plain > 40, `the plain flick moved on ${plain}px after the lift`);
				assert.ok(held < plain / 2, `held 40ms, the window moved on ${held}px after the lift, a plain flick ${plain}px`);
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

	// A handle belongs to the window's end (da-tglm6), not to a label, so a pan
	// moves the picture under it.
	test('mid-drag, each handle stands where it stands at rest', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				await page.locator(`${PANE} .nowview-handles .handle`).first().waitFor();
				const where = () =>
					page.locator(PANE).evaluate((p) =>
						[...p.querySelectorAll('.nowview-handles .handle')].map((h) => {
							const r = h.getBoundingClientRect();
							return [r.left, r.top];
						})
					);
				const rest = await where();
				const y = box.y + box.height / 2;
				await page.mouse.move(box.x + 40, y);
				await page.mouse.down();
				await page.mouse.move(box.x + 160, y, { steps: 6 });
				assert.deepEqual(await where(), rest);
				await page.mouse.up();
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
					// A finger is down until a task after its lift, when #lib/hover.ts lets a redraw through (da-hn2w6):
					// hover's timer, cleared by a press as hover's is, and queued behind it, its listener added after.
					let lifting;
					window.__down = false;
					document.addEventListener('pointerdown', () => (clearTimeout(lifting), (window.__down = true)), true);
					for (const type of ['pointerup', 'pointercancel'])
						document.addEventListener(type, () => (clearTimeout(lifting), (lifting = setTimeout(() => (window.__down = false)))), true);
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
							const pan = document.querySelector(band).querySelector('.slide')?.style.translate;
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
				// picture let go ten frames on. Not before a task after the lift has run: until then hover.ts holds a
				// redraw (da-hn2w6), and a finger cannot be on the button before it is off the picture. The lift
				// reaching the page is not counted in frames: under load it came after the fourth (da-38x0x).
				const frames = await page.evaluate(
					(pane) =>
						new Promise((resolve) => {
							const band = document.querySelector(`${pane} .nowview-band`);
							let [was, n, pressed, tag] = [band.querySelector('.slide')?.style.translate, 0, -1, 0];
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
								const pan = band.querySelector('.slide')?.style.translate;
								if (pressed < 0 && !window.__down && pan !== was) {
									pressed = n;
									frames.push({ ...read(), button: document.querySelector(`${pane} .nowview-now`)?.textContent });
									document.querySelector(`${pane} .nowview-now`).click();
								} else if (pressed > 0) frames.push(read());
								if (pressed > 0 && n - pressed === 10) window.__held.splice(0).forEach((go) => go());
								was = pan;
								if ((pressed > 0 && n - pressed >= 70) || (pressed < 0 && n > 600)) resolve(frames);
								else requestAnimationFrame(frame);
							};
							const frames = [];
							requestAnimationFrame(frame);
						}),
					PANE
				);
				await app.idle();
				const [press, next, ...after] = frames;
				assert.ok(press, 'the coast stopped before the flick’s lift reached the page');
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
