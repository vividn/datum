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
 */
export async function withApp(fn) {
	const context = await browser.newContext({ timezoneId: TIMEZONE });
	try {
		const page = await context.newPage();
		const app = await open(page);
		await fn(app);
	} finally {
		await context.close();
	}
}

/** Navigate to the day frame and wait until the database is open. */
async function open(page) {
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));
	await page.goto(baseUrl() + '/');
	await ready(page);
	return app(page, errors);
}

async function ready(page) {
	// The REPL input is disabled until `initDb()` resolves, so it is the
	// app's own statement that the database is open.
	await page.locator('.repl input:not([disabled])').waitFor();
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

function app(page, errors) {
	return {
		page,
		/** Uncaught errors the page threw, in order. */
		errors,

		/** The seam. One command in, one RunResult out. */
		run: (argv) => page.evaluate((a) => window.datum.run(a), argv),

		/** The seam's line form, as the REPL feeds it. */
		runLine: (line) => page.evaluate((l) => window.datum.runLine(l), line),

		/** What actually got stored, newest first. */
		entries: (limit = 50) => page.evaluate((n) => window.datum.entries(n), limit),

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

		/** The REPL's rendered stdout/stderr blocks, oldest first. */
		outputs: () => page.locator('.repl output').allInnerTexts(),

		/**
		 * What one of the day frame's panes is showing, by its title.
		 *
		 * Each pane is one datum command rendered as text ($lib/panes.ts),
		 * so this is the command's own output as the reader sees it.
		 */
		paneText: (title) =>
			page
				.locator('section.pane', { has: page.locator('h2', { hasText: title }) })
				.locator('.body')
				.innerText(),

		/** Reload the page and wait for the app to come back up. */
		async reload() {
			await page.reload();
			await ready(page);
		},

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
