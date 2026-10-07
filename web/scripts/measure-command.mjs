// Measure one command's life in the browser (da-o3sns): each phase of the
// worker's per-command cost, per command word, at rest and right after a
// write, on a database seeded from a backup or synthetically. Then the
// whole page: `occur coffee` typed into the bar, and every command the page
// sends its worker after the Enter, with when it was answered.
//
// Usage:  pnpm run build && node scripts/measure-command.mjs [--backup file.jsonl] [--entries N] [--runs 7] [--incognito]

import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { median } from './median.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const arg = (name) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? undefined : process.argv[i + 1];
};
const BACKUP = arg('backup');
const ENTRIES = Number(arg('entries') ?? 0);
const RUNS = Number(arg('runs') ?? 7);

const COMMANDS = [
	['now', '--json'],
	['fields', '--json'],
	['redo', '--dry-run'],
	['nowview', '-o', '-', '--width', '600', '--height', '300'],
	['tail', '--json', '-n', '30'],
	['dayview', '-o', '-', '--width', '900', '--day-height', '60'],
	['stats', '--month'],
	['--version']
];

/** The synthetic seed: `count` rows over five years, as measure-burst writes them. */
function synthetic(header, count) {
	const now = Date.now();
	const span = 5 * 365 * 86400_000;
	const lines = [JSON.stringify({ ...header, source_db: 'measure', entry_count: count })];
	for (let i = 0; i < count; i++) {
		const at = new Date(now - span + Math.floor((i * span) / count)).toISOString();
		const field = `f${i % 20}`;
		lines.push(
			JSON.stringify({
				hid: 'meas' + i.toString().padStart(8, '0'),
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
				client_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: '%occur_time%',
				raw_input: `occur ${field}`,
				parser_version: 2
			})
		);
	}
	return lines.join('\n') + '\n';
}

const server = await preview({ root: web, logLevel: 'warn', preview: { host: '127.0.0.1', port: 0 } });
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
mkdirSync(join(web, '..', 'target'), { recursive: true });
const dir = mkdtempSync(join(web, '..', 'target', 'command-measure-'));
const INCOGNITO = process.argv.includes('--incognito');
const context = INCOGNITO
	? await (await chromium.launch()).newContext({ timezoneId: 'UTC' })
	: await chromium.launchPersistentContext(dir, { timezoneId: 'UTC' });
const round = (x) => Math.round(x * 10) / 10;
// Every request the page posts its worker, and when it was answered (vremya's perf-queue).
await context.addInitScript(() => {
	const log = (window.__q = []);
	const Base = window.Worker;
	window.Worker = class extends Base {
		constructor(...args) {
			super(...args);
			this.addEventListener('message', (e) => {
				const r = e.data && !e.data.notice && log.find((x) => x.id === e.data.id && x.end === undefined);
				if (r) r.end = performance.now();
			});
		}
		postMessage(m, ...rest) {
			if (m && Array.isArray(m.argv)) log.push({ id: m.id, argv: m.argv.join(' '), start: performance.now() });
			return super.postMessage(m, ...rest);
		}
	};
});

try {
	const page = await context.newPage();
	await page.goto(base + '/');
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');
	if (BACKUP || ENTRIES) {
		const header = JSON.parse((await page.evaluate(() => window.datum.runQuiet(['db', 'backup', '-']))).stdout.split('\n')[0]);
		const stdin = BACKUP ? readFileSync(BACKUP, 'utf8') : synthetic(header, ENTRIES);
		const r = await page.evaluate((stdin) => window.datum.runQuiet(['db', 'restore', '-'], stdin), stdin);
		if (r.code !== 0) throw new Error(`seed: ${r.stderr}`);
		console.error(`seeded: ${r.stdout}`);
	}
	// A blank page, so no pane competes with what is measured.
	await page.goto(base + '/404-measure');
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function').catch(() => {});
	const runner = await page.evaluate(() => typeof window.datum?.runQuiet === 'function');
	if (!runner) {
		await page.goto(base + '/');
		await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');
	}

	const report = {};
	for (const argv of COMMANDS) {
		const runs = [];
		for (let i = 0; i < RUNS; i++) {
			runs.push(
				await page.evaluate(async (argv) => {
					const t = performance.now();
					const r = await window.datum.runQuiet(argv);
					return { wall: performance.now() - t, code: r.code, ms: r.ms };
				}, argv)
			);
		}
		const pick = (f) => round(median(runs.map(f)));
		report[argv.join(' ')] = {
			code: runs[0].code,
			wall: pick((r) => r.wall),
			...Object.fromEntries(['compile', 'acquire', 'instantiate', 'run', 'flush'].map((k) => [k, pick((r) => r.ms?.[k] ?? 0)]))
		};
	}
	console.log(JSON.stringify(report, null, 1));

	// The whole page, after one write typed into the bar.
	await page.setViewportSize({ width: 1280, height: 800 });
	await page.goto(base + '/');
	const input = page.locator('.prompt-bar input:not([disabled])');
	await input.waitFor({ timeout: 120_000 });
	const settle = () =>
		page.waitForFunction(
			() => {
				const last = Math.max(0, ...window.__q.map((x) => x.end ?? Infinity));
				return window.__q.every((x) => x.end !== undefined) && performance.now() - last > 3000;
			},
			null,
			{ timeout: 300_000, polling: 100 }
		);
	for (let i = 0; i < 3; i++) {
		await settle();
		await input.fill('occur coffee');
		const t0 = await page.evaluate(() => performance.now());
		await input.press('Enter');
		await settle();
		const rows = await page.evaluate((t0) => window.__q.filter((x) => x.start >= t0), t0);
		console.log(`\n== occur coffee #${i + 1}: ${rows.length} commands`);
		for (const r of rows)
			console.log(`  sent +${String(Math.round(r.start - t0)).padStart(5)}  answered +${String(Math.round(r.end - t0)).padStart(5)}  ${r.argv.slice(0, 70)}`);
	}
} finally {
	await context.close();
	await context.browser()?.close();
	rmSync(dir, { recursive: true, force: true });
	await new Promise((resolve) => server.httpServer.close(resolve));
}
