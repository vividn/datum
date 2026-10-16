// The web test map's selection (da-e0gtq), with no browser: which test
// files a PR's changed files run, and the source-map reading the map is
// built with, and the preload that records a process at its exit.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { MAX_BEHIND, READS_TREE, choose, decodeMappings, ours, sourceAt } from '../scripts/testmap.mjs';

const MAP = {
	commit: 'abc123',
	files: {
		'tests/strip.test.mjs': ['src/lib/DayStrip.svelte', 'src/lib/fields.ts'],
		'tests/bar.test.mjs': ['src/lib/Prompt.svelte', 'src/lib/fields.ts'],
		'tests/tree.test.mjs': ['src/lib/layout/tree.ts']
	},
	always: []
};
const TESTS = Object.keys(MAP.files).sort();
const run = (changed, { map = MAP, behind = 0, tests = TESTS } = {}) => choose({ map, behind, changed, tests });
const picked = (r) => (assert.equal(r.full, undefined, `ran everything: ${r.full}`), [...r.picked.keys()].sort());

test('a changed component runs exactly the test files that executed it', () => {
	assert.deepEqual(picked(run(['web/src/lib/DayStrip.svelte'])), ['tests/strip.test.mjs']);
	assert.deepEqual(picked(run(['web/src/lib/fields.ts'])), ['tests/bar.test.mjs', 'tests/strip.test.mjs']);
	assert.equal(run(['web/src/lib/DayStrip.svelte']).picked.get('tests/strip.test.mjs'), 'web/src/lib/DayStrip.svelte');
});

test('a test file added or edited runs itself, and one the map does not know runs', () => {
	const tests = [...TESTS, 'tests/new.test.mjs'];
	assert.deepEqual(picked(run(['web/tests/tree.test.mjs'], { tests: TESTS })), ['tests/tree.test.mjs']);
	assert.deepEqual(picked(run(['web/src/lib/layout/tree.ts'], { tests })), ['tests/new.test.mjs', 'tests/tree.test.mjs']);
	assert.deepEqual(picked(run(['web/tests/gone.test.mjs'])), []);
	// One in a directory of its own runs itself too, not the whole suite.
	assert.deepEqual(picked(run(['web/tests/sub/x.test.mjs'], { tests: [...TESTS, 'tests/sub/x.test.mjs'] })), ['tests/sub/x.test.mjs']);
});

test('a test that reads src/ or the build as data runs on every selection', () => {
	const map = { ...MAP, always: ['tests/bar.test.mjs'] };
	assert.deepEqual(picked(run(['web/src/lib/layout/tree.ts'], { map })), ['tests/bar.test.mjs', 'tests/tree.test.mjs']);
});

test('a change to the suite itself, its fixtures or config runs everything', () => {
	for (const path of ['web/README.md', 'web/tests/support/app.mjs', 'web/tests/fixtures/cli-parity.json', 'web/package.json', 'web/pnpm-lock.yaml', 'web/vite.config.ts', 'web/static/manifest.json', 'web/scripts/test.mjs']) {
		assert.match(run([path]).full?.join() ?? '', /the web suite reads it/, path);
	}
});

test('a Rust change that reaches the CLI module runs everything', () => {
	for (const path of ['crates/datum-core/src/db.rs', 'crates/datum-cli/src/help.md', 'crates/datum-cli/tests/fixtures/x.json', 'Cargo.lock', 'kits/sample.toml', '.forgejo/workflows/ci.yml']) {
		assert.match(run(['web/src/lib/fields.ts', path]).full?.join() ?? '', /can reach the web/, path);
	}
	// What cannot reach it is passed over.
	const r = run(['web/src/lib/fields.ts', 'crates/datum-core/tests/x.rs', 'crates/datum-cli/tests/sync/a.rs', 'docs/a.md', 'README.md', 'xtask/src/testmap.rs']);
	assert.deepEqual(picked(r), ['tests/bar.test.mjs', 'tests/strip.test.mjs']);
	assert.equal(r.notes.length, 5);
});

test('a web/src file no test reached runs everything', () => {
	assert.match(run(['web/src/lib/brandNew.ts']).full?.join() ?? '', /no test's coverage reaches it/);
});

test('a stale or missing map runs everything', () => {
	assert.equal(picked(run(['web/src/lib/fields.ts'], { behind: MAX_BEHIND })).length, 2);
	assert.match(run(['web/src/lib/fields.ts'], { behind: MAX_BEHIND + 1 }).full?.join() ?? '', /commits behind/);
	assert.match(run(['web/src/lib/fields.ts'], { behind: null }).full?.join() ?? '', /not an ancestor/);
	assert.deepEqual(run(['web/src/lib/fields.ts'], { map: null }).full, ['no web test map']);
	assert.match(run([]).full?.join() ?? '', /no file differs/);
});

test('a function start reads back to the source it was compiled from', () => {
	// Line 0: column 0 is a.ts, column 10 b.ts; line 1: column 0 b.ts, column 5 a.ts.
	const lines = decodeMappings('AAAA,UCAA;AAAA,KDAA');
	assert.deepEqual(lines, [[[0, 0], [10, 1]], [[0, 1], [5, 0]]]);
	const starts = [0, 20];
	assert.deepEqual([3, 12, 21, 26].map((o) => sourceAt(lines, starts, o)), [0, 1, 1, 0]);
	// A multi-digit VLQ, and a segment with no source.
	assert.deepEqual(decodeMappings('wCAAA,C'), [[[40, 0]]]);
	assert.equal(ours('../../../../../../src/lib/x.ts'), 'src/lib/x.ts');
	assert.equal(ours('../../../src/lib/wasm/datum-cli.wasm?url'), 'src/lib/wasm/datum-cli.wasm');
	assert.equal(ours('../../node_modules/.pnpm/svelte@4/node_modules/svelte/src/runtime/x.js'), undefined);
});

test('every test that reads src/ or the build as data is one the map always runs', () => {
	const tests = new URL('.', import.meta.url);
	const always = readdirSync(tests, { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && READS_TREE.test(readFileSync(new URL(f, tests), 'utf8')));
	for (const f of ['act-guard', 'calls-pure', 'inlinesvg', 'offline', 'pwa', 'title', 'wasmcache', 'welcome']) {
		assert.ok(always.includes(`${f}.test.mjs`), f);
	}
	assert.ok(!always.includes('echo.test.mjs'), 'echo reads a kit, not the tree');
	assert.ok(READS_TREE.test(`await readFile(join(web, 'build', 'x.js'))`));
});

// The preload writes inside `exit`, which only works because the inspector
// answers an in-thread `post` before it returns. If a Node ever stops doing
// that, a run maps nothing; this is where that shows first.
test('the preload writes what a process ran when it exits, by itself or by process.exit', () => {
	const preload = new URL('./support/coverage.mjs', import.meta.url).href;
	const split = new URL('../src/lib/shellSplit.ts', import.meta.url).href;
	for (const end of ['', 'process.exit(0);']) {
		const dir = mkdtempSync(join(tmpdir(), 'datum-coverage-'));
		const r = spawnSync(
			process.execPath,
			['--import', preload, '--input-type=module', '-e', `const { shellSplit } = await import(${JSON.stringify(split)}); shellSplit('a b'); ${end}`],
			{ env: { ...process.env, NODE_OPTIONS: '', DATUM_TEST_COVERAGE: dir, DATUM_COVERAGE_TEST: 'tests/probe.test.mjs' }, encoding: 'utf8' }
		);
		const written = readdirSync(dir);
		const out = written.map((f) => JSON.parse(readFileSync(join(dir, f), 'utf8')));
		rmSync(dir, { recursive: true, force: true });
		assert.equal(r.status, 0, r.stderr);
		assert.equal(written.length, 1, `${end || 'natural exit'}: ${r.stderr}`);
		assert.equal(out[0].test, 'tests/probe.test.mjs');
		assert.ok(out[0].node.includes('src/lib/shellSplit.ts'), JSON.stringify(out[0].node));
	}
});
