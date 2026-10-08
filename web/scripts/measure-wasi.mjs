// Measure the real CLI under WASI — datum.day's runner since da-g8o8.
//
// Latency (cold, warm at 500 entries, CPU-throttled), size, OPFS across a
// reload, two tabs writing at once, and stdout parity with the native CLI.
// Prints a report; `--json <file>` also writes the raw numbers.
//
// Profiles are PERSISTENT and on disk (under target/): Playwright's default
// contexts are incognito, where Chromium keeps OPFS in memory behind IPC and
// a sync access handle read costs ~100x what it costs a real profile. That
// case is measured too, as "incognito", because a private window is real.
//
// Usage:  pnpm run build && node scripts/measure-wasi.mjs [--only a,b] [--json out]
//         (parity needs a native binary: target/release/datum, or DATUM_BIN)

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { median } from './median.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const repo = join(web, '..');
const datum = process.env.DATUM_BIN ?? join(repo, 'target', 'release', 'datum');
const SEED = 500;
const report = {};

const server = await preview({
	root: web,
	logLevel: 'warn',
	preview: { host: '127.0.0.1', port: 0 }
});
const base = server.resolvedUrls.local[0].replace(/\/$/, '');

const round = (x) => Math.round(x * 10) / 10;

/** A browser with a profile of its own; `incognito` for Playwright's default. */
async function browser({ incognito = false, throttle = 1 } = {}) {
	let context;
	let dir = null;
	if (incognito) {
		const b = await chromium.launch();
		context = await b.newContext({ timezoneId: 'UTC' });
		context.once('close', () => b.close());
	} else {
		dir = mkdtempSync(join(repo, 'target', 'wasi-measure-'));
		context = await chromium.launchPersistentContext(dir, { timezoneId: 'UTC' });
	}
	const tab = async (query = '', init, path = '/') => {
		const page = await context.newPage();
		if (init) await page.addInitScript(init);
		if (throttle > 1) {
			const cdp = await context.newCDPSession(page);
			await cdp.send('Emulation.setCPUThrottlingRate', { rate: throttle });
		}
		await page.goto(`${base}${path}?${query}`);
		await page.waitForFunction(() => window.datum);
		return page;
	};
	const close = async () => {
		await context.close();
		if (dir) rmSync(dir, { recursive: true, force: true });
	};
	return { context, tab, close };
}

/**
 * One command through the seam, with the page's own wall clock around it.
 *
 * The quiet door (da-6918): what is being measured is the process, so the
 * refresh a loud `datum.run` would fold into the same await — three more
 * commands and every pane re-drawn — must stay outside the clock.
 */
function run(page, argv) {
	return page.evaluate(async (argv) => {
		const t = performance.now();
		const result = await window.datum.runQuiet(argv);
		return { ...result, wall: performance.now() - t, at: performance.now() };
	}, argv);
}

const hhmm = (i) => String(Math.floor(i / 60)).padStart(2, '0') + String(i % 60).padStart(2, '0');

/** Cold start, then warm per-command latency at SEED entries. */
async function latency(options) {
	const b = await browser(options);
	try {
		// The bare cold start: the first process a worker ever runs, on a
		// route with no panes to have compiled the module already.
		const bare = await b.tab('', null, '/now');
		const firstProcess = await run(bare, ['occur', 'coffee', '-t', '0000']);
		await bare.close();
		// The day frame's: its panes run dayview and nowview on mount, so the
		// first command a person types queues behind them.
		const page = await b.tab();
		const first = await run(page, ['occur', 'coffee']);
		const seeding = [];
		for (let i = 0; i < SEED; i++) {
			const r = await run(page, ['occur', `f${i % 25}`, '-t', hhmm(i)]);
			if (r.code !== 0) throw new Error(`seed ${i}: ${r.stderr}`);
			seeding.push(r.wall);
		}
		const warm = {};
		for (const argv of [
			['occur', 'coffee'],
			['tail'],
			['dayview', '--width', '800', '--height', '200'],
			['nowview', '-o', '-', '--width', '400', '--height', '300']
		]) {
			const rs = [];
			for (let i = 0; i < 10; i++) rs.push(await run(page, argv));
			warm[argv[0]] = {
				wall: round(median(rs.map((r) => r.wall))),
				run: round(median(rs.map((r) => r.ms.run))),
				instantiate: round(median(rs.map((r) => r.ms.instantiate)))
			};
		}
		return {
			coldBare: { wall: round(firstProcess.wall), ms: firstProcess.ms },
			cold: {
				firstOccurDoneAtMsAfterNavigation: round(first.at),
				wall: round(first.wall),
				ms: first.ms,
				code: first.code
			},
			seedOccurWall: {
				first50: round(median(seeding.slice(0, 50))),
				last50: round(median(seeding.slice(-50)))
			},
			warm
		};
	} finally {
		await b.close();
	}
}

/** Writes survive a reload, and `datum check` passes after hundreds of them. */
async function durability() {
	const b = await browser();
	try {
		let page = await b.tab();
		for (let i = 0; i < 120; i++) await run(page, ['occur', `d${i % 7}`, '-t', hhmm(i)]);
		const before = await run(page, ['check']);
		await page.reload();
		await page.waitForFunction(() => window.datum);
		const tail = await run(page, ['tail', '-n', '200']);
		const after = await run(page, ['check']);
		const rows = tail.stdout.split('\n').filter((l) => / d\d /.test(l.replace(/\x1b\[[0-9;]*m/g, '')));
		return {
			storage: tail.storage,
			checkBeforeReload: { code: before.code, summary: before.stdout.split('\n').pop() },
			rowsAfterReload: rows.length,
			checkAfterReload: { code: after.code, summary: after.stdout.split('\n').pop() }
		};
	} finally {
		await b.close();
	}
}

/** Two tabs logging as fast as `run()` allows. */
async function twoTabs() {
	const b = await browser();
	try {
		const [a, c] = [await b.tab(), await b.tab()];
		// Both tabs start cold; warm them so the race is about the files.
		await Promise.all([run(a, ['--version']), run(c, ['--version'])]);
		const t = Date.now();
		const burst = (page, name) =>
			page.evaluate(async (name) => {
				const out = [];
				for (let i = 0; i < 20; i++) {
					const t = performance.now();
					const r = await window.datum.runQuiet(['occur', name, `i=${i}`]);
					out.push({ code: r.code, wall: performance.now() - t, tries: r.ms.tries, acquire: r.ms.acquire });
				}
				return out;
			}, name);
		const [ra, rc] = await Promise.all([burst(a, 'taba'), burst(c, 'tabb')]);
		const total = Date.now() - t;
		const all = [...ra, ...rc];
		const count = async (field) =>
			(await run(a, ['tail', field, '-n', '100'])).stdout
				.split('\n')
				.filter((l) => l.replace(/\x1b\[[0-9;]*m/g, '').includes(field)).length;
		const result = {
			totalMs: total,
			wall: { median: round(median(all.map((r) => r.wall))), max: round(Math.max(...all.map((r) => r.wall))) },
			acquire: { median: round(median(all.map((r) => r.acquire))), max: round(Math.max(...all.map((r) => r.acquire))) },
			maxTries: Math.max(...all.map((r) => r.tries)),
			failures: all.filter((r) => r.code !== 0).length,
			rows: { taba: await count('taba'), tabb: await count('tabb') },
			check: (await run(c, ['check'])).code
		};
		// One tab goes away; how long until the other is served?
		await a.close();
		const next = await run(c, ['occur', 'after-close']);
		result.afterFirstTabCloses = { code: next.code, wall: round(next.wall), tries: next.ms.tries };
		return result;
	} finally {
		await b.close();
	}
}

/** The same argv, in the browser and through the native binary, byte for byte. */
async function parity() {
	const script = [
		['occur', 'coffee', '-t', '0915'],
		['start', 'work', '-t', '0800'],
		['stop', 'work', '-t', '1200'],
		['switch', 'mood', 'happy', '-t', '0930'],
		['note', 'journal', 'text=a quiet morning', '-t', '1000'],
		['add', 'pushups', 'reps=10', '-t', '1100'],
		['switch', 'mood', 'tired', '-T'],
		['alias', 'set', 'cof', 'occur coffee'],
		['alias', 'list'],
		['cof', '-t', '1400'],
		['get', 'journal:<today>T10:00:00.000Z'],
		['tail'],
		['tail', '-n', '3'],
		['dayview', '--width', '600', '--height', '200'],
		['nowview', '-o', '-', '--width', '400', '--height', '300'],
		['check'],
		['spec', 'set', 'coffee', '--color', '#884400'],
		['nosuch']
	];
	const today = new Date().toISOString().slice(0, 10);
	const argvs = script.map((argv) => argv.map((a) => a.replace('<today>', today)));

	const home = mkdtempSync(join(tmpdir(), 'datum-parity-'));
	const native = argvs.map((argv) => {
		try {
			const stdout = execFileSync(datum, argv, {
				env: {
					PATH: process.env.PATH,
					HOME: home,
					DATUM_DB: join(home, 'datum.db'),
					DATUM_CLIENT_ID: 'web-parity',
					FORCE_COLOR: '1',
					TZ: 'UTC'
				},
				encoding: 'utf8',
				stdio: ['ignore', 'pipe', 'pipe']
			});
			return { code: 0, stdout };
		} catch (e) {
			return { code: e.status ?? 1, stdout: e.stdout ?? '' };
		}
	});
	rmSync(home, { recursive: true, force: true });

	const b = await browser();
	let browserSide;
	try {
		const page = await b.tab('', () => localStorage.setItem('datum-client-id', 'web-parity'));
		browserSide = [];
		for (const argv of argvs) browserSide.push(await run(page, argv));
	} finally {
		await b.close();
	}

	// hids are random and clocks move: number each hid by first appearance,
	// and mask the instants a command stamps with "now".
	const masker = () => {
		const seen = new Map();
		return (text) =>
			text
				.replace(/(?:CREATE|EXISTS|UPDATE)\x1b\[0m \x1b\[\d+m([0-9a-z]{5})|\b([0-9a-z]{5})$/gm, (m, a, b) => {
					const hid = a ?? b;
					if (!seen.has(hid)) seen.set(hid, `<hid${seen.size}>`);
					return m.replace(hid, seen.get(hid));
				})
				.replace(/c\d{8}T\d{6}\.\d{3}Z|\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z(?=\x1b|\s|$)/g, (t) =>
					t.endsWith(':00.000Z') ? t : '<now>'
				)
				// A timeless entry's display clock is its create time.
				.replace(/(\x1b\[90mc\x1b\[0m)\d\d:\d\d:\d\d/g, '$1<clock>')
				.replace(/\n+$/, '');
	};
	const [mn, mb] = [masker(), masker()];
	return argvs.map((argv, i) => {
		const n = { code: native[i].code, stdout: mn(native[i].stdout) };
		const w = { code: browserSide[i].code, stdout: mb(browserSide[i].stdout) };
		const same = n.code === w.code && n.stdout === w.stdout;
		return same ? { argv, same } : { argv, same, native: n, wasi: w };
	});
}

function sizes() {
	const wasm = readFileSync(join(web, 'src/lib/wasm/datum-cli.wasm'));
	// Everything the build ships, by kind: what a first visit downloads.
	const files = readdirSync(join(web, 'build'), { recursive: true, withFileTypes: true })
		.filter((f) => f.isFile() && /\.(js|wasm|css|html)$/.test(f.name))
		.map((f) => readFileSync(join(f.parentPath, f.name)));
	const total = (list) => ({
		raw: list.reduce((n, b) => n + b.length, 0),
		gzip: list.reduce((n, b) => n + gzipSync(b).length, 0)
	});
	return {
		cliWasm: { raw: wasm.length, gzip: gzipSync(wasm).length },
		build: total(files)
	};
}

const sections = {
	size: sizes,
	latency: () => latency(),
	latencyThrottled4x: () => latency({ throttle: 4 }),
	latencyIncognito: () => latency({ incognito: true }),
	durability,
	twoTabs,
	parity
};
// `--only parity,size` runs just those.
const only = process.argv.indexOf('--only');
const wanted = only > 0 ? process.argv[only + 1].split(',') : Object.keys(sections);
try {
	for (const name of wanted) report[name] = await sections[name]();
} finally {
	server.httpServer.close();
}

console.log(JSON.stringify(report, null, 2));
const json = process.argv.indexOf('--json');
if (json > 0) writeFileSync(process.argv[json + 1], JSON.stringify(report, null, 2));
