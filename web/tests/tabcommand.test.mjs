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
import { DESKTOP, PHONE } from './support/layout.mjs';
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

// The › list there (da-zx826). vividn, 2026-11-09: «show setups own options or
// other pages you can go to. like db and sync and stuff could still be on there
// because they switch to their respective tabs». So nothing on it runs as
// `setup <another command>`; picking a page goes to its tab.
test('the › list on the setup tab: its own flags and the pages, nothing that would run after the pill', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const pill = page.locator(`${BAR} .fixed`);
		const box = page.locator(`${BAR} input`);
		const chevron = page.locator(`${BAR} button.prompt`);
		const list = page.locator(`${BAR} #bar-commands`);
		const shown = () =>
			list.locator('button[data-command], button[data-flag], button[data-menu]').evaluateAll((bs) => bs.map((b) => b.dataset.command ?? (b.dataset.flag && `flag ${b.dataset.flag}`) ?? `menu ${b.dataset.menu}`));
		const open = async () => {
			if (!(await list.count())) await chevron.click();
			await list.locator('[data-command="undo"]').waitFor();
		};

		await app.toCollect();
		await open();
		const base = await shown();
		assert.ok(base.includes('menu write') && base.includes('db'), `collect's list is the base row: ${base}`);
		await chevron.click();
		await list.waitFor({ state: 'detached' });

		await page.evaluate(() => window.datum.shell.open('setup'));
		await pill.waitFor();
		await open();
		const here = await shown();
		const flags = here.filter((c) => c.startsWith('flag setup -'));
		assert.ok(flags.length > 0, `setup's flags are listed: ${here}`);
		for (const place of ['sync', 'conflicts', 'db backup', 'db restore']) assert.ok(here.includes(place), `${place} is listed: ${here}`);
		const after = here.filter((c) => !c.startsWith('flag setup -') && !['undo', 'redo', 'help', 'sync', 'conflicts', 'db backup', 'db restore'].includes(c));
		assert.deepEqual(after, [], 'nothing listed would run after the pill');
		assert.equal(await list.locator('button.lead').count(), 0, 'no lead circle');

		// A flag goes after the pill, not as a second `setup`.
		await list.locator(`[data-flag="${flags[0].slice('flag '.length)}"]`).click();
		await page.waitForFunction((flag) => document.querySelector('.prompt-bar input').value === `${flag} `, flags[0].split(' ')[2]);

		// A page goes to its tab, and there the list is that tab's.
		await box.fill('');
		await open();
		await list.locator('[data-command="sync"]').click();
		await page.waitForFunction(() => document.querySelector('.prompt-bar .fixed')?.textContent.trim() === 'sync');
		await open();
		const sync = await shown();
		assert.ok(!sync.includes('sync'), `the sync tab does not list itself: ${sync}`);
		assert.ok(sync.some((c) => c.startsWith('sync ')), `sync's own subcommands are listed: ${sync}`);
	}, PHONE);
});
