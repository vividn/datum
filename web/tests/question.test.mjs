// A question the CLI asks is a dialog with no page code of its own (da-jytnl):
// `DATUM_ASK=` on stderr, exit 12, and the line runs again with the answer.
//
// `sync chain purge --data`, typed, never had a dialog: off a terminal it said
// «nothing changed» and named `--yes`.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

test('a command with no page code asks in a dialog, and the answer takes effect', async () => {
	await withApp(async (app) => { // purges its database
		const { page } = app;
		assert.equal((await app.run(['occur', 'coffee'])).code, 0);
		// A lone member of an open chain: `new-invite` starts one.
		assert.equal((await app.run(['sync', 'new-invite', '--expires', '1d'])).code, 0);
		const logged = (await app.entries()).length;
		const dialog = page.locator('dialog.confirm');
		const input = page.locator('.prompt-bar input');

		await input.fill('sync chain purge --data');
		await input.press('Enter');
		await dialog.waitFor();
		const asked = await dialog.locator('p').innerText();
		assert.ok(asked.includes('every copy'), `the CLI's own words: ${asked}`);
		assert.equal(await dialog.locator('p').getByText('datum', { exact: false }).count(), 0, 'the name to type is said once, by the dialog');
		await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
		await dialog.waitFor({ state: 'detached' });
		assert.equal((await app.entries()).length, logged, 'cancel: nothing changed');

		await input.fill('sync chain purge --data');
		await input.press('Enter');
		await dialog.locator('input').fill('datum');
		await dialog.locator('input').press('Enter');
		await dialog.waitFor({ state: 'detached' });
		// The line runs again once the dialog has gone: its answer lands after.
		const until = Date.now() + 10_000;
		while ((await app.entries()).length && Date.now() < until) await page.waitForTimeout(100);
		assert.deepEqual(await app.entries(), [], 'the name typed: purged');
		assert.deepEqual(app.errors, []);
	});
});
