// The Help button (da-vjwid): press it, then a command, a widget or a pill,
// and that thing is explained in the bar's output instead of acted on;
// press it again for general help. `$lib/help.ts`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { listed } from './support/rows.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const shared = sharedApp({ ...DESKTOP });

const onPage = (fn) =>
	shared.solo(async (app) => {
		// A test that failed armed must not leave the next one armed.
		await app.page.keyboard.press('Escape');
		return fn(app);
	}, PAGE);

const help = (page) => page.locator('.bar.commands button[data-command="help"]');

/** The bar's output section: the lines of the newest entry it shows. */
const said = (page) => page.locator('.prompt-bar .roll .raw p').allInnerTexts();

/** Press Help, then `target`, and wait for the explanation to be the newest entry. */
async function explain(page, target, first) {
	await help(page).click();
	await target.click();
	await page.waitForFunction(
		(want) => document.querySelector('.prompt-bar .roll .raw p')?.textContent.startsWith(want),
		first
	);
	return said(page);
}

describe('the Help button', () => {
	test('is second, after Undo, and says what to do while armed; Escape disarms', async () => {
		await onPage(async ({ page }) => {
			const names = (await listed(page.locator('.bar.commands'), 'button[data-command]')).map((b) => b.command);
			assert.deepEqual(names.slice(0, 3), ['undo', 'redo', 'help']);
			assert.equal((await help(page).innerText()).trim(), 'Help');
			await help(page).click();
			assert.equal((await help(page).innerText()).trim(), '? tap something');
			assert.equal(await help(page).getAttribute('aria-pressed'), 'true');
			await page.keyboard.press('Escape');
			assert.equal((await help(page).innerText()).trim(), 'Help');
		});
	});

	test('then a command: the CLI\'s own help for it, and the command does not run', async () => {
		await onPage(async (app) => {
			const { page } = app;
			const occur = page.locator('.bar.commands button[data-command="occur"]');
			const lines = await explain(page, occur, 'datum occur');
			assert.match(lines[0], /^datum occur — record a point-in-time event/);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '', 'occur filled the line');
			assert.equal((await app.echoes()).at(-1), 'help occur');
			assert.equal((await help(page).innerText()).trim(), 'Help', 'one-shot: it disarms');

			// The next tap acts as it always did.
			await occur.click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur ');
		});
	});

	test('then a widget: its kind\'s help, and a press inside it does nothing else', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur']);
			const coffee = page.locator('section[data-pane="bar"] .bar button.widget', { hasText: 'coffee' }).first();
			const lines = await explain(page, coffee, 'Every field you log');
			assert.equal(lines.length, 3);
			assert.equal((await app.echoes()).at(-1), '? fields');
			assert.equal((await app.entries(5)).length, 0, 'the press logged coffee');
		});
	});

	test('then a pill: what that kind of pill does, with its value; the pill stays', async () => {
		await onPage(async ({ page }) => {
			await page.evaluate(() => window.datum.bar.setPills([{ id: 't', kind: 'time', text: '-t 14:30' }]));
			const pill = page.locator('.prompt-bar [data-pill="t"]');
			const lines = await explain(page, pill, '-t 14:30:');
			assert.match(lines[0], /^-t 14:30: the time cursor/);
			assert.equal(await pill.count(), 1, 'the tap removed the pill');
		});
	});

	test('then Help again: general help, `datum help`', async () => {
		await onPage(async (app) => {
			const { page } = app;
			const lines = await explain(page, help(page), 'datum — ');
			assert.match(lines[0], /^datum — personal data tracker/);
			assert.equal((await app.echoes()).at(-1), 'help');
			assert.equal((await help(page).innerText()).trim(), 'Help');
		});
	});
});
