// The Sync widget (da-x4t0a, da-yoef7, da-5wlnr): this device's name, its
// person's @username and how the page's database stands at the top, then the
// grid, a row per device, each cell its last sync as a mark and its standing.
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
import { OPFS_DIR } from '../src/lib/databases.ts';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const LAPTOP = 'laptop-01';
const REFUSED = 'connection refused by the relay';
const UNREACHED = 'could not reach kiosk: timed out';
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
	// A read creates no database (da-ov6x5): `db migrate` makes it.
	await run(['db', 'migrate']);
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

const ROWS = `${WIDGET} .grid tbody tr`;

/** Each device's row in `db`'s column as `[label, mark]`, top to bottom. */
const lines = (page, db = 'home') =>
	page.locator(ROWS).evaluateAll((trs, db) => trs.map((tr) => [tr.querySelector('th').textContent, tr.querySelector(`td[data-db="${db}"]`)?.dataset.mark]), db);

/**
 * `label`'s cell in `db`'s column, of the device by that name that is in `db`: the laptop is two native copies, so
 * two devices, one in each database. This device's is `.here`'s.
 */
const line = (page, label, db = 'home') =>
	label === 'this'
		? page.locator(`${ROWS}.here td[data-db="${db}"]`)
		: page
				.locator(ROWS)
				.filter({ has: page.locator('th', { hasText: new RegExp(`^${label}$`) }) })
				.locator(`td[data-db="${db}"]:not([data-state="none"])`);

/** Wait for `n` device rows, or `n` of them in `db`: those with a mark there. */
async function rowsDrawn(page, n, db) {
	const sel = db ? `${ROWS} td[data-db="${db}"][data-mark]` : ROWS;
	const h = await page.waitForFunction(([sel, n]) => document.querySelectorAll(sel).length === n, [sel, n]);
	await h.dispose();
}

/** The page on `home`, its five devices drawn. */
async function onHome(app) {
	await app.goto('/sync?db=home');
	await rowsDrawn(app.page, 5, 'home');
}

/** What the widget no longer draws (da-5wlnr): the device list, the other databases, the legend, More, Rename this device, «invite like…». */
async function noneOfTheOld(page) {
	for (const gone of ['.devices', '.others', '.legend', '.key', '.more', '.start', 'p.revoked', '.grid button.like'])
		assert.equal(await page.locator(`${WIDGET} ${gone}`).count(), 0, gone);
	for (const name of ['Turn on sync', 'Rename this device', /^More/]) assert.equal(await page.locator(WIDGET).getByRole('button', { name }).count(), 0, String(name));
}

const DESKTOP = { viewport: { width: 1280, height: 800 } };

describe('the sync widget', () => {
	test('a database in no chain: this device’s row in the grid, «only on this device», and no «Turn on sync» (da-5wlnr)', async () => {
		for (const viewport of [{ width: 390, height: 844 }, DESKTOP.viewport])
			await withWallClock(
				async (app) => {
					const { page } = app;
					await app.goto('/sync');
					await rowsDrawn(page, 1);
					assert.deepEqual(await lines(page, 'datum'), [['datum.day (datum)', undefined]]);
					assert.equal(await page.locator(`${ROWS}.here td[data-db="datum"]`).innerText(), '—');
					assert.equal(await page.locator(`${WIDGET} .grid thead th.here .name`).innerText(), 'datum');
					// Never «synced» with no one to sync with.
					assert.equal(await page.locator(`${WIDGET} header .state`).innerText(), 'only on this device');
					assert.deepEqual(await page.locator(`${WIDGET} .actions[data-db="datum"] button`).allInnerTexts(), ['Add device']);
					await noneOfTheOld(page);
				},
				{ viewport }
			);
	});

	test('✎ edits this device’s name and the username in place: @username under the name, kept across a reload, and told to every chain (da-5wlnr)', async () => {
		for (const viewport of [{ width: 390, height: 844 }, DESKTOP.viewport])
			await withWallClock(
				async (app) => {
					const { page } = app;
					await member(app, 'home', 'phone');
					const top = page.locator(`${WIDGET} header`);
					const edit = top.locator('.edit');
					const rename = async (name, user) => {
						await top.locator('button.rename').click();
						await edit.getByLabel('This device’s name').fill(name);
						await edit.getByLabel('Your username (optional)').fill(user);
						await edit.getByRole('button', { name: 'Save' }).click();
						await edit.waitFor({ state: 'detached' });
					};

					// In no chain: kept on this device, for the chain its first share starts.
					await app.goto('/sync');
					assert.equal(await top.locator('.user').count(), 0);
					await rename('desk', 'bea');
					assert.equal(await top.locator('.device').innerText(), 'desk');
					assert.equal(await top.locator('.user').innerText(), '@bea');
					await app.reload();
					await until(() => top.locator('.device').innerText(), (t) => t === 'desk');
					assert.equal(await top.locator('.user').innerText(), '@bea');
					// Escape puts it back, unsaved.
					await top.locator('button.rename').click();
					await edit.getByLabel('This device’s name').fill('nope');
					await edit.getByLabel('This device’s name').press('Escape');
					assert.equal(await top.locator('.device').innerText(), 'desk');

					// In a chain: its label there, and the username beside it.
					await app.goto('/sync?db=home');
					await rename('laptop browser', 'bo');
					const me = async () => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'chain', 'list', '--json'])).members.find((m) => m.label === 'laptop browser');
					await until(me, Boolean);
					assert.equal((await me()).user, 'bo');
					await until(() => top.locator('.device').innerText(), (t) => t === 'laptop browser');
					assert.equal(await top.locator('.user').innerText(), '@bo');
					// The grid is read after the chain, so its row follows a beat later.
					await until(() => page.locator(`${ROWS}.here th`).innerText(), (t) => t === 'laptop browser');
				},
				{ viewport }
			);
	});

	test('✎ saved after a rename lands meanwhile keeps that rename: only what the edit changed is written', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await member(app, 'home', 'phone');
			await app.goto('/sync?db=home');
			const top = page.locator(`${WIDGET} header`);
			const edit = top.locator('.edit');
			await top.locator('button.rename').click();
			// Another rename lands while the edit is open, as another device's would: the grid reads it.
			await cli(app, ['--db', 'home', 'sync', 'chain', 'rename', 'elsewhere']);
			await until(() => page.locator(`${ROWS}.here th`).innerText(), (t) => t === 'elsewhere');
			await edit.getByLabel('Your username (optional)').fill('cy');
			await edit.getByRole('button', { name: 'Save' }).click();
			await edit.waitFor({ state: 'detached' });
			const me = async () => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).this;
			await until(async () => (await cli(app, ['--db', 'home', 'sync', 'chain', 'list', '--json'])).includes('"user":"cy"'), Boolean);
			assert.equal((await me()).label, 'elsewhere');
		}, DESKTOP);
	});

	test('a row per device, this one first, each cell its last sync as a mark; the top names this device and how the database stands', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await fixture(app);
			await onHome(app);

			const [first, ...rest] = (await lines(page)).filter(([, mark]) => mark);
			assert.deepEqual(first, ['datum.day', 'error']);
			assert.deepEqual(rest.sort(([a], [b]) => a.localeCompare(b)), [
				['laptop', 'synced'],
				['phone', 'pending'],
				['tablet', 'error'],
				['watch', 'pending']
			]);
			assert.equal(await line(page, 'laptop').locator('.mark').innerText(), '✓');
			assert.equal(await line(page, 'laptop').getAttribute('data-state'), 'member');
			assert.match(await line(page, 'phone').getAttribute('title'), /\bpending\b.*\b\d+\b/);
			assert.match(await line(page, 'watch').getAttribute('title'), /never synced/);
			// An error's reason, verbatim, a hover away.
			assert.ok((await line(page, 'tablet').getAttribute('title')).includes(REFUSED));
			// The top: this device, and the database as a whole, its worst device and how many it has not reached.
			assert.equal(await page.locator(`${WIDGET} header .device`).innerText(), 'datum.day');
			const state = page.locator(`${WIDGET} header .state`);
			assert.equal(await state.getAttribute('data-mark'), 'error');
			assert.equal(await state.innerText(), '! 3 devices not reached');

			// Nobody leads `home`: every other device is offline, its state
			// last known, grey — never a green ✓.
			const mark = line(page, 'laptop').locator('.mark');
			assert.match(await mark.getAttribute('class'), /\bstale\b/);
			const color = (l) => l.evaluate((e) => getComputedStyle(e).color);
			assert.notEqual(await color(mark), await color(page.locator(`${WIDGET} .grid tr.here .mark`)));

			// The other databases are the grid's, folded behind «other dbs ›»; and the old lists are gone.
			assert.equal(await page.locator(`${WIDGET} .grid button.fold`).innerText(), 'other dbs ›');
			await noneOfTheOld(page);
		}, DESKTOP);
	});

	test('a cell is a symbol and the time since that device last synced; tapped, the pair’s details and acts, grayed where a key file locks the chain; a database’s name pauses, resumes, and asks before it stops syncing (da-dxng1)', async () => {
		for (const viewport of [{ width: 390, height: 844 }, DESKTOP.viewport])
			await withWallClock(
				async (app) => {
					const { page } = app;
					const ids = await fixture(app);
					await onHome(app);
					const status = async () => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json']));
					// The time is the CLI's last good sync: the laptop's verified a moment ago, the tablet's never.
					const lap = (await status()).members.find((m) => m.endpoint === ids.laptop);
					assert.ok(Date.now() - Date.parse(lap.last_synced) < 60_000, lap.last_synced);
					assert.equal(await line(page, 'laptop').locator('.ago').innerText(), 'now');
					// A screen reader hears what the symbols say: the button's name carries the state and the time.
					assert.match(await line(page, 'laptop').locator('button.cell').getAttribute('aria-label'), /^laptop in home: synced\b.*, last synced just now$/);
					assert.match(await line(page, 'tablet').locator('button.cell').getAttribute('aria-label'), new RegExp(`^tablet in home: error: ${REFUSED}`));
					assert.equal(await line(page, 'tablet').locator('.mark').innerText(), '!');
					assert.equal(await line(page, 'tablet').locator('.ago').count(), 0);
					// Symbols, never words.
					for (const label of ['laptop', 'phone', 'tablet', 'watch'])
						assert.doesNotMatch(await line(page, label).innerText(), /member|reader|synced|pending|error|offline|never/, label);

					const pair = page.locator(`${WIDGET} .grid .pair[data-db="home"]`);
					const acts = pair.locator('button[data-do]');
					await line(page, 'laptop').locator('button.cell').click();
					assert.equal(await pair.locator('.head').innerText(), 'laptop × home');
					assert.match(await pair.locator('.standing').innerText(), /^reads and writes it/);
					assert.match(await pair.locator('.last').innerText(), /^last synced .+, just now$/);
					assert.deepEqual(await acts.allInnerTexts(), ['Make read-only', 'Remove', 'Unjoin', 'Wipe its copy']);
					for (const b of await acts.all()) assert.equal(await b.isDisabled(), false);
					assert.equal(await pair.locator('.why').count(), 0);
					// Another cell takes its place, its error verbatim; tapped again, it closes.
					await line(page, 'tablet').locator('button.cell').click();
					assert.equal(await pair.locator('.failure-text').innerText(), REFUSED);
					await line(page, 'tablet').locator('button.cell').click();
					assert.equal(await pair.count(), 0);

					// A key file locks the chain: nothing here can sign as it, so every act is grayed and says why.
					await cli(app, ['--db', 'home', 'sync', 'admin', 'add', '/data/home-admin.key', '--label', 'laptop key']);
					await onHome(app);
					await line(page, 'laptop').locator('button.cell').click();
					assert.match(await pair.locator('.why').innerText(), /locked: only its admin \(laptop key\)/);
					for (const b of await acts.all()) assert.equal(await b.isDisabled(), true);

					// The database's name opens its menu in the cell's place: pause, resume, stop syncing.
					await page.locator(`${WIDGET} .grid thead th[data-db="home"] button.name`).click();
					assert.equal(await pair.count(), 0);
					const menu = page.locator(`${WIDGET} .grid .dbmenu[data-db="home"]`);
					assert.deepEqual(await menu.locator('button').allInnerTexts(), ['Pause sync', 'Stop syncing']);
					await menu.getByRole('button', { name: 'Pause sync' }).click();
					await menu.getByRole('button', { name: 'Resume sync' }).waitFor();
					assert.ok((await status()).paused);
					assert.equal(await page.locator(`${WIDGET} header .state`).innerText(), 'paused');
					await menu.getByRole('button', { name: 'Resume sync' }).click();
					await menu.getByRole('button', { name: 'Pause sync' }).waitFor();
					assert.ok(!(await status()).paused);
					await menu.getByRole('button', { name: 'Stop syncing' }).click();
					const dialog = page.locator('dialog.confirm');
					const said = await dialog.innerText();
					for (const fact of ['Stop syncing home on this device?', 'keeps every entry', 'other devices keep their copies', 'nothing is deleted'])
						assert.ok(said.includes(fact), fact);
					await dialog.getByRole('button', { name: 'cancel' }).click();
					await dialog.waitFor({ state: 'detached' });
					assert.ok((await status()).chain, 'still in its chain');
				},
				{ viewport }
			);
	});

	test('a device its last session never reached is offline: its reason a hover away, and not an error', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await fixture(app);
			const kiosk = await member(app, 'work', 'kiosk');
			await session(app, 'work', kiosk, '--vector', '{}');
			await session(app, 'work', kiosk, '--error', UNREACHED, '--offline');
			await app.goto('/sync?db=work');
			await rowsDrawn(page, 3, 'work');
			const row = line(page, 'kiosk', 'work');
			assert.equal(await row.getAttribute('data-mark'), 'offline');
			assert.equal(await row.locator('.mark').innerText(), '○');
			// Why is a hover away, in the tooltip, verbatim.
			assert.ok((await row.getAttribute('title')).startsWith(`offline: ${UNREACHED}`));
			// Not up is no fault: the database as a whole is pending, never red, and neither device is reached.
			const state = page.locator(`${WIDGET} header .state`);
			assert.equal(await state.getAttribute('data-mark'), 'pending');
			assert.equal(await state.innerText(), '◔ 2 devices not reached');
		}, DESKTOP);
	});

	test('the syncing mark follows the worker’s broadcast, and a device not reachable is grey', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const ids = await fixture(app);
			await onHome(app);
			const cell = line(page, 'laptop');
			const marked = async (want) => until(() => cell.locator('.mark').getAttribute('class'), (c) => c?.split(' ').includes(want));

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'reachable', lastSeen: Date.now() } });
			await marked('synced');
			assert.equal(await cell.getAttribute('data-mark'), 'synced');

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'online' } });
			await marked('syncing');
			assert.equal(await cell.getAttribute('data-mark'), 'syncing');
			assert.equal(await cell.locator('.mark').innerText(), '↻');

			await broadcast(app, 'home', { [ids.laptop]: { label: 'laptop', reach: 'offline' } });
			await marked('stale');
		}, DESKTOP);
	});

	test('a reader’s cell says reader, and read-only in its title', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const run = inPage(app, 'home');
			const id = endpointId();
			const { secret } = JSON.parse(await run(['sync', 'new-invite', '--reader', '--expires', '1d']));
			await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
			await app.goto('/sync?db=home');
			const reader = line(page, 'kindle');
			await reader.waitFor();
			assert.equal(await reader.getAttribute('data-state'), 'reader');
			assert.match(await reader.getAttribute('title'), /, read-only\b/);
			// This copy is a member.
			assert.equal(await line(page, 'this').getAttribute('data-state'), 'member');
		}, DESKTOP);
	});

	test('a filtered reader’s title says how many fields it is sent', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const run = inPage(app, 'home');
			const id = endpointId();
			await run(['occur', 'tea']);
			const invite = ['sync', 'new-invite', '--fields', 'coffee', '--expires', '1d'];
			const { secret } = JSON.parse(await run(invite));
			await run(['sync', 'handshake', id, '--label', 'kindle', '--addr', '{}', '--invite', secret, '--chain', '']);
			await app.goto('/sync?db=home');
			const reader = line(page, 'kindle');
			await reader.waitFor();
			assert.equal(await reader.getAttribute('data-state'), 'reader');
			// Read-only, and which part it is sent: 1 field of 2.
			const title = await reader.getAttribute('title');
			assert.match(title, /\bread-only\b/);
			assert.match(title, /\b1 of 2\b/);
			assert.doesNotMatch((await line(page, 'this').getAttribute('title')) ?? '', /\bof\b/);
		}, DESKTOP);
	});

	test('an expelled or unjoined device is not drawn at all, and nothing says one is hidden (da-5wlnr)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const phone = await member(app, 'home', 'phone');
			const tablet = await member(app, 'home', 'tablet');
			await member(app, 'home', 'watch');
			await cli(app, ['--db', 'home', 'sync', 'chain', 'expel', phone]);
			await cli(app, ['--db', 'home', 'sync', 'chain', 'expel', tablet, '--unjoin', '--yes']);
			await app.goto('/sync?db=home');
			await line(page, 'watch').waitFor();
			assert.deepEqual((await lines(page)).map(([label]) => label), ['datum.day', 'watch']);
			assert.equal(await page.locator(`${WIDGET} .revoked`).count(), 0);
			// Only watch is reached or not: the expelled two are no device of this database.
			assert.equal(await page.locator(`${WIDGET} header .state`).innerText(), '◔ 1 device not reached');
		}, DESKTOP);
	});

	test('two databases in one chain: one column, the page’s', async () => {
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
			await rowsDrawn(page, 2);
			// `work`'s copy is one of home's devices.
			assert.deepEqual((await lines(page)).map(([label]) => label), ['datum.day', 'work']);
			// One chain, one column in the grid: this db's, whichever of the two the page is on (da-opbpa).
			const head = page.locator(`${WIDGET} .grid thead th.here .name`);
			assert.equal(await head.innerText(), 'home');
			await app.goto('/sync?db=work');
			assert.equal(await head.innerText(), 'work');
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
					// The database, the device, and how far ahead its clock is.
					const said = await notice.innerText();
					assert.match(said, /^⚠/);
					for (const fact of ['home', 'laptop', '1 hour']) assert.ok(said.includes(fact), `${fact}: ${said}`);
				},
				{ clock: Date.now() - 3_600_000 }
			);
		} finally {
			await lap.close();
		}
	});

	test('the grid shows this database’s column first, the others folded behind «other dbs ›», which unfolds them (da-opbpa)', async () => {
		for (const viewport of [{ width: 390, height: 844 }, DESKTOP.viewport])
			await withWallClock(
				async (app) => {
					const { page } = app;
					const heads = () => page.locator(`${WIDGET} .grid thead th[data-db]`).evaluateAll((ths) => ths.map((th) => th.dataset.db));
					const here = () => page.locator(`${WIDGET} .grid thead th.here`).evaluateAll((ths) => ths.map((th) => th.dataset.db));
					const fold = page.locator(`${WIDGET} .grid button.fold`);
					const opened = async (db) => {
						await app.goto(`/sync?db=${db}`);
						await page.locator(`${WIDGET} .grid th[data-db="${db}"]`).waitFor();
					};
					// One database, in a chain of its own: its column alone, and nothing to unfold.
					await member(app, 'datum', 'phone');
					await opened('datum');
					assert.deepEqual(await heads(), ['datum']);
					assert.equal(await fold.count(), 0);

					// Two more, one chained: folded, this one's column alone, marked.
					await member(app, 'home', 'tablet');
					await cli(app, ['--db', 'notes', 'occur', 'jot']);
					await opened('home');
					assert.deepEqual(await heads(), ['home']);
					assert.deepEqual(await here(), ['home']);
					assert.equal(await fold.innerText(), 'other dbs ›');
					await fold.click();
					await until(heads, (h) => h.length === 3);
					assert.equal((await heads())[0], 'home');
					assert.deepEqual((await heads()).toSorted(), ['datum', 'home', 'notes']);
					assert.equal(await fold.innerText(), '‹ fewer');

					// Another database first, chained or not, whichever is open; the fold folds again.
					await opened('notes');
					assert.equal((await heads())[0], 'notes');
					assert.deepEqual(await here(), ['notes']);
					// Unfolded still: the session holds it across databases.
					assert.equal(await fold.getAttribute('aria-expanded'), 'true');
					await fold.click();
					await until(heads, (h) => h.length === 1);
					assert.deepEqual(await heads(), ['notes']);
				},
				{ viewport }
			);
	});
});

// da-77hqt: a write read every database's sync state at once, the page's own
// included, past the second a layout behind waits for: so a tap in collect,
// with the sync tab kept behind it, waited on those reads.
describe('when the sync widget reads again', () => {
	/**
	 * Every `chain list`, `sync status` and `sync grid` from here, by when it
	 * started and what it ran, and when each `occur` began: `[ms, argv]`.
	 */
	const watch = (page) =>
		page.evaluate(() => {
			window.__syncRuns = [];
			const previous = window.datum.setRunner((argv, ...rest) => {
				const i = argv.indexOf('sync');
				const read = i >= 0 && (['status', 'grid'].includes(argv[i + 1]) || (argv[i + 1] === 'chain' && argv[i + 2] === 'list'));
				if (read || argv.includes('occur')) window.__syncRuns.push([performance.now(), argv.join(' ')]);
				return previous(argv, ...rest);
			});
		});
	const runs = (page) => page.evaluate(() => window.__syncRuns);
	/** The rounds of reading `db`'s chain: one per `read()`. */
	const rounds = (seen, db) => seen.filter(([, a]) => a.startsWith(`--db ${db} sync chain list`)).length;

	test('behind collect, a logging write reads no sync state until the page has been quiet a second, then reads once', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await fixture(app);
			await onHome(app);
			await app.toCollect();
			await app.idle();
			await watch(page);
			await app.submit('occur coffee');
			await page.waitForFunction(() => window.__syncRuns.some(([, a]) => a.includes('sync chain list')));
			await app.idle();
			await new Promise((resolve) => setTimeout(resolve, 2_500));
			const seen = await runs(page);
			const [wrote] = seen.find(([, a]) => a.includes('occur coffee'));
			const first = Math.min(...seen.filter(([, a]) => !a.includes('occur')).map(([at]) => at));
			assert.ok(first - wrote >= 1_000, `a sync read began ${Math.round(first - wrote)} ms after the write: ${JSON.stringify(seen)}`);
			assert.equal(rounds(seen, 'home'), 1, JSON.stringify(seen));
		}, DESKTOP);
	});

	test('in front, one write to its database reads once', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await fixture(app);
			await onHome(app);
			await app.idle();
			await watch(page);
			// As the page runs a command (its tab's `sync` pill would take a typed line).
			assert.equal((await page.evaluate(() => window.datum.run(['occur', 'coffee']))).code, 0);
			await app.idle();
			await new Promise((resolve) => setTimeout(resolve, 2_500));
			const seen = await runs(page);
			assert.equal(rounds(seen, 'home'), 1, JSON.stringify(seen));
		}, DESKTOP);
	});

	test('a write to another database, which moves no epoch here, still redraws its column', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const ids = await fixture(app);
			await onHome(app);
			await page.locator(`${WIDGET} .grid button.fold`).click();
			const work = line(page, 'laptop', 'work');
			await until(() => work.getAttribute('data-mark'), (m) => m === 'pending');
			await session(app, 'work', ids.workLaptop, '--error', REFUSED);
			await until(() => work.getAttribute('data-mark'), (m) => m === 'error');
		}, DESKTOP);
	});
});

// da-uk23v: a chain restore's wipe reaching the page takes what it held, an
// entry made offline before the restore with it; it must not be lost.
describe('a chain restore reaching datum.day', () => {
	test('keeps what it took in a backup beside the database, in OPFS past a reload, which its notice names and `db restore --merge` brings back', async () => {
		const lap = await laptop();
		try {
			await withWallClock(async (app) => {
				const home = inPage(app, 'home');
				await chain(app, 'home', lap);
				const good = await lap.run(['db', 'backup', '-']);
				await home(['occur', 'before']);
				await lap.run(['db', 'restore', '-', '--hard', '--chain', '--yes'], good);
				await home(['sync', 'pipe', 'apply'], await lap.run(['sync', 'pipe', 'ops']));
				assert.equal((await home(['tail'])).includes('before'), false);

				const notice = JSON.parse(await home(['sync', 'status', '--json'])).notices.find((n) => n.kind === 'wipe');
				const file = notice?.text.match(/`datum db restore (\S+) --merge`/)?.[1];
				assert.ok(file, JSON.stringify(notice));
				await app.reload();
				const names = await app.page.evaluate(async (dir) => {
					const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
					const names = [];
					for await (const name of files.keys()) names.push(name);
					return names;
				}, OPFS_DIR);
				assert.ok(names.includes(file.split('/').at(-1)), names.join(', '));
				await home(['db', 'restore', file, '--merge']);
				assert.equal((await home(['tail'])).includes('before'), true);
			});
		} finally {
			await lap.close();
		}
	});
});
