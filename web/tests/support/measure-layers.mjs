// Not a test: what the day strip and the nowview cost on a real history at
// phone size, for da-m6i2a's PR body.
//
//   BACKUP=<a `db backup` file> node tests/support/measure-layers.mjs
//
// A phone (390×844, CPU 4×) on the collect tab, the backup restored, then:
// 1. a reload, to the strip's days on screen drawn and the page settled;
// 2. a command, to the page settled;
// 3. a scroll up the strip, a fortnight at a time, to the days on screen drawn;
// Each with the `dayview` runs it cost: the strip's and the nowview's days
// (`--pictures`, `--data-layer`, a day's own picture) and their checks, and
// the page's main thread meanwhile, which a busy box stretches less than the
// wall clock.

import { readFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { startApp, stopApp, withApp, ON_COLLECT } from './app.mjs';

const backup = readFileSync(process.env.BACKUP, 'utf8');

/** The dayview runs `argvs` made, as words: what the bands cost. */
const dayviews = (argvs) =>
	argvs
		.filter((a) => a[0] === 'dayview')
		.map((a) => {
			const kind = a.includes('--digests') ? 'check' : a.includes('--pictures') ? 'pictures' : a.includes('--show') ? 'geometry' : 'band';
			const days = a.includes('--pictures') || a.includes('--digests') ? ` of ${a[a.indexOf(a.includes('--pictures') ? '--pictures' : '--digests') + 1].split(',').length}` : '';
			return `${a.includes('--data-layer') ? 'layer ' : ''}${kind}${days}`;
		});
const tally = (words) => Object.entries(Object.groupBy(words, (w) => w)).map(([w, n]) => `${n.length} ${w}`).join(', ') || 'none';

/** Wait until every band on screen in the strip has a picture of the day it is. */
const bandsDrawn = (page) =>
	page.waitForFunction(() => {
		const stack = document.querySelector('.stack');
		if (!stack) return false;
		const box = stack.getBoundingClientRect();
		const seen = [...stack.querySelectorAll('.band')].filter((b) => {
			const at = b.getBoundingClientRect();
			return at.bottom > box.top && at.top < box.bottom;
		});
		return seen.length > 0 && seen.every((b) => b.querySelector('svg.svg'));
	}, null, { timeout: 600_000, polling: 16 });

/** Every `dayview` argv the page sends its database worker, from the boot on, a reload's included. */
const recordBoot = (context) =>
	context.addInitScript(() => {
		window.__booted = [];
		const post = Worker.prototype.postMessage;
		Worker.prototype.postMessage = function (message, ...rest) {
			if (message?.argv?.[0] === 'dayview') window.__booted.push(message.argv);
			return post.call(this, message, ...rest);
		};
	});

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT, prepare: recordBoot };

/** The page's main thread, in ms: its script and all its tasks, so far. */
const mainThread = async (cdp) => {
	const { metrics } = await cdp.send('Performance.getMetrics');
	const of = (name) => metrics.find((m) => m.name === name).value * 1000;
	return { script: of('ScriptDuration'), tasks: of('TaskDuration') };
};

async function timed(app, cdp, what, act, until) {
	await app.page.evaluate(() => (window.__booted = []));
	const seen = () => app.page.evaluate(() => window.__booted);
	const was = await mainThread(cdp);
	const began = Date.now();
	await act();
	await until();
	const drawn = Date.now() - began;
	await app.idle();
	const settled = Date.now() - began;
	const is = await mainThread(cdp);
	const main = `main thread: script ${Math.round(is.script - was.script)}ms, tasks ${Math.round(is.tasks - was.tasks)}ms`;
	console.log(`${what}: drawn ${drawn}ms, settled ${settled}ms; ${main}; load ${loadavg()[0].toFixed(0)}; dayview: ${tally(dayviews(await seen()))}`);
}

await startApp();
try {
	await withApp(
		async (app) => {
			const { page } = app;
			const cdp = await page.context().newCDPSession(page);
			const restored = await app.run(['db', 'restore', '-'], backup);
			if (restored.code !== 0) throw new Error(restored.stderr);
			await app.idle();
			await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
			await cdp.send('Performance.enable');

			// The first open after a restore fills the database's derived
			// tables, whichever build: a reload not measured, then two that are.
			await app.reload();
			await bandsDrawn(page);
			await app.idle();
			for (const n of [1, 2]) await timed(app, cdp, `reload ${n}`, () => app.reload(), () => bandsDrawn(page));
			// To today's band drawn again with the entry in it. The page's clock
			// is held, so each command logs a field of its own: a second coffee
			// at the same instant would change nothing.
			const command = async (line) => {
				await page.evaluate(() => (window.__barLast = window.datum.bar.last()));
				await page.locator('.prompt-bar input').fill(line);
				await page.locator('.prompt-bar input').press('Enter');
				await page.waitForFunction(() => window.datum.bar.last() !== window.__barLast, null, { timeout: 600_000 });
			};
			for (const [n, field] of [[1, 'coffee'], [2, 'tea']]) {
				await page.evaluate(() => (window.__today = document.querySelector('.band[data-days-ago="0"] svg.svg')));
				await timed(app, cdp, `a command ${n}`, () => command(`occur ${field}`), () =>
					page.waitForFunction(() => document.querySelector('.band[data-days-ago="0"] svg.svg') !== window.__today, null, { timeout: 600_000, polling: 16 })
				);
			}
			for (const n of [1, 2, 3])
				await timed(
					app,
					cdp,
					`scroll up, fortnight ${n}`,
					() => page.locator('.stack').evaluate((stack, n) => {
						const row = stack.querySelector(`.band[data-days-ago="${14 * n}"]`)?.closest('.row');
						if (row) stack.scrollTop += row.getBoundingClientRect().top - stack.getBoundingClientRect().top;
					}, n),
					async () => {
						await page.waitForFunction((n) => !!document.querySelector(`.band[data-days-ago="${14 * n}"] svg.svg`), n, { timeout: 600_000, polling: 16 });
						await bandsDrawn(page);
					}
				);
		},
		PHONE
	);
} finally {
	await stopApp();
}
