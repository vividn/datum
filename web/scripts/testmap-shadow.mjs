// Two narrower selection rules for the web test map, in shadow (da-17v3h): CI
// runs what `testmap.mjs select` chose, and this writes beside it what each rule
// would have run, so a later look can answer «what would they have missed?»
// from real PRs. Measured in #1458 by replay, as its rules 2 and 4.
//
//   node scripts/testmap-shadow.mjs record MAP.json BASE SEL_DIR RED.json
//   node scripts/testmap-shadow.mjs report [REF]
//
// `skip`: a changed line selects the tests that ran the innermost function
// around it, or the file's whole set outside every function (a top level, a
// `<style>`), and a comment, blank or import line, or an insert that only adds
// definitions, selects nothing. The last is a regex, not a parser.
// `narrow`: `skip`, but a line whose tests are more than half the suite selects
// the tests that ran a function of that file no more than half the suite ran.
// A test or support file, `always`, a test the map lacks and every reason for
// the whole suite are `choose`'s, as they are today.
//
// `record` reads SEL_DIR and never writes it. It puts one JSON file,
// `runs/<PR>-<run>.json`, on the `testmap-shadow` branch: the run's head, what
// ran (`actual`, or "full" with `full`'s reasons), `skip`, `narrow`, `failed`
// and `not_run` (null where the suite never said, DATUM_TEST_RED), or `no_rules`
// with why. It always exits 0.
//
// `report` reads that branch, origin's unless REF names one.

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { choose, git, inputs } from './testmap.mjs';

const BRANCH = 'testmap-shadow';

const TRIVIAL = /^\s*(\/\/.*|\/\*.*|\*.*|<!--.*-->\s*|import\s.*|\}\s*from\s.*|)$/;
const DEFINITION =
	/^\s*(export\s+)?(type\s|interface\s|(async\s+)?function[\s*]|(let|const|var)\s+[\w$[{][^=]*(=\s*(\$state(\.raw)?\(|\$derived(\.by)?\(|null\b|undefined\b|true\b|false\b|-?\d|['"`[{]|\(?[^()=]*\)?\s*(:[^=]*)?=>)|;?\s*$))/;

/** Whether new lines only add definitions: nothing runs that another changed line did not name. */
export function definitionsOnly(texts) {
	const real = texts.filter((t) => !TRIVIAL.test(t));
	if (!real.length) return true;
	const depth = (t) => t.length - t.trimStart().length;
	const least = Math.min(...real.map(depth));
	return real.filter((t) => depth(t) === least && !/^\s*[)\]}]/.test(t)).every((t) => DEFINITION.test(t));
}

/** A `-U0` diff of one file: `{a, b, c, d, old, new}` per hunk, `@@ -a,b +c,d @@`. */
export function hunksOf(diff) {
	const lines = diff.split('\n');
	const hunks = [];
	for (let i = 0; i < lines.length; i++) {
		const m = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(lines[i]);
		if (!m) continue;
		const [a, b, c, d] = [m[1], m[2] ?? 1, m[3], m[4] ?? 1].map(Number);
		const body = [];
		for (let left = b + d; left > 0; i++) {
			if (lines[i + 1]?.startsWith('\\')) continue;
			body.push(lines[i + 1]?.slice(1) ?? '');
			left--;
		}
		hunks.push({ a, b, c, d, old: body.slice(0, b), new: body.slice(b) });
	}
	return hunks;
}

/** The lines of the change's old side that the rules read: what `TRIVIAL` and `definitionsOnly` leave. */
export function linesThatCount(hunks) {
	const keep = new Set();
	for (const { a, b, old, new: added } of hunks) {
		if ([...old, ...added].every((t) => TRIVIAL.test(t))) continue;
		if (b === 0 && definitionsOnly(added)) continue;
		const real = old.flatMap((t, i) => (TRIVIAL.test(t) ? [] : [a + i]));
		for (const l of real.length ? real : [Math.max(a, 1), a + 1]) keep.add(l);
	}
	return [...keep];
}

/** A line of a diff's new side (`lag`, the map's commit to the base) on its old side, the map's: a changed line is its hunk's. */
export function toMapLines(line, lag) {
	let delta = 0;
	for (const { a, b, c, d } of lag) {
		const start = d ? c : c + 1;
		if (line < start) break;
		if (line < start + d) {
			const first = Math.max(a, 1);
			return b ? Array.from({ length: b }, (_, i) => first + i) : [first, a + 1];
		}
		delta = (b ? a + b : a + 1) - (start + d);
	}
	return [line + delta];
}

/**
 * `choose`'s `narrow` for a rule, `skip` or `narrow`. `linesOf(p)` is a source's
 * changed lines in the map's coordinates.
 */
export function narrowing(map, rule, linesOf) {
	const { tests, sets, fns } = map.functions;
	const half = Object.keys(map.files).length / 2;
	const setOf = (fn) => sets[fn[2]].map((i) => tests[i]);
	const around = (p, line) => {
		const holding = (fns[p] ?? []).filter(([first, last]) => first <= line && line <= last);
		const least = Math.min(...holding.map(([first, last]) => last - first));
		return holding.length ? holding.filter(([first, last]) => last - first === least).flatMap(setOf) : null;
	};
	const users = (p) => (fns[p] ?? []).map(setOf).filter((s) => s.length <= half).flat();
	return (p, by) => {
		if (p.startsWith('tests/support/')) return by;
		const out = new Set();
		for (const line of linesOf(p)) {
			const t = [...new Set(around(p, line) ?? by)];
			for (const x of rule === 'narrow' && t.length > half ? users(p) : t) out.add(x);
		}
		return out;
	};
}

/** The record of one run: what ran, read from SEL_DIR, and what each rule would have. */
export function recordOf(mapPath, base, sel, redPath, env = process.env) {
	const inputsOf = inputs(mapPath, base);
	const { map } = inputsOf;
	const red = existsSync(redPath) ? JSON.parse(readFileSync(redPath, 'utf8')) : { failed: null, not_run: null };
	const full = existsSync(join(sel, 'FULL'));
	const actual = full ? 'full' : existsSync(join(sel, 'web.tests')) ? readFileSync(join(sel, 'web.tests'), 'utf8').split('\n').filter(Boolean) : [];
	const record = {
		pr: Number(env.PR) || null,
		run: Number(env.RUN) || null,
		head: env.HEAD_SHA ?? git('rev-parse', 'HEAD').trim(),
		at: new Date().toISOString(),
		status: env.STATUS ?? null,
		map: map?.commit ?? null,
		behind: inputsOf.behind ?? null,
		actual,
		...red
	};
	if (full) return { ...record, full: readFileSync(join(sel, 'FULL'), 'utf8').split('\n').filter(Boolean) };
	if (!map?.functions) return { ...record, no_rules: 'the map holds no functions: built before da-17v3h, or indexing them failed' };
	const mergeBase = git('merge-base', base, 'HEAD').trim();
	const memo = new Map();
	const linesOf = (p) => {
		if (!memo.has(p)) {
			const path = `:(top)web/${p}`; // git runs in web/, and a pathspec reads from there
			const hunks = hunksOf(git('diff', '-U0', '--no-renames', `${base}...HEAD`, '--', path));
			const lag = hunksOf(git('diff', '-U0', '--no-renames', map.commit, mergeBase, '--', path));
			memo.set(p, linesThatCount(hunks).flatMap((l) => toMapLines(l, lag)));
		}
		return memo.get(p);
	};
	for (const rule of ['skip', 'narrow']) {
		const r = choose({ ...inputsOf, narrow: narrowing(map, rule, linesOf) });
		record[rule] = r.full ? 'full' : [...r.picked.keys()].sort();
	}
	return record;
}

/** Adds `path` on `BRANCH` and pushes it, reading the branch again when another run got there first. */
function push(path, text) {
	const env = {
		...process.env,
		GIT_INDEX_FILE: join(tmpdir(), `testmap-shadow-${process.pid}.index`),
		GIT_AUTHOR_NAME: 'testmap workflow',
		GIT_AUTHOR_EMAIL: 'testmap@git.vividn.dev',
		GIT_COMMITTER_NAME: 'testmap workflow',
		GIT_COMMITTER_EMAIL: 'testmap@git.vividn.dev'
	};
	const run = (args, input) => {
		const r = spawnSync('git', args, { cwd: fileURLToPath(new URL('..', import.meta.url)), env, input, encoding: 'utf8' });
		return r.status === 0 ? r.stdout.trim() : null;
	};
	// A step that failed must not reach the push: an index that never read the
	// tip would commit a tree without the earlier runs.
	const must = (args, input) => {
		const out = run(args, input);
		if (out === null) throw new Error(`git ${args[0]} failed`);
		return out;
	};
	const ref = `refs/remotes/origin/${BRANCH}`;
	try {
		for (let attempt = 1; attempt <= 5; attempt++) {
			const tip = run(['fetch', '-q', 'origin', `+refs/heads/${BRANCH}:${ref}`]) === null ? null : run(['rev-parse', ref]);
			must(tip ? ['read-tree', tip] : ['read-tree', '--empty']);
			const blob = must(['hash-object', '-w', '--stdin'], text);
			must(['update-index', '--add', '--cacheinfo', `100644,${blob},${path}`]);
			const commit = must(['commit-tree', must(['write-tree']), ...(tip ? ['-p', tip] : []), '-m', path]);
			if (run(['push', '-q', 'origin', `${commit}:refs/heads/${BRANCH}`]) !== null) return true;
			spawnSync('sleep', [String(attempt * Number(process.env.DATUM_SHADOW_BACKOFF_S ?? 2))]);
		}
		return false;
	} finally {
		rmSync(env.GIT_INDEX_FILE, { force: true });
	}
}

function record(mapPath, base, sel, redPath) {
	const r = recordOf(mapPath, base, sel, redPath);
	const size = (x) => (Array.isArray(x) ? x.length : (x ?? '-'));
	console.log(`shadow rules: ran ${size(r.actual)}, skip ${size(r.skip)}, narrow ${size(r.narrow)}${r.no_rules ? ` (${r.no_rules})` : ''}`);
	const path = `runs/${r.pr ?? 'local'}-${r.run ?? r.head.slice(0, 12)}.json`;
	const text = JSON.stringify(r) + '\n';
	if (push(path, text)) console.log(`shadow rules: ${path} on ${BRANCH}`);
	else console.log(`::warning::could not push ${path} to ${BRANCH}`);
}

const mean = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 0);
const median = (xs) => (xs.length ? [...xs].sort((a, b) => a - b)[xs.length >> 1] : 0);

/** The report's lines, from every record and each test file's seconds. */
export function reportOf(records, durations = {}) {
	const seconds = (ts) => ts.reduce((a, t) => a + (durations[t] ?? 0), 0);
	const narrowed = records.filter((r) => Array.isArray(r.actual) && Array.isArray(r.skip));
	const when = records.map((r) => r.at).sort();
	const out = [
		`${records.length} runs of ${new Set(records.map((r) => r.pr)).size} PRs, ${when[0]?.slice(0, 10) ?? '-'} to ${when.at(-1)?.slice(0, 10) ?? '-'}:` +
			` ${records.filter((r) => r.actual === 'full').length} ran the whole suite, ${records.filter((r) => r.no_rules).length} on a map with no functions, ${narrowed.length} to compare`,
		'rule     files mean/median  seconds mean'
	];
	for (const rule of ['actual', 'skip', 'narrow']) {
		const of = narrowed.map((r) => r[rule]);
		out.push(`${rule.padEnd(8)} ${String(Math.round(mean(of.map((x) => x.length)))).padStart(5)} / ${String(median(of.map((x) => x.length))).padEnd(9)} ${Math.round(mean(of.map(seconds)))}`);
	}
	const reds = narrowed.filter((r) => r.failed?.length);
	const green = new Set(records.filter((r) => r.status === 'success').map((r) => r.head));
	const lost = { skip: 0, narrow: 0 };
	out.push(`${reds.length} red runs to compare (+1 is the same head green on another run, so likely a flake; which are regressions is a person's call):`);
	for (const r of reds) {
		const missed = (rule) => r.failed.filter((f) => !r[rule].includes(f));
		for (const rule of ['skip', 'narrow']) if (missed(rule).length === r.failed.length) lost[rule]++;
		out.push(
			`  #${r.pr} run ${r.run} ${r.head.slice(0, 9)}${green.has(r.head) ? ' +1' : ''}: failed ${r.failed.join(' ')}` +
				`; skip misses ${missed('skip').join(' ') || 'none'}; narrow misses ${missed('narrow').join(' ') || 'none'}`
		);
	}
	out.push(`red runs a rule would have let go green, holding no failed file: skip ${lost.skip}, narrow ${lost.narrow}`);
	const unknown = narrowed.filter((r) => r.failed === null && r.status !== 'success').length;
	if (unknown) out.push(`${unknown} runs not green with no failed list (cut short, or a step before the suite failed): read each run's log`);
	return out;
}

function report(ref) {
	if (!ref) {
		git('fetch', '-q', 'origin', `+refs/heads/${BRANCH}:refs/remotes/origin/${BRANCH}`);
		ref = `refs/remotes/origin/${BRANCH}`;
	}
	const paths = git('ls-tree', '-r', '--full-tree', '--name-only', ref, 'runs/').split('\n').filter(Boolean);
	const records = paths.map((p) => JSON.parse(git('show', `${ref}:${p}`)));
	let durations = {};
	try {
		git('fetch', '-q', 'origin', '+refs/heads/testmap:refs/remotes/origin/testmap');
		durations = JSON.parse(git('show', 'refs/remotes/origin/testmap:web-durations.json'));
	} catch {
		console.log('no web-durations.json on the testmap branch: seconds read 0');
	}
	for (const line of reportOf(records, durations)) console.log(line);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const [cmd, ...args] = process.argv.slice(2);
	if (cmd === 'record' && args.length === 4) {
		try {
			record(...args);
		} catch (e) {
			console.log(`::warning::the test map's shadow log failed, and the selection is untouched: ${e.stack}`);
		}
	} else if (cmd === 'report' && args.length <= 1) report(args[0]);
	else {
		console.error('usage: node scripts/testmap-shadow.mjs record MAP.json BASE SEL_DIR RED.json\n       node scripts/testmap-shadow.mjs report [REF]');
		process.exit(2);
	}
}
