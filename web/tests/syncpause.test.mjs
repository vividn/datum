// `datum sync pause` in datum.day (da-wlbyx): vividn calls a paused sync
// risky, so the root bar says so in every tab, and a tap on it resumes.
// The database's leader reads the pause with the rest of its sync state and
// broadcasts it; the tabs draw what they hear.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { relayed, startRelay } from './support/sync.mjs';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

const PAUSED = 'nav.root-bar button.paused';

const run = (tab, argv) => tab.page.evaluate((a) => window.datum.runQuiet(a), argv);

test('a paused sync shows in every tab’s root bar, and a tap resumes it', async () => {
	await withApp(
		async (first) => {
			// An open chain with this copy its first member: a copy that syncs.
			assert.equal((await run(first, ['sync', 'new-invite'])).code, 0);
			const second = await first.newTab();
			assert.equal(await second.page.locator(PAUSED).count(), 0, 'not paused, not shown');

			assert.equal((await run(second, ['sync', 'pause'])).code, 0);
			for (const tab of [first, second]) {
				const badge = tab.page.locator(PAUSED);
				await badge.waitFor();
				assert.equal(await badge.textContent(), 'sync paused');
			}

			await second.page.locator(PAUSED).click();
			for (const tab of [first, second]) await tab.page.locator(PAUSED).waitFor({ state: 'detached' });
			const status = JSON.parse((await run(first, ['sync', 'status', '--json'])).stdout);
			assert.equal(status.paused, null);
		},
		{ prepare: relayed(relay) }
	);
});
