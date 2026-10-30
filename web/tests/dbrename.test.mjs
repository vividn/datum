// `db rename` in datum.day (da-rktt6): the OPFS file moves, and so does
// everything the page keeps by the database's name — its held days, its
// shell, the tab's `?db=` and the sync leader's lock — as `datum --db OLD db
// rename NEW` does natively (da-o7ay8).

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, ready, startApp, stopApp, withApp } from './support/app.mjs';
import { countOf, nativeCopy, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { until } from './support/until.mjs';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

/** The `.db` files in the worker's OPFS directory. */
const files = (app) =>
	app.page.evaluate(async () => {
		const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('datum-cli');
		const names = [];
		for await (const name of dir.keys()) if (name.endsWith('.db')) names.push(name);
		return names.sort();
	});

/** The held days' databases in IndexedDB (#lib/heldDays.ts). */
const heldDbs = (app) =>
	app.page.evaluate(
		() =>
			new Promise((resolve) => {
				const request = indexedDB.open('datum-held-days', 1);
				request.onupgradeneeded = () => request.result.createObjectStore('days', { keyPath: ['db', 'size', 'date'] });
				request.onsuccess = () => {
					const all = request.result.transaction('days').objectStore('days').getAll();
					all.onsuccess = () => resolve([...new Set(all.result.map((row) => row.db))].sort());
				};
			})
	);

const endpointOf = (app, db) => app.page.evaluate((db) => window.datum.sync.states()[db]?.endpoint, db);

test('a chained database renamed at the bar syncs under its new name, the same member, and the page follows it', async () => {
	const native = await nativeCopy(relay);
	try {
		await withWallClock(
			async (app) => {
				const serving = native.spawn(['sync', 'chain', 'invite']);
				const said = await until(() => /datum sync chain join '([^']+)'/.exec(serving.stdout), Boolean);
				const joined = await app.page.evaluate((t) => window.datum.sync.connect('work-2', t), said[1]);
				assert.equal(joined.verified, true);
				native.spawn(['sync', 'run']);
				await app.run(['occur', 'before']);
				await until(() => native.count('before'), (n) => n === 1);
				const endpoint = await until(() => endpointOf(app, 'work-2'), Boolean);
				// A day held for it, as the strip keeps a finished one.
				await app.page.evaluate(
					() =>
						new Promise((resolve) => {
							const request = indexedDB.open('datum-held-days', 1);
							request.onupgradeneeded = () => request.result.createObjectStore('days', { keyPath: ['db', 'size', 'date'] });
							request.onsuccess = () => {
								const tx = request.result.transaction('days', 'readwrite');
								tx.objectStore('days').put({ db: 'work-2', size: 's', date: '2026-01-01', svg: '<svg/>', at: 1 });
								tx.oncomplete = resolve;
							};
						})
				);
				assert.ok(await app.page.evaluate(() => localStorage.getItem('datum.layout.shell.v1.work-2')));
				// Another tab on it goes too.
				const other = await app.newTab('?db=work-2');

				const input = app.page.locator('.prompt-bar input');
				await input.fill('db rename work');
				await input.press('Enter');
				await app.page.waitForURL((url) => url.searchParams.get('db') === 'work');
				await ready(app.page);
				await other.page.waitForURL((url) => url.searchParams.get('db') === 'work');

				assert.equal(await countOf(app, 'before'), 1);
				assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'work.db']);
				assert.deepEqual(await heldDbs(app), ['work']);
				assert.equal(await app.page.evaluate(() => localStorage.getItem('datum.layout.shell.v1.work-2')), null);
				assert.equal(await until(() => endpointOf(app, 'work'), Boolean), endpoint);
				assert.equal(await endpointOf(app, 'work-2'), undefined);

				await app.run(['occur', 'after']);
				await until(() => native.count('after'), (n) => n === 1, 60_000);
				await native.run(['occur', 'back']);
				await until(() => countOf(app, 'back'), (n) => n === 1, 60_000);
				const members = JSON.parse((await native.run(['sync', 'chain', 'list', '--json'])).stdout).members;
				assert.equal(members.filter((m) => m.endpoint === endpoint).length, 1);
			},
			{ prepare: relayed(relay), query: '?db=work-2' }
		);
	} finally {
		await native.close();
	}
});

test('a tab on another database that once named the old one does not make it again after the rename', async () => {
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			// Its worker has the old name's files among its own from now on.
			const other = await app.newTab('?db=stay');
			assert.equal(Number((await other.run(['--db', 'gone-1', 'count', 'mine'])).stdout), 1);

			const input = app.page.locator('.prompt-bar input');
			await input.fill('db rename gone');
			await input.press('Enter');
			await app.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await ready(app.page);

			await other.run(['occur', 'theirs']);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'gone.db', 'stay.db']);
		},
		{ query: '?db=gone-1' }
	);
});

test('a command asked of the old name before the rename, that runs after it, is refused and does not make it again', async () => {
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			const other = await app.newTab('?db=stay');
			assert.equal(Number((await other.run(['--db', 'gone-2', 'count', 'mine'])).stdout), 1);
			const pending = () => app.page.evaluate(async () => (await navigator.locks.query()).pending.length);
			// The files held, so the rename and then the other tab's command queue for them, in that order.
			await other.page.evaluate(() => {
				navigator.locks.request('datum-cli-files', () => new Promise((resolve) => (window.letGo = resolve)));
			});
			const input = app.page.locator('.prompt-bar input');
			await input.fill('db rename gone');
			await input.press('Enter');
			await until(pending, (n) => n === 1);
			const asked = other.run(['--db', 'gone-2', 'count', 'mine']);
			await until(pending, (n) => n === 2);
			await other.page.evaluate(() => window.letGo());

			const refused = await asked;
			assert.notEqual(refused.code, 0);
			assert.match(refused.stderr, /gone-2 was renamed by another tab/);
			await app.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await ready(app.page);
			assert.equal(await countOf(app, 'mine'), 1);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'gone.db', 'stay.db']);
		},
		{ query: '?db=gone-2' }
	);
});

test('a tab that loads on the old name just after the rename, too late to hear it, follows it and does not make it again', async () => {
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			// Away while it moves, as a tab reloading then would be.
			const other = await app.newTab('?db=gone-3');
			await other.page.goto('about:blank');

			const input = app.page.locator('.prompt-bar input');
			await input.fill('db rename gone');
			await input.press('Enter');
			await app.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await ready(app.page);

			const old = new URL(app.page.url());
			old.searchParams.set('db', 'gone-3');
			await other.page.goto(old.href);
			await other.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await ready(other.page);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'gone.db']);
		},
		{ query: '?db=gone-3' }
	);
});

test('a tab whose day strip is still asking as the rename reaches it follows it, and does not freeze', async () => {
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			const other = await app.newTab('?db=gone-4');
			// Its next digests alone held, and answered with none just after it hears
			// the move, so the pictures it then asks for come after `follow`.
			await other.page.evaluate(() => {
				const real = window.datum.setRunner((argv, ...rest) =>
					argv.includes('--digests') && !window.letGo
						? new Promise((resolve) => (window.letGo = () => resolve({ argv, code: 0, stdout: '{}', stderr: '' })))
						: real(argv, ...rest)
				);
				new BroadcastChannel('datum-db-changes').onmessage = ({ data }) => data.moved && window.letGo();
			});
			await app.run(['occur', 'more']);
			await until(() => other.page.evaluate(() => typeof window.letGo), (type) => type === 'function');

			const input = app.page.locator('.prompt-bar input');
			await input.fill('db rename gone');
			await input.press('Enter');
			await app.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await other.page.waitForURL((url) => url.searchParams.get('db') === 'gone');
			await ready(other.page);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'gone.db']);
		},
		{ query: '?db=gone-4' }
	);
});

test('a rename onto a database that holds data is refused with the purge that empties it, and nothing moves', async () => {
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			await app.run(['--db', 'fin', 'occur', 'theirs']);
			const refused = await app.run(['db', 'rename', 'fin']);
			assert.notEqual(refused.code, 0);
			assert.match(refused.stderr, /«fin» exists and holds \d+ entries/);
			assert.match(refused.stderr, /`datum --db fin db purge`.*then `datum --db fin-2 db rename fin`/s);
			assert.equal(new URL(app.page.url()).searchParams.get('db'), 'fin-2');
			assert.equal(await countOf(app, 'mine'), 1);
			assert.equal(Number((await app.run(['--db', 'fin', 'count', 'theirs'])).stdout), 1);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'fin-2.db', 'fin.db']);
		},
		{ query: '?db=fin-2' }
	);
});

test('a rename whose new file cannot be kept loses nothing: the old database stays, with its entries, and the page on it', async () => {
	// OPFS refuses the new name's file, as a full disk would.
	const refuse = `
		const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
		FileSystemFileHandle.prototype.createSyncAccessHandle = function () {
			return this.name === 'full.db' ? Promise.reject(new DOMException('no room', 'QuotaExceededError')) : real.call(this);
		};
	`;
	await withApp(
		async (app) => {
			await app.run(['occur', 'mine']);
			const input = app.page.locator('.prompt-bar input');
			await input.fill('db rename full');
			await input.press('Enter');
			const last = await until(
				() => app.page.evaluate(() => window.datum.bar.last()),
				(last) => last?.line === 'db rename full'
			);
			assert.notEqual(last.result.code, 0);
			assert.match(last.result.stderr, /the rename to full was not kept, and nothing moved/);
			await app.idle();
			assert.equal(new URL(app.page.url()).searchParams.get('db'), 'keep-1');
			assert.equal(await countOf(app, 'mine'), 1);
			assert.deepEqual(await files(app), ['_internal.db', 'datum.db', 'keep-1.db']);
		},
		{ query: '?db=keep-1', prepare: inWorker(refuse) }
	);
});
