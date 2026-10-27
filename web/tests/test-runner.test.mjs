// How many files `pnpm test` runs at once — in node, with no browser.
//
// Seven cans each ran this suite at six files on one 24-core box and the merge
// queue starved for 70 minutes (da-x38v). The cap is still six; what shrinks it
// is the load average the box already carries, read once at the run's start.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { filesAtOnce } from '../scripts/concurrency.mjs';
import { parseShard, shardFiles } from '../scripts/shard.mjs';

describe('filesAtOnce', () => {
	test('an idle box gets the cap, or its cores if it has fewer', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(4, 0, 6), 2); // an idle CI runner
		assert.equal(filesAtOnce(2, 0, 6), 1);
	});

	// The defect lived only between the cap (24 cores) and the load term (a
	// loaded 3-core box): one core charged a file where the load term charges
	// two, so an idle 6-core box got five files — ten cores of work (da-j2okg).
	test('an idle box is committed to no more cores of work than it has', () => {
		const coresPerFile = 2; // a Chromium and the node driving it
		for (const [cores, files] of [
			[4, 2],
			[6, 3],
			[8, 4]
		]) {
			const got = filesAtOnce(cores, 0, 6);
			assert.equal(got, files, `${cores} cores`);
			assert.ok(got * coresPerFile <= cores, `${got} files is ${got * coresPerFile} cores of work on ${cores}`);
		}
	});

	test('the load shrinks it, and never below one', () => {
		assert.equal(filesAtOnce(24, 88, 6), 1); // the night of da-x38v
		assert.equal(filesAtOnce(24, 71.3, 6), 1);
		assert.equal(filesAtOnce(24, 34, 6), 6);
		assert.equal(filesAtOnce(24, 40, 6), 3);
		assert.equal(filesAtOnce(4, 6, 6), 1);
	});

	test('the override ignores the load', () => {
		assert.equal(filesAtOnce(24, 88, 6, 6), 6); // the queue's own run
		assert.equal(filesAtOnce(12, 8.5, 6, 3), 3); // CI's 3 on the 12-core box
	});

	// CI's 3 on the 3-core forge lane was six cores of work on three (da-dwq7g);
	// the 6-core lane measured 1.2 cores a file, so it takes four (da-gimld).
	test('the override is capped at the cores less two', () => {
		assert.equal(filesAtOnce(3, 1.9, 6, 3), 1); // the forge lane
		assert.equal(filesAtOnce(4, 0, 6, 3), 2);
		assert.equal(filesAtOnce(6, 2.3, 6, 4), 4); // the 6-core CI lane
		assert.equal(filesAtOnce(6, 0, 6, 6), 4);
		assert.equal(filesAtOnce(12, 0, 6, 8), 6);
		assert.equal(filesAtOnce(24, 0, 6, 12), 6);
	});

	test('a platform without loadavg reports 0, which is idle', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(24, -1, 6), 6);
	});
});

// Which files a run takes — the other half of `pnpm test`'s arithmetic. The
// invariant the CI matrix rests on: the M shards, concatenated, are the suite
// exactly once (da-buqa).

describe('shardFiles', () => {
	const files = Array.from({ length: 60 }, (_, i) => `tests/f${i}.test.mjs`);

	test('no shard is every file', () => {
		assert.deepEqual(shardFiles(files, undefined), files);
	});

	test('shard N/M takes index % M === N - 1', () => {
		assert.deepEqual(shardFiles(files, { n: 1, total: 4 }).slice(0, 3), [
			'tests/f0.test.mjs',
			'tests/f4.test.mjs',
			'tests/f8.test.mjs'
		]);
		assert.deepEqual(shardFiles(files, { n: 4, total: 4 })[0], 'tests/f3.test.mjs');
	});

	test('the four shards are the suite, once, and are within one of each other', () => {
		const shards = [1, 2, 3, 4].map((n) => shardFiles(files, { n, total: 4 }));
		assert.deepEqual(shards.flat().sort(), [...files].sort());
		assert.deepEqual(
			shards.map((s) => s.length),
			[15, 15, 15, 15]
		);
	});

	test('more shards than files leaves the tail empty, and loses nothing', () => {
		const three = ['a', 'b', 'c'];
		const shards = [1, 2, 3, 4].map((n) => shardFiles(three, { n, total: 4 }));
		assert.deepEqual(shards.flat().sort(), three);
		assert.deepEqual(shards[3], []);
	});
});

describe('parseShard', () => {
	test('N/M, and nothing at all', () => {
		assert.deepEqual(parseShard('2/4'), { n: 2, total: 4 });
		assert.deepEqual(parseShard(' 1/1 '), { n: 1, total: 1 });
		assert.equal(parseShard(undefined), undefined);
		assert.equal(parseShard(''), undefined); // an unset env var in a workflow
	});

	test('nonsense is loud, never a silently whole suite', () => {
		for (const bad of ['0/4', '5/4', '4', 'a/b', '1/0', '-1/4', '1/4/4']) {
			assert.throws(() => parseShard(bad), /--shard wants/, bad);
		}
	});
});

// The retry (da-8bmqe), through the real runner: a temp dir holding a file
// that fails once and then passes, and one that always fails.

import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

/** `scripts/test.mjs` run over `files` alone; cut, with no exit code, a minute on. */
const runner = new URL('../scripts/test.mjs', import.meta.url).pathname;
const suite = (files, retry, extra = {}) => {
	const dir = mkdtempSync(join(tmpdir(), 'datum-retry-'));
	mkdirSync(join(dir, 'tests'));
	for (const [name, body] of Object.entries(files)) writeFileSync(join(dir, 'tests', name), body);
	const env = { ...process.env, DATUM_TEST_RETRY: retry, DATUM_TEST_CONCURRENCY: '1', ...extra };
	delete env.NODE_TEST_CONTEXT; // or the child reports to this runner instead of running
	// The run this file is in sets its own: testmap.yml's `DATUM_TEST_BAIL=0` ran
	// last.test.mjs past the three (da-c8ypa), and its `DATUM_TEST_RECORD=1`
	// wrote these temp files' times over durations.json.
	for (const k of ['DATUM_TEST_BAIL', 'DATUM_TEST_RECORD', 'DATUM_TEST_SHARD']) if (!(k in extra)) delete env[k];
	const r = spawnSync(process.execPath, [runner], { cwd: dir, env, encoding: 'utf8', timeout: 60_000 });
	rmSync(dir, { recursive: true, force: true });
	return { code: r.status, out: r.stdout + r.stderr };
};

// quartz's run held a can six hours (da-cvd7y): `after(stopApp)` timed out
// with Chromium and the server open, and nothing ended the file's process.
// A timer stands in for them here, and lets go of an orphan on its own.
test('a file whose hook failed holding the process open ends, red', () => {
	const held = `import { after, test } from 'node:test';
test('passes', () => {});
after(() => {
	setTimeout(() => {}, 90_000);
	throw new Error('stopApp gave up with Chromium open');
});
`;
	const { code, out } = suite({ 'held.test.mjs': held }, '');
	assert.match(out, /stopApp gave up with Chromium open/);
	assert.equal(code, 1, out);
});

// da-schm5: node passes the file whose top-level hook failed, and names the
// failure by the hook's file, so the runner reads the file's summary instead.
test('a file whose top-level hook failed is ✗ in the tail, and counts toward the three', () => {
	const hook = `import { after } from 'node:test';
export const stop = () => after(() => { throw new Error('stopApp threw'); });
`;
	const broke = `import { test } from 'node:test';
import { stop } from './hook.mjs';
test('passes', () => {});
stop();
`;
	const files = { 'hook.mjs': hook, 'last.test.mjs': `import test from 'node:test';\ntest('p', () => {});\n` };
	for (const f of ['a', 'b', 'c']) files[`${f}.test.mjs`] = broke;
	const { code, out } = suite(files, '');
	for (const f of ['a', 'b', 'c']) assert.match(out, new RegExp(`^ +[\\d.]+ s ✗ tests/${f}\\.test\\.mjs$`, 'm'), out);
	assert.match(out, /^test: stopped after 3 failed files \(tests\/a\.test\.mjs tests\/b\.test\.mjs tests\/c\.test\.mjs\); NOT RUN:\n {2}tests\/last\.test\.mjs$/m, out);
	assert.equal(code, 1, out);
});

// The claim the attribution rests on: a file's events arrive together, ending
// with its summary, however many run at once. b and d break while a, started
// first and slower, still runs, so both their failures wait behind a's events
// and must each land on their own file. Two at once is enough.
test('at concurrency 3, the files whose hook failed are ✗ and no other', (t) => {
	const hook = `import { after } from 'node:test';
export const stop = () => after(() => { throw new Error('stopApp threw'); });
`;
	const clean = (ms) => `import test from 'node:test';\ntest('p', () => new Promise((r) => setTimeout(r, ${ms})));\n`;
	const broke = `import { test } from 'node:test';
import { stop } from './hook.mjs';
test('passes', () => {});
stop();
`;
	const files = { 'hook.mjs': hook, 'a.test.mjs': clean(1500), 'b.test.mjs': broke, 'c.test.mjs': clean(0), 'd.test.mjs': broke, 'e.test.mjs': clean(0) };
	const { code, out } = suite(files, '', { DATUM_TEST_CONCURRENCY: '3' });
	// The cores cap the override: a CI lane of three or four runs one or two.
	const at = Number(out.match(/→ (\d+) files? at once/)?.[1]);
	if (at === 1) return t.skip('this box runs one file at once');
	assert.ok(at >= 2, out);
	for (const f of ['b', 'd']) assert.match(out, new RegExp(`^ +[\\d.]+ s ✗ tests/${f}\\.test\\.mjs$`, 'm'), out);
	for (const f of ['a', 'c', 'e']) assert.match(out, new RegExp(`^ +[\\d.]+ s {3}tests/${f}\\.test\\.mjs$`, 'm'), out);
	assert.equal(code, 1, out);
});

// da-aksfc: node passes a file on its exit code with whatever tests it
// reported, so a process ended mid-file is a pass of the tests before. An exit
// stands in for the events `forceExit` lost. A file whose tests another module
// declares (act-taps-*) still sends its own summary, and stays green.
test('a file cut short with exit code 0 is ✗ and red, and says so', () => {
	const cut = `import test from 'node:test';
test('reported', () => {});
test('exits', () => process.exit(0));
test('never reported', () => {});
`;
	const elsewhere = `import { declare } from './declare.mjs';\ndeclare();\n`;
	const declare = `import test from 'node:test';\nexport const declare = () => test('declared elsewhere', () => {});\n`;
	const { code, out } = suite({ 'cut.test.mjs': cut, 'elsewhere.test.mjs': elsewhere, 'declare.mjs': declare }, '');
	assert.match(out, /^ +[\d.]+ s ✗ tests\/cut\.test\.mjs$/m, out);
	assert.match(out, /^ +[\d.]+ s {3}tests\/elsewhere\.test\.mjs$/m, out);
	assert.match(out, /^test: 1 file cut short, some tests never reported: tests\/cut\.test\.mjs$/m, out);
	assert.equal(code, 1, out);
});

describe('DATUM_TEST_RETRY', () => {
	const once = `import test from 'node:test';
import { existsSync, writeFileSync } from 'node:fs';
const mark = new URL(import.meta.url + '.ran');
test('fails the first time', () => {
	if (existsSync(mark)) return;
	writeFileSync(mark, '');
	throw new Error('first run');
});
`;
	const always = `import test from 'node:test';
test('fails', () => { throw new Error('always'); });
`;

	test('a listed file that fails once is retried, and the suite passes', () => {
		const { code, out } = suite({ 'flaky.test.mjs': once }, 'tests/flaky.test.mjs=da-x');
		assert.match(out, /^RETRIED web\/tests\/flaky\.test\.mjs \(da-x\): first run failed, retry passed$/m);
		assert.match(out, /^ +[\d.]+ s ↻ tests\/flaky\.test\.mjs$/m); // the tail says it healed, not ✗
		assert.equal(code, 0, out);
	});

	test('a listed file whose retry fails is red, and says so', () => {
		const { code, out } = suite({ 'flaky.test.mjs': always }, 'tests/flaky.test.mjs=da-x');
		assert.match(out, /^RETRIED web\/tests\/flaky\.test\.mjs \(da-x\): first run failed, retry failed$/m);
		assert.equal(code, 1, out);
	});

	test('an unlisted failing file is never retried', () => {
		const { code, out } = suite({ 'flaky.test.mjs': once, 'other.test.mjs': once }, 'tests/other.test.mjs=da-x');
		assert.doesNotMatch(out, /RETRIED web\/tests\/flaky/);
		assert.match(out, /^RETRIED web\/tests\/other\.test\.mjs \(da-x\): first run failed, retry passed$/m);
		assert.equal(code, 1, out);
	});

	test('a name that is not a test file is refused, not ignored', () => {
		for (const retry of ['tests/gone.test.mjs=da-x', 'tests/flaky.test.mjs']) {
			const { code, out } = suite({ 'flaky.test.mjs': once }, retry);
			assert.match(out, /DATUM_TEST_RETRY wants tests\/<file>=<bead> naming a file this run has; got /, retry);
			assert.notEqual(code, 0, retry);
		}
		// A DEBUG file is a test file, but CI never runs it, so it is never retried.
		const { code, out } = suite({ 'debug-x.test.mjs': once }, 'tests/debug-x.test.mjs=da-x', { CI: '1', DATUM_TEST_DEBUG: '' });
		assert.match(out, /DATUM_TEST_RETRY wants .* got tests\/debug-x\.test\.mjs=da-x/);
		assert.notEqual(code, 0);
	});

	// da-e0gtq: a PR's selection. Not passed on, or a run THIS file starts
	// would take the PR's list for its own and run nothing.
	test('DATUM_TEST_ONLY runs the files it lists alone, and keeps the list to itself', (t) => {
		const dir = mkdtempSync(join(tmpdir(), 'datum-only-'));
		t.after(() => rmSync(dir, { recursive: true, force: true }));
		const list = join(dir, 'web.tests');
		writeFileSync(list, 'tests/picked.test.mjs\n');
		const picked = `import test from 'node:test';
import assert from 'node:assert/strict';
test('sees no list', () => assert.equal(process.env.DATUM_TEST_ONLY, undefined));
`;
		const { code, out } = suite({ 'picked.test.mjs': picked, 'other.test.mjs': always }, '', { DATUM_TEST_ONLY: list });
		assert.match(out, /^test: 1 of 2 files, the ones this change reaches/m);
		assert.match(out, /^ +[\d.]+ s +tests\/picked\.test\.mjs$/m);
		assert.doesNotMatch(out, /other\.test\.mjs/);
		assert.equal(code, 0, out);
	});
});
