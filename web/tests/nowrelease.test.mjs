// A release is seamless (da-ygpto). vividn, 2026-10-27: «the momentary
// flashing after letting go of panning or zooming is really jarring. Even if
// it is redrawing it shouldn't dim during this period. It should be
// completely seamless. so there should never be a difference between what is
// seen while still holding your finger down and then when you release».
//
// So a pan, a pinch either way and a handle's drag are recorded frame by
// frame, from under the finger to well after datum's answer for the new
// window has replaced the picture: where each point of a known instant
// stands along the band, where the now strip, its line and its word stand,
// and how opaque everything over them is. The last frame under the finger,
// every frame after the lift and the frames after the answer must all
// agree. The ticks and their labels may sharpen; nothing else may move.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { draft } from './support/layout.mjs';

before(startApp);
after(stopApp);

const PANE = 'section.pane[data-title="nowview"]';
const BAND = `${PANE} .nowview-band`;
const TOUCH = { device: { hasTouch: true, isMobile: true }, viewport: { width: 390, height: 640 } };

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
 * Record every frame until `untilSettled`: the centre x of each point, the
 * ends of the now strip, its line and the middle of its word, the opacity
 * everything over the picture multiplies to, the picture's own element (by
 * a number), the band's ends, and whether the pointer is still down. With
 * `labels`, each tick label on show inside the band too, by its words: its
 * middle along time, and its width.
 */
function record(page, labels = false) {
	return page.evaluate(
		([band, labels]) => {
			const ids = new WeakMap();
			let next = 0;
			const frames = (window.__frames = []);
			let down = true;
			for (const type of ['pointerup', 'touchend', 'mouseup'])
				window.addEventListener(type, () => (down = false), { capture: true });
			const opacity = (el) => {
				let o = 1;
				for (let e = el; e && e instanceof Element; e = e.parentElement) {
					const s = getComputedStyle(e);
					o *= Number(s.opacity);
					if (s.visibility === 'hidden' || s.display === 'none') o = 0;
				}
				return o;
			};
			const tick = () => {
				const svg = document.querySelector(`${band} svg.svg`);
				if (svg && !ids.has(svg)) ids.set(svg, ++next);
				const at = {};
				// A point says its instant (`data-at`), which keys it from one picture to the next.
				// The copy drawn: a picture the now strip cuts in two lays the
				// day's data into each stretch, clipped to it (`compose`).
				for (const mark of svg?.querySelectorAll('[data-field][data-at]') ?? []) {
					const r = mark.getBoundingClientRect();
					const x = r.left + r.width / 2;
					const g = mark.closest('[data-lo]');
					const [lo, hi] = ['lo', 'hi'].map((k) => new DOMPoint(Number(g.dataset[k]), 0).matrixTransform(g.getScreenCTM()).x);
					if (x >= Math.min(lo, hi) - 1 && x <= Math.max(lo, hi) + 1) at[`${mark.dataset.field}@${mark.dataset.at}`] = Math.round(x * 10) / 10;
				}
				// And what stands at now, which every picture of a window on now draws.
				const x = (selector, side) => {
					const r = svg?.querySelector(selector)?.getBoundingClientRect();
					if (r) at[`${selector}${side ?? ''}`] = Math.round((side === '<' ? r.left : side === '>' ? r.right : r.left + r.width / 2) * 10) / 10;
				};
				x('.current-time-line');
				x('.now-label');
				x('.now-strip', '<');
				x('.now-strip', '>');
				const b = document.querySelector(band).getBoundingClientRect();
				const ys = svg?.querySelector('[data-axis]')?.getAttribute('data-axis') === 'y';
				for (const l of labels ? (svg?.querySelectorAll('.tick-label') ?? []) : []) {
					const r = l.getBoundingClientRect();
					const [mid, lo, hi] = ys ? [r.top + r.height / 2, b.top, b.bottom] : [r.left + r.width / 2, b.left, b.right];
					if (getComputedStyle(l).visibility !== 'visible' || mid < lo || mid > hi) continue;
					at[`label ${l.textContent}`] = Math.round(mid * 10) / 10;
					at[`width ${l.textContent}`] = Math.round(r.width * 10) / 10;
				}
				// And each parallel state's block by its start: its left edge, its
				// width, and where its stripes cross its top edge past that, as a
				// phase of their period `p` there (da-rdiph).
				const stripes = {};
				for (const block of svg?.querySelectorAll('rect.block.multi-state[data-from]') ?? []) {
					const pattern = document.getElementById(block.getAttribute('fill').slice('url(#'.length, -1));
					const m = block.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
					const r = block.getBoundingClientRect();
					// The tile's edge `u` along it, where it crosses the block's top.
					const cross = (u) => m.a * u + (m.c * (r.top - m.f - m.b * u)) / m.d + m.e;
					const p = Math.abs(cross(Number(pattern.getAttribute('width'))) - cross(0));
					stripes[block.dataset.from] = { left: r.left, width: r.width, phase: (((cross(0) - r.left) % p) + p) % p, p };
				}
				frames.push({ down, svg: svg ? ids.get(svg) : 0, at, stripes, opacity: svg ? opacity(svg) : 0, band: [b.left, b.right] });
				if (!window.__stopFrames) requestAnimationFrame(tick);
			};
			window.__stopFrames = false;
			// Back once a frame under the finger is in hand: the lift comes after it.
			return new Promise((resolve) =>
				requestAnimationFrame(() => {
					tick();
					resolve();
				})
			);
		},
		[BAND, labels]
	);
}

/**
 * Wait for `n` frames after the lift — and, when the release asked datum for
 * a picture, after that picture has replaced the one under the fingers —
 * then stop recording. The frames, and whether a picture was asked.
 */
async function untilSettled(page, n = 20) {
	const after = (answer) =>
		page.waitForFunction(
			([answer, n]) => {
				const f = window.__frames;
				const up = f.findIndex((x) => !x.down);
				if (up < 1) return false;
				const from = answer ? f.findIndex((x, i) => i > up && x.svg !== f[up - 1].svg) : up;
				return from > 0 && f.length - from >= n;
			},
			[answer, n]
		);
	await after(false);
	const asked = await page.evaluate(() => (window.__datumSeen ?? []).some((a) => a[0] === 'nowview'));
	if (asked)
		await after(true).catch(async (e) => {
			const f = await page.evaluate(() => window.__frames);
			throw new Error(`${e.message}: no new picture in ${f.length} frames, the last ${JSON.stringify(f.at(-1))}`);
		});
	const frames = await page.evaluate(() => {
		window.__stopFrames = true;
		return window.__frames;
	});
	return { frames, asked };
}

/**
 * Each frame from the lift on that differs from the last under the fingers,
 * and how: a mark inside the band moved or went, or the opacity changed.
 * `except` are readings left out.
 */
function seams(frames, except = []) {
	const up = frames.findIndex((f) => !f.down);
	const last = frames[up - 1];
	// A label is kept inside the band as it is recorded, along time.
	const inside = (k) => /^(label|width) /.test(k) || (last.at[k] >= last.band[0] && last.at[k] <= last.band[1]);
	const shown = Object.keys(last.at).filter((k) => !except.includes(k) && inside(k));
	const off = [];
	for (const [i, f] of frames.slice(up).entries()) {
		const moved = shown.filter((k) => f.at[k] === undefined || Math.abs(f.at[k] - last.at[k]) > 0.5);
		if (moved.length || Math.abs(f.opacity - last.opacity) > 1e-3)
			off.push(`frame ${i} after the lift (picture ${f.svg}): ${JSON.stringify(f)} — under the fingers ${JSON.stringify(last)}`);
	}
	return shown.length ? off : ['no mark inside the band under the fingers'];
}

/** CDP touches: `moves` is a list of finger lists, the first one where they land; `each` is awaited after each move. */
async function touch(page, moves, midway, each) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	try {
		await at('touchStart', [moves[0][0]]);
		if (moves[0].length > 1) await at('touchStart', moves[0]);
		for (const m of moves.slice(1)) {
			await at('touchMove', m);
			await each?.();
		}
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

/** A coffee every 20 minutes for eight hours back, so a pan always has a mark in the band. */
async function seeded(app) {
	for (let i = 0; i < 24; i++) await app.runLine(`occur coffee -t -${5 + 20 * i}`);
	await app.redraw();
	await app.page.locator(`${BAND} svg.svg [data-field="coffee"] .point-fg`).first().waitFor({ state: 'attached' });
	await app.idle();
	// Datum's answer for a new window held back a third of a second, as a
	// phone's can be: whatever the page shows meanwhile is on screen for
	// twenty frames, not one.
	await app.page.evaluate(() => {
		const previous = window.datum.setRunner((argv, ...rest) =>
			argv[0] === 'nowview' ? new Promise((r) => setTimeout(r, 333)).then(() => previous(argv, ...rest)) : previous(argv, ...rest)
		);
	});
	await app.spyOnRunner();
	return app.page.locator(BAND).boundingBox();
}

const steps = (n, f) => Array.from({ length: n + 1 }, (_, i) => f(i / n));

describe('a release is seamless', () => {
	test('a pan: the frame after each lift, and after the answer, is the frame under the finger', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const y = box.y + box.height / 2;
				// A width back at a time: the first several stay inside what the
				// picture drew past its box (`--surround`) and ask for nothing,
				// and then one leaves it and asks. The finger is held still
				// before each lift, so it carries no speed (da-v8gzq).
				for (let drag = 1; ; drag++) {
					await page.evaluate(() => (window.__datumSeen = []));
					await touch(page, steps(10, (t) => [{ id: 1, x: box.x + 20 + t * (box.width - 40), y }]), async () => {
						await record(page);
						await page.waitForTimeout(150);
					});
					const { frames, asked } = await untilSettled(page);
					assert.deepEqual(seams(frames), [], `the pan ${drag}`);
					if (asked) break;
					assert.ok(drag < 12, 'twelve widths back and nothing asked');
				}
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// 1.7 times as far apart and back to 0.6: no step of the ladder, so a
	// rounding would show; and out, the strip would narrow under a stretch.
	// Out, about the fingers (da-3zpdx), would bring the time past now in:
	// the window stops at now (da-v8gzq), and the strip stays at its edge.
	for (const [name, from, to] of [
		['in', 80, 136],
		['out', 136, 80]
	])
		test(`a pinch ${name}: the frame after the lift, and after the answer, is the frame under the fingers`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					const box = await seeded(app);
					const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
					const both = (apart) => [
						{ id: 1, x: x - apart / 2, y },
						{ id: 2, x: x + apart / 2, y }
					];
					await touch(page, steps(6, (t) => both(from + t * (to - from))), () => record(page));
					const { frames, asked } = await untilSettled(page);
					assert.ok(asked, 'the pinch asked for nothing');
					assert.deepEqual(seams(frames), []);
					assert.deepEqual(app.errors, []);
				},
				{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
			);
		});

	// da-wd6h5, vividn: «let's not cap the max zoom of the nowview. You
	// should be able to keep zooming in/out as much as you like», and «after
	// zooming out sufficiently far the nowview time axis labels should switch
	// to hours (even eventually days) rather than minutes». Pinched past the
	// ladder's ends, 12h and 30s, a step at a time: each release seamless,
	// each window asked as it is, and datum's picture of it labelled in the
	// unit its majors step by.
	for (const [name, history, from, to, units] of [
		['out past 12h and two days', '12h', 136, 80, ['h', 'h', 'd']],
		['in below 30s', '30s', 80, 136, ['s', 's']]
	])
		test(`a pinch ${name}: each release seamless, and the axis labelled in its unit`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					// A window of seconds holds no coffee: a sip every two, so the band always has a mark.
					if (from < to) for (let s = 2; s <= 40; s += 2) await app.runLine(`occur sip -t -${s}s`);
					const box = await seeded(app);
					const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
					const both = (apart) => [
						{ id: 1, x: x - apart / 2, y },
						{ id: 2, x: x + apart / 2, y }
					];
					const minutes = (h) => Number(h.slice(0, -1)) / (h.endsWith('s') ? 60 : 1) * (h.endsWith('h') ? 60 : 1);
					let was = minutes(history);
					for (const [i, unit] of units.entries()) {
						await page.evaluate(() => (window.__datumSeen = []));
						await touch(page, steps(6, (t) => both(from + t * (to - from))), () => record(page));
						const { frames, asked } = await untilSettled(page);
						assert.ok(asked, `pinch ${i + 1} asked for nothing`);
						assert.deepEqual(seams(frames), [], `pinch ${i + 1}`);
						const last = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
						const now = minutes(last[last.indexOf('--history') + 1]);
						assert.ok(from > to ? now > was * 1.5 : now < was / 1.5, `pinch ${i + 1}: ${was}m to ${now}m`);
						was = now;
						const labels = await page.locator(`${BAND} svg.svg .tick-label`).allTextContents();
						assert.ok(labels.length > 0, `pinch ${i + 1}: no labels at ${now}m`);
						assert.deepEqual([...new Set(labels.map((l) => l.at(-1)))], [unit], `pinch ${i + 1} to ${now}m: ${labels}`);
					}
					assert.ok(from > to ? was > 2880 : was < 0.5, `the pinches ended at ${was}m`);
					assert.deepEqual(app.errors, []);
				},
				{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history })) }
			);
		});

	// vividn, 2026-10-28: «when zooming the nowview, can we correct the time
	// axis labels in realtime? right now after you let go they readjust
	// themselves, and it would be nice for that to be handled dynamically
	// while you are zooming» (da-gy4xz). Pinched a ladder step or more at a
	// time, out past hours and in past seconds, and held still under the
	// fingers until the page has heard datum: the labels on show then are
	// the ones datum's picture of the window draws, word for word and where
	// it draws them, as wide as it draws them — no label moves at the lift
	// or when the picture lands, and none comes or goes.
	for (const [name, history, from, to, orientation] of [
		['out', '30m', 136, 56],
		['in', '12h', 56, 136],
		['out with time running down', '30m', 136, 56, 'vertical']
	])
		test(`a pinch ${name} across the ladder: under the fingers the labels are datum's for the window, and the release moves none`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					const box = await seeded(app);
					// Outermost, so it counts each run from where it is asked.
					await page.evaluate(() => {
						window.__nowviews = 0;
						const previous = window.datum.setRunner((argv, ...rest) => {
							if (argv[0] !== 'nowview') return previous(argv, ...rest);
							window.__nowviews += 1;
							return previous(argv, ...rest).finally(() => (window.__nowviews -= 1));
						});
					});
					const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
					// Along time: down the band when it runs down (da-78evn).
					const both = (apart) =>
						orientation
							? [
									{ id: 1, x, y: y - apart / 2 },
									{ id: 2, x, y: y + apart / 2 }
								]
							: [
									{ id: 1, x: x - apart / 2, y },
									{ id: 2, x: x + apart / 2, y }
								];
					const shown = () =>
						page.locator(`${BAND} svg.svg .tick-label`).evaluateAll((all) => all.filter((l) => getComputedStyle(l).visibility === 'visible').map((l) => l.textContent));
					for (let i = 1; i <= 3; i++) {
						const before = await shown();
						await page.evaluate(() => (window.__datumSeen = []));
						await touch(page, steps(6, (t) => both(from + t * (to - from))), async () => {
							// Held still until nothing is asked: the last answer is the window under the fingers.
							await page.waitForFunction(() => window.__nowviews === 0);
							await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
							assert.equal(await page.evaluate(() => window.__nowviews), 0, `pinch ${i}: still asking once settled`);
							await record(page, true);
						});
						const { frames, asked } = await untilSettled(page);
						assert.ok(asked, `pinch ${i} asked for nothing`);
						const up = frames.findIndex((f) => !f.down);
						const words = (f) => Object.keys(f.at).filter((k) => k.startsWith('label ')).sort();
						const under = words(frames[up - 1]);
						// The control: the labels under the fingers are not the ones the gesture began with.
						assert.ok(under.length > 1, `pinch ${i}: ${under.length} labels under the fingers`);
						assert.notDeepEqual(under, before.map((w) => `label ${w}`).sort(), `pinch ${i}: the labels under the fingers are the stretched picture's`);
						// Time running down, its labels alone: a wider label widens the axis strip, and
						// datum's picture of the window lays every lane further across (da-dz7wb).
						const across = orientation ? Object.keys(frames[up - 1].at).filter((k) => !/^(label|width) /.test(k)) : [];
						assert.deepEqual(seams(frames, across), [], `pinch ${i}`);
						for (const [j, f] of frames.slice(up).entries()) assert.deepEqual(words(f), under, `pinch ${i}, frame ${j} after the lift`);
						assert.ok(frames.at(-1).svg !== frames[up - 1].svg, `pinch ${i}: datum's picture never landed`);
					}
					assert.deepEqual(app.errors, []);
				},
				{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history, ...(orientation && { orientation }) })) }
			);
		});

	// vividn, 2026-10-28: «the multi state stripes also go crazy while you
	// are zooming. they should be better anchored so that the stripes don't
	// move up and down while you are zooming» (da-rdiph). A parallel state
	// well inside the window, pinched in frame by frame: its stripes cross
	// its top edge where they did past its left edge, however far it has
	// stretched, and the release draws them there.
	test('a pinch over a parallel state: its stripes go with its block, and the release draws them where they were', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const line of ['switch posture vertical,sit -t -20', 'switch posture stand -t -12']) await app.runLine(line);
				const box = await seeded(app);
				await page.locator(`${BAND} svg.svg rect.block.multi-state`).waitFor({ state: 'attached' });
				const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
				const both = (apart) => [
					{ id: 1, x: x - apart / 2, y },
					{ id: 2, x: x + apart / 2, y }
				];
				let recording = false;
				const start = async () => {
					if (!recording) await record(page);
					recording = true;
					await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
				};
				await touch(page, steps(12, (t) => both(80 + t * 56)), undefined, start);
				const { frames } = await untilSettled(page);
				const up = frames.findIndex((f) => !f.down);
				const key = Object.keys(frames[up - 1].stripes)[0];
				assert.ok(key, `no parallel state under the fingers: ${JSON.stringify(frames[up - 1])}`);
				const drift = (a, b) => {
					const d = Math.abs(a.phase - b.phase) % a.p;
					return Math.min(d, a.p - d);
				};
				const under = frames.slice(0, up).map((f) => f.stripes[key]);
				// The control: the block stretched and moved, inside the band, under the fingers.
				assert.ok(under.length >= 6, `${under.length} frames under the fingers`);
				assert.ok(under.at(-1).width / under[0].width > 1.4, `the block went from ${under[0].width}px to ${under.at(-1).width}px`);
				assert.ok(under.every((s) => s.left > box.x && s.left + s.width < box.x + box.width), 'the block left the band');
				for (const [i, s] of under.entries())
					assert.ok(drift(s, under[0]) <= 1.5, `frame ${i}: the stripes are ${drift(s, under[0])}px from where they were past the block's edge`);
				const last = under.at(-1);
				for (const [i, f] of frames.slice(up).entries()) {
					const s = f.stripes[key];
					assert.ok(s && Math.abs(s.left - last.left) <= 0.5 && drift(s, last) <= 0.5, `frame ${i} after the lift: ${JSON.stringify(s)}, under the fingers ${JSON.stringify(last)}`);
				}
				assert.deepEqual(seams(frames), []);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// vremya on #1280: a hold that never lets go is worse than a flash. The
	// window moved from outside while the picture is held (the session that
	// carried the pinch's window forgotten): the hold lets go, and the band
	// takes a press again.
	test('a window moved from outside while a pinch waits for its picture: the hold lets go', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const box = await seeded(app);
				const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
				const both = (apart) => [
					{ id: 1, x: x - apart / 2, y },
					{ id: 2, x: x + apart / 2, y }
				];
				await touch(page, steps(6, (t) => both(80 + t * 56)), () => page.evaluate(() => {}));
				assert.notEqual(await page.locator(BAND).evaluate((b) => b.style.getPropertyValue('--zoom')), '1', 'the pinch is not held');
				await page.evaluate(() => window.datum.layout.forgetSession());
				await page.waitForFunction((band) => document.querySelector(band).style.getPropertyValue('--zoom') === '1', BAND);
				await page.locator(`${PANE} .nowview-handles .handle`).first().waitFor();
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	test('a handle dragged: the frame after the lift, and after the answer, is the frame under the pointer', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seeded(app);
				const far = await page.locator(`${PANE} .nowview-handles .handle[data-at="-30m"]`).boundingBox();
				const [x, y] = [far.x + far.width / 2, far.y + far.height / 2];
				await page.mouse.move(x, y);
				await page.mouse.down();
				// Two fifths of the way to now: a window of no step of the ladder.
				const now = await page.locator(`${BAND} svg.svg .current-time-line`).boundingBox();
				await page.mouse.move(x + (now.x - x) * 0.4, y, { steps: 6 });
				await record(page);
				await page.mouse.up();
				const { frames, asked } = await untilSettled(page);
				assert.ok(asked, 'the drag asked for nothing');
				assert.deepEqual(seams(frames), []);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});
});
