// Where a datum.day command's time goes, and what a persistent CLI instance
// would save (da-zv6hn). vividn, 2026-11-03: «every separate widget has to
// reopen a wasm connection all the time right? … can we just have a persistent
// wasm connection that everyone shares?»
//
// It replays one tap's processes, as measure-log.mjs recorded them at the
// worker's door (`--json`, each command's `request`: argv, env, stdin), on the
// same 290k fixture, through the app's own database worker in a blank page:
// no layout, no panes, no sync leader, so each process is alone in the queue.
//
//  1. ALONE: each process `--runs` times, timed by the worker (compile,
//     acquire, instantiate, run, flush) and inside `run` by the CLI itself,
//     which needs the measurement patch (measure-persist.patch: `DATUM_PHASES=1`
//     prints the marks it set: each `Db::open` and its migrate steps, the
//     parse, the dispatch, the exit).
//  2. PERSISTENT: the same processes as lines of ONE process under
//     `DATUM_LINES=1` (the patch's loop: one `Db` open, each line parsed and
//     dispatched as the REPL does), `--runs` passes of the tap, each line timed.
//     A line that reads stdin is left out: the loop's stdin is its lines.
//  3. UNCHANGED: each read before and after the tap's write, at one frozen
//     instant, its stdout compared: what a pane skipped on unchanged output
//     would save, at best.
//  4. HANDLES: which OPFS sync access handle modes this Chromium lets two
//     workers hold on one file at once (the parallel-reads question).
//
// Usage:  (from the repo root) git apply web/scripts/measure-persist.patch, then in web/:
//         pnpm run build
//         node scripts/measure-log.mjs --layout desktop --presses 4 --json ../target/tap.json
//         node scripts/measure-persist.mjs --tap ../target/tap.json [--runs 7] [--json out]

import { closeSync, mkdtempSync, openSync, readFileSync, readSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { median } from './median.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const repo = join(web, '..');
const arg = (name, fallback) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? fallback : process.argv[i + 1];
};
const RUNS = Number(arg('runs', 7));
const TAP = arg('tap');
const JSON_OUT = arg('json', null);
const PARTS = arg('parts', '1,2,3,4').split(',').map(Number);
const ONLY = arg('only', null);
const ZONE = 'Europe/Berlin';
const FROZEN = 'DATUM_WEB_FROZEN_AT=';
const round = (x) => (x == null ? null : Math.round(x * 10) / 10);
const log = (...a) => console.log(...a);
if (!TAP) throw new Error('--tap <measure-log --json file> is required');

// The fixture measure-log built: its newest, B the page's copy.
const fixtures = join(repo, 'target', 'measure-log');
const newest = readdirSync(fixtures)
	.map((d) => join(fixtures, d, 'meta.json'))
	.filter((f) => statSync(f, { throwIfNoEntry: false }))
	.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
const meta = JSON.parse(readFileSync(newest, 'utf8'));

// The last tap's processes, in the order sent; a dial's are sync-worker runs too.
const tap = JSON.parse(readFileSync(TAP, 'utf8'));
const recorded = tap.presses.at(-1).commands.map((c) => c.request).filter((r) => r?.argv);
const requests = recorded.map((r) => ({ ...r, env: (r.env ?? []).filter((v) => !v.startsWith(FROZEN)) }));
const label = (r) =>
	r.argv
		.filter((w, i) => !['--db', '--as'].includes(w) && !['--db', '--as'].includes(r.argv[i - 1]))
		.join(' ')
		.slice(0, 48);
const writes = (r) => !r.pane && !r.session && /^(start|stop|end|switch|moment|occur)$/.test(r.argv.find((w) => !w.startsWith('-')) ?? '');
log(`replaying ${requests.length} processes of tap ${tap.presses.length} (${tap.layout}), ${RUNS} runs, on ${meta.entries} entries`);

// The worker the app builds, served by the preview at its own path.
const built = join(web, '.svelte-kit', 'output', 'client', '_app', 'immutable', 'workers');
const workerFile = readdirSync(built).find((f) => /^db\.worker-.*\.js$/.test(f));
if (!workerFile) throw new Error(`no db.worker in ${built}: pnpm run build`);

const server = await preview({ root: web, logLevel: 'warn', preview: { host: '127.0.0.1', port: 0 } });
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
const profile = mkdtempSync(join(repo, 'target', 'measure-persist-profile-'));
const context = await chromium.launchPersistentContext(profile, { timezoneId: ZONE });
// Isolated as the app is (vite.config.ts, service-worker.ts), so the probes see what it sees.
const ISOLATION = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
await context.route(`${base}/__blank`, (r) => r.fulfill({ contentType: 'text/html', headers: ISOLATION, body: '<!doctype html><title>measure</title>' }));
const CHUNK = 64 << 20;
const seedSize = statSync(meta.b).size;
await context.route(`${base}/__seed/*`, (r) => {
	const at = Number(r.request().url().split('/').pop()) * CHUNK;
	const body = Buffer.alloc(Math.min(CHUNK, seedSize - at));
	const fd = openSync(meta.b, 'r');
	readSync(fd, body, 0, body.length, at);
	closeSync(fd);
	return r.fulfill({ body, contentType: 'application/octet-stream' });
});

const page = await context.newPage();
await page.goto(`${base}/__blank`);
// Each request's row of part 1, by the request: `--only` measures some of them.
const aloneOf = new Map();
const report = { entries: meta.entries, layout: tap.layout, runs: RUNS, load: (await import('node:os')).loadavg()[0] };

/** B's file into OPFS afresh, so every part starts from the fixture. */
async function seed() {
	const t = Date.now();
	await page.evaluate(
		async ([chunk, size]) => {
			const root = await navigator.storage.getDirectory();
			const dir = await root.getDirectoryHandle('datum-cli', { create: true });
			for await (const name of dir.keys()) await dir.removeEntry(name);
			const out = await (await dir.getFileHandle('datum.db', { create: true })).createWritable();
			for (let i = 0; i * chunk < size; i++) await out.write(await (await fetch(`/__seed/${i}`)).arrayBuffer());
			await out.close();
		},
		[CHUNK, seedSize]
	);
	log(`  seeded OPFS in ${Math.round((Date.now() - t) / 1000)} s`);
}

/** A database worker of the app's own, and `window.__run(request)` through it. */
const startWorker = (url) =>
	page.evaluate(async (url) => {
		window.__worker?.terminate();
		const worker = (window.__worker = new Worker(url, { type: 'module' }));
		const waiting = new Map();
		let id = 0;
		worker.onmessage = ({ data }) => {
			if (data?.notice) return;
			waiting.get(data.id)?.(data);
			waiting.delete(data.id);
		};
		window.__run = (request) =>
			new Promise((resolve) => {
				const at = performance.now();
				waiting.set(++id, (data) => resolve({ wall: performance.now() - at, ...data }));
				worker.postMessage({ op: 'cli', ...request, id });
			});
		await new Promise((resolve) => {
			waiting.set(++id, resolve);
			worker.postMessage({ op: 'open', id });
		});
	}, url);

const run = (request) => page.evaluate((r) => window.__run(r), request);

/** `DATUM_PHASES …` → the marks as [name, ms] in order. */
const marksOf = (stderr, tag) => {
	const line = stderr.split('\n').find((l) => l.startsWith(tag + ' '));
	return line ? line.slice(tag.length + 1).split(' ').filter(Boolean).map((p) => [p.split('=')[0], Number(p.split('=')[1])]) : null;
};

/** One process's `run`, split: the two opens (alias pass and dispatch), parse, dispatch, exit. */
function split(marks) {
	const out = { open1: 0, open2: 0, parse: 0, dispatch: 0, exit: 0, other: 0 };
	let parsed = false;
	for (const [name, ms] of marks) {
		if (name.startsWith('open.')) out[parsed ? 'open2' : 'open1'] += ms;
		else if (name === 'parse.opened') out.open1 += ms;
		else if (name === 'parsed') (out.parse += ms), (parsed = true);
		else if (name === 'dispatch.opened') out.open2 += ms;
		else if (name === 'dispatch.ran') out.dispatch += ms;
		else if (name === 'end') out.exit += ms;
		else if (name === 'mem') continue;
		else out.other += ms;
	}
	return out;
}

/** The migrate steps of each open, summed across the opens. */
function migrateSteps(marks) {
	const out = {};
	for (const [name, ms] of marks) if (name.startsWith('open.')) out[name.slice(5)] = (out[name.slice(5)] ?? 0) + ms;
	return out;
}

const workerUrl = `${base}/_app/immutable/workers/${workerFile}`;

// ---- 1. alone ------------------------------------------------------------------

if (PARTS.includes(1)) {
log('\n1. each process alone');
await seed();
await startWorker(workerUrl);
report.alone = [];
for (const request of requests) {
	if (ONLY && !label(request).includes(ONLY)) continue;
	const runs = [];
	for (let i = 0; i < RUNS; i++) {
		const got = await run({ ...request, env: [...request.env, 'DATUM_PHASES=1'] });
		if (!got.ok) throw new Error(`${label(request)}: ${got.error}`);
		const cli = got.cli;
		const marks = marksOf(cli.stderr, 'DATUM_PHASES') ?? [];
		const mem = marks.find(([n]) => n === 'mem')?.[1] / 2 ** 20;
		runs.push({ wall: got.wall, code: cli.code, ...cli.ms, ...split(marks), steps: migrateSteps(marks), mem });
	}
	const pick = (f) => round(median(runs.map(f)));
	const row = {
		argv: label(request),
		kind: request.session ? 'session' : request.pane ? 'pane' : 'first',
		code: runs[0].code,
		...Object.fromEntries(['wall', 'compile', 'acquire', 'instantiate', 'run', 'flush', 'open1', 'parse', 'open2', 'dispatch', 'exit', 'other', 'mem'].map((k) => [k, pick((r) => r[k])])),
		steps: Object.fromEntries(Object.keys(runs[0].steps).map((k) => [k, pick((r) => r.steps[k] ?? 0)])),
		// The worst run, which a median hides: a one-time cost lands in one of them.
		worst: round(Math.max(...runs.map((r) => r.wall))),
		memMax: round(Math.max(...runs.map((r) => r.mem ?? 0))),
		worstRun: runs.reduce((a, b) => (b.wall > a.wall ? b : a))
	};
	report.alone.push(row);
	aloneOf.set(request, row);
	if (ONLY) log(JSON.stringify(runs.map((r) => ({ wall: round(r.wall), open1: round(r.open1), open2: round(r.open2), dispatch: round(r.dispatch), steps: r.steps }))));
	log(
		`  ${row.kind.padEnd(7)} ${row.argv.padEnd(48)} wall ${String(row.wall).padStart(6)}  inst ${String(row.instantiate).padStart(5)}  run ${String(row.run).padStart(6)} = open ${String(round(row.open1 + row.open2)).padStart(5)} parse ${String(row.parse).padStart(5)} dispatch ${String(row.dispatch).padStart(6)} exit ${String(row.exit).padStart(4)}  flush ${row.flush}  ${row.mem ? ` mem ${Math.round(row.mem)} MiB` : ''}  worst ${row.worst} ms, ${Math.round(row.memMax)} MiB  (code ${row.code})`
	);
}

}

// ---- 2. persistent ---------------------------------------------------------------

if (PARTS.includes(2)) {
log('\n2. the same processes as lines of one process (DATUM_LINES)');
await seed();
await startWorker(workerUrl);
// The recorded `ops --since` says the peer holds this copy's own ops up to the
// seq the measured page had signed; a fresh seed has signed fewer, which the
// CLI reads as another copy signing with this key. The write, alone, signs them.
const ownSeq = Math.max(
	0,
	...requests.map((r) => {
		const since = r.argv[r.argv.indexOf('--since') + 1];
		return r.argv.includes('--since') ? (JSON.parse(since)[r.argv[r.argv.indexOf('--as') + 1]] ?? 0) : 0;
	})
);
for (let i = 0; i < ownSeq; i++) await run(requests.find(writes));
const lined = requests.filter((r) => !r.stdin);
report.leftOut = requests.filter((r) => r.stdin).map(label);
if (report.leftOut.length) log(`  left out (they read stdin): ${report.leftOut.join('; ')}`);
// The env every line shares is the process's; each line's own is on the line.
const shared = lined.map((r) => r.env).reduce((a, b) => a.filter((v) => b.includes(v)));
const line = (r) => [...r.env.filter((v) => !shared.includes(v)).map((v) => '=' + v), ...r.argv].join('\t');
const stdin = Array.from({ length: RUNS }, () => lined.map(line).join('\n')).join('\n') + '\n';
const persistent = await run({ argv: [], env: [...shared, 'DATUM_LINES=1'], stdin });
if (!persistent.ok) throw new Error(persistent.error);
const stderr = persistent.cli.stderr.split('\n');
const opened = marksOf(stderr.find((l) => l.startsWith('DATUM_OPEN')) ?? '', 'DATUM_OPEN') ?? [];
// Each line's stderr is what it printed since the line before's marker.
const perLine = [];
let said = [];
for (const l of stderr.slice(stderr.findIndex((l) => l.startsWith('DATUM_OPEN')) + 1)) {
	if (!l.startsWith('DATUM_LINE ')) {
		said.push(l);
		continue;
	}
	const [, code, ...rest] = l.split(' ');
	const marks = rest.map((p) => [p.split('=')[0], Number(p.split('=')[1])]);
	const mem = marks.find(([n]) => n === 'mem')?.[1];
	const timed = marks.filter(([n]) => n !== 'mem');
	perLine.push({ code: Number(code), ms: timed.reduce((n, [, ms]) => n + ms, 0), mem, stderr: said.join('\n').slice(0, 400) });
	said = [];
}
if (perLine.length !== lined.length * RUNS) throw new Error(`${perLine.length} lines answered of ${lined.length * RUNS}: ${persistent.cli.stderr.slice(-2000)}`);
const peak = perLine.at(-1)?.mem;
report.persistent = {
	memMiB: peak && round(peak / 2 ** 20),
	process: { wall: round(persistent.wall), ...Object.fromEntries(Object.entries(persistent.cli.ms).map(([k, v]) => [k, round(v)])) },
	open: round(opened.filter(([n]) => n !== 'mem').reduce((n, [, ms]) => n + ms, 0)),
	firstPass: round(perLine.slice(0, lined.length).reduce((n, l) => n + l.ms, 0)),
	lines: lined.map((r, i) => {
		const mine = perLine.filter((_, j) => j % lined.length === i);
		const alone = aloneOf.get(r) ?? {};
		return { argv: label(r), code: mine[0].code, error: mine.find((l) => l.code)?.stderr, first: round(mine[0].ms), later: round(median(mine.slice(1).map((l) => l.ms))), alone: alone.wall, aloneDispatch: alone.dispatch };
	})
};
log(`  one process: wall ${report.persistent.process.wall} ms, its open ${report.persistent.open} ms, first pass ${report.persistent.firstPass} ms, memory at the end ${report.persistent.memMiB} MiB`);
for (const l of report.persistent.lines)
	log(`  ${l.argv.padEnd(48)} line ${String(l.first).padStart(6)} first, ${String(l.later).padStart(6)} later   alone ${String(l.alone).padStart(6)} (dispatch ${l.aloneDispatch})  (code ${l.code})${l.error ? ' ' + l.error.replace(/\n/g, ' | ') : ''}`);
const sum = (xs) => round(xs.reduce((a, b) => a + b, 0));
report.persistent.tapAlone = sum(report.persistent.lines.map((l) => l.alone));
report.persistent.tapLater = sum(report.persistent.lines.map((l) => l.later));
log(`  the tap's ${lined.length} processes: alone ${report.persistent.tapAlone} ms in all, as lines ${report.persistent.tapLater} ms`);

}

// ---- 3. unchanged ------------------------------------------------------------------

if (PARTS.includes(3)) {
log('\n3. which reads print the same after the write (one frozen instant)');
await seed();
await startWorker(workerUrl);
const write = requests.find(writes);
const reads = requests.filter((r) => r !== write && !r.stdin && !r.session);
const frozen = `${FROZEN}${Date.now() + 5000}`;
// The tap flips its field: put the field in the other state first, as it was
// before the measured tap, so the write changes what a reader shows.
const verb = write.argv.find((w) => !w.startsWith('-'));
const flip = { start: 'end', end: 'start', stop: 'start' }[verb];
if (flip) await run({ ...write, argv: write.argv.map((w) => (w === verb ? flip : w)) });
const before = [];
for (const r of reads) before.push((await run({ ...r, env: [...r.env, frozen] })).cli?.stdout);
await run(write);
report.unchanged = [];
for (const [i, r] of reads.entries()) {
	const after = (await run({ ...r, env: [...r.env, frozen] })).cli?.stdout;
	const same = after === before[i];
	report.unchanged.push({ argv: label(r), same, alone: aloneOf.get(r)?.wall });
	log(`  ${same ? 'same   ' : 'changed'} ${label(r)}`);
}

}

// ---- 4. handles ----------------------------------------------------------------------

if (PARTS.includes(4)) {
log('\n4. sync access handles two workers may hold on one file at once');
report.handles = await page.evaluate(async () => {
	const src = `onmessage = async ({ data }) => {
		const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('probe', { create: true });
		const file = await dir.getFileHandle('f', { create: true });
		try {
			const h = await file.createSyncAccessHandle(data.mode ? { mode: data.mode } : undefined);
			postMessage({ ok: true, mode: h.mode ?? null });
			onmessage = () => { h.close(); postMessage({ closed: true }); };
		} catch (e) { postMessage({ ok: false, error: e.name }); }
	};`;
	const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
	const take = (mode) =>
		new Promise((resolve) => {
			const w = new Worker(url);
			w.onmessage = ({ data }) => resolve({ w, ...data });
			w.postMessage({ mode });
		});
	const drop = (h) => new Promise((resolve) => ((h.w.onmessage = () => (h.w.terminate(), resolve())), h.w.postMessage(0)));
	const out = [];
	for (const [a, b] of [
		['readwrite', 'readwrite'],
		['read-only', 'read-only'],
		['readwrite', 'read-only'],
		['read-only', 'readwrite'],
		['readwrite-unsafe', 'readwrite-unsafe'],
		['readwrite-unsafe', 'read-only']
	]) {
		const first = await take(a);
		const second = await take(b);
		out.push({ first: a, second: b, firstOk: first.ok, firstMode: first.mode, secondOk: second.ok, secondError: second.error ?? null });
		for (const h of [first, second]) if (h.ok) await drop(h);
	}
	// What a long-lived process could block on for its next request.
	return { pairs: out, crossOriginIsolated, sharedArrayBuffer: typeof SharedArrayBuffer === 'function', jspi: typeof WebAssembly.Suspending === 'function', userAgent: navigator.userAgent };
});
log(`  crossOriginIsolated ${report.handles.crossOriginIsolated}, SharedArrayBuffer ${report.handles.sharedArrayBuffer}, JSPI (WebAssembly.Suspending) ${report.handles.jspi}; ${report.handles.userAgent}`);
for (const h of report.handles.pairs)
	log(`  ${h.first.padEnd(17)} then ${h.second.padEnd(17)} → ${h.firstOk ? `held (mode ${h.firstMode})` : 'refused'}, then ${h.secondOk ? 'held' : `refused: ${h.secondError}`}`);

}

await context.close();
await new Promise((resolve) => server.httpServer.close(resolve));
rmSync(profile, { recursive: true, force: true });
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify(report, null, '\t'));
