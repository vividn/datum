// The app with the network taken away.
//
// datum.day is local-first: the data is on the device and the network is
// only for sync, so an installed app has to open its database with no
// network at all. It did not. `$app/manifest`'s `immutable` list comes from
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
import { until } from './support/until.mjs';

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
				// `reload` waits for the CLI widget to enable, which the app does
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
			.map((entry) => path.posix.join(path.relative(workers, entry.parentPath), entry.name));
		assert.ok(emitted.length > 0, `no worker bundle under ${workers}`);

		const sw = fs.readFileSync(path.join(built, 'service-worker.js'), 'utf8');
		const missing = emitted.filter((name) => !sw.includes(`/_app/immutable/workers/${name}`));
		assert.deepEqual(missing, [], 'not named in the precache list');

		// The two that matter most, named rather than counted: without them
		// there is no datum and no OPFS, so there is no database.
		assert.ok(
			emitted.some((name) => /datum-cli-.*\.wasm$/.test(name)),
			`no datum-cli wasm among ${emitted.join(', ')}`
		);
		assert.ok(
			emitted.some((name) => name.startsWith('db.worker-')),
			`no database worker among ${emitted.join(', ')}`
		);
	});
});

// An origin that fails slowly (da-n8w0e): datum.day's gave no first byte for
// 25-40 s and then a 500. A hanging fetch never rejects, so a navigation the
// service worker sent to the network waited as long as the network did.
// The routes below stand in for that origin, for the service worker's own
// fetches as much as the page's.
describe('with an origin that hangs or answers 500', () => {
	const hang = () => new Promise(() => {});
	const fail = (route) => route.fulfill({ status: 500, body: 'origin error' });

	test('a navigation gets the cached shell, and an asset what the origin sent', async () => {
		await withApp(async ({ page }) => {
			await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
			const shell = await page.evaluate(async () => (await caches.match('/')).text());
			const context = page.context();
			await context.route('**/hangs', hang);
			await context.route('**/fails', fail);
			await context.route('**/hangs.js', hang);
			await context.route('**/fails.js', fail);

			// Not a precached path, so the service worker asks the network.
			for (const path of ['/hangs', '/fails']) {
				const started = Date.now();
				const response = await page.goto(new URL(path, page.url()).href, { waitUntil: 'commit' });
				assert.equal(response.status(), 200, path);
				assert.equal(await response.text(), shell, path);
				assert.ok(Date.now() - started < 10_000, `${path} took ${Date.now() - started} ms`);
			}

			// An asset is never answered with HTML: its 500 is passed on, and
			// a hang still hangs past the navigation's timeout.
			const asked = await page.evaluate(async () => {
				const fails = (await fetch('/fails.js')).status;
				const hangs = await Promise.race([
					fetch('/hangs.js').then((r) => r.status),
					new Promise((done) => setTimeout(() => done('pending'), 5000))
				]);
				return { fails, hangs };
			});
			assert.deepEqual(asked, { fails: 500, hangs: 'pending' });
		});
	});
});

// A deploy (da-v1wqf): `/` is precached, and was answered cache-first, so the
// page showed the build the current worker cached until a new worker
// installed, and a deploy took several reloads to arrive. A route stands in
// for the deploy: the origin's `/` from then on carries a mark the cached
// shell does not.
describe('after a deploy', () => {
	test('a navigation to / gets the new shell at once, and the cached one offline', async () => {
		await withApp(async ({ page }) => {
			await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
			assert.equal(
				await page.evaluate(async () => (await navigator.serviceWorker.getRegistration()).updateViaCache),
				'none'
			);
			const shell = await page.evaluate(async () => (await caches.match('/')).text());
			const root = new URL('/', page.url()).href;
			const deployed = (url) => url.href === root;
			const mark = '<meta name="deploy" content="next">';
			await page.context().route(deployed, async (route) => {
				const response = await route.fetch();
				await route.fulfill({ response, body: (await response.text()).replace('<head>', `<head>${mark}`) });
			});
			assert.ok(!shell.includes(mark));

			const online = await page.goto(root, { waitUntil: 'commit' });
			assert.ok((await online.text()).includes(mark), 'the deploy is not the page');

			await page.context().unroute(deployed);
			await page.context().setOffline(true);
			try {
				const offline = await page.goto(root, { waitUntil: 'commit' });
				assert.equal(offline.status(), 200);
				assert.equal(await offline.text(), shell);
			} finally {
				await page.context().setOffline(false);
			}
		});
	});
});

// A planted cache (da-dvvl9): `caches.match` looks in every cache on the
// origin, the one created first answering, so a cache any script on the
// origin made under another name answered for every build after it. Planted
// here before a build's worker installs, as a script run once on the origin
// would be before the next deploy: one under no build's name, which the
// activate deletes, and one under a kept build's (`datum/v/<version>-`),
// which it keeps and which still answers nothing.
describe('with a cache planted under another name', () => {
	test("the page gets this build's bytes, and the planted cache is gone", async () => {
		await withApp(async ({ page, reload }) => {
			await page.waitForFunction(() => navigator.serviceWorker.controller !== null);
			const build = await page.evaluate(async () => {
				const name = (await caches.keys()).find((key) => key.startsWith('datum-'));
				const cache = await caches.open(name);
				const chunk = (await cache.keys())
					.map((request) => new URL(request.url).pathname)
					.find((path) => path.startsWith('/_app/immutable/') && path.endsWith('.js'));
				const shell = await (await cache.match('/')).text();
				const code = await (await cache.match(chunk)).text();
				for (const planted of ['planted', 'datum/v/0.0.0-kept']) {
					const foreign = await caches.open(planted);
					for (const path of ['/', chunk, '/elsewhere']) await foreign.put(path, new Response('planted'));
				}
				// A new build's install: its cache is made after the planted ones.
				await caches.delete(name);
				await (await navigator.serviceWorker.getRegistration()).unregister();
				return { name, chunk, shell, code };
			});

			await reload();
			await until(
				() => page.evaluate(async () => (await navigator.serviceWorker.ready).active?.state),
				(state) => state === 'activated'
			);
			assert.deepEqual((await page.evaluate(() => caches.keys())).sort(), ['datum/v/0.0.0-kept', build.name].sort());

			// A path no cache holds, so the shell is what answers it, and `/`
			// offline below, which the planted caches hold.
			const away = new URL('/slow', page.url()).href;
			await page.context().route(away, () => new Promise(() => {}));
			const slow = await page.goto(away, { waitUntil: 'commit' });
			assert.equal(await slow.text(), build.shell, 'a slow navigation');
			await page.context().unroute(away);

			const root = new URL('/', page.url()).href;

			await page.context().setOffline(true);
			try {
				const offline = await page.goto(root, { waitUntil: 'commit' });
				assert.equal(await offline.text(), build.shell, 'an offline navigation');
				const asked = await page.evaluate(async (chunk) => ({
					chunk: await (await fetch(chunk)).text(),
					elsewhere: (await fetch('/elsewhere')).status
				}), build.chunk);
				assert.deepEqual(asked, { chunk: build.code, elsewhere: 503 });
			} finally {
				await page.context().setOffline(false);
			}
		});
	});
});
