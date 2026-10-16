// The web test map's instrumented run (da-e0gtq): what each test file
// EXECUTED, for `scripts/testmap.mjs index` to read back to source files.
//
// Loaded into every process of the run by `scripts/test.mjs` (NODE_OPTIONS
// `--import`), only when `DATUM_TEST_COVERAGE` names a directory; it writes
// nothing else and touches nothing when it is unset.
//
// Two sides, one rule: a FUNCTION ran. A module's top level runs wherever its
// chunk loads, which in a bundle is every test, so it counts for nothing.
// - node: the process's own V8, by the inspector, for the `src/` modules a
//   test imports (the pure tests, the harness), and the `tests/support/`
//   ones, whose top level counts: node loads each alone, never in a bundle
//   (da-icb7g).
// - Chromium: every page of every context Playwright opens in the process,
//   started before the page can navigate and taken before it closes. What is
//   kept is each function's START offset in its script, which lies inside
//   the one source file the function was compiled from; workers and the
//   service worker are not seen, and `index` leaves their sources unmapped.
//
// A process that is not a test file's (one a test spawns) writes under the
// test that spawned it, by `DATUM_COVERAGE_TEST`.

import { Session } from 'node:inspector';
import { writeFileSync } from 'node:fs';
import { basename, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const dir = process.env.DATUM_TEST_COVERAGE;
const web = fileURLToPath(new URL('../..', import.meta.url));
const file = process.argv.slice(1).find((a) => a.endsWith('.test.mjs'));
// A test file of this suite: test-runner.test.mjs runs files of its own.
const test = file && relative(web, file).startsWith('tests/') ? relative(web, file) : process.env.DATUM_COVERAGE_TEST;

if (dir && test) {
	process.env.DATUM_COVERAGE_TEST = test;
	const session = new Session();
	session.connect();
	session.post('Profiler.enable');
	session.post('Profiler.startPreciseCoverage', { callCount: true, detailed: false });

	/** Script pathname → the start offsets of the functions that ran in it. */
	const browser = {};
	const ran = (functions) => functions.map((f) => f.ranges[0]).filter((r) => r.count > 0 && r.startOffset > 0);

	const started = new WeakSet();
	const start = async (page) => {
		if (started.has(page)) return;
		started.add(page);
		await page.coverage.startJSCoverage({ resetOnNavigation: false });
		const close = page.close.bind(page);
		page.close = async (...a) => (await take(page), close(...a));
	};
	const take = async (page) => {
		if (!started.has(page)) return;
		started.delete(page);
		const entries = await page.coverage.stopJSCoverage().catch(() => []);
		for (const { url, functions } of entries) {
			if (!url.startsWith('http')) continue;
			const at = (browser[new URL(url).pathname] ??= new Set());
			for (const r of ran(functions)) at.add(r.startOffset);
		}
	};
	const watch = async (context) => {
		context.on('page', start);
		for (const page of context.pages()) await start(page);
		const close = context.close.bind(context);
		context.close = async (...a) => {
			for (const page of context.pages()) await take(page);
			return close(...a);
		};
		return context;
	};
	const watchBrowser = (b) => {
		const newContext = b.newContext.bind(b);
		// `newPage` makes its context through this too.
		b.newContext = async (...a) => watch(await newContext(...a));
		const close = b.close.bind(b);
		b.close = async (...a) => {
			for (const context of b.contexts()) for (const page of context.pages()) await take(page);
			return close(...a);
		};
		return b;
	};
	const launch = chromium.launch.bind(chromium);
	chromium.launch = async (...a) => watchBrowser(await launch(...a));
	const persistent = chromium.launchPersistentContext.bind(chromium);
	chromium.launchPersistentContext = async (...a) => watch(await persistent(...a));

	process.on('exit', () => {
		// In-thread, so the callback runs inside `post`, before `exit` returns;
		// testmap-pure.test.mjs holds that, and a run where it stops says so.
		let wrote = false;
		session.post('Profiler.takePreciseCoverage', (_, taken) => {
			const node = taken.result
				.filter((s) => s.url.startsWith('file:'))
				.map((s) => [relative(web, fileURLToPath(s.url)), s.functions])
				.filter(([p, fns]) => (p.startsWith('src/') ? ran(fns).length : p.startsWith('tests/support/') && fns.some((f) => f.ranges[0].count > 0)))
				.map(([p]) => p);
			const out = { test, node, browser: Object.fromEntries(Object.entries(browser).map(([k, v]) => [k, [...v]])) };
			writeFileSync(`${dir}/${basename(test)}.${process.pid}.json`, JSON.stringify(out));
			wrote = true;
		});
		if (!wrote) process.stderr.write(`coverage: ${test} (pid ${process.pid}) wrote nothing: the inspector answered after exit\n`);
	});
}
