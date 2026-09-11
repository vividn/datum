// The harness: a real browser, the real built app, one `run(argv)` seam.
//
// Everything here exists to hand a test three things — `run`, `entries` and
// the page — and then get out of the way. The tests are about datum's
// behaviour, not about the harness, so the harness stays a file.
//
// Why a browser at all: the seam's storage is SQLite-wasm on OPFS inside a
// dedicated worker, which exists nowhere else. A node-side stub would test
// a stub, and the assertion this suite most needs to make — that entries
// survive a reload — is only meaningful against real durable storage. It is
// the assertion that would have caught the storage regression in da-5iqw.
//
// The app is served from the *built* output rather than the dev server, so
// what the tests exercise is what deploys: the same worker chunk, the same
// cross-origin-isolation headers (see web/vite.config.ts).

import { fileURLToPath } from 'node:url';
import { preview } from 'vite';
import { chromium } from 'playwright';

const root = fileURLToPath(new URL('../..', import.meta.url));

/** Every entry the tests log lands on this day, in this zone. */
export const TIMEZONE = 'UTC';

let server;
let browser;

/**
 * Run `prelude` inside the database worker, before the app's own module.
 *
 * Nothing here is a mock of the database: OPFS is real and present and the
 * CLI runs its real course — the prelude only decides what the platform
 * hands it. The app's own worker module is then imported
 * for real; the wrapper has to hold the messages the page posts while that
 * import is in flight, since the module registers its listener at the end
 * of it.
 */
export function inWorker(prelude) {
	return (context) =>
		context.addInitScript((prelude) => {
			const Real = Worker;
			window.Worker = class extends Real {
				constructor(url, options) {
					const target = new URL(url, location.href).href;
					const source = `
						${prelude}
						const queued = [];
						const buffer = (event) => queued.push(event.data);
						self.addEventListener('message', buffer);
						import(${JSON.stringify(target)}).then(() => {
							self.removeEventListener('message', buffer);
							for (const data of queued) self.dispatchEvent(new MessageEvent('message', { data }));
						});
					`;
					super(URL.createObjectURL(new Blob([source], { type: 'text/javascript' })), options);
				}
			};
		}, prelude);
}

/**
 * Pin the database worker's clock to `ms`, and return a setter that moves it.
 *
 * Playwright's `context.clock` fakes the page and not its workers, and the
 * CLI runs in a worker: a test that fakes the day has to fake it there too,
 * or datum draws the real one. The setter goes over a BroadcastChannel,
 * which the worker hears before the next command it is sent.
 */
export function workerClock(ms) {
	return {
		prepare: inWorker(`
			let fixed = ${ms};
			new BroadcastChannel('datum-test-clock').onmessage = (e) => { fixed = e.data; };
			const Real = Date;
			self.Date = class extends Real {
				constructor(...args) { args.length ? super(...args) : super(fixed); }
				static now() { return fixed; }
			};
		`),
		set: (page, to) =>
			page.evaluate((to) => new BroadcastChannel('datum-test-clock').postMessage(to), to)
	};
}

/** Build-independent: `npm test` runs `vite build` first (see package.json). */
export async function startApp() {
	server = await preview({
		root,
		logLevel: 'warn',
		preview: { host: '127.0.0.1', port: 0, strictPort: false }
	});
	browser = await chromium.launch();
}

export async function stopApp() {
	await browser?.close();
	await new Promise((resolve, reject) =>
		server?.httpServer.close((e) => (e ? reject(e) : resolve()))
	);
	// The preview server renders the page in THIS process, so anything the
	// app opens during its server render — a BroadcastChannel, a Worker — is
	// a live handle here, and one that nothing closes keeps node alive after
	// the last test has passed. That is a hang, not a failure, and CI reported
	// it as neither until the job's own timeout (2026-09-09, the change
	// channel of da-eovo). Name it, so the next one is a red line, not a
	// stuck runner.
	const left = process
		.getActiveResourcesInfo()
		.filter((r) => r === 'MessagePort' || r === 'Worker');
	if (left.length) {
		throw new Error(
			`${left.join(', ')} still open after the browser and server closed: ` +
				'something the page does during its server render is holding the process, and the suite will not exit'
		);
	}
}

function baseUrl() {
	const url = server.resolvedUrls?.local?.[0];
	if (!url) throw new Error('preview server reported no local URL');
	return url.replace(/\/$/, '');
}

/**
 * Run `fn` against a freshly opened app in its own browser context.
 *
 * A context per test is a database per test: OPFS is partitioned by
 * context, so tests neither see each other's entries nor collide on the
 * UNIQUE slug of a field logged at the same minute.
 *
 * `prepare` gets the context before the first page exists, which is the
 * only moment an init script can beat the app's own code to the browser.
 *
 * `locale` pins the reader's locale, which is otherwise whatever the
 * machine running the suite happens to have. A test about anything the
 * browser formats by locale must set it, or it asserts about the runner
 * rather than about the app (da-6o81).
 *
 * `viewport` is the size of the screen the app is asked to lay itself out
 * on. The default is Playwright's desktop 1280x720, which is a viewport no
 * phone has — a layout test must name the one it means (da-hkpd).
 *
 * `device` is one of Playwright's own device profiles, spread in whole —
 * a phone is not only a small viewport, it is a device pixel ratio, a
 * touch pointer and overlay scrollbars that take no layout width, and a
 * test about what overflows on a phone wants all four (da-45wt). An
 * explicit `viewport` still wins, so a profile can be resized.
 *
 * `query` is appended to the URL.
 */
export async function withApp(fn, { prepare, locale, viewport, device, query = '' } = {}) {
	const context = await browser.newContext({
		...device,
		timezoneId: TIMEZONE,
		locale,
		...(viewport ? { viewport } : {})
	});
	try {
		await prepare?.(context);
		const page = await context.newPage();
		const app = await open(page, query);
		await fn(app);
	} finally {
		await context.close();
	}
}

/** Navigate to the day frame and wait until the database is open. */
async function open(page, query) {
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));
	await page.goto(baseUrl() + '/' + query);
	await ready(page);
	await countRuns(page);
	return app(page, errors, query);
}

/**
 * Count the commands in flight behind the seam, for `idle()`.
 *
 * Installed through `setRunner`, the seam's own extension point — the
 * same hook the wasm CLI arrives through and `spyOnRunner` chains onto —
 * and after `ready`, so the runner it delegates to has been imported.
 * Installed on every open rather than on first use: a count that starts
 * after a command has already begun is a count that reaches zero early.
 */
function countRuns(page) {
	return page.evaluate(() => {
		window.__datumInflight = 0;
		const previous = window.datum.setRunner((argv, stdin) => {
			window.__datumInflight += 1;
			return previous(argv, stdin).finally(() => (window.__datumInflight -= 1));
		});
	});
}

async function ready(page) {
	// A prompt is disabled until `initDb()` resolves, so an enabled one is
	// the app's own statement that the database is open. The first one: a
	// layout may hold two REPLs (da-jdop), and the input widget's prompt at
	// /input counts too (da-0dhc).
	await page.locator('.repl input:not([disabled]), .input-widget input:not([disabled])').first().waitFor();
	await page.waitForFunction(() => typeof window.datum?.run === 'function');
	// Every pane is a command that runs on mount ($lib/panes.ts) and shows
	// `…` until it answers. Waiting them out means a command a test runs
	// next is the next command the runner sees, not a race with the frame
	// still settling.
	await page.waitForFunction(() =>
		[...document.querySelectorAll('section.pane .body')].every(
			(body) => body.textContent.trim() !== '…'
		)
	);
}

function app(page, errors, query) {
	return {
		page,
		/** Uncaught errors the page threw, in order. */
		errors,

		/** The seam. One command in — and its stdin, if any — one RunResult out. */
		run: (argv, stdin) => page.evaluate(([a, s]) => window.datum.run(a, s), [argv, stdin]),

		/** The seam's line form, as the REPL feeds it. */
		runLine: (line) => page.evaluate((l) => window.datum.runLine(l), line),

		/** What actually got stored, newest first: the CLI's `db backup -` rows. */
		entries: (limit = 50) => page.evaluate((n) => window.datum.entries(n), limit),

		/**
		 * The tail listing as data: the CLI's rows with the colours it
		 * would paint them, and its painted columns. What the tail pane
		 * renders — so a test can hold what is on screen to what datum
		 * said, rather than to a colour written down here.
		 */
		tailRows: () => page.evaluate(() => window.datum.tailRows()),

		/**
		 * The beta banner's sentence about the data.
		 *
		 * The banner reads the storage that actually opened, so this is the
		 * app's own on-screen claim about whether entries survive a reload.
		 */
		bannerText: () => page.locator('aside.beta p').innerText(),

		/** Type a line into the REPL and submit it, as a user does. */
		async submit(line) {
			const before = await page.locator('.repl .echo').count();
			const input = page.locator('.repl input');
			await input.fill(line);
			await input.press('Enter');
			await page.waitForFunction(
				(n) => document.querySelectorAll('.repl .echo').length > n,
				before
			);
		},

		/** Press a widget in the bar, by the field it names. */
		async pressWidget(field) {
			const before = await page.locator('.repl .echo').count();
			await page.locator('.bar button.widget', { hasText: field }).first().click();
			await page.waitForFunction(
				(n) => document.querySelectorAll('.repl .echo').length > n,
				before
			);
		},

		/** The REPL's echoed command lines, oldest first, `❯ ` stripped. */
		echoes: async () =>
			(await page.locator('.repl .echo').allInnerTexts()).map((t) =>
				t.replace(/^\s*❯\s*/, '').trim()
			),

		/**
		 * The REPL's rendered stdout/stderr blocks, oldest first.
		 *
		 * `pre`, because `pre` is what CliOutput renders. The selector read
		 * `output`, an element the app has never had, so this always
		 * answered `[]` — which nothing noticed, a helper that lies being
		 * silent until someone finally asks it something (da-b049).
		 */
		outputs: () => page.locator('.repl pre').allInnerTexts(),

		/**
		 * What one of the day frame's panes is showing, by its title.
		 *
		 * Each pane is one datum command rendered as text ($lib/panes.ts),
		 * so this is the command's own output as the reader sees it. A
		 * pane's title is its `data-title` (da-jdop): the h2 when it has one
		 * tab, the active tab's label when it has several — so a pane
		 * behind another tab is not "showing" anything, and is not found.
		 */
		paneText: (title) =>
			page
				.locator(`section.pane[data-title="${title}"]`)
				.locator('.body')
				.innerText(),

		/**
		 * A pane's table, as the reader sees it: header words and cell text.
		 *
		 * The tail pane is an HTML `<table>` of the CLI's own cells
		 * ($lib/TailTable.svelte), so this is where a test reads what it
		 * shows. Cell text only — the colour is read separately, off the
		 * spans, because it is a different claim.
		 */
		paneTable: (title) =>
			page
				.locator(`section.pane[data-title="${title}"]`)
				.evaluate((pane) => ({
					headers: [...pane.querySelectorAll('thead th')].map((th) => th.textContent),
					keys: [...pane.querySelectorAll('thead th')].map((th) => th.dataset.key),
					rows: [...pane.querySelectorAll('tbody tr')].map((tr) =>
						[...tr.querySelectorAll('td')].map((td) => td.textContent)
					)
				})),

		/**
		 * The chip colours a pane's table cells are painted with, by column.
		 *
		 * One array per row — a cell can carry more than one chip, because
		 * the CLI's state cell is `[LEAD][state][TRAIL]` and each slot is
		 * its own painted block. Backgrounds only, in DOM order, as the
		 * browser reports them.
		 */
		paneCellColors: (title, key) =>
			page
				.locator(`section.pane[data-title="${title}"]`)
				.evaluate(
					(pane, column) =>
						[...pane.querySelectorAll(`tbody td[data-key="${column}"]`)].map((td) =>
							[...td.querySelectorAll('span')]
								.map((s) => getComputedStyle(s).backgroundColor)
								.filter((c) => c !== 'rgba(0, 0, 0, 0)')
						),
					key
				),

		/**
		 * The SVG a pane is showing, as its markup.
		 *
		 * The visual panes' stdout is an SVG document, which CliOutput
		 * draws inline (da-s5da) — so this is the string `run(argv)`
		 * returned, read back off the screen as the page holds it.
		 *
		 * The last picture in the pane: the day strip stacks the days
		 * before today above it, as many as its box holds (da-zb0d), and
		 * today's band is the one under them all. nowview has one.
		 */
		async paneSvg(title) {
			return page
				.locator(`section.pane[data-title="${title}"]`)
				.locator('svg.svg')
				.last()
				.evaluate((svg) => svg.outerHTML);
		},

		/**
		 * Open a second tab of the app, in the SAME browser context.
		 *
		 * The same context is the whole point: one origin is one OPFS, so
		 * two pages here contend for the single-writer SAH pool exactly as
		 * two real tabs do. Two *contexts* get separate storage and would
		 * never reproduce it (da-60ww).
		 */
		async newTab() {
			return open(await page.context().newPage(), query);
		},

		/** Navigate this tab to `path` — `/`, `/input` — and wait for it to come up. */
		async goto(path) {
			await page.goto(baseUrl() + path);
			await ready(page);
		},

		/** Reload the page and wait for the app to come back up. */
		async reload() {
			await page.reload();
			await ready(page);
			await countRuns(page);
		},

		/**
		 * Wait until no command is in flight behind the seam.
		 *
		 * A submit bumps the frame's epoch, and every pane re-runs its
		 * command on it — the strip one per band it holds — behind one
		 * database lock, so how long the frame takes to settle grows with
		 * how much of it is drawn. Counting the runs through the seam's own
		 * extension point (as `spyOnRunner` does) is exact where a quiet
		 * DOM is a guess: the runs have all started by the time `submit`
		 * returns, because the REPL renders its echo and only then says
		 * `ran`, and a pane's screen is a microtask behind its result —
		 * so a poll that sees zero sees every pane drawn.
		 */
		idle: () => page.waitForFunction(() => window.__datumInflight === 0),

		/**
		 * Record every argv the active runner is handed, then delegate.
		 *
		 * Uses `setRunner`, the seam's own extension point — the same hook
		 * the wasm CLI arrives through — so the spy sees exactly what the
		 * implementation sees and nothing has to be mocked.
		 */
		async spyOnRunner() {
			await page.evaluate(async () => {
				// `setRunner` hands back the runner it replaced, and the seam
				// imports its default runner lazily on first use — so something
				// has to have run before there is anything to delegate to.
				await window.datum.run(['tail']);
				window.__datumSeen = [];
				const previous = window.datum.setRunner((argv) => {
					window.__datumSeen.push(argv);
					return previous(argv);
				});
			});
			return () => page.evaluate(() => window.__datumSeen);
		}
	};
}
