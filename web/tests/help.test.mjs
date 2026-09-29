// The Help button (da-vjwid): press it, then a command, a widget or a pill,
// and that thing is explained in a modal instead of acted on (da-35f23);
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

/** The help modal, while it is open. */
const sheet = (page) => page.locator('dialog.sheet[open]:has(pre.help)');

/** Press Help, then `target`, and wait for the modal: its title and its lines. */
async function explain(page, target) {
	const output = () => page.locator('.prompt-bar .roll').allInnerTexts();
	const [echoes, shown] = [await page.locator('.repl .echo').count(), await output()];
	await help(page).click();
	await target.click();
	await sheet(page).waitFor();
	const title = (await sheet(page).locator('.sheet-head span').innerText()).trim();
	const lines = (await sheet(page).locator('pre.help').innerText()).split('\n');
	// Not in the bar's output section, and not in the REPL.
	assert.deepEqual(await output(), shown, 'the explanation went to the output');
	assert.equal(await page.locator('.repl .echo').count(), echoes, 'the explanation was recorded');
	return { title, lines };
}

/** Close the modal with Escape, for the next test. */
async function dismiss(page) {
	await page.keyboard.press('Escape');
	await sheet(page).waitFor({ state: 'detached' });
}

const label = async (page) => (await help(page).locator('.label:not(.hidden)').innerText()).trim();

describe('the Help button', () => {
	test('is second, after Undo, single-outlined; armed it asks "? on what" in the same width; Escape disarms', async () => {
		await onPage(async ({ page }) => {
			const names = (await listed(page.locator('.bar.commands'), 'button[data-command]')).map((b) => b.command);
			assert.deepEqual(names.slice(0, 3), ['undo', 'redo', 'help']);

			// Undo runs at once and wears the double outline; Help waits, and wears one.
			const undo = page.locator('.bar.commands button[data-command="undo"]');
			assert.equal(await undo.getAttribute('data-act'), 'run');
			assert.equal(await help(page).getAttribute('data-act'), 'arm');
			const style = (el) => el.evaluate((b) => getComputedStyle(b).borderTopStyle);
			assert.equal(await style(undo), 'double');
			assert.equal(await style(help(page)), 'solid');

			assert.equal(await label(page), 'Help');
			const before = (await help(page).boundingBox()).width;
			await help(page).click();
			assert.equal(await label(page), '? on what\n(tap: all)');
			assert.equal(await help(page).getAttribute('aria-pressed'), 'true');
			const armed = (await help(page).boundingBox()).width;
			assert.ok(Math.abs(armed - before) <= 3, `the width moved: ${before} -> ${armed}`);
			// And the armed words fit inside it: the box holds by not growing, not by spilling.
			const spill = await help(page).locator('.armed').evaluate((l) => {
				const words = document.createRange();
				words.selectNodeContents(l);
				return words.getBoundingClientRect().width - l.clientWidth;
			});
			assert.ok(spill <= 1, `the armed label spills out of the button by ${spill}px`);
			await page.keyboard.press('Escape');
			assert.equal(await label(page), 'Help');
		});
	});

	test('then a command: the CLI\'s own help for it in a modal, and the command does not run', async () => {
		await onPage(async ({ page }) => {
			const occur = page.locator('.bar.commands button[data-command="occur"]');
			const { title, lines } = await explain(page, occur);
			assert.equal(title, 'datum help occur');
			assert.match(lines[0], /^datum occur — record a point-in-time event/);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '', 'occur filled the line');
			assert.equal(await label(page), 'Help', 'one-shot: it disarms');
			await dismiss(page);

			// The next tap acts as it always did.
			await occur.click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur ');
		});
	});

	// da-803mk: a subcommand is explained by the CLI's help for it, and the
	// Escape that closes the modal leaves the subcommands open.
	test('then a subcommand: `datum help db backup`, and the bar stays in db', async () => {
		await onPage(async ({ page }) => {
			await page.locator('.bar.commands button[data-command="db"]').click();
			const backup = page.locator('.bar.commands button[data-command="db backup"]');
			const { title, lines } = await explain(page, backup);
			assert.equal(title, 'datum help db backup');
			assert.match(lines[0], /^datum db — /);
			assert.ok(lines.some((l) => l.includes('datum db backup <PATH | ->')), lines.join('\n'));
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '', 'backup filled the line');
			await dismiss(page);
			assert.equal(await page.locator('.bar.commands button[data-back]').count(), 1, 'Escape left db too');
		});
	});

	// da-st2xl: a held command's flag is explained by its line of the CLI's
	// flag table, and the tap appends nothing. A right click is the hold.
	test('then a flag of a held command: its help line, and the line is left alone', async () => {
		await onPage(async ({ page }) => {
			await page.locator('.bar.commands button[data-command="occur"]').click({ button: 'right' });
			const t = page.locator('.bar.commands button[data-flag="occur -t"]');
			await t.waitFor();
			const { title, lines } = await explain(page, t);
			assert.equal(title, 'datum occur -t');
			assert.deepEqual(lines, ['Time override (offset, HH:MM, 9:15am, a date, or both)']);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), 'occur ', '-t was appended');
			await dismiss(page);
			assert.equal(await page.locator('.bar.commands button[data-back]').count(), 1, 'Escape left the flags too');
		});
	});

	test('then a widget: its kind\'s help, and a press inside it does nothing else', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur']);
			const coffee = page.locator('section[data-pane="bar"] .bar button.widget', { hasText: 'coffee' }).first();
			const { title, lines } = await explain(page, coffee);
			assert.equal(title, 'fields');
			assert.match(lines[0], /^Every field you log/);
			assert.equal((await app.entries(5)).length, 0, 'the press logged coffee');
			await dismiss(page);
		});
	});

	test('then a pill: what that kind of pill does, with its value; the pill stays', async () => {
		await onPage(async ({ page }) => {
			await page.evaluate(() => window.datum.bar.setPills([{ id: 't', kind: 'time', text: '-t 14:30' }]));
			const pill = page.locator('.prompt-bar [data-pill="t"]');
			const { title, lines } = await explain(page, pill);
			assert.equal(title, '-t 14:30');
			assert.match(lines[0], /^-t 14:30: the time cursor/);
			assert.equal(await pill.count(), 1, 'the tap removed the pill');
			await dismiss(page);
		});
	});

	test('then Help again: general help, `datum help`; the modal closes by ✕, Escape and a tap outside', async () => {
		await onPage(async ({ page }) => {
			const closes = {
				'✕': () => sheet(page).locator('button.close').click(),
				Escape: () => page.keyboard.press('Escape'),
				// The backdrop is the dialog's own box outside its content: a corner of the page.
				outside: () => page.mouse.click(4, 4)
			};
			for (const [how, close] of Object.entries(closes)) {
				const { title, lines } = await explain(page, help(page));
				assert.equal(title, 'datum help');
				assert.match(lines[0], /^datum — personal data tracker/);
				assert.equal(await label(page), 'Help');
				// A tap inside it closes nothing.
				await sheet(page).locator('pre.help').click();
				assert.equal(await sheet(page).count(), 1, `a tap inside closed it (before ${how})`);
				await close();
				await sheet(page).waitFor({ state: 'detached' });
			}
		});
	});
});
