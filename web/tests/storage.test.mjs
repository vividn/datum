// What the app does when durable storage never answers.
//
// `db.worker.ts` has always had an in-memory fallback and the banner has
// always had a sentence for it, but both were reachable only from a
// `catch` — and a browser that will not give the worker a sync access
// handle does not always reject. Observed in headless chromium: the OPFS
// open simply never settled, nothing threw, and the page sat at
// "opening database…" forever with every pane on its `…` placeholder
// (da-mskb).
//
// So the test is not that the page loads. It is that the app gives up on
// durable storage, says out loud that entries are in memory, and still
// works — the honest state the code already knew how to display.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

/**
 * Run `prelude` inside the database worker, before the app's own module.
 *
 * Nothing here is a mock of the database: OPFS is real and present and
 * `installOpfsSAHPoolVfs` runs its real course — the prelude only decides
 * what the platform hands it. The app's own worker module is then imported
 * for real; the wrapper has to hold the messages the page posts while that
 * import is in flight, since the module registers its listener at the end
 * of it.
 */
function inWorker(prelude) {
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

/** A sync access handle that is never granted and never refused. */
const NEVER = `FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});`;

/**
 * A sync access handle refused until `until` (epoch ms), then real.
 *
 * An absolute instant rather than a delay, because the prelude runs afresh
 * on every load of the page and a delay would restart the outage under a
 * reload — which is the read the test is making.
 */
const notBefore = (until) => `
	const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
	FileSystemFileHandle.prototype.createSyncAccessHandle = function (...args) {
		if (Date.now() < ${until}) return Promise.reject(new DOMException('not yet', 'InvalidStateError'));
		return real.apply(this, args);
	};
`;

/** Poll `read` until `ok` accepts what it says, or give up loudly. */
async function until(read, ok, timeout = 30000) {
	const deadline = Date.now() + timeout;
	for (;;) {
		const seen = await read();
		if (ok(seen)) return seen;
		if (Date.now() > deadline) throw new Error(`timed out waiting; last saw: ${seen}`);
		await new Promise((resolve) => setTimeout(resolve, 100));
	}
}

before(startApp);
after(stopApp);

describe('when durable storage never answers', () => {
	test('the app opens in memory and says so', async () => {
		await withApp(
			async ({ run, entries, bannerText }) => {
				// Reaching here at all is the fix: `withApp` waits for the
				// REPL to enable and every pane to answer, which the app
				// does only once `initDb()` resolves. Before the deadline
				// that wait never ended.
				assert.match(await bannerText(), /held in memory only/);

				const logged = await run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.deepEqual(
					(await entries()).map((e) => e.field),
					['coffee']
				);
			},
			{ prepare: inWorker(NEVER) }
		);
	});
});

// The recovery the banner promises, driven rather than read. This is the
// test that would have caught both defects the first cut of the fallback
// shipped with a green suite: a retry that could not retry (sqlite-wasm
// caches the install's rejection) and a copy that could not copy (generated
// columns refuse INSERT). Neither shows unless the handle is really refused
// and then really granted.
describe('when durable storage arrives late', () => {
	test('the tab takes it over on its own, and keeps what was typed meanwhile', async () => {
		// Absolute, so the outage does not restart under the reload below.
		const heals = Date.now() + 5000;
		await withApp(
			async (app) => {
				assert.match(await app.bannerText(), /held in memory only/);
				const typed = await app.run(['occur', 'coffee', '-t', '0700']);
				assert.equal(typed.code, 0, typed.stderr);

				// Bounded: the worker retries with backoff, so a healthy build
				// heals within a few seconds. Never healing is the defect this
				// exists for.
				await app.page.waitForFunction(() => !document.querySelector('section.pane.alarm'), null, {
					timeout: 30000
				});

				// Through a reload, which is the read only durable storage
				// answers: the entry typed while in memory was moved across.
				await app.reload();
				assert.deepEqual(
					(await app.entries()).map((e) => e.field),
					['coffee']
				);
			},
			{ prepare: inWorker(notBefore(heals)) }
		);
	});
});

// Two tabs of datum.day, and the invariant local-first exists for: both
// write, both stay in sync, nothing typed in either is ever lost (da-eovo).
//
// What used to happen instead: the SAH pool is single-writer, so the second
// tab got no database, opened in memory, showed an orange frame and saw
// none of the first tab's rows. Now one worker owns the database and the
// others borrow it over a BroadcastChannel — so the second tab is exactly
// as durable as the first, and there is no second writer to conflict with.
//
// These have to be two pages in ONE browser context. Two contexts get
// separate storage, so the second tab would open OPFS happily and every
// test here would pass against a build that never learned to share.
describe('two tabs', () => {
	test('both are durable, and each sees the other write without a reload', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();

			// The second tab is not a warning any more.
			assert.doesNotMatch(await second.bannerText(), /memory only/);
			assert.equal(
				await second.page.locator('section.pane.alarm').count(),
				0,
				'the second tab is durable, so its repl frame is not alarmed'
			);

			// Through the REPL, because that is the path a person takes and
			// the one that tells the other tabs anything happened.
			await first.submit('occur coffee -t 0700');
			await second.submit('occur tea -t 0800');

			// Both rows in both tails, oldest first, with nobody reloading.
			for (const tab of [first, second]) {
				const text = await until(
					() => tab.paneText('tail'),
					(t) => t.includes('coffee') && t.includes('tea')
				);
				assert.ok(text.indexOf('coffee') < text.indexOf('tea'), text);
			}
		});
	});

	test('both keep everything through a reload', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			await first.submit('occur coffee -t 0700');
			await second.submit('occur tea -t 0800');

			// A reload hands the database on and takes it back: the reloading
			// tab's worker dies, so its lock frees and the other tab opens the
			// pool. Durability has to survive that, twice.
			await first.reload();
			await second.reload();
			for (const tab of [first, second]) {
				assert.deepEqual(
					(await tab.entries()).map((e) => e.field).sort(),
					['coffee', 'tea']
				);
			}
		});
	});

	test('twenty alternating writes all land, once each', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const tabs = [first, second];
			// Fired without awaiting, so both tabs' statements are in the
			// relay at once — the queue, not a taking-turns discipline, is
			// what has to keep them apart.
			const logged = await Promise.all(
				Array.from({ length: 20 }, (_, i) =>
					tabs[i % 2].run(['occur', 'ping', '-t', `07${String(i).padStart(2, '0')}`])
				)
			);
			logged.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));

			// Counted by hid, not by "more than before": a delta-blind probe
			// cannot tell twenty rows from ten written twice.
			const third = await second.newTab();
			for (const tab of [first, second, third]) {
				const rows = await tab.entries(50);
				assert.equal(rows.length, 20);
				assert.equal(new Set(rows.map((e) => e.hid)).size, 20);
			}
		});
	});

	// Both tabs logging the same field in the same minute is the one case
	// where one connection can refuse a write, and it must refuse it the way
	// the CLI does: the slug is UNIQUE, so the second is a benign duplicate
	// (exit 11) of a row that is already there — never a row that went
	// missing. Two tabs must not make this any worse than one tab does.
	test('the same field in the same minute is a duplicate, not a loss', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const both = await Promise.all([
				first.run(['occur', 'coffee', '-t', '0700']),
				second.run(['occur', 'coffee', '-t', '0700'])
			]);
			assert.deepEqual(
				both.map((r) => r.code).sort(),
				[0, 11],
				both.map((r) => `${r.code}: ${r.stderr}`).join(' | ')
			);
			for (const tab of [first, second]) {
				assert.deepEqual(
					(await tab.entries()).map((e) => e.field),
					['coffee']
				);
			}
		});
	});

	test('closing the owning tab leaves the other working, and a new tab sees it all', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			await second.submit('occur coffee -t 0700');

			// The first tab won the lock, so it is the one holding the
			// database. Take it away.
			await first.page.close();

			const typed = await second.run(['occur', 'tea', '-t', '0800']);
			assert.equal(typed.code, 0, typed.stderr);

			// The handoff is not instant — the closing tab's sync access
			// handles are released on its own schedule — so wait for the tab
			// to say it is durable before asking a question only durable
			// storage can answer.
			await second.page.waitForFunction(() => !document.querySelector('section.pane.alarm'), null, {
				timeout: 30000
			});
			await second.reload();
			assert.deepEqual(
				(await second.entries()).map((e) => e.field).sort(),
				['coffee', 'tea']
			);

			const third = await second.newTab();
			assert.doesNotMatch(await third.bannerText(), /memory only/);
			assert.deepEqual(
				(await third.entries()).map((e) => e.field).sort(),
				['coffee', 'tea']
			);
		});
	});
});
