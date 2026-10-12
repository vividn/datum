// The Sync widget (da-x4t0a, redone as a list in da-yoef7): the page's
// database's devices, a line each, then the other databases a line each.
//
// The fixture is two chained databases beside the default `datum`, which
// is in no chain; the tests open the page on `home`. Their states are set
// without a network: members are admitted as a session would admit them
// (`sync new-invite`, which starts an open chain, then `sync handshake
// --invite`), a native copy's `describe` (its client and kind) comes over
// the pipe transport (`sync pipe ops` | `sync pipe apply`), and `sync
// record-session` says how each member's last session went. No tab leads
// `home` or `work`, so the only reachability is what a test broadcasts on
// the sync worker's own channel.
//
//            home        work
//   this     !           ◔
//   laptop   ✓           ◔      (one client, two native copies)
//   phone    ◔ to send   —
//   tablet   ! refused   —
//   watch    ◔ never     —

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, withWallClock } from './support/sync.mjs';

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

/** Each device's line as `[label, mark]`, top to bottom. */
const lines = (page) =>
	page.locator(`${WIDGET} .devices li`).evaluateAll((lis) => lis.map((li) => [li.querySelector('.name').textContent, li.dataset.mark]));

const line = (page, client) => page.locator(`${WIDGET} .devices li[data-client="${client}"]`);

/** The page on `home`, its five devices drawn. */
async function onHome(app) {
	await app.goto('/sync?db=home');
	const h = await app.page.waitForFunction((sel) => document.querySelectorAll(sel).length === 5, `${WIDGET} .devices li`);
	await h.dispose();
}

const DESKTOP = { viewport: { width: 1280, height: 800 } };

describe('the sync widget', () => {
	test('a line per device, this one first, with its state as a shape and a word, and the other databases a line each', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const ids = await fixture(app);
			await onHome(app);

			const [first, ...rest] = await lines(page);
			assert.deepEqual(first, ['datum.day', 'error']);
			assert.deepEqual(rest.sort(([a], [b]) => a.localeCompare(b)), [
				['laptop', 'synced'],
				['phone', 'pending'],
				['tablet', 'error'],
				['watch', 'pending']
			]);
			assert.equal(await line(page, 'this').locator('.kind').innerText(), 'this device');
			assert.equal(await line(page, LAPTOP).locator('.state').innerText(), '✓ synced');
			assert.match(await line(page, LAPTOP).locator('.kind').innerText(), /CLI/);
			assert.match(await line(page, LAPTOP).locator('.seen').innerText(), /^(now|\d+ min ago|\d+ h ago)$/);
			assert.match(await line(page, ids.phone).getAttribute('title'), /pending, \d+ to send/);
			assert.match(await line(page, ids.watch).getAttribute('title'), /never synced/);
			assert.equal(await line(page, ids.watch).locator('.seen').innerText(), 'never');
			// An error's reason, verbatim, under its line: no tap needed.
			assert.equal(await line(page, ids.tablet).locator('.reason').innerText(), REFUSED);
			// The database as a whole is its worst device.
			assert.equal(await page.locator(`${WIDGET} header .title`).innerText(), 'Sync · home');
			assert.equal(await page.locator(`${WIDGET} header .state`).innerText(), '! error');

			// Nobody leads `home`: every other device is offline, its state
			// last known, grey — never a green ✓.
			assert.equal(await line(page, LAPTOP).getAttribute('data-reach'), 'offline');
			assert.match(await line(page, LAPTOP).getAttribute('class'), /\bstale\b/);
			const color = (l) => l.evaluate((e) => getComputedStyle(e).color);
			assert.notEqual(await color(line(page, LAPTOP).locator('.state')), await color(page.locator(`${WIDGET} .legend .synced`)));

			// The others, one line each: work in a chain of its own, datum in none.
			const others = await page.locator(`${WIDGET} .others li`).evaluateAll((lis) =>
				lis.map((li) => [li.dataset.db, li.dataset.mark, li.querySelector('.seen').textContent])
			);
			assert.deepEqual(others.sort(), [
				['datum', 'none', 'not syncing'],
				['work', 'pending', '2 devices']
			]);
			// The legend says only what is in use, and the whole of it is a tap away.
			assert.deepEqual(await page.locator(`${WIDGET} .legend > span`).allInnerTexts(), [
				'✓ synced',
				'◔ pending',
				'! error',
				'— not syncing'
			]);
			await page.locator(`${WIDGET} .legend .info`).click();
			assert.match(await page.locator(`${WIDGET} .key`).innerText(), /↻ syncing/);

			// A tap on another database switches to it, as the logo does.
			await page.locator(`${WIDGET} .others li[data-db="work"] a`).click();
			await page.waitForURL(/[?&]db=work\b/);
		}, DESKTOP);
	});

	test('the reachability dot and the syncing mark follow the worker’s broadcast', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const ids = await fixture(app);
			await onHome(app);
			const row = line(page, LAPTOP);
			const reach = async (want) => {
				const h = await page.waitForFunction(
					([sel, want]) => document.querySelector(sel)?.dataset.reach === want,
					[`${WIDGET} .devices li[data-client="${LAPTOP}"]`, want]
				);
				await h.dispose();
			};

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'reachable', lastSeen: Date.now() } });
			await reach('reachable');
			assert.equal(await row.locator('.dot').innerText(), '◐');
			assert.doesNotMatch(await row.getAttribute('class'), /\bstale\b/);
			assert.equal(await row.getAttribute('data-mark'), 'synced');

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'online' } });
			await reach('online');
			assert.equal(await row.locator('.dot').innerText(), '●');
			assert.equal(await row.getAttribute('data-mark'), 'syncing');
			assert.equal(await row.locator('.symbol').innerText(), '↻');

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'offline' } });
			await reach('offline');
			assert.equal(await row.locator('.dot').innerText(), '○');
			assert.match(await row.getAttribute('class'), /\bstale\b/);
		}, DESKTOP);
	});

	test('a reader carries the lock beside its state, and the legend names it', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const run = inPage(app, 'home');
			const id = endpointId();
			const { secret } = JSON.parse(await run(['sync', 'new-invite', '--reader', '--expires', '1d']));
			await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
			await app.goto('/sync?db=home');
			const reader = line(page, id);
			await reader.waitFor();
			assert.equal(await reader.locator('.reader').innerText(), '🔒︎');
			assert.match(await reader.getAttribute('title'), /, read-only\b/);
			// This copy is a member: no lock.
			assert.equal(await line(page, 'this').locator('.reader').count(), 0);
			assert.match(await page.locator(`${WIDGET} .legend .reader`).innerText(), /^🔒︎\s*read-only$/);
		}, DESKTOP);
	});

	test('a filtered reader carries the filter too, its title says how many fields, and the legend names it', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const run = inPage(app, 'home');
			const id = endpointId();
			await run(['occur', 'tea']);
			const invite = ['sync', 'new-invite', '--fields', 'coffee', '--expires', '1d'];
			const { secret } = JSON.parse(await run(invite));
			await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
			await app.goto('/sync?db=home');
			const reader = line(page, id);
			await reader.waitFor();
			assert.equal(await reader.locator('.reader').innerText(), '🔒︎');
			assert.equal(await reader.locator('.filter').innerText(), '⛛');
			assert.match(await reader.getAttribute('title'), /, read-only, filtered: 1 of 2 fields\b/);
			assert.equal(await line(page, 'this').locator('.filter').count(), 0);
			assert.match(await page.locator(`${WIDGET} .legend .filter`).innerText(), /^⛛\s*partial$/);
		}, DESKTOP);
	});

	test('a revoked device has no line, the list says one is hidden, and the legend has no «not syncing» (da-73gyz)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const phone = await member(app, 'home', 'phone');
			await cli(app, ['--db', 'home', 'sync', 'chain', 'revoke', phone]);
			// Every other database chained too, so no «not syncing» comes from them.
			await cli(app, ['sync', 'new-invite']);
			await app.goto('/sync?db=home');
			const hidden = page.locator(`${WIDGET} p.revoked`);
			await hidden.waitFor();
			assert.equal(await hidden.innerText(), '1 former member hidden — More lists it');
			assert.equal(await page.locator(`${WIDGET} .devices li[data-client="${phone}"]`).count(), 0);
			assert.deepEqual(await page.locator(`${WIDGET} .legend > span`).allInnerTexts(), ['✓ synced']);
		}, DESKTOP);
	});

	test('two databases in one chain: the other is one line, «same devices»', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const [home, work] = [inPage(app, 'home'), inPage(app, 'work')];
			await work(['occur', 'tea']);
			// `work` joins `home`'s chain as a session would: admitted there, then
			// handed the chain's ops over the pipe.
			const { secret } = JSON.parse(await home(['sync', 'new-invite', '--expires', '1d']));
			const id = (await work(['sync', 'endpoint-key'])).trim().split('\n').at(-1);
			await home(['sync', 'handshake', id, '--label', 'work', '--addr', '{}', '--invite', secret, '--chain', '']);
			await work(['sync', 'pipe', 'apply'], await home(['sync', 'pipe', 'ops']));
			assert.equal(JSON.parse(await work(['sync', 'status', '--json'])).chain, JSON.parse(await home(['sync', 'status', '--json'])).chain);

			await app.goto('/sync?db=home');
			const other = page.locator(`${WIDGET} .others li[data-db="work"]`);
			await other.waitFor();
			assert.equal(await other.locator('.seen').innerText(), 'same devices');
			// And home's own list still draws, `work`'s copy one of its devices.
			assert.deepEqual((await lines(page)).map(([label]) => label), ['datum.day', 'work']);
		}, DESKTOP);
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
