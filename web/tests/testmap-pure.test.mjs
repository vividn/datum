// The web test map's selection (da-e0gtq), with no browser: which test
// files a PR's changed files run, and the source-map reading the map is
// built with, and the preload that records a process at its exit.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { MAX_BEHIND, READS_TESTS, READS_TREE, SYNC_TESTS, UPGRADE_TESTS, actLines, choose, decodeMappings, ours, sourceAt } from '../scripts/testmap.mjs';

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
const run = (changed, { map = MAP, behind = 0, tests = TESTS, acts } = {}) => choose({ map, behind, changed, tests, acts });
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
	for (const path of ['web/README.md', 'web/tests/fixtures/cli-parity.json', 'web/package.json', 'web/pnpm-lock.yaml', 'web/vite.config.ts', 'web/static/manifest.json', 'web/scripts/test.mjs']) {
		assert.match(run([path]).full?.join() ?? '', /the web suite reads it/, path);
	}
});

test("the web's build, a fixture it reads, a kit or CI runs everything", () => {
	for (const path of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates/datum-cli/tests/fixtures/x.json', 'kits/sample.toml', '.forgejo/workflows/ci.yml']) {
		assert.match(run(['web/src/lib/fields.ts', path]).full?.join() ?? '', /can reach the web/, path);
	}
	// What cannot reach it is passed over.
	const r = run(['web/src/lib/fields.ts', 'crates/datum-core/tests/x.rs', 'crates/datum-cli/tests/sync/a.rs', 'docs/a.md', 'README.md', 'xtask/src/testmap.rs']);
	assert.deepEqual(picked(r), ['tests/bar.test.mjs', 'tests/strip.test.mjs']);
	assert.equal(r.notes.length, 5);
});

test("a crate's source beside a web/src change is a note, and the web change keeps its selection (da-q587o)", () => {
	const crates = ['crates/datum-core/src/db.rs', 'crates/datum-cli/src/help.md', 'crates/datum-cli/Cargo.toml', 'crates/datum-cli/tests/fixtures.rs'];
	const r = run(['web/src/lib/DayStrip.svelte', ...crates]);
	assert.deepEqual(picked(r), ['tests/strip.test.mjs']);
	assert.deepEqual(
		r.notes,
		crates.map((p) => `${p}: a crate; staging's full web run tests it`)
	);
});

test('a migration runs the upgrade tests: alone with no map, stale or not, and beside a web change on top of its selection (da-ako1r)', () => {
	const migration = 'crates/datum-core/src/migrations/mod.rs';
	const tests = [...TESTS, ...UPGRADE_TESTS];
	const known = { ...MAP, files: { ...MAP.files, [UPGRADE_TESTS[0]]: [] } };
	const alone = [migration, 'crates/datum-core/src/schema.sql', 'docs/a.md'];
	for (const opts of [{}, { map: null }, { behind: MAX_BEHIND + 1 }, { behind: null }, { map: { ...MAP, always: ['tests/bar.test.mjs'] } }]) {
		assert.deepEqual(picked(run(alone, { tests, ...opts })), UPGRADE_TESTS);
	}
	assert.equal(run(alone, { tests }).picked.get(UPGRADE_TESTS[0]), `${migration}: a migration`);
	assert.deepEqual(picked(run([migration, 'web/src/lib/DayStrip.svelte'], { tests, map: known })), ['tests/strip.test.mjs', ...UPGRADE_TESTS].sort());
	// Beside what can reach the web, that decides, and any other crate file selects nothing.
	assert.match(run([migration, 'kits/sample.toml'], { tests }).full?.join() ?? '', /can reach the web/);
	assert.match(run([migration, 'web/src/lib/fields.ts'], { tests, map: null }).full?.join() ?? '', /no web test map/);
	assert.deepEqual(picked(run(['web/src/lib/DayStrip.svelte', 'crates/datum-core/src/db.rs'], { tests, map: known })), ['tests/strip.test.mjs']);
});

test('every test moving a file back a schema is an upgrade test, so a migration runs it', () => {
	const dir = new URL('.', import.meta.url);
	const moving = readdirSync(dir, { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && /SET value = '\d+' WHERE key = 'schema_version'/.test(readFileSync(new URL(f, dir), 'utf8')))
		.map((f) => `tests/${f}`);
	assert.deepEqual(moving.sort(), [...UPGRADE_TESTS].sort());
});

test("a change to datum.day's sync store runs the web sync tests: #1601's diff and #1600's, alone and beside a web change (da-33tn8)", () => {
	const tests = [...TESTS, ...SYNC_TESTS];
	const known = { ...MAP, files: { ...MAP.files, ...Object.fromEntries(SYNC_TESTS.map((t) => [t, []])) } };
	// #1601 (8ba40167) and #1600 (fcabd5ce), neither of which ran syncgate.
	const pr1601 = ['crates/datum-cli/tests/sync_chain/handshake.rs', 'crates/datum-commands/src/commands/sync.rs', 'crates/datum-core/src/db/sync/peers.rs', 'crates/datum-parse/src/cli.rs', 'crates/datum-parse/src/parse/sync.rs', 'crates/datum-sync-wire/src/web.rs', 'crates/datum-sync/src/lib.rs'];
	const pr1600 = ['AGENTS.md', 'crates/datum-commands/tests/sync_uncovered_reads.rs', 'crates/datum-core/src/db/mod.rs', 'crates/datum-core/src/db/sync.rs', 'crates/datum-core/src/db/sync_uncovered.sql', 'crates/datum-core/src/db/sync/status.rs'];
	for (const diff of [pr1601, pr1600]) for (const opts of [{ map: known }, { map: null }, { behind: null }]) {
		const p = picked(run(diff, { tests, ...opts }));
		assert.ok(p.includes('tests/syncgate.test.mjs'), `syncgate not run: ${p}`);
		assert.deepEqual(p, [...SYNC_TESTS].sort());
	}
	assert.equal(run(['crates/datum-sync-wire/src/web.rs'], { tests }).picked.get('tests/syncgate.test.mjs'), "crates/datum-sync-wire/src/web.rs: datum.day's sync store");
	assert.deepEqual(picked(run(['crates/datum-sync-wire/src/web.rs', 'web/src/lib/DayStrip.svelte'], { tests, map: known })), ['tests/strip.test.mjs', ...SYNC_TESTS].sort());
	// The native transport and a crate's own tests reach no browser.
	assert.deepEqual(picked(run(['crates/datum-sync/src/lib.rs', 'crates/datum-sync-wire/tests/a.rs'], { tests, map: known })), []);
});

test('every test that starts a relay is a sync test, so a change to the sync store runs it', () => {
	const dir = new URL('.', import.meta.url);
	const relaying = readdirSync(dir, { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && /\bstartRelay\(/.test(readFileSync(new URL(f, dir), 'utf8')))
		.map((f) => `tests/${f}`);
	assert.deepEqual(relaying.sort(), [...SYNC_TESTS].sort());
});

test('a use:act line added or taken out runs the double-outline sweep, live; any other change leaves it to staging (da-isvl4)', () => {
	const diff = [
		'diff --git a/web/src/lib/A.svelte b/web/src/lib/A.svelte',
		'--- a/web/src/lib/A.svelte',
		'+++ b/web/src/lib/A.svelte',
		'@@ -3 +3 @@',
		"-\t<button use:act={'run'} onclick={go}>Turn on sync</button>",
		"+\t<button use:act={'open'} onclick={go}>Turn on sync</button>",
		' \tunchanged use:act context'
	].join('\n');
	assert.equal(actLines(diff).length, 2);
	assert.deepEqual(actLines(diff.replaceAll('use:act', 'onclick')), []);
	const tests = [...TESTS, 'tests/act-taps-desktop.test.mjs', 'tests/act-taps-dialogs-menu.test.mjs', 'tests/act-guard.test.mjs'];
	const map = { ...MAP, files: { ...MAP.files, 'tests/act-taps-desktop.test.mjs': [], 'tests/act-taps-dialogs-menu.test.mjs': [], 'tests/act-guard.test.mjs': [] } };
	const sweep = ['tests/act-taps-desktop.test.mjs', 'tests/act-taps-dialogs-menu.test.mjs'];
	const r = run(['web/src/lib/DayStrip.svelte'], { map, tests, acts: actLines(diff) });
	assert.deepEqual(r.sweep, sweep);
	assert.deepEqual(picked(r), [...sweep, 'tests/strip.test.mjs'].sort());
	assert.match(r.picked.get(sweep[0]), /use:act/);
	// Beside the whole suite it is still named, so the run sets the sweep live.
	assert.deepEqual(run(['web/src/lib/DayStrip.svelte'], { map, tests, behind: MAX_BEHIND + 1, acts: actLines(diff) }).sweep, sweep);
	assert.deepEqual(run(['web/src/lib/DayStrip.svelte'], { map: null, tests, acts: actLines(diff) }).sweep, sweep);
	// No use:act line changed: the sweep's files are not picked, and it stays skipped.
	const none = run(['web/src/lib/DayStrip.svelte'], { map, tests });
	assert.deepEqual(none.sweep, []);
	assert.deepEqual(picked(none), ['tests/strip.test.mjs']);
});

test('a test or support file changed runs the rules over every test file too: #1668 and #1671 ran tutorial-switch and never wait-caps (da-04897)', () => {
	const tests = [...TESTS, 'tests/tutorial-switch.test.mjs', ...READS_TESTS];
	const map = { ...MAP, files: { ...MAP.files, 'tests/strip.test.mjs': [...MAP.files['tests/strip.test.mjs'], 'tests/support/daystrip.mjs'], ...Object.fromEntries([...READS_TESTS, 'tests/tutorial-switch.test.mjs'].map((t) => [t, []])) } };
	const r = run(['web/tests/tutorial-switch.test.mjs'], { map, tests });
	assert.deepEqual(picked(r), ['tests/tutorial-switch.test.mjs', ...READS_TESTS].sort());
	assert.ok(r.picked.get('tests/wait-caps.test.mjs').startsWith('web/tests/tutorial-switch.test.mjs: '));
	assert.deepEqual(picked(run(['web/tests/support/daystrip.mjs'], { map, tests })), ['tests/strip.test.mjs', ...READS_TESTS].sort());
	// A deleted one too: a list the holders read may still name it.
	assert.deepEqual(picked(run(['web/tests/gone.test.mjs'], { map, tests })), [...READS_TESTS].sort());
	// A src file selects neither, unless the map links it.
	assert.deepEqual(picked(run(['web/src/lib/DayStrip.svelte'], { map, tests })), ['tests/strip.test.mjs']);
	const linked = { ...map, files: { ...map.files, 'tests/wait-caps.test.mjs': ['src/lib/DayStrip.svelte'] } };
	assert.deepEqual(picked(run(['web/src/lib/DayStrip.svelte'], { map: linked, tests })), ['tests/strip.test.mjs', 'tests/wait-caps.test.mjs']);
});

test('every test that lists its own directory is a rule over every test file, so a test changed runs it', () => {
	const dir = new URL('.', import.meta.url);
	const scanning = readdirSync(dir, { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && /new URL\('\.\/?', import\.meta\.url\)/.test(readFileSync(new URL(f, dir), 'utf8')) && /\breaddir/.test(readFileSync(new URL(f, dir), 'utf8')))
		.map((f) => `tests/${f}`);
	assert.deepEqual(scanning.sort(), [...READS_TESTS].sort());
});

test('a web/src file no test reached runs everything', () => {
	assert.match(run(['web/src/lib/brandNew.ts']).full?.join() ?? '', /no test's coverage reaches it/);
});

test('a support module runs the tests whose node ran it, and one no test ran runs everything', () => {
	const map = { ...MAP, files: { ...MAP.files, 'tests/strip.test.mjs': [...MAP.files['tests/strip.test.mjs'], 'tests/support/daystrip.mjs'] } };
	assert.deepEqual(picked(run(['web/tests/support/daystrip.mjs'], { map })), ['tests/strip.test.mjs']);
	assert.match(run(['web/tests/support/app.mjs'], { map }).full?.join() ?? '', /no test's coverage reaches it/);
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
	assert.deepEqual(lines, [[[0, 0, 0], [10, 1, 0]], [[0, 1, 0], [5, 0, 0]]]);
	const starts = [0, 20];
	assert.deepEqual([3, 12, 21, 26].map((o) => sourceAt(lines, starts, o)), [0, 1, 1, 0]);
	// A multi-digit VLQ, and a segment with no source.
	assert.deepEqual(decodeMappings('wCAAA,C'), [[[40, 0, 0]]]);
	assert.equal(ours('../../../../../../src/lib/x.ts'), 'src/lib/x.ts');
	assert.equal(ours('../../../src/lib/wasm/datum-cli.wasm?url'), 'src/lib/wasm/datum-cli.wasm');
	// A worker's map, which `index` leaves out of every set by these names.
	assert.equal(ours('../../../../.svelte-kit/output/src/lib/calls.ts'), 'src/lib/calls.ts');
	assert.equal(ours('../../../../.svelte-kit/output/src/lib/wasm/datum-cli.wasm?url'), 'src/lib/wasm/datum-cli.wasm');
	assert.equal(ours('../.svelte-kit/generated/client/app.js'), undefined);
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
test('the preload writes what a process ran when it exits, by itself or by process.exit', (t) => {
	const preload = new URL('./support/coverage.mjs', import.meta.url).href;
	const split = new URL('../src/lib/shellSplit.ts', import.meta.url).href;
	const until = new URL('./support/until.mjs', import.meta.url).href;
	for (const end of ['', 'process.exit(0);']) {
		const dir = mkdtempSync(join(tmpdir(), 'datum-coverage-'));
		t.after(() => rmSync(dir, { recursive: true, force: true }));
		const r = spawnSync(
			process.execPath,
			['--import', preload, '--input-type=module', '-e', `const { shellSplit } = await import(${JSON.stringify(split)}); await import(${JSON.stringify(until)}); shellSplit('a b'); ${end}`],
			{ env: { ...process.env, NODE_OPTIONS: '', DATUM_TEST_COVERAGE: dir, DATUM_COVERAGE_TEST: 'tests/probe.test.mjs' }, encoding: 'utf8' }
		);
		const written = readdirSync(dir);
		const out = written.map((f) => JSON.parse(readFileSync(join(dir, f), 'utf8')));
		assert.equal(r.status, 0, r.stderr);
		assert.equal(written.length, 1, `${end || 'natural exit'}: ${r.stderr}`);
		assert.equal(out[0].test, 'tests/probe.test.mjs');
		assert.ok(out[0].node.includes('src/lib/shellSplit.ts'), JSON.stringify(out[0].node));
		assert.ok(out[0].nodeFns['src/lib/shellSplit.ts'].every((r) => /^\d+-\d+$/.test(r)) && out[0].nodeFns['src/lib/shellSplit.ts'].length, JSON.stringify(out[0].nodeFns));
		// A support module counts by its top level alone: until.mjs's functions never ran.
		assert.ok(out[0].node.includes('tests/support/until.mjs'), JSON.stringify(out[0].node));
	}
});
