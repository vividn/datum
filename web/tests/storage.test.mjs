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
