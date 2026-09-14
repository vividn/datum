// Measure a burst of cursor clicks on the dayview (da-eoq3).
//
// Ten clicks 50 ms apart across today's band, against thousands of entries
// over three years, in a PERSISTENT profile (see measure-wasi.mjs: an
// incognito OPFS costs ~100x). Reports, per burst: how many `nowview`
// processes started, how many pictures the nowview painted, the time from
// the last click to the final picture, whether that picture is the last
// click's, and what else ran during the burst.
//
// And where each process's time went — acquire / instantiate / run / flush,
// with the acquisitions the burst paid for (da-dvzx): the commands of a
// burst share one acquisition of the OPFS handles, so `acquire` is the phase
// this measures the effect of.
//
// Usage:  pnpm run build && node scripts/measure-burst.mjs [--entries N] [--throttle 4]

import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { preview } from 'vite';

const web = fileURLToPath(new URL('..', import.meta.url));
const flag = (name, fallback) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? fallback : Number(process.argv[i + 1]);
};
const ENTRIES = flag('entries', 20000);
const THROTTLE = flag('throttle', 1);
const BURSTS = 3;

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;

/** Where a set of processes spent their time, and what they paid to acquire. */
const phases = (all) => ({
	processes: all.length,
	acquisitions: all.filter((ms) => ms.tries > 0).length,
	medianMs: Object.fromEntries(
		['acquire', 'instantiate', 'run', 'flush'].map((key) => [
			key,
			Math.round(median(all.map((ms) => ms[key])) * 10) / 10
		])
	),
	totalAcquireMs: Math.round(all.reduce((n, ms) => n + ms.acquire, 0))
});

const server = await preview({ root: web, logLevel: 'warn', preview: { host: '127.0.0.1', port: 0 } });
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
mkdirSync(join(web, '..', 'target'), { recursive: true });
const dir = mkdtempSync(join(web, '..', 'target', 'burst-measure-'));
const context = await chromium.launchPersistentContext(dir, { timezoneId: 'UTC' });

try {
	const page = await context.newPage();
	if (THROTTLE > 1) {
		const cdp = await context.newCDPSession(page);
		await cdp.send('Emulation.setCPUThrottlingRate', { rate: THROTTLE });
	}
	await page.goto(base + '/');
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');

	// The seed: ENTRIES rows over three years ending now, 20 fields, in one
	// `db restore -` under the header the CLI itself writes.
	const seeded = await page.evaluate(async (count) => {
		const backup = await window.datum.runQuiet(['db', 'backup', '-']);
		const header = JSON.parse(backup.stdout.split('\n')[0]);
		const now = Date.now();
		const span = 3 * 365 * 86400_000;
		const lines = [JSON.stringify({ ...header, source_db: 'burst', entry_count: count })];
		for (let i = 0; i < count; i++) {
			const at = new Date(now - span + Math.floor((i * span) / count)).toISOString();
			const field = `f${i % 20}`;
			lines.push(
				JSON.stringify({
					hid: 'burst' + i.toString().padStart(8, '0'),
					slug: `${field}:${at}`,
					content_hash: i.toString(16).padStart(64, '0'),
					field,
					verb: 'point',
					state: i % 7,
					dur: 'MOMENT',
					following_state: null,
					data: null,
					occur_time: { utc: at, offset_minutes: 0, timezone: 'UTC' },
					create_time: at,
					modify_time: at,
					device_id: '01M281T64122KN04Z1VN626W9J',
					id_structure: '%occur_time%',
					raw_input: `occur ${field}`,
					parser_version: 2
				})
			);
		}
		const r = await window.datum.runQuiet(['db', 'restore', '-'], lines.join('\n') + '\n');
		return { code: r.code, stderr: r.stderr, stdout: r.stdout };
	}, ENTRIES);
	if (seeded.code !== 0) throw new Error(`seed: ${seeded.stderr}`);
	await page.reload();
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');
	const TODAY = '.band[data-days-ago="0"] svg.svg';
	const NOWVIEW = 'section.pane[data-title="nowview"] .body';
	await page.locator(TODAY).waitFor();
	await page.locator(`${NOWVIEW} svg.svg`).waitFor();

	// Every command through the seam, and every picture the nowview paints.
	await page.evaluate((scope) => {
		window.__burst = { runs: [], paints: [] };
		const previous = window.datum.setRunner(async (argv, stdin, onStderr) => {
			const run = { argv, t0: performance.now() };
			window.__burst.runs.push(run);
			const result = await previous(argv, stdin, onStderr);
			Object.assign(run, { t1: performance.now(), superseded: !!result.superseded, ms: result.ms });
			return result;
		});
		new MutationObserver((records) => {
			for (const r of records) {
				for (const n of r.addedNodes) {
					if (n.nodeName.toLowerCase() === 'svg') window.__burst.paints.push(performance.now());
				}
			}
		}).observe(document.querySelector(scope), { childList: true, subtree: true });
	}, NOWVIEW);

	const quiet = async () => {
		await page.waitForFunction(() => {
			const last = Math.max(0, ...window.__burst.runs.map((r) => r.t1 ?? Infinity), ...window.__burst.paints);
			return window.__burst.runs.every((r) => r.t1) && performance.now() - last > 1500;
		}, null, { timeout: 120_000, polling: 100 });
	};
	await quiet();

	const reports = [];
	for (let b = 0; b < BURSTS; b++) {
		const burst = await page.evaluate(
			async ([band, b]) => {
				window.__burst.runs.length = 0;
				window.__burst.paints.length = 0;
				const box = document.querySelector(band).getBoundingClientRect();
				const y = box.top + box.height * 0.6;
				let last = 0;
				for (let i = 0; i < 10; i++) {
					const x = box.left + box.width * (0.15 + ((i + b) % 10) * 0.08);
					const target = document.elementFromPoint(x, y);
					const init = { bubbles: true, clientX: x, clientY: y };
					target.dispatchEvent(new PointerEvent('pointerdown', init));
					target.dispatchEvent(new MouseEvent('click', init));
					last = performance.now();
					if (i < 9) await new Promise((r) => setTimeout(r, 50));
				}
				return { last };
			},
			[TODAY, b]
		);
		await quiet();
		const got = await page.evaluate(() => ({
			runs: window.__burst.runs.map(({ argv, t0, t1, superseded, ms }) => ({ argv, t0, t1, superseded, ms })),
			paints: [...window.__burst.paints],
			pill: document.querySelector('.prompt-bar .pill.time')?.firstChild?.textContent ?? null
		}));
		const started = got.runs.filter((r) => !r.superseded);
		const nowviews = started.filter((r) => r.argv[0] === 'nowview');
		const others = {};
		for (const r of started.filter((r) => r.argv[0] !== 'nowview')) others[r.argv[0]] = (others[r.argv[0]] ?? 0) + 1;
		const finalAt = nowviews.at(-1)?.argv[nowviews.at(-1).argv.indexOf('--at') + 1];
		// Where a process's time went, over every process the burst started —
		// not just the nowviews, since they all share the one acquisition.
		const timed = started.filter((r) => r.ms);
		reports.push({
			nowviewRequests: got.runs.filter((r) => r.argv[0] === 'nowview').length,
			nowviewStarted: nowviews.length,
			nowviewPaints: got.paints.length,
			lastClickToFinalPictureMs: Math.round(Math.max(...got.paints) - burst.last),
			medianNowviewWallMs: Math.round(
				nowviews.map((r) => r.t1 - r.t0).sort((a, b) => a - b)[Math.floor(nowviews.length / 2)]
			),
			finalAt,
			pill: got.pill,
			otherCommands: others,
			...phases(timed.map((r) => r.ms))
		});
	}

	// The other shape of burst: jobs that are all in the worker's queue at
	// once — the pane fan-out of one epoch, a hand on the widget bar — which
	// is where an acquisition is shared (da-dvzx). The clicks above are not
	// that: the page keeps one nowview out and one waiting (da-eoq3), so its
	// next request arrives only after the last one's result and the queue
	// drains in between.
	const queued = await page.evaluate(async () => {
		const at = (i) => new Date(Date.now() - i * 60_000).toISOString();
		const t = performance.now();
		const runs = await Promise.all(
			Array.from({ length: 10 }, (_, i) =>
				window.datum.runQuiet(['nowview', '-o', '-', '--width', '400', '--height', '300', '--at', at(i)])
			)
		);
		return { spanMs: Math.round(performance.now() - t), ms: runs.map((r) => r.ms), codes: runs.map((r) => r.code) };
	});

	console.log(
		JSON.stringify(
			{
				entries: ENTRIES,
				throttle: THROTTLE,
				seeded: seeded.stdout,
				bursts: reports,
				queuedAtOnce: { spanMs: queued.spanMs, failures: queued.codes.filter((c) => c !== 0).length, ...phases(queued.ms) }
			},
			null,
			2
		)
	);
} finally {
	await context.close();
	rmSync(dir, { recursive: true, force: true });
	await new Promise((resolve) => server.httpServer.close(resolve));
}
