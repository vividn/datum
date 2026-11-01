// The test map's shadow rules (da-17v3h), with no browser: what each would have
// picked, that CI's own selection does not move for them, and the log's way from
// a run to the report, in a throwaway repository.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { functionsOf, linesOf } from '../scripts/testmap.mjs';
import { definitionsOnly, hunksOf, linesThatCount, narrowing, reportOf, toMapLines } from '../scripts/testmap-shadow.mjs';

const scripts = fileURLToPath(new URL('../scripts/', import.meta.url));
// Apart from any read of it, or READS_TREE takes this file for one reading the tree.
const list = (dir) => join(dir, 'web.tests');
const SOURCE = 'src/lib/a.ts';
const TESTS = ['tests/boot.test.mjs', 'tests/boot2.test.mjs', 'tests/boot3.test.mjs', 'tests/deep.test.mjs', 'tests/other.test.mjs'];
const TEXT = `import x from './x';
// about mount
export function mount() {
	return x;
}

export function deep() {
	return 1;
}
export const top = 1;
`;
// `mount` (3-5) ran in every boot test, three of five; `deep` (7-9) in one.
const FUNCTIONS = functionsOf([
	{ source: SOURCE, first: 3, last: 5, tests: new Set(TESTS.slice(0, 3)) },
	{ source: SOURCE, first: 7, last: 9, tests: new Set(['tests/deep.test.mjs']) }
]);
const FILES = {
	'tests/boot.test.mjs': [SOURCE],
	'tests/boot2.test.mjs': [SOURCE],
	'tests/boot3.test.mjs': [SOURCE],
	'tests/deep.test.mjs': [SOURCE],
	'tests/other.test.mjs': ['src/lib/b.ts']
};

test('comments, blanks and imports count for nothing, nor does an insert of definitions alone', () => {
	const of = (diff) => linesThatCount(hunksOf(diff));
	assert.deepEqual(of('@@ -2 +2 @@\n-// old\n+// new\n'), []);
	assert.deepEqual(of('@@ -1,0 +2 @@\n+import y from "./y";\n'), []);
	assert.deepEqual(of('@@ -5,0 +6,3 @@\n+function added() {\n+\treturn 2;\n+}\n'), []);
	assert.ok(definitionsOnly(['export const n = 1;', "let s = $state('');", 'type T = string;']));
	assert.ok(!definitionsOnly(['mount();']));
	// Git gives a change group's deletions, then its additions: the comment counts for nothing, the code does.
	assert.deepEqual(of('@@ -1,2 +1,2 @@\n-// a\n-\tb();\n+// x\n+\ty();\n'), [2]);
	// A line that runs, a changed one, and a deleted `-- x` that reads like a header.
	assert.deepEqual(of('@@ -5,0 +6 @@\n+\tmount();\n'), [5, 6]);
	assert.deepEqual(of('@@ -4 +4 @@\n-\treturn x;\n+\treturn y;\n'), [4]);
	assert.deepEqual(of('@@ -4 +4 @@\n--- x\n+++ y\n'), [4]);
	assert.deepEqual(of('@@ -4 +4 @@\n-\treturn x;\n\\ No newline at end of file\n+\treturn y;\n\\ No newline at end of file\n'), [4]);
});

test("a changed line reads back to the map's own lines across what moved since", () => {
	// Two lines inserted after 5, then lines 10-11 deleted, then 20 changed.
	const lag = hunksOf('@@ -5,0 +6,2 @@\n+a\n+b\n@@ -10,2 +11,0 @@\n-c\n-d\n@@ -20 +20 @@\n-e\n+f\n');
	assert.deepEqual(toMapLines(5, lag), [5]);
	assert.deepEqual(toMapLines(6, lag), [5, 6]);
	assert.deepEqual(toMapLines(8, lag), [6]);
	assert.deepEqual(toMapLines(11, lag), [9]);
	assert.deepEqual(toMapLines(12, lag), [12]);
	assert.deepEqual(toMapLines(20, lag), [20]);
	assert.deepEqual(toMapLines(21, lag), [21]);
});

test('a function read back from a bundle takes the lines of its own source', () => {
	// offset, source, line from 0
	const flat = [[0, 0, 9], [10, 1, 2], [20, 1, 4], [30, 0, 7], [40, 1, 6], [50, 1, 90]];
	assert.deepEqual(linesOf(flat, 12, 45, 1), [3, 7]);
	assert.equal(linesOf(flat, 31, 39, 1), null);
});

test('skip takes the innermost function, the whole file outside any; narrow turns a boot function to the narrower ones', () => {
	const map = { files: FILES, functions: FUNCTIONS };
	const by = Object.keys(FILES).filter((t) => FILES[t].includes(SOURCE));
	const rule = (name, lines) => [...narrowing(map, name, () => lines)(SOURCE, by)].sort();
	assert.deepEqual(rule('skip', [4]), TESTS.slice(0, 3));
	assert.deepEqual(rule('narrow', [4]), ['tests/deep.test.mjs']);
	for (const name of ['skip', 'narrow']) assert.deepEqual(rule(name, [8]), ['tests/deep.test.mjs']);
	// Outside every function: the file's whole set, which is a majority too.
	assert.deepEqual(rule('skip', [10]), by.slice().sort());
	assert.deepEqual(rule('narrow', [10]), ['tests/deep.test.mjs']);
	// A support module is by file under both.
	assert.deepEqual(narrowing(map, 'narrow', () => [])('tests/support/s.mjs', ['tests/deep.test.mjs']), ['tests/deep.test.mjs']);
});

test('the report names what a rule would have missed', () => {
	const run = { pr: 7, run: 70, head: 'a'.repeat(40), at: '2026-11-01T00:00:00Z', status: 'failure', actual: ['tests/a.test.mjs', 'tests/b.test.mjs'], skip: ['tests/a.test.mjs', 'tests/b.test.mjs'], narrow: ['tests/b.test.mjs'], failed: ['tests/a.test.mjs'], not_run: [] };
	const lines = reportOf([run, { ...run, run: 71, status: 'success', failed: [] }, { ...run, run: 72, actual: 'full', skip: undefined, narrow: undefined, full: ['ci'], failed: null }], { 'tests/a.test.mjs': 10, 'tests/b.test.mjs': 5 }).join('\n');
	assert.match(lines, /3 runs of 1 PRs/);
	assert.match(lines, /1 ran the whole suite/);
	assert.match(lines, /#7 run 70 a{9} \+1: failed tests\/a\.test\.mjs; skip misses none; narrow misses tests\/a\.test\.mjs/);
	assert.match(lines, /holding no failed file: skip 0, narrow 1/);
	assert.match(lines, /narrow\s+1 \/ 1\s+5/);
});

/** A repository with the two scripts, a base and a PR's branch changing `edit` in a.ts; a bare origin. */
function repo(t, edit) {
	const dir = mkdtempSync(join(tmpdir(), 'datum-shadow-'));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const work = join(dir, 'work');
	const web = join(work, 'web');
	const env = { ...process.env, GIT_DIR: undefined, GIT_AUTHOR_NAME: 't', GIT_AUTHOR_EMAIL: 't@t', GIT_COMMITTER_NAME: 't', GIT_COMMITTER_EMAIL: 't@t', CI: '1' };
	const git = (...a) => execFileSync('git', a, { cwd: work, env, encoding: 'utf8' }).trim();
	const put = (path, text) => {
		mkdirSync(dirname(join(work, path)), { recursive: true });
		writeFileSync(join(work, path), text);
	};
	mkdirSync(work);
	git('init', '-q', '-b', 'staging');
	git('init', '-q', '--bare', join(dir, 'origin.git'));
	git('remote', 'add', 'origin', join(dir, 'origin.git'));
	mkdirSync(join(web, 'scripts'), { recursive: true });
	for (const f of ['testmap.mjs', 'testmap-shadow.mjs']) copyFileSync(join(scripts, f), join(web, 'scripts', f));
	put(`web/${SOURCE}`, TEXT);
	for (const f of TESTS) put(`web/${f}`, '');
	git('add', '.');
	git('commit', '-q', '-m', 'base');
	const commit = git('rev-parse', 'HEAD');
	git('checkout', '-q', '-b', 'pr');
	put(`web/${SOURCE}`, edit(TEXT));
	git('commit', '-qam', 'pr');
	const map = (functions) => {
		const path = join(dir, functions ? 'map.json' : 'plain.json');
		writeFileSync(path, JSON.stringify({ commit, files: FILES, always: [], ...(functions && { functions: FUNCTIONS }) }));
		return path;
	};
	const node = (script, ...args) => spawnSync(process.execPath, [join(web, 'scripts', script), ...args], { cwd: work, env: { ...env, PR: '9', RUN: '90', HEAD_SHA: 'b'.repeat(40), STATUS: 'failure', DATUM_SHADOW_BACKOFF_S: '0' }, encoding: 'utf8' });
	return { dir, git, map, node };
}

test("a change a rule narrows logs its would-be selection, and CI's selection is byte for byte what it was", (t) => {
	const { dir, git, map, node } = repo(t, (s) => s.replace('return x;', 'return x + 1;'));
	const select = (mapPath, out) => {
		const r = node('testmap.mjs', 'select', mapPath, 'staging', out);
		assert.equal(r.status, 0, r.stderr);
		return readFileSync(list(out));
	};
	const sel = join(dir, 'sel');
	const withFunctions = select(map(true), sel);
	assert.deepEqual(withFunctions, select(map(false), join(dir, 'plain-sel')));
	assert.equal(withFunctions.toString(), `${TESTS.slice(0, 4).join('\n')}\n`);

	const red = join(dir, 'red.json');
	writeFileSync(red, JSON.stringify({ failed: ['tests/boot.test.mjs'], not_run: [] }));
	const r = node('testmap-shadow.mjs', 'record', map(true), 'staging', sel, red);
	assert.equal(r.status, 0, r.stderr);
	assert.match(r.stdout, /ran 4, skip 3, narrow 1/);
	assert.deepEqual(readFileSync(list(sel)), withFunctions, 'the shadow leaves the selection as it found it');

	const logged = JSON.parse(git('--git-dir', join(dir, 'origin.git'), 'show', 'testmap-shadow:runs/9-90.json'));
	assert.deepEqual(logged.actual, TESTS.slice(0, 4));
	assert.deepEqual(logged.skip, TESTS.slice(0, 3));
	assert.deepEqual(logged.narrow, ['tests/deep.test.mjs']);
	assert.equal(logged.head, 'b'.repeat(40));
	assert.deepEqual(logged.failed, ['tests/boot.test.mjs']);

	const report = node('testmap-shadow.mjs', 'report');
	assert.equal(report.status, 0, report.stderr);
	assert.match(report.stdout, /#9 run 90 b{9}: failed tests\/boot\.test\.mjs; skip misses none; narrow misses tests\/boot\.test\.mjs/);
	assert.match(report.stdout, /holding no failed file: skip 0, narrow 1/);
});

test('a comment alone narrows to nothing, and a second run keeps the first on the branch', (t) => {
	const { dir, git, map, node } = repo(t, (s) => s.replace('// about mount', '// of mount'));
	const sel = join(dir, 'sel');
	assert.equal(node('testmap.mjs', 'select', map(true), 'staging', sel).status, 0);
	assert.match(node('testmap-shadow.mjs', 'record', map(true), 'staging', sel, join(dir, 'none')).stdout, /ran 4, skip 0, narrow 0/);
	const again = spawnSync(process.execPath, [join(dir, 'work/web/scripts/testmap-shadow.mjs'), 'record', map(true), 'staging', sel, join(dir, 'none')], { cwd: join(dir, 'work'), env: { ...process.env, PR: '9', RUN: '91', HEAD_SHA: 'c'.repeat(40), GIT_COMMITTER_NAME: 't', GIT_COMMITTER_EMAIL: 't@t' }, encoding: 'utf8' });
	assert.equal(again.status, 0, again.stderr);
	const files = git('--git-dir', join(dir, 'origin.git'), 'ls-tree', '-r', '--name-only', 'testmap-shadow');
	assert.equal(files, 'runs/9-90.json\nruns/9-91.json');
	const logged = JSON.parse(git('--git-dir', join(dir, 'origin.git'), 'show', 'testmap-shadow:runs/9-90.json'));
	assert.equal(logged.failed, null, 'no red file: the suite never said');
});

test('a whole-suite run and a map before the functions are logged as such', (t) => {
	const { dir, git, map, node } = repo(t, (s) => s.replace('return x;', 'return x + 1;'));
	const sel = join(dir, 'sel');
	assert.equal(node('testmap.mjs', 'select', map(false), 'staging', sel).status, 0);
	assert.match(node('testmap-shadow.mjs', 'record', map(false), 'staging', sel, join(dir, 'none')).stdout, /map holds no functions/);
	const logged = JSON.parse(git('--git-dir', join(dir, 'origin.git'), 'show', 'testmap-shadow:runs/9-90.json'));
	assert.equal(logged.actual.length, 4);
	assert.equal(logged.skip, undefined);

	mkdirSync(join(dir, 'full'));
	writeFileSync(join(dir, 'full/FULL'), 'a reason\n');
	assert.equal(node('testmap-shadow.mjs', 'record', map(true), 'staging', join(dir, 'full'), join(dir, 'none')).status, 0);
	const full = JSON.parse(git('--git-dir', join(dir, 'origin.git'), 'show', 'testmap-shadow:runs/9-90.json'));
	assert.equal(full.actual, 'full');
	assert.deepEqual(full.full, ['a reason']);
});

test('a failure in the shadow exits 0, warns, and leaves the selection', (t) => {
	const { dir, git, map, node } = repo(t, (s) => s.replace('return x;', 'return x + 1;'));
	const sel = join(dir, 'sel');
	assert.equal(node('testmap.mjs', 'select', map(true), 'staging', sel).status, 0);
	const before = readFileSync(list(sel));
	// A map that does not parse.
	const broken = join(dir, 'broken.json');
	writeFileSync(broken, '{not json');
	const parse = node('testmap-shadow.mjs', 'record', broken, 'staging', sel, join(dir, 'none'));
	assert.equal(parse.status, 0, parse.stderr);
	assert.match(parse.stdout, /::warning::the test map's shadow log failed/);
	// An origin that refuses the push.
	git('remote', 'set-url', 'origin', join(dir, 'nowhere.git'));
	const noPush = node('testmap-shadow.mjs', 'record', map(true), 'staging', sel, join(dir, 'none'));
	assert.equal(noPush.status, 0, noPush.stderr);
	assert.match(noPush.stdout, /::warning::could not push/);
	assert.deepEqual(readFileSync(list(sel)), before);
});
