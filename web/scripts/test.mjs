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
// Any further arguments go to `node --test` unchanged.

import { availableParallelism, loadavg } from 'node:os';
import { readdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { filesAtOnce } from './concurrency.mjs';
import { parseShard, shardFiles } from './shard.mjs';

const args = process.argv.slice(2);
const flag = args.indexOf('--shard');
const spec = flag === -1 ? process.env.DATUM_TEST_SHARD : args.splice(flag, 2)[1];
if (flag !== -1 && spec === undefined) throw new Error('--shard wants an N/M after it');
const shard = parseShard(spec);

const cores = availableParallelism();
// Read once: a run's concurrency is fixed at its start.
const load = loadavg()[0];
const override = process.env.DATUM_TEST_CONCURRENCY ? Number(process.env.DATUM_TEST_CONCURRENCY) : undefined;
const concurrency = filesAtOnce(cores, load, 6, override);
console.log(
	`test: ${cores} cores, load ${load.toFixed(1)} → ${concurrency} file${concurrency === 1 ? '' : 's'} at once${override ? ' (override)' : ''}`
);

// The same set the old `tests/**/*.test.mjs` glob matched, listed here so the
// shard can be taken from it — and so the run SAYS what it ran.
const all = readdirSync('tests', { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.test.mjs'))
	.map((f) => `tests/${f}`)
	.sort();
const files = shardFiles(all, shard);
if (shard) {
	console.log(`test: shard ${shard.n}/${shard.total} — ${files.length} of ${all.length} files:`);
	for (const f of files) console.log(`  ${f}`);
}

const child = spawn(
	process.execPath,
	['--test', `--test-concurrency=${concurrency}`, '--test-timeout=120000', ...files, ...args],
	{ stdio: 'inherit' }
);
child.on('exit', (code, signal) => process.exit(signal ? 1 : (code ?? 1)));
