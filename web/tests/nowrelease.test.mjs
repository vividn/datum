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
 * a number), the band's ends, and whether the pointer is still down.
 */
function record(page) {
	return page.evaluate(
		(band) => {
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
				frames.push({ down, svg: svg ? ids.get(svg) : 0, at, opacity: svg ? opacity(svg) : 0, band: [b.left, b.right] });
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
		BAND
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
	const shown = Object.keys(last.at).filter((k) => !except.includes(k) && last.at[k] >= last.band[0] && last.at[k] <= last.band[1]);
	const off = [];
	for (const [i, f] of frames.slice(up).entries()) {
		const moved = shown.filter((k) => f.at[k] === undefined || Math.abs(f.at[k] - last.at[k]) > 0.5);
		if (moved.length || Math.abs(f.opacity - last.opacity) > 1e-3)
			off.push(`frame ${i} after the lift (picture ${f.svg}): ${JSON.stringify(f)} — under the fingers ${JSON.stringify(last)}`);
	}
	return shown.length ? off : ['no mark inside the band under the fingers'];
}

/** CDP touches: `moves` is a list of finger lists, the first one where they land. */
async function touch(page, moves, midway) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	try {
		await at('touchStart', [moves[0][0]]);
		if (moves[0].length > 1) await at('touchStart', moves[0]);
		for (const m of moves.slice(1)) await at('touchMove', m);
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
