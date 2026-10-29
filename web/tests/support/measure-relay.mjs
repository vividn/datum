// Not a test: what today's band costs to draw again on a write, on a dense
// day (2,500 points), at phone size and CPU 4x, for da-mi2s6's PR body.
//
//   BACKUP=<a `db backup` file> node tests/support/measure-relay.mjs
//
// The main thread from the command's submit to the page settled, traced
// (`traceOf`), and in the page the time spent in `parseFromString`,
// `serializeToString` and each `getBoundingClientRect` that laid the page
// out (over 2 ms), with where it was called from.

import { readFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { startApp, stopApp, withApp, todayAt, ON_COLLECT } from './app.mjs';
import { traceOf } from './pans.mjs';

const backup = readFileSync(process.env.BACKUP, 'utf8');
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT, clock: todayAt(23, 40) };
const TODAY = '.band[data-days-ago="0"] svg.svg';
const round = (n) => Math.round(n);
const tally = (names) => Object.entries(Object.groupBy(names, (n) => n)).map(([n, all]) => `${all.length} ${n}`).join(', ') || 'none';

await startApp();
try {
	await withApp(async (app) => {
		const { page } = app;
		const restored = await app.run(['db', 'restore', '-'], backup);
		if (restored.code !== 0) throw new Error(restored.stderr);
		await app.reload();
		await page.locator(TODAY).waitFor({ timeout: 600_000 });
		await app.idle();
		await page.waitForTimeout(2000);
		await app.idle();
		console.log(`today's band: ${await page.locator(TODAY).evaluate((svg) => svg.querySelectorAll('[data-state]').length)} marks`);
		await page.evaluate(() => {
			const spent = (window.spent = { parse: [0, 0], serialize: [0, 0], layout: [] });
			const wrap = (proto, name, add) => {
				const was = proto[name];
				proto[name] = function (...args) {
					const t = performance.now();
					try {
						return was.apply(this, args);
					} finally {
						add(performance.now() - t, this);
					}
				};
			};
			wrap(DOMParser.prototype, 'parseFromString', (ms) => ((spent.parse[0] += 1), (spent.parse[1] += ms)));
			wrap(XMLSerializer.prototype, 'serializeToString', (ms) => ((spent.serialize[0] += 1), (spent.serialize[1] += ms)));
			wrap(Element.prototype, 'getBoundingClientRect', (ms, el) => {
				if (ms > 2) spent.layout.push({ ms, el: `${el.localName}.${[...el.classList].join('.')}`, at: new Error().stack.split('\n').slice(2, 6).map((l) => l.trim()).join(' < ') });
			});
		});
		const cdp = await page.context().newCDPSession(page);
		await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
		for (const pass of [1, 2, 3]) {
			await page.evaluate(() => Object.assign(window.spent, { parse: [0, 0], serialize: [0, 0], layout: [] }));
			const trace = await traceOf(page, cdp, async () => {
				await app.runLine(`occur coffee -t ${pass}`);
				await app.redraw();
				await page.waitForFunction((n) => document.querySelectorAll('.band[data-days-ago="0"] svg.svg [data-state]').length >= n, 0);
				await app.idle();
				await page.waitForTimeout(500);
			});
			const spent = await page.evaluate(() => window.spent);
			console.log(
				`write ${pass}: main thread ${round(trace.threadMs)}ms (thread time), ${round(trace.taskMs)}ms (task), script ${round(trace.scriptMs)}ms; ` +
					`longest task ${trace.mostTaskMs}ms, long tasks [${trace.longTasks}]; layouts ${trace.layouts}; ` +
					`parseFromString ${spent.parse[0]}× ${round(spent.parse[1])}ms, serializeToString ${spent.serialize[0]}× ${round(spent.serialize[1])}ms; ` +
					`load ${loadavg()[0].toFixed(0)}\n  laid out from: ${tally(trace.laidOut)}\n  forced: ${spent.layout.map((l) => `${round(l.ms)}ms ${l.el} at ${l.at}`).join('\n          ') || 'none over 2ms'}`
			);
		}
		// A zoom's release: every band on screen laid out again from its held layer (da-syaz8).
		const at = await page.locator('.band[data-days-ago="0"]').evaluate((el) => {
			const box = el.getBoundingClientRect();
			return { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2) };
		});
		for (const [pass, deltaY] of [[1, -300], [2, 300]]) {
			await page.evaluate(() => Object.assign(window.spent, { parse: [0, 0], serialize: [0, 0], layout: [] }));
			const trace = await traceOf(page, cdp, async () => {
				await page.locator('.strip').evaluate((strip, { at, deltaY }) => {
					strip.dispatchEvent(new WheelEvent('wheel', { ctrlKey: true, deltaY, clientX: at.x, clientY: at.y, bubbles: true, cancelable: true }));
				}, { at, deltaY });
				await page.waitForFunction(() => !document.querySelector('.band.stretched'), null, { timeout: 120_000 });
				await page.waitForTimeout(3000);
				await app.idle();
			});
			const spent = await page.evaluate(() => window.spent);
			console.log(
				`zoom ${pass}: main thread ${round(trace.threadMs)}ms (thread time), script ${round(trace.scriptMs)}ms; ` +
					`longest task ${trace.mostTaskMs}ms, long tasks [${trace.longTasks}]; layouts ${trace.layouts}; ` +
					`parseFromString ${spent.parse[0]}× ${round(spent.parse[1])}ms, serializeToString ${spent.serialize[0]}× ${round(spent.serialize[1])}ms` +
					`\n  forced: ${spent.layout.map((l) => `${round(l.ms)}ms ${l.el} at ${l.at.slice(80, 260)}`).join('\n          ') || 'none over 2ms'}`
			);
		}
	}, PHONE);
} finally {
	await stopApp();
}
