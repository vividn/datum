// `pnpm test`: the web suite, with its parallelism bounded by the box.
//
// Every test file launches its own Chromium (tests/support/app.mjs), and
// `node --test` runs one file per core minus one: 23 at once on the 24-core
// build box — 163 Chrome processes across three cans at 17:1x on 2026-09-15.
// A flat `--test-concurrency=6` fixed the box and broke CI: its runners have
// four cores, so their default was THREE, and six was a raise — twenty-one
// tests timed out under the contention (PR #213's first run). So the cap is
// the smaller of six and what the machine has AND what the machine's current
// load leaves — see `filesAtOnce`.
//
// `DATUM_TEST_CONCURRENCY` overrides the load, but not the cores: it is capped
// at what the cores allow an idle box (da-dwq7g). The merge queue's
// `scripts/mq-check.sh` sets it, because the refinery's verdict is the one run
// that must not yield to the cans'.
//
// `--shard N/M` (or `DATUM_TEST_SHARD=N/M`, so a workflow can choose it
// without touching the command line) runs a quarter — or a third, or an
// M-th — of the files instead of all of them; see `shard.mjs`. The files it
// took are printed before it runs them. No flag is every file, as before.
//
// LONGEST FIRST (da-gqwh2). With files running side by side, a slow file that
// starts last sets the run's tail alone, so the files start in the order of
// `durations.json` — seconds per file, slowest first, from a whole run
// (`DATUM_TEST_RECORD=1 pnpm test` rewrites it; a file it does not know yet
// starts first, since it may be slow). `node --test` sorts the files it is
// given by name whatever order they come in, so the suite runs through
// `run()`, which keeps the order. It prints each file's wall time at the end,
// and a cut file by name. `--test-name-pattern=RE` is the one `node --test`
// flag it passes on (to `run()`'s `testNamePatterns`); anything else is refused.
//
// The LANE is not the host. A CI job's `availableParallelism()` and
// `loadavg()` are the host's (12 cores, load 11 on a 3-core lane), so the
// cores are capped by the cgroup's own `cpu.max`, and its `cpu.stat` is read
// before and after: the run's last line says how much CPU the suite really
// used and how often the lane throttled it.
//
// The per-file budget is FIVE minutes, and it was two until 2026-09-16. Two
// was set for a smaller suite and had fallen below what the slowest files
// honestly cost: measured alone on a 24-core box, `reachable.test.mjs` takes
// 117 s (48 tests) and `daystrip.test.mjs` 76 s. A file that needs 117 s with
// a whole box to itself cannot fit 120 s on a 2-core runner at any
// concurrency — which is how ten files came to be cut as «timed out» in a run
// that reported `# fail 0` (da-buqa). 300 s is about 2.5x the slowest measured
// standalone: still inside five minutes for a hang, and no honest file trips
// it. The number lives here alone, so GitHub's four shards and the forge's one
// job read the same budget. Making the slow files snappier is the follow-up;
// the budget is not the place to assert speed.
//
// The budget is each test's and each hook's, not the file's: `run()` hands it
// to the file's process as `--test-timeout`, and that process ends only when
// nothing is left open in it. So `after(stopApp)` timing out under load 50,
// Chromium and the server still open, left its file red and running and the
// run waiting on it for six hours (da-cvd7y). `forceExit` ends a file's
// process once its tests and hooks are done, whatever it still holds.
//
// A TOP-LEVEL HOOK that fails (`after(stopApp)` throwing or timing out) is
// passed by node: its file completes as passed, and the hook is a lone
// `test:fail` named by the HOOK's file (da-schm5). Each file's events arrive
// together, ending with its `test:summary`, so the summary after such a
// failure names the file it belongs to, at any concurrency, and that file is
// failed here: ✗ in the tail, and one of the three below.
//
// FAIL FAST (da-s2g46). Once THREE files have failed the run is aborted: the
// files in flight are cut, and the ones never started are named under
// `NOT RUN:`, so a stopped suite cannot read as a pass. `DATUM_TEST_BAIL=0`
// runs everything, for the whole picture.
//
// DEBUG WIDGETS ARE NOT TESTED IN CI (da-s2g46). Their tests live in
// `tests/debug*.test.mjs`, and with `CI` set those files are skipped and
// named — vividn, 2026-09-24: "they are not important in userland". Locally,
// or with `DATUM_TEST_DEBUG=1`, they run.
//
// RETRY ONCE, FOR NAMED FILES ONLY (da-8bmqe). `DATUM_TEST_RETRY` lists
// `tests/<file>=<bead>` pairs: a listed file that fails is run once more
// after the suite, and a retry that passes clears it. Its failure does not
// count toward the fail-fast three. Every retry is logged as `RETRIED
// web/tests/<file> (<bead>): …`, and an unlisted file is never retried. The
// list lives in ci.yml, beside the beads whose fixes take each file off it.
//
// ONLY THE FILES A CHANGE REACHES, ON A PR (da-e0gtq). `DATUM_TEST_ONLY`
// names a file listing test files, one per line — `testmap.mjs select`'s
// choice — and the run takes those of its files alone. `DATUM_TEST_COVERAGE`
// names a directory, and every process of the run records into it what it
// executed (tests/support/coverage.mjs), which `testmap.mjs index` makes the
// map that choice is read from.

import { availableParallelism, loadavg } from 'node:os';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import { run } from 'node:test';
import { spec as Spec, tap } from 'node:test/reporters';
import { filesAtOnce } from './concurrency.mjs';
import { parseShard, shardFiles } from './shard.mjs';

const args = process.argv.slice(2);
const flag = args.indexOf('--shard');
const spec = flag === -1 ? process.env.DATUM_TEST_SHARD : args.splice(flag, 2)[1];
if (flag !== -1 && spec === undefined) throw new Error('--shard wants an N/M after it');
const pattern = '--test-name-pattern=';
const names = args.filter((a) => a.startsWith(pattern)).map((a) => a.slice(pattern.length));
const testNamePatterns = names.length ? names : undefined;
const rest = args.filter((a) => !a.startsWith(pattern));
if (rest.length) throw new Error(`test.mjs takes --shard N/M and --test-name-pattern=RE, not ${rest.join(' ')}`);
const shard = parseShard(spec);

const cgroup = (f) => {
	try {
		return readFileSync(`/sys/fs/cgroup/${f}`, 'utf8');
	} catch {
		return undefined;
	}
};
const cpuStat = () =>
	Object.fromEntries(
		(cgroup('cpu.stat') ?? '')
			.trim()
			.split('\n')
			.map((l) => l.split(' '))
			.map(([k, v]) => [k, Number(v)])
	);
// No `cpu.max` is the host's root cgroup, whose `cpu.stat` is every process's.
const max = cgroup('cpu.max');
const [quota, period] = (max ?? 'max').trim().split(' ');
const ratio = Number(quota) / Number(period); // NaN for `max`, or a read that is not two numbers
const lane = Number.isFinite(ratio) ? ratio : undefined;

const cores = Math.max(1, Math.min(availableParallelism(), Math.floor(lane ?? Infinity)));
// Read once: a run's concurrency is fixed at its start.
const load = loadavg()[0];
const override = process.env.DATUM_TEST_CONCURRENCY ? Number(process.env.DATUM_TEST_CONCURRENCY) : undefined;
const concurrency = filesAtOnce(cores, load, 6, override);
console.log(
	`test: ${cores} cores${lane ? ` (cgroup cpu.max ${lane})` : ''}, load ${load.toFixed(1)} → ${concurrency} file${concurrency === 1 ? '' : 's'} at once${override ? ` (override ${override}${concurrency < Math.floor(override) ? ', capped by cores' : ''})` : ''}`
);

// The same set the old `tests/**/*.test.mjs` glob matched, listed here so the
// shard can be taken from it — and so the run SAYS what it ran.
const listed = readdirSync('tests', { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.test.mjs'))
	.map((f) => `tests/${f}`)
	.sort();
const debug = (f) => /^tests\/debug[^/]*\.test\.mjs$/.test(f);
const skipDebug = process.env.CI && process.env.DATUM_TEST_DEBUG !== '1';
const all = skipDebug ? listed.filter((f) => !debug(f)) : listed;
if (skipDebug) console.log(`test: skipped under CI (DEBUG widgets): ${listed.filter(debug).join(' ')}`);
const retry = new Map(
	(process.env.DATUM_TEST_RETRY ?? '')
		.split(/\s+/)
		.filter(Boolean)
		.map((p) => p.split('='))
);
// `all`, not `listed`: a DEBUG file skipped under CI would never be retried.
for (const [f, bead] of retry) if (!all.includes(f) || !bead) throw new Error(`DATUM_TEST_RETRY wants tests/<file>=<bead> naming a file this run has; got ${f}=${bead ?? ''}`);
const record = new URL('durations.json', import.meta.url);
const known = JSON.parse(readFileSync(record, 'utf8'));
const onlyList = process.env.DATUM_TEST_ONLY;
// Not passed on: a run a test starts (test-runner.test.mjs) takes its own files.
delete process.env.DATUM_TEST_ONLY;
const only = onlyList && new Set(readFileSync(onlyList, 'utf8').split('\n').filter(Boolean));
const files = shardFiles(all, shard).filter((f) => !only || only.has(f)).sort((a, b) => (known[b] ?? Infinity) - (known[a] ?? Infinity) || a.localeCompare(b));
if (only) console.log(`test: ${files.length} of ${all.length} files, the ones this change reaches (${onlyList})`);
if (process.env.DATUM_TEST_COVERAGE) {
	// Absolute, for the processes a test spawns somewhere else.
	process.env.DATUM_TEST_COVERAGE = resolve(process.env.DATUM_TEST_COVERAGE);
	mkdirSync(process.env.DATUM_TEST_COVERAGE, { recursive: true });
	process.env.NODE_OPTIONS = `${process.env.NODE_OPTIONS ?? ''} --import=${new URL('../tests/support/coverage.mjs', import.meta.url)}`;
	console.log(`test: recording what each file executes into ${process.env.DATUM_TEST_COVERAGE}`);
}
if (shard) {
	console.log(`test: shard ${shard.n}/${shard.total} — ${files.length} of ${all.length} files:`);
	for (const f of files) console.log(`  ${f}`);
}

const before = cpuStat();
const t0 = Date.now();
const took = {};
process.exitCode = 1; // until the run's own summary says it passed
const bail = process.env.DATUM_TEST_BAIL === '0' ? Infinity : 3;
const failed = [];
const healed = new Set();
// `run()` starts the first `concurrency` files at once, and on node 22 their
// `test:dequeue` is emitted before it returns, so no listener sees it.
const started = new Set(files.slice(0, concurrency));
const hooked = new Set(); // files whose top-level hook failed
let stray = false; // a hook failure whose file's summary is still to come
let notRun;
const stop = new AbortController();
const stream = run({
	files,
	concurrency,
	signal: stop.signal,
	timeout: 300_000,
	forceExit: true,
	testNamePatterns
});
// An aborted run still dequeues and completes every file, so what had
// started is read up to the abort and no further.
stream.on('test:dequeue', (t) => {
	if (t.nesting === 0 && files.includes(t.name) && !notRun) started.add(t.name);
});
const fail = (f) => {
	if (!notRun && !retry.has(f) && failed.push(f) >= bail) {
		notRun = files.filter((f) => !started.has(f));
		stop.abort();
	}
};
stream.on('test:complete', (t) => {
	if (t.nesting !== 0 || !files.includes(t.name)) return;
	took[t.name] = hooked.has(t.name) ? { ...t.details, passed: false } : t.details;
	if (!took[t.name].passed) fail(t.name);
});
stream.on('test:fail', (t) => {
	if (t.nesting === 0 && t.details.error?.failureType === 'hookFailed') stray = true;
});
stream.on('test:summary', (s) => {
	if (s.file === undefined && s.success) process.exitCode = 0;
	const f = s.file && relative(process.cwd(), s.file);
	if (!stray || !files.includes(f)) return;
	stray = false;
	hooked.add(f);
	// Its `test:complete` may have come first, as passed.
	if (took[f]?.passed) {
		took[f] = { ...took[f], passed: false };
		fail(f);
	}
});
// After the reporter's last line, so the summary is the run's tail.
const reporter = () => (process.stdout.isTTY ? new Spec() : tap);
const out = stream.compose(reporter());
out.pipe(process.stdout);
out.on('end', async () => {
	// A file the runner never reported as its own is a red run, and says which.
	const missing = files.filter((f) => !(f in took));
	if (notRun) {
		process.exitCode = 1;
		console.log(`test: stopped after ${bail} failed files (${failed.join(' ')}); NOT RUN:`);
		for (const f of notRun) console.log(`  ${f}`);
	}
	if (missing.length) {
		process.exitCode = 1;
		console.log(`test: ${missing.length} file${missing.length === 1 ? '' : 's'} never completed: ${missing.join(' ')}`);
	}
	const red = Object.keys(took).filter((f) => !took[f].passed);
	if (!notRun && !missing.length) {
		for (const f of red.filter((f) => retry.has(f))) {
			let passed = false;
			const again = run({ files: [f], timeout: 300_000, forceExit: true, testNamePatterns });
			again.on('test:summary', (s) => {
				if (s.file === undefined) passed = s.success;
			});
			for await (const chunk of again.compose(reporter())) process.stdout.write(chunk);
			console.log(`RETRIED web/${f} (${retry.get(f)}): first run failed, retry ${passed ? 'passed' : 'failed'}`);
			if (passed) healed.add(f);
		}
		if (red.length && healed.size === red.length) process.exitCode = 0;
	}
	const ran = Object.entries(took).sort(([, a], [, b]) => b.duration_ms - a.duration_ms);
	console.log(`test: ${ran.length} files, slowest first (✗ failed or cut, ↻ failed and then passed its retry):`);
	for (const [f, d] of ran) console.log(`  ${(d.duration_ms / 1000).toFixed(1).padStart(6)} s ${d.passed ? ' ' : healed.has(f) ? '↻' : '✗'} ${f}`);
	const wall = (Date.now() - t0) / 1000;
	const after = cpuStat();
	if (max !== undefined) {
		const used = (after.usage_usec - before.usage_usec) / 1e6;
		console.log(
			`test: ${wall.toFixed(0)} s wall, lane CPU ${used.toFixed(0)} s = ${((100 * used) / wall).toFixed(0)}%` +
				`, throttled ${after.nr_throttled - before.nr_throttled} of ${after.nr_periods - before.nr_periods} periods` +
				` for ${((after.throttled_usec - before.throttled_usec) / 1e6).toFixed(1)} s`
		);
	}
	if (process.env.DATUM_TEST_RECORD && !shard && !notRun && ran.length === all.length) {
		// A failed file's time is a lower bound: it keeps the larger of that and
		// what it took before, so a cut sorts first and an early failure stays put.
		// A failed file with no history stays out, unknown, so it starts first.
		const secs = ([f, d]) => [f, Math.round(Math.max(d.duration_ms / 1000, d.passed ? 0 : (known[f] ?? 0)))];
		const kept = ran.filter(([f, d]) => d.passed || f in known);
		writeFileSync(record, JSON.stringify(Object.fromEntries(kept.map(secs)), null, '\t') + '\n');
		console.log(`test: wrote ${record.pathname}`);
	}
});
