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

import { after } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { preview } from 'vite';
import { chromium } from 'playwright';
import { OPFS_DIR } from '../../src/lib/databases.ts';
import { DESK } from './desk.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));

/** Every entry the tests log lands on this day, in this zone. */
export const TIMEZONE = 'UTC';

/**
 * Today at `hour`:`minute` in the harness's zone, as an instant — for
 * `withApp`'s `clock`.
 *
 * **A test that seeds a time must say which day it means.** A bare
 * `-t 0930` means "today at 09:30" against whatever clock the app is
 * holding, so a suite that does not pin that clock seeds a different
 * instant every hour of the day: before 10:00 local the same seed lands
 * in the FUTURE, `datum now` answers that nothing is current, and the
 * panes that draw a current state draw none. Those tests passed every
 * afternoon and failed every night, and the failure was twice charged to
 * innocent branches (da-ldct).
 *
 * So every app is held — at `HELD_AT` unless a test names another instant
 * here — and seeds against it (da-ydao). Today rather than a fixed calendar
 * date, because the seeds and the clock then move together and there is no
 * date in the file to go stale.
 */
export function todayAt(hour, minute = 0) {
	const today = new Date();
	return Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate(), hour, minute);
}

/**
 * Log `lines` and leave the frame showing every one of them.
 *
 * Seeding through `submit` is seeding through the REPL, and each one bumps
 * the frame's epoch: every pane re-runs its command on it — the day strip
 * once per band it holds — behind one database lock. A five-line day
 * therefore paid for five whole redraws, when only the last one is ever
 * looked at (da-x6ea).
 *
 * So: all but the last line through the seam's QUIET door, which stores
 * the same row and moves nothing, and the last one through the bar. The
 * one redraw it buys is over all of them, because a pane re-reads the
 * database rather than accumulating what it has been told.
 *
 * Then `idle`, because `submit` returns on the REPL's own echo while the
 * pane commands it set off are still in flight — a measurement taken
 * straight after it can be of the pane as it was an entry ago.
 *
 * A line that fails THROWS, naming itself and its code (da-pm48t): a seed
 * refused in silence reads, further on, as data the page failed to draw
 * (da-edr7t). The code and not stderr, which a refusal can leave empty —
 * a duplicate `occur` exits 11 with nothing on it. The bar's line is read
 * back off the bar's own receipt, because its echo looks the same either way.
 */
export async function seedLines(app, lines) {
	if (!lines.length) return;
	const refused = (line, { code, stderr }) => {
		if (code !== 0) throw new Error(`seed line \`${line}\` exited ${code}${stderr ? `: ${stderr}` : ''}`);
	};
	for (const line of lines.slice(0, -1)) refused(line, await app.runLine(line));
	await app.submit(lines.at(-1));
	refused(lines.at(-1), (await app.page.evaluate(() => window.datum.bar.last())).result);
	await app.idle();
}

/**
 * Ask `body` the same question at every screen in `screens`, one app.
 *
 * A subtest per screen, so the report still names the size that failed —
 * but ONE boot behind them all. Opening the app is 2.2 s of wasm compile,
 * database and pane commands; resizing the page is ten milliseconds, and
 * the frame is CSS, which relays itself on a resize exactly as it lays
 * itself out on a boot. Asking four screens for four boots is what made
 * reachable.test.mjs the suite's slowest file at 117 s (da-x6ea).
 *
 * Two animation frames after the resize: a ResizeObserver — the strip's,
 * the tail's — fires on the frame after the layout it observed, so a box
 * read any sooner can be the box from the screen before.
 */
export async function atEachScreen(t, page, screens, body) {
	for (const viewport of screens) {
		await t.test(`${viewport.width}x${viewport.height}`, async () => {
			await page.setViewportSize(viewport);
			await page.evaluate(
				() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
			);
			await body(viewport);
		});
	}
}

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
			// One wrapper, however many preludes: a wrapper around a wrapper
			// buffers each message twice and hands the app both copies. Each
			// prelude is a block of its own, so two may name the same thing.
			const preludes = (window.__datumWorkerPreludes ??= []);
			preludes.push(`{${prelude}}`);
			if (preludes.length > 1) return;
			const Real = Worker;
			window.Worker = class extends Real {
				constructor(url, options) {
					const target = new URL(url, location.href).href;
					// What the page knows that a worker cannot read for itself.
					const fromPage = { heldAt: window.__datumHeldAt };
					const source = `
						const fromPage = ${JSON.stringify(fromPage)};
						${preludes.join('\n')}
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
 * The instant every app is held at unless its test names another (da-ydao):
 * today at 15:00, so a `-t 0700` seed is in the past whatever hour the
 * suite happens to run at.
 */
export const HELD_AT = todayAt(15);

/**
 * Hold `Date` at `at` on the page and in every worker it starts.
 *
 * Only `Date`. Timers, animation frames and `performance.now()` stay the
 * browser's own, because Playwright's fake clock replaces
 * `requestAnimationFrame` with a timer and every test that waits on frames
 * would silently wait on something else (measured on PR 27: 14 of 80
 * wheel reads unmoved). And the minute tick ($lib/clock.ts) compares
 * minutes, so a `Date` that never moves never ticks — no redraw can land
 * inside a test's window by the wall clock turning.
 *
 * The CLI runs in the database worker, so the worker is held too, and a
 * move goes to both over a BroadcastChannel, which the worker hears
 * before the next command it is sent. A move is also written down
 * (`localStorage`), because a reload runs these scripts again: the page
 * reads where the clock stands, and hands it to the workers it starts
 * (`fromPage`, `inWorker`), so a reload does not wind time back.
 *
 * `pin: false` leaves the page's `Date` alone, for `holdTimers`, where it
 * is Playwright's.
 */
function holdDate(context, at, pin = true) {
	return Promise.all([
		inWorker(`
			let fixed = fromPage.heldAt;
			new BroadcastChannel('datum-test-clock').onmessage = (e) => { fixed = e.data; };
			${heldDate}
		`)(context),
		context.addInitScript(`{
			let fixed = ${at};
			try { fixed = Number(localStorage.getItem('datum-test-clock') ?? fixed); } catch {}
			window.__datumHeldAt = fixed;
			new BroadcastChannel('datum-test-clock').onmessage = (e) => { fixed = window.__datumHeldAt = e.data; };
			${pin ? heldDate : ''}
		}`)
	]);
}

/** `Date`, held at the `fixed` in scope: every other use of it is the real one. */
const heldDate = `
	const Real = Date;
	self.Date = class extends Real {
		constructor(...args) { args.length ? super(...args) : super(fixed); }
		static now() { return fixed; }
	};
`;

/**
 * The one way a test moves time (`app.clock`), on both sides of the seam.
 *
 * `set(ms)` puts the clock at an instant, `forward(ms)` moves it on — and,
 * under `holdTimers`, fires what came due, so a minute forward is a minute
 * tick. The CLI's clock is moved first, so the run a tick starts sees the
 * new minute. `at()` is where it stands, and `reset()` puts it back where
 * the app opened, for a shared app's next test (`empty`).
 */
function clockOf(page, at, holdTimers) {
	let now = at;
	const move = (to) =>
		page.evaluate((to) => {
			try { localStorage.setItem('datum-test-clock', String(to)); } catch {}
			new BroadcastChannel('datum-test-clock').postMessage(to);
		}, (now = to));
	return {
		at: () => now,
		async set(to) {
			await move(to);
			if (holdTimers) await page.clock.setSystemTime(new Date(to));
		},
		async forward(ms) {
			await move(now + ms);
			if (holdTimers) await page.clock.fastForward(ms);
		},
		async reset() {
			if (now !== at) await this.set(at);
		}
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
 *
 * `timezone` is the reader's zone, and so the `TZ` the CLI is handed. UTC
 * unless a test is about a zone, which UTC's zero offset cannot tell from
 * no zone at all (da-y6ej).
 *
 * `engine` is a Playwright browser type — `webkit` — for a test that has
 * to hold on more than chromium. It gets a browser of its own on a fresh
 * profile (`launchPersistentContext`), torn down with the test.
 *
 * `reducedMotion` is `reduce` for a reader who has asked the system for
 * no animation, which is a different app: what fades for everyone else has
 * to be shown and taken away instead (da-jprg).
 *
 * `home` is the layout this device already has open, as if it had been
 * here before — `main` unless a test says otherwise, because the suite is
 * written against `main` and a FRESH phone opens `mobile` (da-shzs). Written
 * only where nothing is stored, so a reload keeps what the test changed.
 * It has also unlocked it (da-gshh), which a fresh device finds locked.
 * `home: null` is a device that has never opened the site.
 *
 * `desk` is the desktop tree `main` is drawn from: `DESK` (support/desk.mjs),
 * the tree the suite is written against, unless a test says otherwise.
 * `desk: null` is the shipped default (da-ncsyn).
 *
 * `clock` is the instant the app is held at — the page's and, because
 * the CLI runs there, the worker's. EVERY app is held (da-ydao): at
 * `HELD_AT` unless a test names another instant, with `todayAt` above.
 * `clock: null` is the wall clock, for a test about real time passing.
 * `app.clock` moves it (`clockOf`).
 *
 * `holdTimers` hands the page's clock to Playwright, which holds its
 * timers too: nothing the page schedules runs until `app.clock.forward`
 * or `page.clock.runFor` says so. For a test that DRIVES time — a minute
 * tick, a fade — and only that, since it also turns animation frames into
 * timers. It holds the boot as well: `page.clock.runFor(200)` and
 * `app.idle()` before asserting, since a pane's box arrives on a timer.
 */
export async function withApp(fn, options) {
	const { app, close } = await openApp(options);
	try {
		await fn(app);
	} finally {
		await close();
	}
}

/** `withApp`'s own body: the app, and the one call that takes it down again. */
async function openApp({
	prepare,
	clock = HELD_AT,
	holdTimers = false,
	locale,
	viewport,
	device,
	query = '',
	engine,
	reducedMotion,
	home = 'main',
	desk = DESK,
	timezone = TIMEZONE
} = {}) {
	const options = {
		...device,
		timezoneId: timezone,
		locale,
		...(reducedMotion ? { reducedMotion } : {}),
		...(viewport ? { viewport } : {})
	};
	const profile = engine && (await mkdtemp(join(tmpdir(), 'datum-profile-')));
	const context = engine
		? await engine.launchPersistentContext(profile, options)
		: await browser.newContext(options);
	const close = async () => {
		await context.close();
		if (profile) await rm(profile, { recursive: true, force: true });
	};
	try {
		if (clock !== null) {
			await holdDate(context, clock, !holdTimers);
			if (holdTimers) await context.clock.install({ time: new Date(clock) });
		}
		if (desk) await context.addInitScript((tree) => (window.__datumDesktopLayout = tree), desk);
		if (home)
			await context.addInitScript((name) => {
				const key = 'datum.layout.root.v1';
				const lock = `datum.layout.locked.v1.${name}`;
				try {
					if (!localStorage.getItem(key)) localStorage.setItem(key, JSON.stringify({ names: [name], active: 0 }));
					// A device that has been here has unlocked its home (da-gshh):
					// the suite drags, splits and closes its panes.
					if (localStorage.getItem(lock) === null) localStorage.setItem(lock, '0');
				} catch {}
			}, home);
		await prepare?.(context);
		const page = await context.newPage();
		const app = await open(page, query);
		if (clock !== null) app.clock = clockOf(page, clock, holdTimers);
		return { app, close };
	} catch (e) {
		await close();
		throw e;
	}
}

/**
 * ONE app for a whole file, and a ROOM of its own for each test (da-xamd).
 *
 * `withApp` opens a browser context per test, and a context is 2.5 s of
 * wasm compile, database and pane commands (docs/web-conventions.md #21).
 * Six files reach the runner's budget only by sharing one app, and what a
 * shared app costs is shared STATE — for the shell and the layout menu,
 * the layout state a test mutates on its way through.
 *
 * `app().room('acts')` is the answer: `/acts?from=main` opens `main`'s tree
 * as a PREVIEW tab under a name of the test's own, written nowhere
 * (shell.ts), so the test may split it, drag it, lock it and save it
 * without the next test ever meeting any of it. The room is taken fresh:
 * every root tab but `main` is closed, every layout the reader saved is
 * deleted and every draft and lock this device kept is dropped, so what a
 * test opens onto is the layout world a fresh install has.
 *
 * The DATABASE is one too — one app is one OPFS (da-vk1x) — so `solo` and
 * `room` EMPTY it whenever the test before wrote (`empty`, da-6b3dj), and
 * a test may count rows as it would in a context of its own.
 *
 * So a test in a shared app starts with, whichever door it came in by:
 * - the page at `/`, the screen it opened on, the pointer at the corner, no
 *   menu or modal open, the bar's line and pills and the time cursor empty,
 *   no errors yet, and no command in flight (`home`, `settled`);
 * - no layout of the reader's own and none of their history (`wipe`), the
 *   locks the page opened with (`relock`), and `main` as it ships (`restoreMain`)
 *   — or as `draft` has it (`redraft`), or a room in front of it (`room`);
 * - an empty database and the clock where the app opened (`empty`), and
 *   `spyOnRunner` free to be called again.
 *
 * What they do
 * NOT put back is the rest of the page: the REPL's scrollback, a widget
 * option THIS SESSION set on `main`'s own widgets (a room's are new ids, so
 * a test that sets one wants a room), a runner a test installed, and
 * anything in `localStorage` outside the layouts. A test that reads any of
 * those from a fresh device, reloads, navigates elsewhere or opens a second
 * tab still wants `withApp` of its own.
 *
 * Usage: `const shared = sharedApp(DESKTOP)` at the top of a file, then
 * either shape inside a test —
 *
 *     await shared.solo(async (app) => { … })        // `main` alone
 *     await shared.room('acts', async (app, room) => { … })
 *     await shared.fresh(async (app) => { … })       // a room, named for you
 *
 * A test that seeded through the quiet door and wants the panes to show it
 * calls `app.redraw()`, not `reload`: a reload is a boot, and the boot is
 * what sharing the app saves (da-s4i98).
 *
 * — or `await shared()` for the app itself. It opens on the FIRST ask
 * rather than in a `before`, so it cannot race the file's own
 * `before(startApp)`, and it books the `after` that takes it down.
 */
export function sharedApp(options) {
	let shared = null;
	let close = null;
	after(async () => {
		// The browser may already be gone: `after(stopApp)` is registered
		// first and closing a browser closes its contexts.
		await close?.().catch(() => {});
		shared = null;
		close = null;
	});
	const open = async () => {
		if (!shared) ({ app: shared, close } = await openApp(options));
		return shared;
	};
	/**
	 * `fn` against the shared app with the layout world back to `main` alone —
	 * or to `main` opened from `draft`, a device's stored draft of it (`redraft`).
	 */
	open.solo = async (fn, draft) => fn(await (await open()).solo(draft));
	/**
	 * `fn` against the shared app with every layout this device kept
	 * dropped and the page NOT rebuilt: for a test whose first act is a
	 * navigation, which builds the page itself.
	 */
	open.wipe = async (fn) => fn(await (await open()).wipe());
	/** `fn` against a room of its own; the room's name comes second. */
	open.room = async (name, fn) => {
		const app = await open();
		await app.room(name);
		return fn(app, name);
	};
	/**
	 * `fn` against a room under a name no test before it asked for
	 * (da-s4i98): the same panes as `solo`, but MOUNTED ANEW, for a test
	 * whose widgets keep state of their own between redraws — a panned
	 * nowview, a zoomed strip — which `solo` would hand to the next test.
	 */
	let rooms = 0;
	open.fresh = (fn) => open.room(`fresh-${++rooms}`, fn);
	return open;
}

/** Navigate to the day frame and wait until the database is open. */
async function open(page, query) {
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));
	await page.goto(baseUrl() + '/' + query);
	await ready(page);
	await countRuns(page);
	// What the database's change counter reads as it opened — empty, on a
	// fresh context: `empty` has nothing to do until something writes.
	// And the locks it opened with — `home`'s unlocked `main` — which `wipe`
	// takes with every other layout key and `relock` puts back.
	// A test may have blocked the store, which is then nothing to put back.
	const locks = await page.evaluate(() => {
		try {
			return Object.fromEntries(Object.entries(localStorage).filter(([key]) => key.startsWith('datum.layout.locked.v1.')));
		} catch {
			return {};
		}
	});
	return app(page, errors, query, await changeCounter(page), locks, await changeCounter(page, '_internal'));
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
		window.__datumStarted = 0;
		const previous = window.datum.setRunner((...args) => {
			window.__datumInflight += 1;
			window.__datumStarted += 1;
			return previous(...args).finally(() => (window.__datumInflight -= 1));
		});
	});
}

/**
 * Every command a tidy set off, run out: two frames for a pane that asks
 * one late — a restored `main`'s nowview does — then nothing in flight, so a
 * test that counts runs counts its own (da-6b3dj).
 */
async function settled(app) {
	await app.page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
	await app.idle();
}

/** The database the page is on: `?db=`'s, or the default one (da-vk1x). */
const pageDb = (page) => page.evaluate(() => new URLSearchParams(location.search).get('db') || 'datum');

/**
 * Database `db`'s files gone from OPFS, under the worker's own lock: no
 * handle is open there ($lib/cli.ts), and the next command makes it anew.
 */
function removeDb(page, db) {
	return page.evaluate(
		async ([dir, db]) =>
			navigator.storage?.getDirectory &&
			navigator.locks.request('datum-cli-files', async () => {
				const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
				for (const name of [`${db}.db`, `${db}.db-journal`]) await files.removeEntry(name).catch(() => {});
			}),
		[OPFS_DIR, db]
	);
}

/** The change counter of the page's database — or of `named` — read under the worker's own lock: 0 with no file. */
export function changeCounter(page, named) {
	return page.evaluate(async ([dir, named]) => {
		const db = named ?? (new URLSearchParams(location.search).get('db') || 'datum');
		// A browser with no OPFS keeps the database in memory, and has no file to read.
		if (!navigator.storage?.getDirectory) return 0;
		return navigator.locks.request('datum-cli-files', async () => {
			const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
			const bytes = await files
				.getFileHandle(`${db}.db`)
				.then((f) => f.getFile())
				.then((f) => f.slice(24, 28).arrayBuffer())
				.catch(() => new ArrayBuffer(0));
			return bytes.byteLength === 4 ? new DataView(bytes).getUint32(0) : 0;
		});
	}, [OPFS_DIR, named]);
}

async function ready(page) {
	// A prompt is disabled until `initDb()` resolves, so an enabled one is
	// the app's own statement that the database is open. The bottom bar's
	// (da-0a5o): it is on every page, whatever layout is in front.
	await page.locator('.prompt-bar input:not([disabled])').waitFor();
	// And the shell has filled the tree: the root bar names a tab.
	await page.locator('.root-bar [role="tab"]').first().waitFor();
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

function app(page, errors, query, clean, locks, internal) {
	/** The layouts of the reader's own the last `wipe` deleted. */
	let wiped = [];
	/** The screen the app opened on, which `home` puts back. */
	const viewport = page.viewportSize();
	const self = {
		page,
		/** Uncaught errors the page threw, in order. */
		errors,

		/**
		 * The seam. One command in — and its stdin, if any — one RunResult out.
		 *
		 * The QUIET door (`window.datum.runQuiet`, da-6918), on purpose: a
		 * test uses this to seed and to read, and nothing on the page is to
		 * move because it did. A test about the page reacting drives the
		 * page — `submit`, `pressWidget` — and the console's own loud door
		 * (`window.datum.run`, which goes through `engine.press`) is what
		 * console.test.mjs asserts. Quiet also keeps `db backup -` /
		 * `db restore -` literal: the loud door hands a file command to the
		 * browser's dialog ($lib/dbio.ts), which is not what a test wants.
		 */
		run: (argv, stdin) => page.evaluate(([a, s]) => window.datum.runQuiet(a, s), [argv, stdin]),

		/** The seam's line form, split by the app's own `shellSplit`. Quiet, as `run` is. */
		runLine: (line) => page.evaluate((l) => window.datum.runQuietLine(l), line),

		/**
		 * What a test LOGGED, newest first: the CLI's `db backup -` rows
		 * without the TRACK documents that declare each field on its first
		 * use (da-c8mh), the spec entry that first use writes (da-lc26c),
		 * or an alias entry (da-b3752).
		 * `allEntries` keeps those.
		 */
		entries: async (limit = 50) =>
			(await page.evaluate((n) => window.datum.entries(n), Math.max(limit * 3, 50)))
				.filter((row) => row.extra_metadata?.track === undefined && !['spec', 'alias'].includes(row.field))
				.slice(0, limit),

		/** Every stored row, declarations included. */
		allEntries: (limit = 50) => page.evaluate((n) => window.datum.entries(n), limit),

		/**
		 * Declare `field` quietly, before the command a test is watching.
		 * A field's FIRST log writes the TRACK document that declares it
		 * beside the entry (da-c8mh), so a test about ONE document has to
		 * say the field already exists.
		 */
		declare: async (field, verb = 'occur') => {
			// A minute before NOW: the clock is held (da-ydao), and a log that
			// follows at the same instant would share the declaration's slug.
			const at = self.clock?.at();
			if (at !== undefined) await self.clock.set(at - 60_000);
			const done = await page.evaluate(
				([f, v]) => window.datum.runQuiet(['track', f, '--verb', v]),
				[field, verb]
			);
			if (at !== undefined) await self.clock.set(at);
			if (done.code !== 0) throw new Error(`track ${field} --verb ${verb}: ${done.stderr}`);
		},

		/**
		 * The tail listing as data: the CLI's rows with the colours it
		 * would paint them, and its painted columns. What the tail pane
		 * renders — so a test can hold what is on screen to what datum
		 * said, rather than to a colour written down here.
		 */
		tailRows: () => page.evaluate(() => window.datum.tailRows()),

		/**
		 * The beta hint's sentence about the data (da-0a5o: the banner's
		 * text, behind the ⓘ beside `beta` in the root bar since da-y992).
		 *
		 * The hint reads the storage that actually opened, so this is the
		 * app's own on-screen claim about whether entries survive a reload.
		 * Opened if it is not, and left open: a poll reads it again.
		 */
		async bannerText() {
			const hint = page.locator('.beta-hint p');
			if ((await hint.count()) === 0) await page.locator('.root-bar .hint-button').click();
			return hint.innerText();
		},

		/**
		 * Type a line into the bottom bar and submit it, as a user does,
		 * and wait for it in the REPL's scrollback — the bar is the REPL's
		 * prompt (da-rfs9).
		 */
		async submit(line) {
			const before = await page.locator('.repl .echo').count();
			const input = page.locator('.prompt-bar input');
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
		 * The REPL's rendered stdout blocks, oldest first.
		 *
		 * What a command PRINTED, without the progress a file command
		 * writes to stderr as it runs (da-h9vj). `outputs()` holds both
		 * streams, so for a `db restore` its last block is
		 * `restore: reading 3/3`, not the summary — the two are plain
		 * `pre` alike, since the error paint follows the exit code and a
		 * successful command's stderr is not an error (da-q15y).
		 */
		stdouts: () => page.locator('.repl pre[data-stream="stdout"]').allInnerTexts(),

		/**
		 * What one of the day frame's panes is showing, by its title.
		 *
		 * Each pane is one datum command rendered as text ($lib/panes.ts),
		 * so this is the command's own output as the reader sees it. A
		 * pane's title is its `data-title` (da-jdop): the h2 when it has one
		 * tab, the active tab's label when it has several — so a pane
		 * behind another tab is not "showing" anything, and is not found.
		 * The tabs behind it are mounted too (da-rjmg): the body read is
		 * the one in front.
		 */
		paneText: (title) =>
			page
				.locator(`section.pane[data-title="${title}"] > .content > .tab-body:not(.behind)`)
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
		 *
		 * `on` is its query, this tab's unless said — `'?db=other'` for the
		 * second tab of another database (da-vk1x), which is the same origin
		 * and so the same OPFS, one file along.
		 */
		async newTab(on = query) {
			return open(await page.context().newPage(), on);
		},

		/**
		 * Every layout this device kept, gone (da-xamd): the reader's saved
		 * layouts and every `datum.layout.*` key, the open list included.
		 *
		 * The page in front of you is NOT rebuilt, which is the point: a test
		 * that is about to NAVIGATE gets its fresh world for nothing, since
		 * the navigation builds the page from what is left — and a rebuild
		 * here would be one day frame drawn for no one. A test that stays on
		 * this page wants `solo`.
		 *
		 * THE INVARIANT, because the deletion goes behind a page that is
		 * still alive: nothing may run a shell act between this and the next
		 * navigation. The open list is written by `settle` on every change to
		 * the tree, so one act in that window would put the stale list back
		 * over the world just wiped. `solo` and `room` are safe by settling
		 * it again on purpose (`settleRoot`); a caller of `wipe` alone must
		 * navigate next.
		 */
		async wipe() {
			await page.evaluate(() => {
				for (const key of Object.keys(localStorage))
					if (key.startsWith('datum.layout.')) localStorage.removeItem(key);
			});
			// The reader's saves, and their HISTORY with them — a delete
			// leaves a save's versions behind, which a layout saved again
			// under the name would list: `_internal` holds the saves alone and
			// nothing caches them ($lib/layout/store.ts), so its file goes, as
			// `empty` takes the page's database. Only if something was saved
			// since the last wipe, by the same witness `empty` reads; the
			// listing after makes it anew, so the counter it leaves is one no
			// read moves.
			wiped = [];
			if ((await changeCounter(page, '_internal')) === internal) return self;
			wiped = await page.evaluate(async () =>
				(await window.datum.layouts.listLayouts()).filter((l) => l.source === 'user').map((l) => l.name)
			);
			await removeDb(page, '_internal');
			await page.evaluate(() => window.datum.layouts.listLayouts());
			internal = await changeCounter(page, '_internal');
			return self;
		},

		/**
		 * `wipe`, and the page in front of you back to `main` alone, as it
		 * came: every other root tab closed and `main` itself restored if the
		 * test before it left a mark.
		 *
		 * At most ONE frame is drawn — the tabs are closed before the saves
		 * are dropped, and `main` is put back while it is behind another tab
		 * wherever it can be.
		 *
		 * `{ keepDb: true }` leaves the database as it is, for a test that
		 * put the layout back and knows its data is still what it seeded.
		 */
		async solo(draft, { keepDb = false } = {}) {
			await self.home();
			await page.evaluate(async () => {
				const { layout, shell } = window.datum;
				const tabs = () => layout.current().root.tabs;
				for (let i = tabs().length - 1; i >= 0; i--) if (tabs()[i].layout !== 'main') shell.close(i);
			});
			await self.wipe();
			await self.relock();
			// A draft rebuilds `main` whole, so what it was left as is moot.
			if (draft === undefined) await self.restoreMain();
			await self.settleRoot();
			if (draft !== undefined) await self.redraft(draft);
			if (!keepDb) await self.empty();
			await ready(page);
			await settled(self);
			return self;
		},

		/**
		 * The locks the page opened with, back (da-6b3dj): written where
		 * `wipe` took them from, and every open tab's own `locked` turned to
		 * match — a lock is the TAB's too, so a test that locked `main` would
		 * otherwise hand the next one a locked `main`.
		 */
		async relock() {
			await page.evaluate((locks) => {
				const { layout, shell } = window.datum;
				for (const [key, value] of Object.entries(locks)) localStorage.setItem(key, value);
				for (const tab of layout.current().root.tabs) {
					const stored = locks[`datum.layout.locked.v1.${tab.layout}`];
					if (stored !== undefined && !!tab.locked !== (stored === '1')) shell.toggleLock(tab.id);
				}
			}, locks);
			return self;
		},

		/**
		 * `main` opened again from a DRAFT (`draft` in `support/layout.mjs`,
		 * a `Layout` as JSON), as a device that had it stored opens it
		 * (da-6b3dj) — what an init script writing the draft before the page
		 * loads gives `withApp`, on a page that is already up.
		 *
		 * Through the shell's own door: with `main` closed behind a scratch
		 * preview, the draft is written where boot reads it and
		 * `shell.open('main')` builds the tab from it exactly as boot does;
		 * the scratch tab goes after. `solo` puts `main` back after the test.
		 */
		async redraft(json) {
			await page.evaluate(async () => {
				const { layout, shell } = window.datum;
				const at = (name) => layout.current().root.tabs.findIndex((t) => t.layout === name);
				// Seeded from `input`, which runs nothing: `main` is not drawn twice.
				await shell.openPreview('__redraft', 'input');
				shell.close(at('main'));
			});
			// Emptied now, while only the scratch tab is drawn: the draft's
			// panes then draw once, on the empty database.
			await self.empty();
			await page.evaluate(async (json) => {
				const { layout, shell } = window.datum;
				const at = (name) => layout.current().root.tabs.findIndex((t) => t.layout === name);
				localStorage.setItem('datum.layout.draft.v1.main', json);
				if (!(await shell.open('main'))) throw new Error('main did not open from its draft');
				shell.close(at('__redraft'));
			}, json);
			await ready(page);
			return self;
		},

		/**
		 * The page's database EMPTIED, and every pane re-drawn from it
		 * (da-6b3dj): what a fresh context gave a test, without a boot.
		 *
		 * The worker opens its files per burst and gives them back under the
		 * `datum-cli-files` Web Lock ($lib/cli.ts), so under that lock no
		 * handle is open and the file can go; the next command makes an
		 * empty one, as on a first visit. The panes hear it the way they
		 * hear another tab's write — the change channel ($lib/changes.ts) —
		 * and this waits for that refresh to start and then to finish.
		 *
		 * Only if something WROTE since it was last empty — or opened: SQLite
		 * bumps its header's change counter (offset 24) on every write, the
		 * same witness `cli.ts` announces changes by, so a counter that has
		 * not moved is a database that is still empty and costs nothing.
		 * `_internal` stays: its layouts are `wipe`'s. The bar's pills and
		 * the time cursor go too, and the clock goes back to where the app
		 * opened (da-ydao); the REPL's scrollback does NOT, so a test reads
		 * its own commands from the end.
		 */
		async empty() {
			await self.idle();
			await self.clock?.reset();
			await page.evaluate(() => {
				window.datum.bar.setPills([]);
				window.datum.cursor.set(null);
			});
			if ((await changeCounter(page)) === clean) return self;
			await removeDb(page, await pageDb(page));
			await self.redraw();
			clean = await changeCounter(page);
			return self;
		},

		/**
		 * Every pane re-drawn from the page's database as it now is, without
		 * a boot (da-s4i98): for a test that seeded through the QUIET door,
		 * which moves nothing, and wants the panes to show what it stored.
		 *
		 * Announced on the change channel ($lib/changes.ts) as another tab's
		 * write is, which the page answers as it answers a command of its own
		 * — the epoch bumped and the widget bar rebuilt — so the panes draw
		 * what a reload would have drawn them, on a page that keeps its
		 * compiled wasm. Waits for that refresh to start and then to finish.
		 */
		async redraw() {
			await self.idle();
			const started = await page.evaluate((db) => {
				const n = window.__datumStarted;
				const channel = new BroadcastChannel('datum-db-changes');
				channel.postMessage({ from: 'test', dbs: [db] });
				channel.close();
				return n;
			}, await pageDb(page));
			await page.waitForFunction((n) => window.__datumStarted > n, started);
			await self.idle();
			return self;
		},

		/**
		 * Back to `/`, if the test before left the app somewhere else — a
		 * notice page (`/nothing`) draws no tree at all.
		 *
		 * The QUERY is kept, because `?db=<name>` is the page's whole state
		 * (da-vk1x) and a navigation that drops it is a navigation into
		 * another database.
		 *
		 * The screen back to the size the app opened on, the pointer at its
		 * corner, the page's errors forgotten, any modal closed and the bar's
		 * line empty, for a shared app's next test
		 * (da-6b3dj).
		 *
		 * And no menu left open over it (da-b7sfp): a pointerdown on the
		 * page's own body is a reader tapping nothing, which closes every
		 * menu (PaneMenu's window listener) and hover card (`away` in
		 * hover.ts) wherever the focus is — a test that ends with the Bar
		 * submenu open handed the next one a menu it never opened.
		 */
		async home() {
			// What a test before may have changed of the page itself: the
			// screen it was resized to, and the errors it has already read.
			if (JSON.stringify(page.viewportSize()) !== JSON.stringify(viewport)) await page.setViewportSize(viewport);
			errors.length = 0;
			// And the pointer off whatever the last test left it hovering.
			await page.mouse.move(0, 0);
			const url = new URL(page.url());
			if (url.pathname !== '/') await self.goto('/' + url.search);
			// A test that went round `goto` — a bare `page.reload()` — left a
			// document with no count behind the seam, and `idle` would wait on it.
			if (await page.evaluate(() => window.__datumInflight === undefined)) await countRuns(page);
			await page.evaluate(() => document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
			await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
			// A modal left open (the Track form, the `+N` sheet) takes every
			// pointer on the page: closed as a reader closes one, with Escape.
			for (let i = 0; i < 3 && (await page.locator('dialog[open]').count()) > 0; i++) await page.keyboard.press('Escape');
			await page.waitForFunction(() => !document.querySelector('dialog[open]'));
			// And the bar's line empty: what a test left typed there filters
			// the fields widget for the next one ($lib/bar.ts's `text`).
			const line = page.locator('.prompt-bar input');
			if ((await line.inputValue()) !== '') await line.fill('');
			await line.blur();
			return self;
		},

		/**
		 * Write the open list out again: `wipe` took it with everything else,
		 * and a test that stays on this page wants the device to say what is
		 * actually open. An identity `apply` is the shell's own settle.
		 */
		async settleRoot() {
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			return self;
		},

		/** `main` back to the tree it ships with, if something moved it. Draws nothing on its own. */
		async restoreMain() {
			await page.evaluate(async (saved) => {
				const { layout, shell } = window.datum;
				const day = layout.current().root.tabs.find((t) => t.layout === 'main');
				// A test that SAVED `main` left it drawn from a save `wipe` has
				// just deleted, and a tab that matches its save is not modified
				// — so nothing below would move it (da-6b3dj). The reset makes
				// it differ from that stale baseline; the restore then goes to
				// what `main` means now, the shipped tree.
				if (day && saved) await shell.resetLayout(day.id);
				// Svelte's `get`, by hand: a store calls a new subscriber at
				// once with the value it holds, and `subscribe` hands back the
				// unsubscriber — named here rather than invoked on the same
				// line, which three reviews in a row read as a leak.
				let dirty = false;
				const stop = shell.modified.subscribe((m) => (dirty = !!day && m.has(day.id)));
				stop();
				if (dirty) await shell.restore(day.id);
			}, wiped.includes('main'));
			return self;
		},

		/**
		 * A ROOM of this test's own in a shared app (da-xamd): `main`'s tree
		 * as a PREVIEW tab called `name`, written nowhere, on a fresh world.
		 *
		 * Whatever the test does in here — split it, drag it, lock it, save
		 * it under its own name — is the room's and not `main`'s, and a
		 * preview leaves no trace at all unless the test asks for one.
		 *
		 * The room is opened FIRST and the world tidied behind it, so ONE
		 * day frame is drawn: the room's. Closing the last test's tabs first
		 * would draw `main` on the way past, which is the whole cost of a
		 * boot again. Closing them AFTER draws nothing: every one of them is
		 * behind the room by then, the renderer draws the active tab alone,
		 * and the tab in front is the same object throughout — `closeTab`
		 * moves the active INDEX when it removes a tab before it, never the
		 * tab, and the strip is keyed by tab id, so nothing remounts.
		 *
		 * A room's panes carry the room's name: the tree is `main`'s inlined
		 * a SECOND time, so `inline` re-mints the ids it already has as
		 * `<id>@<room>` (shell.ts). `inRoom` in `support/layout.mjs` names one.
		 */
		async room(name) {
			await self.home();
			// The route's own door (`routes/[name]/+page.svelte`), called
			// where the route calls it. Not `goto`: a real navigation is a
			// whole boot again — a fresh wasm compile and every pane's first
			// command — which is the cost a shared app is here to avoid.
			// That the URL reaches this is `preview.test.mjs`'s to say.
			//
			// The seed is what `main` RESOLVES to, and a save shadows the
			// shipped tree: so if the test before saved anything, the world is
			// tidied first (da-x92rt) — at the cost, in that case alone, of a
			// frame of `main` put back.
			if ((await changeCounter(page, '_internal')) !== internal) {
				await self.wipe();
				await self.relock();
				await self.restoreMain();
			}
			const missing = await page.evaluate((n) => window.datum.shell.openPreview(n, 'main'), name);
			if (missing !== null) throw new Error(`a room seeded from a layout that is nothing: ${missing}`);
			await page.evaluate(async (keep) => {
				const { layout, shell } = window.datum;
				const tabs = () => layout.current().root.tabs;
				for (let i = tabs().length - 1; i >= 0; i--) {
					const held = tabs()[i].layout;
					if (held !== 'main' && held !== keep) shell.close(i);
				}
			}, name);
			await self.wipe();
			await self.relock();
			await self.restoreMain();
			await self.settleRoot();
			await self.empty();
			await ready(page);
			await settled(self);
			return name;
		},

		/**
		 * Navigate this tab to `path` — `/`, `/input` — and wait for it to
		 * come up. A `/<name>` opens that tab and then the page is `/`
		 * again, or a notice for a name that is nothing (da-0a5o); either
		 * is waited for, so the tab is open by the time this returns.
		 */
		async goto(path) {
			await page.goto(baseUrl() + path);
			await ready(page);
			await page.waitForFunction(() => location.pathname === '/' || !!document.querySelector('main .notice'));
			// A new document: `idle` counts through the runner installed here.
			await countRuns(page);
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
		 * command on it — the strip one per picture it holds — behind one
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
				// Quiet: priming the runner is not a command the page should
				// show, and the spy is to see the test's commands, not this one's
				// refresh.
				window.__datumSeen = [];
				// Once a page: a shared app's next test asks again, and a
				// second spy over the first would record every argv twice.
				if (window.__datumSpied) return;
				window.__datumSpied = true;
				await window.datum.runQuiet(['tail']);
				window.__datumSeen = [];
				const previous = window.datum.setRunner((argv, ...rest) => {
					window.__datumSeen.push(argv);
					return previous(argv, ...rest);
				});
			});
			return () => page.evaluate(() => window.__datumSeen);
		}
	};
	return self;
}
