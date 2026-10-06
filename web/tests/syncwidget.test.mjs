// The Sync widget, read-only half (da-x4t0a): clients × databases.
//
// The fixture is two chained databases beside the page's own `datum`, which
// is in no chain. Their states are set without a network: members are
// admitted as a session would admit them (`sync new-invite`, which starts an
// open chain, then `sync handshake --invite`), a native copy's `describe`
// (its client and kind) comes over the pipe transport (`sync pipe ops` | `sync pipe
// apply`), and `sync record-session` says how each member's last session went. No tab
// leads `home` or `work`, so the only reachability is what a test
// broadcasts on the sync worker's own channel.
//
//            datum   home        work
//   this     —       !           ◔
//   laptop   —       ✓           ◔      (one client, two native copies)
//   phone    —       ◔ to send   —
//   tablet   —       ! refused   —
//   watch    —       ◔ never     —

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, withWallClock } from './support/sync.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';

before(startApp);
after(stopApp);

const LAPTOP = 'laptop-01';
const REFUSED = 'connection refused by the relay';
const WIDGET = 'section.pane[data-title="sync"]';
/** Nothing here dials: the copies only pipe ops, so the relay is never asked. */
const NO_RELAY = { url: 'http://127.0.0.1:9' };

const endpointId = () =>
	generateKeyPairSync('ed25519').publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');

/** `datum <argv>` in the page; its stdout. */
async function cli(app, argv, stdin) {
	const r = await app.page.evaluate(([a, s]) => window.datum.runQuiet(a, s), [argv, stdin]);
	assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	return r.stdout;
}

/** A native copy under the laptop's client: its `run`, which must succeed. */
async function laptop() {
	const copy = await nativeCopy(NO_RELAY);
	const run = async (argv, stdin) => {
		const r = await copy.run(argv, stdin, { DATUM_CLIENT_ID: LAPTOP });
		assert.equal(r.code, 0, `native ${argv.join(' ')}: ${r.stderr}`);
		return r.stdout;
	};
	return { run, close: copy.close };
}

/**
 * Admit endpoint `id` on the copy `run` runs in, as a session redeeming an
 * invite does. A day to redeem it in.
 */
async function admit(run, id, label) {
	const { secret } = JSON.parse(await run(['sync', 'new-invite', '--expires', '1d']));
	// `--chain ''`: the redeemer is in no chain yet (da-mepav).
	await run(['sync', 'handshake', id, '--label', label, '--addr', '{}', '--invite', secret, '--chain', '']);
}

/** `datum --db <db> <argv>` in the page. */
const inPage = (app, db) => (argv, stdin) => cli(app, ['--db', db, ...argv], stdin);

/** Chain `db` in the page with the laptop in it, described as itself; the laptop's endpoint. */
async function chain(app, db, lap) {
	const id = JSON.parse(await lap.run(['sync', 'status', '--json'])).this.endpoint;
	await admit(inPage(app, db), id, 'laptop');
	await cli(app, ['--db', db, 'occur', 'coffee']);
	await lap.run(['sync', 'pipe', 'apply'], await cli(app, ['--db', db, 'sync', 'pipe', 'ops']));
	await lap.run(['sync', 'chain', 'rename', 'laptop']);
	await cli(app, ['--db', db, 'sync', 'pipe', 'apply'], await lap.run(['sync', 'pipe', 'ops']));
	return id;
}

async function member(app, db, label) {
	const id = endpointId();
	await admit(inPage(app, db), id, label);
	return id;
}

const session = (app, db, id, ...how) => cli(app, ['--db', db, 'sync', 'record-session', id, ...how]);

/** The fixture in the table above; the members' endpoints. */
async function fixture(app) {
	const [a, b] = [await laptop(), await laptop()];
	try {
		const ids = { laptop: await chain(app, 'home', a), workLaptop: await chain(app, 'work', b) };
		ids.phone = await member(app, 'home', 'phone');
		ids.tablet = await member(app, 'home', 'tablet');
		// Admitted by the laptop, so this copy has never met it: `never`.
		ids.watch = endpointId();
		await admit(a.run, ids.watch, 'watch');
		await cli(app, ['--db', 'home', 'sync', 'pipe', 'apply'], await a.run(['sync', 'pipe', 'ops']));
		const vector = (await cli(app, ['--db', 'home', 'sync', 'pipe', 'vector'])).trim();
		await session(app, 'home', ids.laptop, '--vector', vector, '--verify', 'match');
		await session(app, 'home', ids.phone, '--vector', '{}');
		await session(app, 'home', ids.tablet, '--error', REFUSED);
		await session(app, 'work', ids.workLaptop, '--vector', '{}');
		return ids;
	} finally {
		await a.close();
		await b.close();
	}
}

/** What the worker's leader would post for `db`, on its channel. */
const broadcast = (app, db, peers) =>
	app.page.evaluate(([db, peers]) => new BroadcastChannel('datum-sync').postMessage({ state: { db, peers } }), [db, peers]);

/** Each row as `[label, {db: mark}]`, top to bottom. */
const marks = (page) =>
	page.locator(`${WIDGET} tbody tr`).evaluateAll((rows) =>
		rows.map((r) => [
			r.querySelector('.name').textContent,
			Object.fromEntries([...r.querySelectorAll('td')].map((td) => [td.dataset.db, td.dataset.mark]))
		])
	);

const cell = (page, client, db) => page.locator(`${WIDGET} tr[data-client="${client}"] td[data-db="${db}"]`);

describe('the sync widget', () => {
	test('draws a symbol per client and database, this client first, and greys what is not reachable', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const ids = await fixture(app);
				await app.goto('/sync');
				await page.locator(`${WIDGET} td[data-db="home"]`).first().waitFor();
				const rows = await page.waitForFunction(() => document.querySelectorAll('section.pane[data-title="sync"] tbody tr').length === 5);
				await rows.dispose();

				const none = { datum: 'none' };
				// This client first; the rest in the chain's own order, which is not this test's.
				const [first, ...rest] = await marks(page);
				assert.deepEqual(first, ['datum.day', { home: 'error', work: 'pending', ...none }]);
				assert.deepEqual(rest.sort(([a], [b]) => a.localeCompare(b)), [
					['laptop', { home: 'synced', work: 'pending', ...none }],
					['phone', { home: 'pending', work: 'none', ...none }],
					['tablet', { home: 'error', work: 'none', ...none }],
					['watch', { home: 'pending', work: 'none', ...none }]
				]);
				// Shape, not colour alone.
				assert.deepEqual(
					await page.locator(`${WIDGET} tr[data-client="${LAPTOP}"] .symbol`).allInnerTexts(),
					['—', '✓', '◔']
				);
				assert.match(await cell(page, ids.phone, 'home').getAttribute('title'), /pending, \d+ to send/);
				assert.match(await cell(page, ids.watch, 'home').getAttribute('title'), /never synced/);
				// The laptop is one client with two copies, and a CLI.
				assert.match(await page.locator(`${WIDGET} tr[data-client="${LAPTOP}"] .kind`).innerText(), /CLI/);

				// An error's reason, verbatim, on a tap.
				await cell(page, ids.tablet, 'home').locator('button').click();
				assert.equal(await cell(page, ids.tablet, 'home').locator('.reason').innerText(), REFUSED);

				// Nobody leads `home`: every other client is offline, its state
				// last known, grey, with its time — never a green ✓.
				const synced = cell(page, LAPTOP, 'home');
				assert.equal(await page.locator(`${WIDGET} tr[data-client="${LAPTOP}"]`).getAttribute('data-reach'), 'offline');
				assert.match(await synced.getAttribute('class'), /\bstale\b/);
				assert.match(await synced.locator('.at').innerText(), /\d\d:\d\d/);
				const color = (l) => l.evaluate((e) => getComputedStyle(e).color);
				assert.equal(await color(synced), await color(cell(page, LAPTOP, 'datum')));
			},
			{ viewport: { width: 1280, height: 800 } }
		);
	});

	test('the reachability dot and the syncing mark follow the worker’s broadcast', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const ids = await fixture(app);
				await app.goto('/sync');
				const row = page.locator(`${WIDGET} tr[data-client="${LAPTOP}"]`);
				await row.waitFor();
				const reach = async (want) => {
					const h = await page.waitForFunction(
						([sel, want]) => document.querySelector(sel)?.dataset.reach === want,
						[`${WIDGET} tr[data-client="${LAPTOP}"]`, want]
					);
					await h.dispose();
				};

				await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'reachable', lastSeen: Date.now() } });
				await reach('reachable');
				assert.equal(await row.locator('.dot').innerText(), '◐');
				assert.doesNotMatch(await cell(page, LAPTOP, 'home').getAttribute('class'), /\bstale\b/);
				assert.equal(await cell(page, LAPTOP, 'home').getAttribute('data-mark'), 'synced');

				await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'online' } });
				await reach('online');
				assert.equal(await row.locator('.dot').innerText(), '●');
				assert.equal(await cell(page, LAPTOP, 'home').getAttribute('data-mark'), 'syncing');
				assert.equal(await cell(page, LAPTOP, 'home').locator('.symbol').innerText(), '↻');

				await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'offline' } });
				await reach('offline');
				assert.equal(await row.locator('.dot').innerText(), '○');
				assert.match(await cell(page, LAPTOP, 'home').getAttribute('class'), /\bstale\b/);
			},
			{ viewport: { width: 1280, height: 800 } }
		);
	});

	test('a reader carries the lock beside its state, and the legend names it', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const run = inPage(app, 'home');
				const id = endpointId();
				const { secret } = JSON.parse(await run(['sync', 'new-invite', '--reader', '--expires', '1d']));
				await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
				await app.goto('/sync');
				const reader = cell(page, id, 'home');
				await reader.waitFor();
				assert.equal(await reader.locator('.reader').innerText(), '🔒︎');
				assert.match(await reader.getAttribute('title'), /, read-only\b/);
				// This copy is a member: no lock.
				assert.equal(await cell(page, 'this', 'home').locator('.reader').count(), 0);
				assert.match(await page.locator(`${WIDGET} .legend .reader`).innerText(), /^🔒︎\s*read-only$/);
			},
			{ viewport: { width: 1280, height: 800 } }
		);
	});

	test('a filtered reader carries the filter too, its title says how many fields, and the legend names it', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const run = inPage(app, 'home');
				const id = endpointId();
				await run(['occur', 'tea']);
				const invite = ['sync', 'new-invite', '--fields', 'coffee', '--expires', '1d'];
				const { secret } = JSON.parse(await run(invite));
				await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
				await app.goto('/sync');
				const reader = cell(page, id, 'home');
				await reader.waitFor();
				assert.equal(await reader.locator('.reader').innerText(), '🔒︎');
				assert.equal(await reader.locator('.filter').innerText(), '⛛');
				assert.match(await reader.getAttribute('title'), /, read-only, filtered: 1 of 2 fields\b/);
				assert.equal(await cell(page, 'this', 'home').locator('.filter').count(), 0);
				assert.match(await page.locator(`${WIDGET} .legend .filter`).innerText(), /^⛛\s*partial$/);
			},
			{ viewport: { width: 1280, height: 800 } }
		);
	});

	test('is compact at its legible floor and full above it', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await fixture(app);
			await app.goto('/sync');
			const widget = page.locator(`${WIDGET} .sync`);
			await page.locator(`${WIDGET} tr[data-client="${LAPTOP}"]`).waitFor();

			assert.match(await widget.getAttribute('class'), /\bfull\b/);
			assert.ok(await widget.locator('.seen').count());
			assert.match(await widget.locator('.legend').innerText(), /synced/);
			// The actions (da-bjxts) are the full form's.
			assert.ok(await widget.locator('.actions button, .chain').count());

			// registry.ts reaches Svelte components node cannot load: the hook stubs them.
			resolveLibAsSource(import.meta.url);
			const { KINDS } = await import('../src/lib/layout/registry.ts');
			const floor = KINDS.sync.legible.width;
			await widget.evaluate((e, w) => (e.style.width = `${w}px`), floor);
			const h = await page.waitForFunction((sel) => !document.querySelector(sel).classList.contains('full'), `${WIDGET} .sync`);
			await h.dispose();
			assert.equal(await widget.locator('.seen, .kind, .at, .actions, .add, .chain').count(), 0);
			assert.doesNotMatch(await widget.locator('.legend').innerText(), /synced/);
			// Symbols and dots still: every cell keeps its shape.
			assert.deepEqual(
				await page.locator(`${WIDGET} tr[data-client="${LAPTOP}"] .symbol`).allInnerTexts(),
				['—', '✓', '◔']
			);
		});
	});

	test('names a device whose changes wait for this clock to catch up with its own (da-ns8dq)', async () => {
		const lap = await laptop();
		try {
			// This page's clock an hour behind the laptop's: what the laptop
			// signs is an hour ahead of it, and waits.
			await withWallClock(
				async (app) => {
					await chain(app, 'home', lap);
					await app.goto('/sync');
					const notice = app.page.locator(`${WIDGET} .notice.clock[data-db="home"]`);
					await notice.waitFor();
					assert.match(
						await notice.innerText(),
						/^⚠ home: \d+ changes? from laptop (is|are) waiting: its clock is 1 hour ahead\. Fix that device's clock; they apply once the time catches up\.$/
					);
				},
				{ clock: Date.now() - 3_600_000 }
			);
		} finally {
			await lap.close();
		}
	});
});
