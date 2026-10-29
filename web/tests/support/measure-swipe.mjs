// Not a test: what a swipe up on the output section does, frame by frame, on
// a phone (390×844, touch, CPU 4×), for da-6lqa6's bisect.
//
//   [DX=-20] [CPU=4] [BRIEF=1] node tests/support/measure-swipe.mjs
//
// Four commands logged, the bar at the row; then a short pull (60px, held)
// and a fling (200px in six frames, lifted), each driven by CDP touch
// events, drifting DX px sideways. Each frame: how far the bar is open, and
// where each command is drawn; the pointer events the bar heard, each with
// how late the page heard it; the long tasks. BRIEF: one line a gesture.

import { startApp, stopApp, withApp } from './app.mjs';
import { withBar } from './layout.mjs';

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, prepare: withBar };

async function watch(page, act) {
	await page.evaluate(() => {
		const seen = (window.__swipe = { frames: [], events: [], long: [] });
		const shell = document.querySelector('.shell');
		const px = (n) => Math.round(parseFloat(getComputedStyle(shell).getPropertyValue(n)) || 0);
		const on = (e) => seen.events.push({ t: Math.round(e.timeStamp), late: Math.round(performance.now() - e.timeStamp), type: e.type, y: Math.round(e.clientY ?? e.touches?.[0]?.clientY ?? -1) });
		for (const type of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'touchstart', 'touchend', 'touchcancel'])
			addEventListener(type, on, { capture: true, passive: true });
		seen.off = () => {
			for (const type of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'touchstart', 'touchend', 'touchcancel'])
				removeEventListener(type, on, { capture: true });
		};
		new PerformanceObserver((l) => l.getEntries().forEach((e) => seen.long.push([Math.round(e.startTime), Math.round(e.duration)]))).observe({ type: 'longtask' });
		const frame = (t) => {
			if (!window.__swipe) return;
			const ran = [...document.querySelectorAll('.log [data-ran], .prompt-bar [data-ran]')].map((el) => {
				const r = el.getBoundingClientRect();
				return `${el.closest('.log') ? 'L' : 'S'}${el.dataset.ran.slice(-6)}@${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)}`;
			});
			seen.frames.push({ t: Math.round(t), shown: px('--shown'), log: px('--log-height'), ran: ran.join(' | ') });
			requestAnimationFrame(frame);
		};
		requestAnimationFrame(frame);
	});
	const t0 = await page.evaluate(() => performance.now());
	await act();
	await page.waitForTimeout(700);
	const seen = await page.evaluate(() => {
		const s = window.__swipe;
		s.off();
		window.__swipe = null;
		return s;
	});
	return { ...seen, t0: Math.round(t0) };
}

const DX = Number(process.env.DX ?? 0);
const drag = async (cdp, x, y, dy, moves, lift) => {
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
	for (let i = 1; i <= moves; i++) {
		const sent = Date.now();
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: x + (DX * i) / moves, y: y + (dy * i) / moves }] });
		await new Promise((r) => setTimeout(r, Math.max(0, 16 - (Date.now() - sent))));
	}
	if (lift) await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
};

function report(what, s) {
	console.log(`\n== ${what} (t0 ${s.t0})`);
	console.log('events:', s.events.map((e) => `${e.t - s.t0}+${e.late}:${e.type}:${e.y}`).join(' '));
	console.log('long tasks:', JSON.stringify(s.long.map(([t, d]) => [t - s.t0, d])));
	if (process.env.BRIEF) {
		const up = s.events.find((e) => e.type === 'pointerup' || e.type === 'pointercancel');
		const end = s.frames.at(-1);
		const still = s.frames.findIndex((f, i) => s.frames.slice(i).every((g) => g.shown === end.shown && g.log === end.log));
		const strip = (f) => f.ran.split(' | ').filter((r) => r.startsWith('S')).map((r) => r.split('@')[1].split(',')[0]).join(',');
		console.log(`moves ${s.events.filter((e) => e.type === 'pointermove').length}, ${up ? up.type + '@' + (up.t - s.t0) : 'held'}; end shown ${end.shown} log ${end.log}, still from ${s.frames[still].t - s.t0}ms (${up ? s.frames[still].t - up.t : '-'}ms after the lift); strip x ${strip(s.frames[0])} -> ${strip(end)}; long ${JSON.stringify(s.long.map(([t, d]) => d))}`);
		return;
	}
	let last = '';
	for (const f of s.frames) {
		const line = `shown ${f.shown} log ${f.log} :: ${f.ran}`;
		if (line !== last) console.log(`${f.t - s.t0}ms  ${line}`);
		last = line;
	}
}

await startApp();
try {
	await withApp(async (app) => {
		const { page } = app;
		for (const c of ['occur coffee', 'occur tea', 'switch mood happy', 'occur water']) await app.submit(c);
		await app.idle();
		await page.waitForTimeout(500);
		const cdp = await page.context().newCDPSession(page);
		await cdp.send('Emulation.setCPUThrottlingRate', { rate: Number(process.env.CPU ?? 4) });
		const box = await page.locator('.prompt-bar .section.output').boundingBox();
		const x = Math.round(box.x + box.width / 2);
		const y = Math.round(box.y + box.height / 2);
		report('short pull, held', await watch(page, () => drag(cdp, x, y, -60, 6, false)));
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
		await page.waitForTimeout(800);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(800);
		report('fling', await watch(page, () => drag(cdp, x, y, -200, 6, true)));
	}, PHONE);
} finally {
	await stopApp();
}
