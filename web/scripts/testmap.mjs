// The web test map (da-e0gtq), da-fnt3v's Rust map for the web suite: a PR
// runs only the web test files whose coverage reaches its change.
//
//   node scripts/testmap.mjs index RAW_DIR OUT.json
//   node scripts/testmap.mjs select MAP.json BASE OUT_DIR
//
// `index` reads one instrumented run (`DATUM_TEST_COVERAGE=RAW_DIR pnpm
// test`, tests/support/coverage.mjs) into `{commit, files: {test: [source,
// …]}, always}`. A test reaches a source file when a function compiled from
// it RAN: in node by its URL, in the browser by the function's first byte
// read back through the build's source maps. A module's top level counts for
// nothing, since in a bundle it runs for every test; so, as in the Rust map,
// a change to a value at a module's top level selects the tests that ran a
// function OF THAT FILE, not those that read the value from elsewhere, and
// staging's full run is what catches that. A source a worker bundles is left
// out (workers are not recorded), so a change to it runs everything. `always`
// is every test that reads `src/` or the build as data, which no coverage
// sees. A `tests/support/` module is reached by the tests whose node ran any
// of it, its top level too, since node loads it alone (da-icb7g).
//
// `select` reads a PR's changed files against the map and writes the test
// files to run, one per line, to OUT_DIR/web.tests, or OUT_DIR/FULL with
// why everything runs. A `web/src/` or `web/tests/support/` file selects the
// tests whose set has it, and a test file added or edited selects itself;
// every test the map does not know runs. A crate's file selects nothing:
// staging's full run tests what it does to the web. Everything runs for any
// other change that can reach the web, a `web/src/` or support file no set
// holds, or a map missing or more than MAX_BEHIND commits behind the base.
//
// The map also holds `functions`, each function's source lines and the tests
// that ran it. `select` never reads it: it is for `testmap-shadow.mjs`, which
// logs what two narrower rules would have picked (da-17v3h).

import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// 25 is past the worst lag vremya measured on the two-hour timer, 21 commits on
// 10-19, where 10 sent 18 of 94 long PR runs to the whole suite (da-eqjzg,
// da-reck6). Keep xtask/src/testmap.rs's equal.
export const MAX_BEHIND = 25;

/** Changed paths no web test reads: the docs, xtask, the repo's own scripts, markdown. */
const NOT_WEB = /^(docs|xtask|scripts)\/|\.md$/;
/**
 * A crate's file but the CLI fixtures dbio.test.mjs reads: the CLI module is
 * built from it, and a PR's web run leaves what that changes to staging's full
 * run, as a PR changing crates/ alone runs no web suite (da-q587o).
 */
const CRATE = /^crates\/(?!datum-cli\/tests\/fixtures\/)/;
/**
 * A test that reads `src/` or the build as data: a `readFile`/`readdir`, sync
 * or not, naming one of them on its line. A test that reads the tree any other
 * way must say so here, or the map takes it for one that ran nothing there.
 */
export const READS_TREE = /\bread(?:File|dir)(?:Sync)?\([^\n]*\b(?:src|build|built|web)\b/;

/**
 * Which test files a change runs: `{full: [reason, …]}`, or `{picked: Map(test
 * → the change that picked it), notes}`. `changed` is repo-relative,
 * `tests` every test file this tree has (`tests/x.test.mjs`), `behind` how
 * far the map's commit is behind the base, or null when it is no ancestor.
 */
export function choose({ map, behind, changed, tests, narrow }) {
	if (!map) return { full: ['no web test map'] };
	const full = [];
	if (behind === null) full.push(`the map's commit ${map.commit} is not an ancestor of the base`);
	else if (behind > MAX_BEHIND) full.push(`the map is ${behind} commits behind the base (more than ${MAX_BEHIND})`);
	if (!changed.length) full.push('no file differs from the base');
	const picked = new Map();
	const pick = (test, why) => tests.includes(test) && !picked.has(test) && picked.set(test, why);
	const notes = [];
	for (const path of changed) {
		const p = path.slice('web/'.length);
		if (/^web\/tests\/.+\.test\.mjs$/.test(path)) {
			if (tests.includes(p)) pick(p, `${path}: added or edited`);
			else notes.push(`${path}: deleted`);
		} else if (/^web\/(src|tests\/support)\//.test(path)) {
			const by = Object.keys(map.files).filter((t) => map.files[t].includes(p));
			if (!by.length) full.push(`${path}: no test's coverage reaches it (new, unreached, or a worker's)`);
			for (const t of narrow ? narrow(p, by) : by) pick(t, path);
		} else if (path.startsWith('web/')) full.push(`${path}: the web suite reads it (fixtures, config, static, scripts)`);
		else if (CRATE.test(path)) notes.push(`${path}: a crate; staging's full web run tests it`);
		else if (NOT_WEB.test(path)) notes.push(`${path}: no web test reads it`);
		else full.push(`${path}: can reach the web (the CLI's build, a fixture, a kit, CI)`);
	}
	if (full.length) return { full };
	for (const t of map.always) pick(t, 'reads src/ or the build as data');
	for (const t of tests) if (!(t in map.files)) pick(t, 'not in the map');
	return { picked, notes };
}

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

/** A source map's `mappings`, per generated line: `[column, source index, source line from 0]` in column order. */
export function decodeMappings(mappings) {
	let source = 0;
	let original = 0;
	return mappings.split(';').map((line) => {
		let column = 0;
		const segments = [];
		for (const segment of line.split(',').filter(Boolean)) {
			const v = [];
			let value = 0;
			let shift = 0;
			for (const ch of segment) {
				const d = B64.indexOf(ch);
				value += (d & 31) << shift;
				if (d & 32) shift += 5;
				else {
					v.push(value & 1 ? -(value >>> 1) : value >>> 1);
					value = shift = 0;
				}
			}
			column += v[0];
			if (v.length > 1) segments.push([column, (source += v[1]), (original += v[2])]);
		}
		return segments;
	});
}

/**
 * The source of the byte at `offset` in a script: `lines` its decoded
 * mappings, `starts` the offset each of its lines starts at. The segment at
 * or before it on its line, else the line's first.
 */
export function sourceAt(lines, starts, offset) {
	let lo = 0;
	let hi = starts.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (starts[mid] <= offset) lo = mid;
		else hi = mid - 1;
	}
	const column = offset - starts[lo];
	const segments = lines[lo] ?? [];
	return (segments.findLast(([c]) => c <= column) ?? segments[0])?.[1];
}

/**
 * The first and last source line, from 1, of the bytes `start` to `end` that
 * came from `source`; null for none. `flat` is a script's `[offset, source,
 * line from 0]` in offset order.
 */
export function linesOf(flat, start, end, source) {
	let lo = 0;
	let hi = flat.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (flat[mid][0] <= start) lo = mid + 1;
		else hi = mid;
	}
	let first = Infinity;
	let last = -1;
	for (let i = Math.max(lo - 1, 0); i < flat.length && flat[i][0] < end; i++) {
		if (flat[i][1] !== source) continue;
		first = Math.min(first, flat[i][2]);
		last = Math.max(last, flat[i][2]);
	}
	return last < 0 ? null : [first + 1, last + 1];
}

/**
 * `{tests, sets, fns: {source: [[first line, last line, index into sets]]}}`
 * from `{source, first, last, tests: Set}`s: 4890 functions share 998 sets of
 * tests, measured in #1458.
 */
export function functionsOf(ran) {
	const tests = [...new Set(ran.flatMap((f) => [...f.tests]))].sort();
	const ids = new Map(tests.map((t, i) => [t, i]));
	const sets = [];
	const seen = new Map();
	const by = new Map();
	for (const f of ran) {
		const key = `${f.source}:${f.first}:${f.last}`;
		by.set(key, by.get(key) ?? { ...f, tests: new Set() });
		for (const t of f.tests) by.get(key).tests.add(t);
	}
	const fns = {};
	for (const f of [...by.values()].sort((a, b) => a.first - b.first || a.last - b.last)) {
		const set = [...f.tests].map((t) => ids.get(t)).sort((a, b) => a - b);
		const key = set.join();
		if (!seen.has(key)) seen.set(key, sets.push(set) - 1);
		(fns[f.source] ??= []).push([f.first, f.last, seen.get(key)]);
	}
	return { tests, sets, fns };
}

/**
 * A source map's path for one of `web/src/`, as `src/…`; undefined for anything
 * else. A worker's map names it under `.svelte-kit/output/`, a page's does not.
 */
export const ours = (source) => {
	const p = source.split('?')[0].replace(/^(\.\.\/)+(\.svelte-kit\/output\/)?/, '');
	return p.startsWith('src/') ? p : undefined;
};

const web = fileURLToPath(new URL('..', import.meta.url));
export const git = (...args) => execFileSync('git', args, { cwd: web, encoding: 'utf8', maxBuffer: 1 << 28 });

/** `ranges` read back to source lines, leaving out what a worker shares. */
function placed(ranges, unmapped) {
	const starts = new Map();
	const nodeLines = (source) => {
		if (!starts.has(source)) {
			const text = readFileSync(join(web, source), 'utf8');
			const at = [0];
			for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) at.push(i + 1);
			starts.set(source, at.map((offset, line) => [offset, 0, line]));
		}
		return starts.get(source);
	};
	const out = [];
	for (const { script, index: i, source: own, range, tests } of ranges.values()) {
		const [start, end] = range.split('-').map(Number);
		const source = own ?? script.sources[i];
		if (!source || unmapped.has(source)) continue;
		const lines = own ? linesOf(nodeLines(own), start, end, 0) : linesOf(script.flat, start, end, i);
		if (lines) out.push({ source, first: lines[0], last: lines[1], tests });
	}
	return out;
}

function index(raw, out) {
	const build = join(web, 'build');
	const scripts = new Map();
	const script = (path) => {
		if (!scripts.has(path)) {
			const file = join(build, path);
			if (!existsSync(`${file}.map`)) return scripts.set(path, null).get(path);
			const text = readFileSync(file, 'utf8');
			const map = JSON.parse(readFileSync(`${file}.map`, 'utf8'));
			const starts = [0];
			for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) starts.push(i + 1);
			const lines = decodeMappings(map.mappings);
			const flat = lines.flatMap((segments, g) => segments.map(([c, source, line]) => [starts[g] + c, source, line]));
			scripts.set(path, { lines, flat, starts, sources: map.sources.map(ours) });
		}
		return scripts.get(path);
	};
	const workers = join(build, '_app/immutable/workers');
	const unmapped = new Set(
		readdirSync(workers)
			.filter((f) => f.endsWith('.map'))
			.flatMap((f) => JSON.parse(readFileSync(join(workers, f), 'utf8')).sources.map(ours))
			.filter(Boolean)
	);
	const files = {};
	/** `script:start-end` → the source a function came from and the tests that ran it, for `functions`. */
	const ranges = new Map();
	const ranIn = (key, where, test) => {
		if (!ranges.has(key)) ranges.set(key, { ...where, tests: new Set() });
		ranges.get(key).tests.add(test);
	};
	let lost = 0;
	let unplaced = 0;
	for (const f of readdirSync(raw).filter((f) => f.endsWith('.json'))) {
		const { test, node, nodeFns = {}, browser } = JSON.parse(readFileSync(join(raw, f), 'utf8'));
		const set = (files[test] ??= new Set());
		for (const s of node) set.add(s);
		for (const [source, fns] of Object.entries(nodeFns)) for (const r of fns) ranIn(`${source}:${r}`, { source, range: r }, test);
		for (const [path, offsets] of Object.entries(browser)) {
			// A page's inline scripts are src/app.html's, which no set holds.
			if (!path.endsWith('.js')) continue;
			const s = script(path);
			if (!s) {
				lost += offsets.length;
				continue;
			}
			for (const r of offsets) {
				const i = sourceAt(s.lines, s.starts, parseInt(r));
				if (i === undefined) unplaced++;
				else {
					set.add(s.sources[i]);
					ranIn(`${path}:${r}`, { script: s, index: i, range: r }, test);
				}
			}
		}
	}
	for (const set of Object.values(files)) for (const s of [...set]) if (!s || unmapped.has(s)) set.delete(s);
	const always = readdirSync(join(web, 'tests'), { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && READS_TREE.test(readFileSync(join(web, 'tests', f), 'utf8')))
		.map((f) => `tests/${f}`);
	const map = {
		commit: git('rev-parse', 'HEAD').trim(),
		files: Object.fromEntries(Object.entries(files).sort().map(([t, s]) => [t, [...s].sort()])),
		always
	};
	// The shadow rules' alone: losing it must not lose the map.
	try {
		map.functions = functionsOf(placed(ranges, unmapped));
		console.log(`functions, for the shadow rules: ${Object.values(map.functions.fns).flat().length} in ${Object.keys(map.functions.fns).length} sources, ${map.functions.sets.length} sets of tests`);
	} catch (e) {
		console.log(`::warning::no functions in the web map, so the shadow rules log nothing: ${e.message}`);
	}
	writeFileSync(out, JSON.stringify(map) + '\n');
	const sources = new Set(Object.values(map.files).flat());
	console.log(`web test map of ${map.commit}: ${Object.keys(files).length} test files, ${sources.size} source files reached`);
	console.log(`left out, a worker's: ${[...unmapped].join(' ')}`);
	console.log(`always run, reading src/ or the build: ${always.join(' ')}`);
	if (lost) console.log(`${lost} functions in scripts with no source map, not mapped`);
	// Vite's preload helper, which it puts unmapped at the head of a chunk.
	if (unplaced) console.log(`${unplaced} functions on a line the source map says nothing of (Vite's preload helper), not mapped`);
}

/** What `choose` reads, from a map's path and the base. */
export function inputs(mapPath, base) {
	const map = existsSync(mapPath) ? JSON.parse(readFileSync(mapPath, 'utf8')) : undefined;
	const ancestor = map && spawnSync('git', ['merge-base', '--is-ancestor', map.commit, base], { cwd: web }).status === 0;
	const behind = ancestor ? Number(git('rev-list', '--count', `${map.commit}..${base}`)) : null;
	const changed = git('diff', '--name-only', '--no-renames', `${base}...HEAD`).split('\n').filter(Boolean);
	// test.mjs's own list: the DEBUG files are not run under CI.
	const skipDebug = process.env.CI && process.env.DATUM_TEST_DEBUG !== '1';
	const tests = readdirSync(join(web, 'tests'), { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.test.mjs') && !(skipDebug && /^debug[^/]*\.test\.mjs$/.test(f)))
		.map((f) => `tests/${f}`)
		.sort();
	return { map, behind, changed, tests };
}

function select(mapPath, base, out) {
	mkdirSync(out, { recursive: true });
	for (const f of ['FULL', 'web.tests']) rmSync(join(out, f), { force: true });
	const { map, behind, changed, tests } = inputs(mapPath, base);
	if (map) console.log(`web test map of ${map.commit}: ${Object.keys(map.files).length} test files`);
	const { full, picked, notes } = choose({ map, behind, changed, tests });
	if (full) {
		console.log('FULL WEB SUITE:');
		for (const r of full) console.log(`  ${r}`);
		writeFileSync(join(out, 'FULL'), full.join('\n') + '\n');
		return;
	}
	for (const n of notes) console.log(n);
	for (const [t, why] of picked) console.log(`  ${t} ← ${why}`);
	console.log(`${picked.size} of ${tests.length} web test files run, ${tests.length - picked.size} skipped`);
	writeFileSync(join(out, 'web.tests'), [...picked.keys()].map((t) => `${t}\n`).join(''));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const [cmd, ...args] = process.argv.slice(2);
	if (cmd === 'index' && args.length === 2) index(...args);
	else if (cmd === 'select' && args.length === 3) select(...args);
	else {
		console.error('usage: node scripts/testmap.mjs index RAW_DIR OUT.json\n       node scripts/testmap.mjs select MAP.json BASE OUT_DIR');
		process.exit(2);
	}
}
