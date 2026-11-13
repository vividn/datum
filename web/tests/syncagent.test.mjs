// «Add agent» in the Sync widget (da-bdyor): the code it makes restores a
// native copy of the agent from nothing, at the level chosen, and the
// agent's device row is marked as one.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { DESKTOP, WIDGET } from './support/syncactions.mjs';
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

test('«Add agent» makes a code a fresh native copy restores from, and marks the agent’s row', async () => {
	await withWallClock(
		async (app) => {
			const native = await nativeCopy(relay);
			try {
				await app.run(['occur', 'coffee']);
				await app.goto('/sync');
				const widget = app.page.locator(WIDGET);
				// Shared once, the database is in a chain, which an agent's code needs.
				await widget.locator('.ways[data-db="datum"]').getByRole('button', { name: 'Share write access' }).click();
				await widget.locator('.offer .qr svg').waitFor();
				await widget.getByRole('button', { name: 'Add agent ▾' }).click();
				const card = widget.locator('.card.agent');
				await card.getByLabel('Name').fill('claude');
				// The chain has no admin, so a full agent cannot write (ístina's F19): the CLI refuses that member.
				const writes = card.getByRole('checkbox', { name: 'it writes too' });
				await card.getByLabel('It gets').selectOption('light');
				assert.equal(await writes.count(), 1);
				await writes.check();
				await card.getByLabel('It gets').selectOption('full');
				assert.equal(await writes.count(), 0);
				await card.getByRole('button', { name: 'Make code' }).click();
				const code = (await widget.locator('.agentcode .ticket').textContent()).trim();
				assert.match(code, /^datum-agent:[A-Z2-7]+$/);

				const restored = await native.run(['sync', 'restore', code]);
				assert.equal(restored.code, 0, restored.stderr);
				assert.match(restored.stdout, /lost with this sandbox$/);
				assert.equal(await native.count('coffee'), 1);
				// A reader: it writes nothing.
				assert.notEqual((await native.run(['occur', 'tea'])).code, 0);

				const row = widget.locator('.grid tbody tr', { hasText: 'claude' });
				await until(() => row.locator('.kind').allInnerTexts(), (k) => k.includes('agent'));
			} finally {
				await native.close();
			}
		},
		{ prepare: relayed(relay), viewport: DESKTOP }
	);
});
