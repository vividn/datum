// A tab's fixed command (da-339ec). vividn, 2026-11-09: «while on the setup
// screen, there should be a special pill that appears, non dismissable at the
// very begining of the input "setup", similarly for some other tabs», and
// «keep the CLI output closed by default on this tab».
//
// So on the setup tab the line runs as `setup <typed>`, the pill outlives
// Backspace and the clear, its output starts closed and the device's fold
// comes back on leaving; a tab with no command is the line it was.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';
import { unfold } from './support/bar.mjs';

before(startApp);
after(stopApp);

const BAR = '.prompt-bar';

test('the setup tab: `setup` is a pill no key takes, a line runs after it, and its output starts closed', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const pill = page.locator(`${BAR} .fixed`);
		const output = page.locator(`${BAR} .section.output`);
		const box = page.locator(`${BAR} input`);

		await app.toCollect();
		await unfold(page);
		assert.equal(await pill.count(), 0, 'collect has no command');

		await page.evaluate(() => window.datum.shell.open('setup'));
		await pill.waitFor();
		assert.equal((await pill.innerText()).trim(), 'setup');
		await output.waitFor({ state: 'detached' });

		await box.fill('coffee');
		await box.press('Enter');
		await page.waitForFunction(() => window.datum.bar.last()?.line === 'setup coffee');
		assert.equal(await page.evaluate(() => window.datum.bar.last().result.code), 0);
		assert.equal(await output.count(), 0, 'the run did not leave the output closed on this tab');
		// A line that says the command already (the sync widget's `sync status ` prefill) is not said twice.
		await box.fill('setup tea');
		await box.press('Enter');
		await page.waitForFunction(() => window.datum.bar.last()?.line !== 'setup coffee');
		assert.equal(await page.evaluate(() => window.datum.bar.last().line), 'setup tea', 'a line that says the command was prefixed again');

		// Backspace in an empty box, and the clear, leave it.
		await box.focus();
		await box.press('Backspace');
		await box.fill('tea');
		await page.locator(`${BAR} button.prompt.clears`).click();
		assert.equal(await box.inputValue(), '');
		assert.equal((await pill.innerText()).trim(), 'setup');
		// The history's line comes back without the command, so it is not said twice.
		await box.press('ArrowUp');
		assert.equal(await box.inputValue(), 'tea');

		// Back on collect: no pill, the output as this device left it, and a line as typed.
		await app.toCollect();
		await output.waitFor();
		assert.equal(await pill.count(), 0);
		await box.fill('moment tea');
		await box.press('Enter');
		await page.waitForFunction(() => window.datum.bar.last()?.line === 'moment tea');
	}, DESKTOP);
});
