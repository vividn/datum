// The fields widget's outlines (da-z8z40): a button a tap RUNS is drawn
// in the DOUBLE outline, and one that opens or fills is not — rule 1 of
// the design language, derived from `pressKind` in $lib/commands.ts, the
// same answer the press itself takes. The pure table is in
// `commands-pure.test.mjs`; here is what a reader sees, and what a tap on
// each button then does.
//
// A file of its own (da-lt0k): node's budget is per FILE, and the other
// `fields*.test.mjs` are already near a minute each under load.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { FIXTURE, MOOD, type } from './support/fields.mjs';

/** Every button the bar draws, by what it is, with its top border's style. */
const outlines = (page) =>
	page.locator('.bar').evaluate((el) =>
		Object.fromEntries(
			[...el.querySelectorAll('button')].map((b) => [
				b.dataset.field ?? (b.dataset.state ? `=${b.dataset.state}` : b.textContent.trim()),
				getComputedStyle(b).borderTopStyle
			])
		)
	);

/** How many entries the database holds, by the CLI's own count. */
const entries = async (app) =>
	JSON.parse((await app.run(['count', '--group-by', 'field', '--json'])).stdout).reduce((n, g) => n + g.count, 0);

describe('the outline says what a tap does', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();

	test('a point and a block are double; a switch, a record and the navigation are not', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of [...FIXTURE, ...MOOD]) await type(page, line);
			await page.locator('.bar [data-field="mood"]').waitFor();
			await app.idle();
			assert.deepEqual(await outlines(page), {
				'Track +': 'none',
				coffee: 'double',
				work: 'double',
				untracked: 'double',
				mood: 'solid',
				note: 'none'
			});

			// The state list: every state runs; Back and + do not.
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-back]').waitFor();
			const list = await outlines(page);
			const states = Object.keys(list).filter((k) => k.startsWith('='));
			assert.ok(states.length > 0, 'no state drawn');
			assert.deepEqual(list, {
				'< Back': 'solid',
				...Object.fromEntries(states.map((k) => [k, 'double'])),
				'+ Add state': 'solid'
			});
			await page.locator('.bar [data-back]').click();

			// Under a pending verb the press is that verb: `switch` on a
			// many-stated field still asks which, `add` on a record still fills.
			const input = page.locator('.prompt-bar input');
			await input.fill('switch ');
			await page.waitForFunction(() => document.querySelectorAll('.bar [data-field]').length === 1);
			assert.equal((await outlines(page)).mood, 'solid');
			await input.fill('add ');
			await page.locator('.bar [data-field="note"]').waitFor();
			assert.equal((await outlines(page)).note, 'none');
			await input.fill('occur ');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="note"]'));
			assert.equal((await outlines(page)).coffee, 'double');
		});
	});

	// The behaviour behind the look: tap every field button in turn, and a
	// double outline is exactly the ones that wrote an entry.
	test('a double-outlined tap writes an entry; a single-outlined one does not', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of [...FIXTURE, ...MOOD]) await type(page, line);
			await page.locator('.bar [data-field="mood"]').waitFor();
			await app.idle();

			for (const field of ['coffee', 'work', 'untracked', 'mood', 'note']) {
				const button = page.locator(`.bar [data-field="${field}"]`);
				const double = (await button.evaluate((b) => getComputedStyle(b).borderTopStyle)) === 'double';
				const had = await entries(app);
				const echoed = (await app.echoes()).length;
				await app.clock.forward(1000);
				await button.click();
				if (double) await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoed);
				await app.idle();
				assert.equal((await app.echoes()).length > echoed, double, `${field}: ran`);
				assert.equal((await entries(app)) > had, double, `${field}: double=${double}`);
				// A switch's list, or a record's line, is put back before the next.
				if (field === 'mood') await page.locator('.bar [data-back]').click();
				await page.locator('.prompt-bar input').fill('');
			}
		});
	});
});
