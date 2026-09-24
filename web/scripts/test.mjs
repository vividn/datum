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
// `DATUM_TEST_CONCURRENCY` overrides the number exactly, load and cores and
// all: the merge queue's `scripts/mq-check.sh` sets it, because the refinery's
// verdict is the one run that must not yield to the cans'.
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

import { availableParallelism, loadavg } from 'node:os';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { run } from 'node:test';
import { spec as Spec, tap } from 'node:test/reporters';
import { filesAtOnce } from './concurrency.mjs';
import { parseShard, shardFiles } from './shard.mjs';

const args = process.argv.slice(2);
const flag = args.indexOf('--shard');
const spec = flag === -1 ? process.env.DATUM_TEST_SHARD : args.splice(flag, 2)[1];
if (flag !== -1 && spec === undefined) throw new Error('--shard wants an N/M after it');
const pattern = '--test-name-pattern=';
const testNamePatterns = args.filter((a) => a.startsWith(pattern)).map((a) => a.slice(pattern.length));
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
	`test: ${cores} cores${lane ? ` (cgroup cpu.max ${lane})` : ''}, load ${load.toFixed(1)} → ${concurrency} file${concurrency === 1 ? '' : 's'} at once${override ? ' (override)' : ''}`
);

// The same set the old `tests/**/*.test.mjs` glob matched, listed here so the
// shard can be taken from it — and so the run SAYS what it ran.
const all = readdirSync('tests', { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.test.mjs'))
	.map((f) => `tests/${f}`)
	.sort();
const record = new URL('durations.json', import.meta.url);
const known = JSON.parse(readFileSync(record, 'utf8'));
const files = shardFiles(all, shard).sort((a, b) => (known[b] ?? Infinity) - (known[a] ?? Infinity) || a.localeCompare(b));
if (shard) {
	console.log(`test: shard ${shard.n}/${shard.total} — ${files.length} of ${all.length} files:`);
	for (const f of files) console.log(`  ${f}`);
}

const before = cpuStat();
const t0 = Date.now();
const took = {};
process.exitCode = 1; // until the run's own summary says it passed
const stream = run({
	files,
	concurrency,
	timeout: 300_000,
	testNamePatterns: testNamePatterns.length ? testNamePatterns : undefined
});
stream.on('test:complete', (t) => {
	if (t.nesting === 0 && files.includes(t.name)) took[t.name] = t.details;
});
stream.on('test:summary', (s) => {
	if (s.file === undefined && s.success) process.exitCode = 0;
});
// After the reporter's last line, so the summary is the run's tail.
const out = stream.compose(process.stdout.isTTY ? new Spec() : tap);
out.pipe(process.stdout);
out.on('end', () => {
	// A file the runner never reported as its own is a red run, and says which.
	const missing = files.filter((f) => !(f in took));
	if (missing.length) {
		process.exitCode = 1;
		console.log(`test: ${missing.length} file${missing.length === 1 ? '' : 's'} never completed: ${missing.join(' ')}`);
	}
	const ran = Object.entries(took).sort(([, a], [, b]) => b.duration_ms - a.duration_ms);
	console.log(`test: ${ran.length} files, slowest first (✗ failed or cut):`);
	for (const [f, d] of ran) console.log(`  ${(d.duration_ms / 1000).toFixed(1).padStart(6)} s ${d.passed ? ' ' : '✗'} ${f}`);
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
	if (process.env.DATUM_TEST_RECORD && !shard && ran.length === all.length) {
		// A failed file's time is a lower bound: it keeps the larger of that and
		// what it took before, so a cut sorts first and an early failure stays put.
		const secs = ([f, d]) => [f, Math.round(Math.max(d.duration_ms / 1000, d.passed ? 0 : (known[f] ?? 0)))];
		writeFileSync(record, JSON.stringify(Object.fromEntries(ran.map(secs)), null, '\t') + '\n');
		console.log(`test: wrote ${record.pathname}`);
	}
});
