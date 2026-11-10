// THE OPEN ROOT TAB'S MENU IN NORMAL MODE (da-bfqw4). vividn, 2026-11-10:
// «Choose another... / Modify (enters modification mode, like long
// pressing) / Save (appears onl if modified. if db is in a sync chain, then
// "Save & sync") / Reset (appears only if modified. Otherwise if there are
// previous saved versions, "Revert") / Factory default (only for prebuilt
// layouts and only appears if different) / Reorder <- ->, menu should stay
// open and in the same place so that you can quickly bump over a few / x
// Close», and of Choose another, «replace in place».

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	chooseRoot,
	chooser,
	exactly,
	inLayoutMode,
	leaveLayoutMode,
	manageRoot,
	menuLines,
	rootMenuButton,
	rootSettled
} from './support/layout.mjs';
import { relayed, startRelay } from './support/sync.mjs';
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

const SIZES = [
	['a phone', PHONE],
	['a desktop', DESKTOP]
];

/** The open tab's menu, its lines read once the bar is at rest, and closed again. */
async function lines(page) {
	await rootSettled(page);
	await rootMenuButton(page).click();
	const all = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return all;
}

/** The labels of the root tabs, in order. */
const tabs = (page) => page.locator('.root-bar [role="tab"]').allInnerTexts().then((all) => all.map((t) => t.replace('•', '').trim()));

/** The items marked, as the orange dot's meaning: label only. */
const marked = (page) => page.locator('[role="menu"] [role="menuitem"].marked').allInnerTexts();

const AROUND = ['Save layout as', 'Reorder ← →', '× Close'];

for (const [name, size] of SIZES) {
	describe(name, () => {
		test('each item appears exactly when its rule says', async () => {
			await withApp(async ({ page, errors }) => {
				// Fresh and as it shipped: nothing to save, reset, revert or send back.
				assert.deepEqual(await lines(page), ['Choose another…', 'Modify', ...AROUND]);

				// Modified: Save and Reset, marked, and Factory default, it now differing.
				await manageRoot(page, 'split: a new pane to the left');
				await leaveLayoutMode(page);
				assert.deepEqual(await lines(page), ['Choose another…', 'Modify', 'Save', 'Reset', 'Factory default', ...AROUND]);
				await rootMenuButton(page).click();
				assert.deepEqual(await marked(page), ['Save', 'Reset']);
				await page.keyboard.press('Escape');

				// Saved: unmodified, with a save behind it, so Revert; still not as it shipped.
				await chooseRoot(page, 'Save');
				await until(() => lines(page), (l) => l.includes('Revert'));
				assert.deepEqual(await lines(page), ['Choose another…', 'Modify', 'Revert', 'Factory default', ...AROUND]);

				// Factory default sends it back as a draft, so it is modified against its save.
				await chooseRoot(page, 'Factory default');
				assert.deepEqual(await lines(page), ['Choose another…', 'Modify', 'Save', 'Reset', ...AROUND]);
				// And Reset goes back to the save.
				await chooseRoot(page, 'Reset');
				assert.deepEqual(await lines(page), ['Choose another…', 'Modify', 'Revert', 'Factory default', ...AROUND]);
				assert.deepEqual(errors, []);
			}, size);
		});

		test('Modify enters modify mode, as a long press does', async () => {
			await withApp(async ({ page }) => {
				assert.equal(await inLayoutMode(page), false);
				await chooseRoot(page, 'Modify');
				await page.locator('.frame[data-layout-mode]').waitFor();
			}, size);
		});

		test('Choose another replaces the tab in place', async () => {
			await withApp(
				async ({ page, errors }) => {
					assert.deepEqual(await tabs(page), ['main', 'cli']);
					await chooseRoot(page, 'Choose another…');
					await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
					await chooser(page).waitFor({ state: 'detached' });
					await until(() => tabs(page), (t) => t[0] === 'stats');
					assert.deepEqual(await tabs(page), ['stats', 'cli'], 'not in place');
					assert.equal(await rootMenuButton(page).innerText(), 'stats');
					assert.deepEqual(errors, []);
				},
				{ ...size, home: { names: ['main', 'cli'], active: 0 } }
			);
		});

		test('three presses of → move the tab three places, the menu open and still throughout', async () => {
			await withApp(
				async ({ page, errors }) => {
					await rootSettled(page);
					await rootMenuButton(page).click();
					const menu = page.locator('[role="menu"]');
					const right = menu.getByRole('menuitem', { name: 'move this tab right', exact: true });
					assert.equal(await menu.getByRole('menuitem', { name: 'move this tab left', exact: true }).isDisabled(), true);
					const at = await menu.boundingBox();
					const orders = [];
					for (let i = 0; i < 3; i++) {
						await right.click();
						orders.push(await until(() => tabs(page), (t) => t.indexOf('main') === i + 1));
						assert.equal(await menu.count(), 1, `the menu closed after press ${i + 1}`);
						assert.deepEqual(await menu.boundingBox(), at, `the menu moved after press ${i + 1}`);
					}
					assert.deepEqual(orders, [
						['cli', 'main', 'tail', 'stats'],
						['cli', 'tail', 'main', 'stats'],
						['cli', 'tail', 'stats', 'main']
					]);
					// At the end, → has nowhere to go, and the tab in front is still `main`.
					assert.equal(await right.isDisabled(), true);
					assert.equal(await page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText(), 'main');
					await page.keyboard.press('Escape');
					assert.deepEqual(errors, []);
				},
				{ ...size, home: { names: ['main', 'cli', 'tail', 'stats'], active: 0 } }
			);
		});
	});
}

test('in a database in a sync chain, Save reads «Save & sync»', async () => {
	await withApp(
		async ({ page }) => {
			await manageRoot(page, 'split: a new pane to the left');
			await leaveLayoutMode(page);
			assert.ok((await lines(page)).includes('Save'));
			// An open chain with this copy its first member: a copy that syncs.
			assert.equal((await page.evaluate(() => window.datum.runQuiet(['sync', 'new-invite']))).code, 0);
			await until(() => lines(page), (l) => l.includes('Save & sync'));
			assert.ok(!(await lines(page)).includes('Save'));
		},
		{ ...DESKTOP, prepare: relayed(relay) }
	);
});
