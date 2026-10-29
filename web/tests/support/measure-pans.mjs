// Not a test: what a finger's drag on the day strip costs, at rest, on a
// real history at phone size, for da-uovtn's PR body.
//
//   BACKUP=<a `db backup` file> node tests/support/measure-pans.mjs
//
// A phone (390×844, touch, CPU 4×) on the collect tab, the backup restored
// and drawn, then, each driven by CDP touch events a frame apart:
// 1. a vertical drag on today's band: the stack's own scroll;
// 2. the strip zoomed in, a horizontal drag on it: the pan of the window;
// 3. the nowview's own pan, the same way.
// Each twice: once traced (`traceOf`), once with its frames watched
// (`framesOf`), so the watching adds nothing to the trace.

import { readFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { startApp, stopApp, withApp, ON_COLLECT } from './app.mjs';
import { framesOf, liftFinger, readFrames, touchDrag, traceOf } from './pans.mjs';

const backup = readFileSync(process.env.BACKUP, 'utf8');
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };
const MOVES = 30;
const TODAY = '.band[data-days-ago="0"]';
const NOWBAND = 'section.pane[data-title="nowview"] .nowview-band';

const centre = (page, selector) =>
	page.locator(selector).first().evaluate((el) => {
		const box = el.getBoundingClientRect();
		const top = Math.max(box.top, 0);
		const bottom = Math.min(box.bottom, innerHeight);
		return { x: Math.round(box.left + box.width / 2), y: Math.round((top + bottom) / 2) };
	});

const tally = (names) => Object.entries(Object.groupBy(names, (n) => n.slice(0, 60))).map(([n, all]) => `${all.length} ${n}`).join(', ') || 'none';
const round = (n) => (Number.isFinite(n) ? Math.round(n) : n);

function say(what, trace, frames) {
	const f = readFrames(frames);
	console.log(
		`${what}: ` +
			`first touchmove → first scroll ${round(f.toScrollMs)}ms, → band moved ${round(f.toBandMs)}ms; ` +
			`band astray of the finger: most ${f.mostStray.toFixed(1)}px over ${f.strays.length} frames; ` +
			`main thread ${round(trace.taskMs)}ms (${round(trace.taskMs / MOVES)}ms a move), script ${round(trace.scriptMs)}ms; ` +
			`longest task ${trace.mostTaskMs}ms, long tasks [${trace.longTasks}]; ` +
			`paints ${trace.paints}, restyles ${trace.restyled.runs} (most ${trace.restyled.most} elements, ${trace.restyled.total} in all), ` +
			`layouts ${trace.layouts}; layerize ${trace.layerize.runs} frames, ${(trace.layerize.ms / (trace.layerize.runs || 1)).toFixed(1)}ms a frame (most ${trace.layerize.most.toFixed(1)}); load ${loadavg()[0].toFixed(0)}\n  painted: ${tally(trace.painted)}\n  laid out from: ${tally(trace.laidOut)}`
	);
}

/**
 * A page at rest that changes one small thing every frame, as a clock tick
 * or the nowview's flow does: what Chrome's Layerize costs a frame then.
 */
async function atRest(page, cdp, what) {
	const trace = await traceOf(page, cdp, () =>
		page.evaluate(
			() =>
				new Promise((done) => {
					const dot = document.body.appendChild(document.createElement('div'));
					dot.style.cssText = 'position:fixed;left:0;top:0;width:4px;height:4px;background:red;will-change:transform';
					let n = 0;
					const step = () => {
						dot.style.translate = `${n % 50}px 0`;
						if (++n < 60) requestAnimationFrame(step);
						else (dot.remove(), done());
					};
					requestAnimationFrame(step);
				})
		)
	);
	console.log(`${what}, at rest, a div moved every frame: layerize ${trace.layerize.runs} frames, ${(trace.layerize.ms / (trace.layerize.runs || 1)).toFixed(1)}ms a frame (most ${trace.layerize.most.toFixed(1)}); main thread ${round(trace.taskMs)}ms; load ${loadavg()[0].toFixed(0)}`);
}

await startApp();
try {
	await withApp(async (app) => {
		const { page } = app;
		const restored = await app.run(['db', 'restore', '-'], backup);
		if (restored.code !== 0) throw new Error(restored.stderr);
		await app.reload();
		await page.locator(`${TODAY} svg.svg`).waitFor({ timeout: 600_000 });
		await app.idle();
		await page.waitForTimeout(2000);
		await app.idle();
		const cdp = await page.context().newCDPSession(page);
		await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
		const bottom = () => page.locator('.stack').evaluate((s) => (s.scrollTop = s.scrollHeight));

		await atRest(page, cdp, 'whole day');

		// 1. Vertical: the finger down the band, the stack back through the days.
		for (const pass of [1, 2]) {
			await bottom();
			await page.waitForTimeout(500);
			const at = await centre(page, TODAY);
			const drag = () => touchDrag(cdp, { x: at.x, y: at.y }, { x: 0, y: 8 }, MOVES);
			const trace = await traceOf(page, cdp, drag);
			await bottom();
			await page.waitForTimeout(500);
			say(`vertical ${pass}`, trace, await framesOf(page, TODAY, drag));
		}
		await bottom();

		// 2. Zoomed in, sideways: the pan.
		const at = await centre(page, TODAY);
		await page.locator('.strip').evaluate((strip, at) => {
			strip.dispatchEvent(new WheelEvent('wheel', { ctrlKey: true, deltaY: -300, clientX: at.x, clientY: at.y, bubbles: true, cancelable: true }));
		}, at);
		await page.waitForFunction(() => !!document.querySelector('.unzoom'));
		await page.waitForFunction(() => !document.querySelector('.band.stretched'));
		await page.waitForTimeout(1500);
		await app.idle();
		await atRest(page, cdp, 'zoomed');
		for (const pass of [1, 2]) {
			const way = pass % 2 ? -1 : 1;
			const from = { x: at.x - way * 100, y: at.y };
			// Traced with the finger down: the lift commits the window, which
			// is the redraw a pan is owed and not a step of it.
			const trace = await traceOf(page, cdp, () => touchDrag(cdp, from, { x: way * 6, y: 0 }, MOVES, { lift: false }));
			await liftFinger(cdp);
			await page.waitForTimeout(1500);
			await app.idle();
			say(`horizontal ${pass}`, trace, await framesOf(page, TODAY, () => touchDrag(cdp, { x: from.x + way * 200, y: at.y }, { x: -way * 6, y: 0 }, MOVES)));
			await page.waitForTimeout(1500);
			await app.idle();
		}

		// 3. The nowview's pan, if the tab shows one.
		if (await page.locator(NOWBAND).count()) {
			for (const pass of [1, 2]) {
				const at = await centre(page, NOWBAND);
				// Inside the band, which is half the phone's width: a press off it
				// is the pane's swipe to the next tab.
				const from = { x: at.x - 60, y: at.y };
				const trace = await traceOf(page, cdp, () => touchDrag(cdp, from, { x: 4, y: 0 }, MOVES, { lift: false }));
				await liftFinger(cdp);
				await page.waitForTimeout(1500);
				await app.idle();
				say(`nowview ${pass}`, trace, await framesOf(page, NOWBAND, () => touchDrag(cdp, { x: from.x + 120, y: at.y }, { x: -4, y: 0 }, MOVES)));
				await page.waitForTimeout(1500);
				await app.idle();
			}
		} else console.log('nowview: not on this tab');
	}, PHONE);
} finally {
	await stopApp();
}
