// Measure the press→feedback chain: a click on a field dot in the widget
// bar, to the moment each pane has redrawn (da-a39xn).
//
// vividn, 2026-09-22: "Click an occured field dot: the output section feels
// instantaneous; ~500-750ms later, the tail widget and nowview update; ~1.5s
// later, finally the ghost text of the command appears in the input bar."
// This script says where those milliseconds go.
//
// What it records, per press, on the BUILT app in a PERSISTENT profile (see
// measure-wasi.mjs: an incognito OPFS costs ~100x):
//
//  · every command through the seam — when it was DISPATCHED (the caller
//    asked), when it SETTLED, and the worker's own phase breakdown
//    (compile / acquire / instantiate / run / flush) for the process itself.
//    The gap between dispatch and the start of the process is queue wait:
//    one WASI process at a time per worker (#lib/cli.ts `enqueue`).
//  · when each region of the page redrew — the bottom bar's output section,
//    the tail pane, the nowview pane, and the ghost in the input box.
//
// Usage:  pnpm run build && node scripts/measure-press.mjs [--entries N]
//                                                          [--presses N]
//                                                          [--throttle R]
//                                                          [--json out]

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { median } from './median.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const flag = (name, fallback) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? fallback : Number(process.argv[i + 1]);
};
const text = (name) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? null : process.argv[i + 1];
};
const ENTRIES = flag('entries', 2000);
const PRESSES = flag('presses', 5);
const THROTTLE = flag('throttle', 1);
const JSON_OUT = text('json');
/**
 * Drop the REPL widget from the layout before measuring.
 *
 * Not a cosmetic variation: the REPL pane is the only thing that calls
 * `registerRepl`, and `engine.press` takes a different path without one —
 * `alone()`, which AWAITS `ran()` and so awaits the whole refresh, where
 * `repl.runArgv` does not (`on:ran={() => void ran()}` in ReplWidget). The
 * ghost is shown after that await either way, so the two layouts put it in
 * two different places in the chain.
 */
const NO_REPL = process.argv.includes('--no-repl');
/** A layout to open by name before measuring — `mobile` is vividn's phone one. */
const LAYOUT = text('layout');
const WIDE = flag('width', 1280);
const HIGH = flag('height', 800);
/**
 * Hold the widget bar's own two commands (`fields --json`, `now --json`)
 * until the panes' have settled, through the seam and without touching the
 * app: the counterfactual for "the bar's refresh is at the HEAD of the
 * queue and every pane pays for it". Not a fix — a price.
 */
const DEFER_BAR = process.argv.includes('--defer-bar');

const round = (x) => Math.round(x * 10) / 10;

const server = await preview({
	root: web,
	logLevel: 'warn',
	preview: { host: '127.0.0.1', port: 0 }
});
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
mkdirSync(join(web, '..', 'target'), { recursive: true });
const dir = mkdtempSync(join(web, '..', 'target', 'press-measure-'));
const context = await chromium.launchPersistentContext(dir, {
	timezoneId: 'UTC',
	viewport: { width: WIDE, height: HIGH }
});

const report = {
	entries: ENTRIES,
	throttle: THROTTLE,
	viewport: `${WIDE}x${HIGH}`,
	layout: LAYOUT ?? 'default',
	repl: !NO_REPL,
	deferBar: DEFER_BAR,
	presses: []
};

try {
	const page = await context.newPage();
	if (THROTTLE > 1) {
		const cdp = await context.newCDPSession(page);
		await cdp.send('Emulation.setCPUThrottlingRate', { rate: THROTTLE });
	}
	await page.goto(base + '/');
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');

	// The seed: ENTRIES rows over three years ending now, across a handful
	// of fields, in one `db restore -` under the header the CLI writes.
	// `coffee` is the point field the press logs.
	const FIELDS = ['coffee', 'water', 'mood', 'posture', 'sleep', 'read', 'walk', 'code'];
	const seeded = await page.evaluate(
		async ([count, fields]) => {
			const backup = await window.datum.runQuiet(['db', 'backup', '-']);
			const header = JSON.parse(backup.stdout.split('\n')[0]);
			const now = Date.now();
			const span = 3 * 365 * 86400_000;
			const lines = [JSON.stringify({ ...header, source_db: 'press', entry_count: count })];
			for (let i = 0; i < count; i++) {
				const at = new Date(now - span + Math.floor((i * span) / count)).toISOString();
				const field = fields[i % fields.length];
				lines.push(
					JSON.stringify({
						hid: 'press' + i.toString().padStart(8, '0'),
						slug: `${field}:${at}`,
						content_hash: i.toString(16).padStart(64, '0'),
						field,
						verb: 'point',
						state: null,
						dur: 'MOMENT',
						following_state: null,
						data: null,
						occur_time: { utc: at, offset_minutes: 0, timezone: 'UTC' },
						create_time: at,
						modify_time: at,
						client_id: '01M281T64122KN04Z1VN626W9J',
						id_structure: '%occur_time%',
						raw_input: `occur ${field}`,
						parser_version: 2
					})
				);
			}
			const r = await window.datum.runQuiet(['db', 'restore', '-'], lines.join('\n') + '\n');
			return { code: r.code, stderr: r.stderr };
		},
		[ENTRIES, FIELDS]
	);
	if (seeded.code !== 0) throw new Error(`seed: ${seeded.stderr}`);

	await page.reload();
	await page.waitForFunction(() => typeof window.datum?.runQuiet === 'function');

	// The layout under measurement: whatever the panes say they are. Named
	// in the report, because a number without its layout is not reproducible.
	await page.locator('section.pane').first().waitFor();
	await page.locator(`.bar button.widget[data-field="${FIELDS[0]}"]`).waitFor();

	// A named layout instead of the default one — the reader's own saved
	// tree, `mobile` among them. Opened after the reload, since the live
	// tree is rebuilt by one and a layout opened before it is overwritten.
	if (LAYOUT) {
		await page.waitForFunction(() => !!document.querySelector('section.pane'), null, {
			timeout: 10_000
		});
		if (!(await page.evaluate((name) => window.datum.shell.open(name), LAYOUT)))
			throw new Error(`no layout named ${LAYOUT}`);
		await page.waitForTimeout(1500);
	}

	// Swap the repl widget for the input widget, which draws a prompt and
	// does NOT register (its own comment says so).
	if (NO_REPL) {
		// After the layout store has loaded, not before: the tree is rebuilt
		// asynchronously after a reload and a swap applied first is overwritten.
		await page.waitForFunction(
			() => !!document.querySelector('section.pane[data-title="repl"]'),
			null,
			{ timeout: 10_000 }
		);
		console.log('dropping the repl widget from the layout');
		await page.evaluate(() => {
			const swap = (node) =>
				node.children
					? { ...node, children: node.children.map(swap) }
					: {
							...node,
							tabs: node.tabs.map((tab) =>
								tab.content.kind === 'repl'
									? { ...tab, content: { ...tab.content, kind: 'input' } }
									: tab.content.children || tab.content.tabs
										? { ...tab, content: swap(tab.content) }
										: tab
							)
						};
			window.datum.layout.apply((layout) => ({ ...layout, root: swap(layout.root) }));
		});
		await page.waitForFunction(
			() => !document.querySelector('section.pane[data-title="repl"]'),
			null,
			{ timeout: 10_000 }
		);
	}

	report.panes = await page.evaluate(() =>
		[...document.querySelectorAll('section.pane')].map((p) => p.dataset.title ?? '(untitled)')
	);
	report.bands = await page.evaluate(() => document.querySelectorAll('.band').length);

	// Every command through the seam, and every redraw of the regions vividn
	// named. Installed BEFORE the first press — a recorder that starts after
	// a command has begun is a recorder that misses its dispatch.
	await page.evaluate((deferBar) => {
		window.__press = { runs: [], paints: [], t0: 0 };
		const isBar = (argv) =>
			(argv[0] === 'fields' || argv[0] === 'now') && argv[1] === '--json' && argv.length === 2;
		const previous = window.datum.setRunner(async (argv, stdin, onStderr, key) => {
			const run = { argv, key, dispatched: performance.now() };
			window.__press.runs.push(run);
			if (deferBar && isBar(argv)) {
				// Two macrotasks for the panes to reach the seam (they dispatch
				// on the epoch, a Svelte tick after this one), then their runs.
				await new Promise((r) => setTimeout(r, 0));
				await new Promise((r) => setTimeout(r, 0));
				await Promise.allSettled(
					window.__press.runs.filter((r) => !isBar(r.argv) && r.promise).map((r) => r.promise)
				);
			}
			run.started = performance.now();
			const promise = previous(argv, stdin, onStderr, key);
			run.promise = promise;
			const result = await promise;
			run.settled = performance.now();
			run.superseded = !!result.superseded;
			run.ms = result.ms;
			return result;
		});
		const watch = (name, selector) => {
			const node = document.querySelector(selector);
			if (!node) return;
			new MutationObserver(() => window.__press.paints.push({ name, at: performance.now() })).observe(
				node,
				{ childList: true, subtree: true, characterData: true, attributes: true }
			);
		};
		watch('output', '.prompt-bar .section.output');
		watch('tail', 'section.pane[data-title="tail"] .body');
		watch('nowview', 'section.pane[data-title="nowview"] .body');
		watch('strip', 'section.pane[data-title="dayview"] .body');
		watch('fields', '.bar .slot');
		// The ghost is its own record: the span appearing, not the box
		// mutating — the fade removes it a second later and that is not a
		// redraw anyone is waiting for.
		new MutationObserver((records) => {
			for (const r of records)
				for (const n of r.addedNodes)
					if (n.nodeType === 1 && n.classList?.contains('ghost'))
						window.__press.paints.push({ name: 'ghost', at: performance.now() });
		}).observe(document.querySelector('.prompt-bar'), { childList: true, subtree: true });
	}, DEFER_BAR);

	const quiet = () =>
		page.waitForFunction(
			() => {
				const runs = window.__press.runs;
				if (!runs.every((r) => r.settled)) return false;
				const last = Math.max(
					0,
					...runs.map((r) => r.settled),
					...window.__press.paints.map((p) => p.at)
				);
				return performance.now() - last > 1200;
			},
			null,
			{ timeout: 120_000, polling: 100 }
		);
	await quiet();

	for (let i = 0; i < PRESSES; i++) {
		await page.evaluate((field) => {
			window.__press.runs.length = 0;
			window.__press.paints.length = 0;
			const dot = document.querySelector(`.bar button.widget[data-field="${field}"]`);
			// t=0 is the press itself, stamped on the same stack as the click.
			window.__press.t0 = performance.now();
			dot.click();
		}, FIELDS[0]);
		await quiet();
		const got = await page.evaluate(() => ({
			t0: window.__press.t0,
			runs: window.__press.runs.map((r) => ({ ...r })),
			paints: window.__press.paints.map((p) => ({ ...p }))
		}));
		report.presses.push(shape(got));
	}

	// The control: each command's round-trip through the seam ALONE, with
	// nothing else queued — so "the CLI is slow" and "we run it N times" can
	// be told apart. The argv are the ones the press just dispatched, verbatim
	// (the pane's own `-n`, the pane's own `--width`): a control that asks a
	// different question than the chain did answers a different question.
	report.solo = await page.evaluate(async (argvs) => {
		const one = async (argv) => {
			const t = performance.now();
			const r = await window.datum.runQuiet(argv);
			const wallMs = performance.now() - t;
			// Let the queue drain and the handles go back, so each is alone.
			await new Promise((s) => setTimeout(s, 400));
			return { argv: argv.join(' '), wallMs, ms: r.ms };
		};
		const out = [];
		for (const argv of argvs) {
			// Five each, reporting the median: the first pays for whatever the
			// page last left cold.
			const runs = [];
			for (let i = 0; i < 5; i++) runs.push(await one(argv));
			runs.sort((a, b) => a.wallMs - b.wallMs);
			out.push(runs[2]);
		}
		return out;
	}, report.presses.at(-1).commands.map((c) => c.words));

	// And the other shape: the same commands asked for all at once, which is
	// what one epoch's pane fan-out is. If the worker queue is the wall, the
	// span of ten concurrent asks is ten serial runs, not one.
	report.concurrent = await page.evaluate(async () => {
		const argv = ['tail', '-n', '10', '--json'];
		const t = performance.now();
		const runs = await Promise.all(Array.from({ length: 8 }, () => window.datum.runQuiet(argv)));
		return {
			spanMs: performance.now() - t,
			runMs: runs.map((r) => r.ms.run),
			acquisitions: runs.filter((r) => r.ms.tries > 0).length
		};
	});
} finally {
	await context.close();
	rmSync(dir, { recursive: true, force: true });
	await server.close();
}

/** One press's runs and paints, as offsets from the press itself. */
function shape({ t0, runs, paints }) {
	const started = runs.filter((r) => !r.superseded);
	return {
		commands: started.map((r) => ({
			// Both shapes: the words for the control to re-run VERBATIM, and
			// the joined string for the report to print. Rebuilding the words
			// by splitting the string on spaces would be a different command
			// the moment one argument contains one.
			words: r.argv,
			argv: r.argv.join(' '),
			key: r.key ?? null,
			dispatchedMs: round(r.dispatched - t0),
			// When the seam actually saw it: the same as `dispatched` unless
			// --defer-bar held it back, and then the difference is the hold,
			// not queue wait.
			startedMs: round((r.started ?? r.dispatched) - t0),
			settledMs: round(r.settled - t0),
			// The process's own time, and what it waited in the queue for.
			processMs: r.ms ? round(r.ms.acquire + r.ms.instantiate + r.ms.run + r.ms.flush) : null,
			queueWaitMs: r.ms
				? round(
						r.settled -
							(r.started ?? r.dispatched) -
							(r.ms.acquire + r.ms.instantiate + r.ms.run + r.ms.flush)
					)
				: null,
			phases: r.ms
				? Object.fromEntries(
						['compile', 'acquire', 'instantiate', 'run', 'flush'].map((k) => [k, round(r.ms[k])])
					)
				: null
		})),
		superseded: runs.length - started.length,
		firstPaintMs: Object.fromEntries(
			[...new Set(paints.map((p) => p.name))].map((name) => [
				name,
				round(paints.find((p) => p.name === name).at - t0)
			])
		),
		lastPaintMs: Object.fromEntries(
			[...new Set(paints.map((p) => p.name))].map((name) => [
				name,
				round(paints.filter((p) => p.name === name).at(-1).at - t0)
			])
		)
	};
}

const last = report.presses.at(-1);
console.log(`layout: ${report.panes.join(' / ')}   (${report.bands} day bands in the strip)`);
console.log(`layout opened: ${report.layout}   repl widget mounted: ${report.repl}   bar deferred: ${report.deferBar}`);
console.log(`seeded: ${report.entries} entries   throttle: ${report.throttle}x   ${report.viewport}`);
console.log('\n── one command alone, nothing else queued ──');
for (const s of report.solo) {
	console.log(
		`  ${s.argv.padEnd(42)} ${String(round(s.wallMs)).padStart(6)}ms  ` +
			`(acquire ${round(s.ms.acquire)} · instantiate ${round(s.ms.instantiate)} · run ${round(s.ms.run)} · flush ${round(s.ms.flush)})`
	);
}
console.log(
	`\n── 8 of the same command asked for at once: ${round(report.concurrent.spanMs)}ms span, ` +
		`median run ${round(median(report.concurrent.runMs))}ms each ──`
);
console.log(`\n── the chain, press ${report.presses.length} of ${report.presses.length} ──`);
console.log('  press (click on the coffee dot)                    t=0');
for (const c of last.commands) {
	console.log(
		`  ${c.argv.slice(0, 44).padEnd(46)} asked ${String(c.dispatchedMs).padStart(7)}  ` +
			`seam ${String(c.startedMs).padStart(7)}  settled ${String(c.settledMs).padStart(7)}  ` +
			`(queued ${String(c.queueWaitMs).padStart(6)} · ran ${String(c.processMs).padStart(6)})`
	);
}
console.log('  painted:');
for (const [name, at] of Object.entries(last.firstPaintMs)) {
	console.log(`    ${name.padEnd(10)} first ${String(at).padStart(7)}   last ${String(last.lastPaintMs[name]).padStart(7)}`);
}
console.log(`  superseded (never ran): ${last.superseded}`);

const across = (pick) => report.presses.map(pick).filter((x) => x !== undefined);
console.log('\n── medians across every press ──');
for (const name of ['output', 'fields', 'nowview', 'tail', 'strip', 'ghost']) {
	const firsts = across((p) => p.firstPaintMs[name]);
	const lasts = across((p) => p.lastPaintMs[name]);
	if (firsts.length) console.log(`  ${name.padEnd(10)} first ${String(round(median(firsts))).padStart(7)}ms   settled ${String(round(median(lasts))).padStart(7)}ms`);
}
console.log(
	`  commands per press: ${median(report.presses.map((p) => p.commands.length))}` +
		`   superseded: ${median(report.presses.map((p) => p.superseded))}`
);

if (JSON_OUT) {
	writeFileSync(JSON_OUT, JSON.stringify(report, null, '\t'));
	console.log(`\nraw numbers: ${JSON_OUT}`);
}
