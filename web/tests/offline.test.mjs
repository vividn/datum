// The app with the network taken away.
//
// datum.day is local-first: the data is on the device and the network is
// only for sync, so an installed app has to open its database with no
// network at all. It did not. `$service-worker`'s `build` list comes from
// the *client* Vite manifest, and Vite compiles workers in a rollup pass of
// its own — so the SQLite worker, its chunks, the OPFS async proxy and
// sqlite3.wasm (more than half the payload) were in no precache list, and
// offline the fetch handler answered a request for a wasm module with the
// cached HTML shell (da-d1l9).
//
// The first test here is the bug as the user meets it: load once, go
// offline, reload, log an entry. The second reads the built service worker
// directly, so the file that regresses names itself.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const built = path.join(web, 'build');

before(startApp);
after(stopApp);

describe('with no network', () => {
	test('the app reloads and the database still opens', async () => {
		await withApp(async ({ page, run, entries, reload }) => {
			// Online first, because that is when the service worker installs.
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, 0);

			// `controller` is non-null only once a service worker has
			// installed (precache included) and claimed this page.
			await page.waitForFunction(() => navigator.serviceWorker.controller !== null);

			// The HTTP cache would answer some of these requests even with
			// the network down, which would let a missing precache entry
			// pass. Empty it, so what survives is only what the service
			// worker put in Cache Storage.
			const cdp = await page.context().newCDPSession(page);
			await cdp.send('Network.clearBrowserCache');

			await page.context().setOffline(true);
			try {
				// `reload` waits for the REPL to enable, which the app does
				// only after `initDb()` resolves — so reaching the end of it
				// with no network *is* the database opening offline.
				await reload();

				// And it is the same database, not a fresh in-memory one.
				assert.deepEqual(
					(await entries()).map((e) => e.field),
					['coffee']
				);

				const logged = await run(['occur', 'tea', '-t', '0800']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.deepEqual(
					(await entries()).map((e) => e.field).sort(),
					['coffee', 'tea']
				);
			} finally {
				await page.context().setOffline(false);
			}
		});
	});

	test('the service worker precaches every file of the worker bundle', () => {
		const workers = path.join(built, '_app', 'immutable', 'workers');
		const emitted = fs
			.readdirSync(workers, { recursive: true, withFileTypes: true })
			.filter((entry) => entry.isFile())
			.map((entry) => path.posix.join(path.relative(workers, entry.parentPath), entry.name))
			// Behind `?wasi` while da-1vat measures it; see vite.config.ts.
			.filter((name) => !name.includes('/datum-cli-'));
		assert.ok(emitted.length > 0, `no worker bundle under ${workers}`);

		const sw = fs.readFileSync(path.join(built, 'service-worker.js'), 'utf8');
		const missing = emitted.filter((name) => !sw.includes(`/_app/immutable/workers/${name}`));
		assert.deepEqual(missing, [], 'not named in the precache list');

		// The two that matter most, named rather than counted: without them
		// there is no SQLite and no OPFS, so there is no database.
		assert.ok(
			emitted.some((name) => name.endsWith('.wasm')),
			`no sqlite3 wasm among ${emitted.join(', ')}`
		);
		assert.ok(
			emitted.some((name) => name.startsWith('db.worker-')),
			`no database worker among ${emitted.join(', ')}`
		);
	});
});
