// A purge clears the CLI widget's scrollback wherever it came from (da-pvoai):
// ordered by another member and applied as the page syncs, or run on the page.
// What the commands before it printed holds the purged data, and so does what
// was typed; what is left is one line saying what happened.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
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

/** What a tab's CLI widget holds of its database, as stored and as drawn. */
const held = (app) =>
	app.page.evaluate(() => {
		dispatchEvent(new Event('pagehide'));
		return JSON.parse(localStorage.getItem('datum.repl.v1') ?? '{"scrollback":[],"history":[]}');
	});

describe('a purge clears the CLI widget’s scrollback', () => {
	test('ordered by another member: every tab keeps only the notice, which names the backup (da-pvoai)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					assert.equal((await pairWith(app, native)).verified, true);
					await app.submit('occur coffee');
					await app.submit('tail coffee');
					const other = await app.newTab();
					assert.match((await app.outputs()).join('\n'), /coffee/);

					assert.equal((await native.run(['sync', 'chain', 'purge', '--data', '--yes'])).code, 0);
					// It pushes to the page as it starts, as a member's daemon does.
					native.spawn(['sync', 'run']);
					await until(() => countOf(app, 'coffee'), (n) => n === 0);

					for (const tab of [app, other]) {
						await until(() => tab.echoes(), (e) => e.length === 1 && e[0] === 'sync chain purge');
						const said = (await tab.outputs()).join('\n');
						assert.match(said, /purged by .*\.wiped-.*\.jsonl/);
						assert.doesNotMatch(said, /coffee/);
					}
					const kept = await held(app);
					assert.deepEqual(kept.scrollback.map((c) => c.line), ['sync chain purge']);
					assert.deepEqual(kept.history, [], 'what was typed outlived the purge');

					// Once: a command after it stays, and the notice heard again on a load clears nothing, its recall neither.
					await app.submit('occur tea');
					await app.reload();
					await until(() => app.echoes(), (e) => e.length === 2);
					const again = await held(app);
					assert.deepEqual(again.scrollback.map((c) => c.line), ['sync chain purge', 'occur tea']);
					assert.deepEqual(again.history, ['occur tea']);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});

	test('run on the page: `sync chain purge --data` leaves only itself (da-pvoai)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					assert.equal((await pairWith(app, native)).verified, true);
					await app.submit('occur coffee');
					await app.submit('tail coffee');
					// Declined at its question it exits 0 too, and purged nothing: nothing is cleared.
					const declined = app.submit('sync chain purge --data');
					await app.page.locator('dialog[open]').waitFor();
					await app.page.keyboard.press('Escape');
					await declined;
					assert.equal(await countOf(app, 'coffee'), 1);
					assert.equal((await app.echoes()).length, 3);
					await app.submit('sync chain purge --data --yes');
					assert.equal(await countOf(app, 'coffee'), 0);
					await until(() => app.echoes(), (e) => e.length === 1);
					assert.deepEqual(await app.echoes(), ['sync chain purge --data --yes']);
					assert.deepEqual((await held(app)).history, ['sync chain purge --data --yes']);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});
});
