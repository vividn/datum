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
 * Make the OPFS sync access handle never arrive, inside the database worker.
 *
 * Nothing here is a mock of the database: OPFS is real and present, and
 * `installOpfsSAHPoolVfs` runs its real course — it just awaits a handle
 * that never comes, which is the exact shape the deadline exists for. The
 * app's own worker module is then imported for real; the prelude only
 * has to hold the messages the page posts while that import is in flight,
 * since the module registers its listener at the end of it.
 */
function stallOpfs(context) {
	return context.addInitScript(() => {
		const Real = Worker;
		window.Worker = class extends Real {
			constructor(url, options) {
				const target = new URL(url, location.href).href;
				const source = `
					FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});
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
	});
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
			{ prepare: stallOpfs }
		);
	});
});

// What the app does when the database is already open in another tab.
//
// The OPFS SAH pool is single-writer and one origin is one OPFS, so the
// second tab of datum.day cannot have the database — it falls back to
// memory, safely, and the first tab is never touched. That much was
// already right. What was wrong was the diagnosis: "entries are held in
// memory only" is equally true of a browser with no OPFS at all, and only
// this case has a remedy the reader can act on (da-60ww).
//
// The test has to be two pages in ONE context. Two contexts get separate
// storage, so the second tab would open OPFS happily and the test would
// pass against a build that never learned to tell the cases apart.
describe('when another tab already holds the database', () => {
	test('the second tab names the other tab, and the first keeps its data', async () => {
		await withApp(async (first) => {
			assert.doesNotMatch(await first.bannerText(), /memory only/);
			const logged = await first.run(['occur', 'coffee', '-t', '0700']);
			assert.equal(logged.code, 0, logged.stderr);

			const second = await first.newTab();
			const banner = await second.bannerText();
			// The cause, and the thing to do about it.
			assert.match(banner, /Another datum\.day tab has the database open/);
			assert.match(banner, /close it and reload/);
			// Memory really does mean memory: none of the first tab's data.
			assert.deepEqual(await second.entries(), []);

			// And the first tab is unharmed — through its own reload, which
			// is the read that only durable storage can answer.
			assert.doesNotMatch(await first.bannerText(), /memory only/);
			await first.reload();
			assert.deepEqual(
				(await first.entries()).map((e) => e.field),
				['coffee']
			);
		});
	});
});
