// A kit ships a widget (da-gh37k). vividn, 2026-10-19: «kits should be able to
// install various widgets and layouts. For example the chore kit should have
// a special chorelist widget based off of its special command».
//
// The chores kit's `chorelist` is installed through the seam, as a person
// would (`kit install -`), and held to the bead's three claims, each read
// off the page: its layout draws the widget with the rows `chorelist --json`
// answers, cell for cell and in the rule's colour; a tap on a due chore logs
// it and the row moves on to its next due date; and the widget is offered in
// the database the kit went into, and not in another.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { chooser, leaveLayoutMode, manage } from './support/layout.mjs';

before(startApp);
after(stopApp);

const CHORES = readFileSync(new URL('../../kits/chores.toml', import.meta.url), 'utf8');

/** The kit widgets the tail pane's Change Widget offers, as `kit:<kit> <name>`. */
async function kitOffers(page) {
	await manage(page, 'tail', 'Change Widget');
	await chooser(page).waitFor();
	const offered = await chooser(page)
		.locator('button.widget[data-source^="kit:"]')
		.evaluateAll((els) => els.map((el) => `${el.dataset.source} ${el.querySelector('.name').textContent}`));
	await page.keyboard.press('Escape');
	await chooser(page).waitFor({ state: 'detached' });
	await leaveLayoutMode(page);
	return offered;
}

/** Each drawn row's cells, and the colour it is drawn in. */
const drawn = (page) =>
	page.locator('.kit-widget[data-widget="chorelist"] .row').evaluateAll((rows) =>
		rows.map((row) => ({ cells: [...row.children].map((c) => c.textContent), color: getComputedStyle(row).color }))
	);

test('the chores kit’s chorelist: drawn as the CLI answers, a tap logs the chore, and only its database offers it', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const ok = async (argv, stdin) => {
			const r = await app.run(argv, stdin);
			assert.equal(r.code, 0, `datum ${argv.join(' ')}: ${r.stderr}`);
			return r.stdout;
		};
		const rows = async () => JSON.parse(await ok(['chorelist', '--json']));
		await ok(['kit', 'install', '-', '--yes'], CHORES);
		// A chore done every 7 days, shirked to long ago: overdue, so red.
		await ok(['newchore', 'vacuum', '7']);
		await ok(['occur', 'vacuum', '-T', 'next_date=2000-01-01']);
		const [due] = await rows();
		assert.deepEqual([due.field, due.due, due.status, due.row_color], ['vacuum', '2000-01-01', 'overdue', '#e01b24']);

		// Offered here, in the open database's Change Widget.
		assert.deepEqual(await kitOffers(page), ['kit:chores chore list']);

		// The kit's layout places it; chosen from the root bar's `+`.
		await page.locator('.root-bar button.add').first().click();
		await chooser(page).locator('button.layout[data-source="kit:chores"]').click();
		await chooser(page).waitFor({ state: 'detached' });
		const row = page.locator('.kit-widget[data-widget="chorelist"] button.row', { hasText: 'vacuum' });
		await row.waitFor();
		assert.deepEqual(await drawn(page), [{ cells: due.row_cells, color: 'rgb(224, 27, 36)' }]);

		// A tap logs it: done today, green, and due a week on. A minute on,
		// so its instant is not the one `newchore`'s TRACK entry holds.
		await app.clock.forward(60_000);
		await row.click();
		await page.locator('.kit-widget[data-widget="chorelist"] button.row', { hasText: 'done' }).waitFor();
		const [done] = await rows();
		assert.equal(done.status, 'done');
		assert.notEqual(done.due, due.due, 'the due date moved');
		assert.ok(done.last_occur, 'the tap logged an occurrence');
		assert.deepEqual(await drawn(page), [{ cells: done.row_cells, color: 'rgb(38, 162, 105)' }]);

		// Another database, the same page and menu: nothing of the kit's.
		await page.goto(`${page.url()}?db=other`);
		await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
		assert.deepEqual(await kitOffers(page), []);
		assert.deepEqual(app.errors, []);
	});
});
