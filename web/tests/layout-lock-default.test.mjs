// A layout that SHIPPED opens locked; one a person made opens unlocked
// (da-gshh).
//
// vividn, 2026-09-17 16:09: «default/loaded layouts should start locked […]
// When a user creates a new layout it should start unlocked.» The lock is
// still the device's (da-2a5y): only its first value changed, so every
// device here is FRESH (`home: null`), which the harness otherwise spares
// the suite by unlocking what shipped.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, rootMenuButton } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const FRESH_DESKTOP = { ...DESKTOP, home: null };
const FRESH_PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: null };

/** Whether the root tab in front is locked. */
const locked = (page) =>
	page.evaluate(() => {
		const root = window.datum.layout.current().root;
		return !!root.tabs[root.active].locked;
	});

/** Everything a lock takes away: the draggable borders, a pane's `+`. */
const controls = (page) => page.locator('[role="separator"], section.pane button.add').count();

/** This device's say on `name`'s lock, as stored. */
const stored = (page, name) => page.evaluate((n) => localStorage.getItem(`datum.layout.locked.v1.${n}`), name);

/** Locked, and nothing about it an edit: no dot, no draft. */
async function lockedClean(page, name) {
	await until(() => locked(page), (l) => l === true);
	assert.equal(await controls(page), 0, `${name} opened with a border or a +`);
	assert.equal(await page.locator('.root-bar [role="tab"].modified').count(), 0, `${name} opened modified`);
	assert.equal(await page.evaluate((n) => localStorage.getItem(`datum.layout.draft.v1.${n}`), name), null);
	assert.equal(await stored(page, name), null, 'a first open wrote a lock down');
}

describe('a layout that shipped', () => {
	for (const [what, device, name] of [
		['a fresh phone: `mobile`', FRESH_PHONE, 'mobile'],
		['a fresh desktop: `day`', FRESH_DESKTOP, 'day']
	])
		test(`${what} opens locked; unlocked it drags, and stays unlocked over a reload`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await lockedClean(page, name);

				await chooseRoot(page, 'Unlock layout');
				await until(() => locked(page), (l) => l === false);
				const border = page.locator('[role="separator"]').first();
				const before = await border.getAttribute('aria-valuenow');
				const box = await border.boundingBox();
				const horizontal = (await border.getAttribute('aria-orientation')) === 'horizontal';
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.mouse.down();
				for (let i = 1; i <= 5; i++)
					await page.mouse.move(box.x + box.width / 2 + (horizontal ? 0 : i * 10), box.y + box.height / 2 + (horizontal ? i * 10 : 0));
				await page.mouse.up();
				await until(() => border.getAttribute('aria-valuenow'), (v) => v !== before);

				await app.reload();
				assert.equal(await locked(page), false, 'a reload locked it again');
				assert.equal(await stored(page, name), '0');
				assert.deepEqual(app.errors, []);
			}, device);
		});

	test('a one-widget built-in (`/nowview`) opens locked too', async () => {
		await withApp(async (app) => {
			await app.goto('/nowview');
			await rootMenuButton(app.page).filter({ hasText: 'nowview' }).waitFor();
			await lockedClean(app.page, 'nowview');
		}, FRESH_DESKTOP);
	});

	test('a device that already had `day` open, with nothing said of its lock, finds it locked', async () => {
		await withApp(
			async ({ page }) => lockedClean(page, 'day'),
			{
				...FRESH_DESKTOP,
				prepare: (context) =>
					context.addInitScript(() => {
						if (!localStorage.getItem('datum.layout.root.v1'))
							localStorage.setItem('datum.layout.root.v1', JSON.stringify({ names: ['day'], active: 0 }));
					})
			}
		);
	});

	test('Reset default layouts locks it again', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await chooseRoot(page, 'Unlock layout');
			await until(() => locked(page), (l) => l === false);
			await page.locator('.root-bar button.gear').click();
			await page.getByRole('menuitem', { name: 'Reset default layouts…', exact: true }).click();
			const dialog = page.locator('dialog.confirm');
			await dialog.getByRole('button', { name: 'reset', exact: true }).click();
			await dialog.waitFor({ state: 'detached' });
			await lockedClean(page, 'day');
			await app.reload();
			await lockedClean(page, 'day');
		}, FRESH_DESKTOP);
	});
});

describe('a layout a person made', () => {
	test('`Save layout as…` opens unlocked, even from a locked built-in', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await until(() => locked(page), (l) => l === true);
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as…');
			await rootMenuButton(page).filter({ hasText: 'mine' }).waitFor();
			await until(() => locked(page), (l) => l === false);
			assert.ok((await controls(page)) > 0);
			await app.reload();
			assert.equal(await locked(page), false);
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	test('a save of their own over `day` is theirs: with nothing said, it opens unlocked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await chooseRoot(page, 'Unlock layout');
			await until(() => locked(page), (l) => l === false);
			await chooseRoot(page, 'Save layout');
			await until(() => page.evaluate(() => window.datum.shell.versions('day').catch(() => [])), (v) => v.length === 1);
			await page.evaluate(() => localStorage.removeItem('datum.layout.locked.v1.day'));
			await app.reload();
			assert.equal(await locked(page), false);
		}, FRESH_DESKTOP);
	});
});
