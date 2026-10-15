// The offer of a new default (da-cr0w2).
//
// vividn, 2026-10-08: «Do we have a way to trigger a prompt to reset to
// default views? I think we talked about that at one point, but I've never
// seen it appear. Let's get that working. That way users can decide».
//
// So a device that started on an earlier default is asked ONCE per new one
// — what changed, then «Use the new layout», which is «Reset default
// layouts», or «Keep mine» — and either answer is remembered. A device that
// has never been here starts on the newest and is never asked.
//
// The suite ships its OWN new defaults (`window.__datumNewDefaults`, as
// support/desk.mjs pins a tree): datum's list is whatever it is today, and
// «a newer default than the one this device started on» has to be true
// whatever that is.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const USE = 'Use the new layout';
const KEEP = 'Keep mine';
const FIRST = 'A first visit opens on a welcome tab.';
const SECOND = 'The day view moved to the bottom.';

/** Where a device keeps how many new defaults it has seen (layout/draft.ts). */
const SEEN = 'datum.layout.defaults.v1';

/**
 * The tabs a first visit opens, by how many new defaults datum had shipped
 * when it opened them.
 *
 * ⚑ WHEN THE LAST TEST FAILS the default moved. Append the sentence that
 * says what changed to `NEW_DEFAULTS` (layout/defaults.ts) — that is what
 * offers it to everybody already here — and its tabs to this list.
 */
const FIRST_VISIT = [['main'], ['welcome', 'setup', 'collect'], ['welcome', 'setup', 'collect']];

/** From the next load on, datum has shipped `news`. */
const ship = (context, news) => context.addInitScript((news) => (window.__datumNewDefaults = news), news);

/**
 * A device that has been here before `news` shipped: its open list is
 * written (`home`) — `tail` alone, so a reset is something to see — and it
 * has never been offered anything.
 */
const returning = (news) => ({ ...DESKTOP, home: 'tail', prepare: (context) => ship(context, news) });

const rootNames = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

const seen = (page) => page.evaluate((key) => localStorage.getItem(key), SEEN);

/**
 * The offer, if this load made one. The app is `ready`, so the boot is over
 * — and the boot is what asks, in the same turn it finishes in — so no
 * dialog now is no dialog at all.
 */
const offer = (page) => page.locator('dialog.confirm');

describe('a new default is offered', () => {
	test('once, saying what changed; «Use the new layout» is the reset, and is remembered', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const text = await offer(page).locator('p').innerText();
			assert.ok(text.includes(`${FIRST} ${SECOND}`), text);
			assert.deepEqual(await offer(page).getByRole('button').allInnerTexts(), [USE, KEEP]);
			assert.deepEqual(await rootNames(page), ['tail']);

			await offer(page).getByRole('button', { name: USE, exact: true }).click();
			await until(() => rootNames(page), (names) => names.join() === FIRST_VISIT.at(-1).join());

			await app.reload();
			assert.equal(await offer(page).count(), 0);
			assert.deepEqual(await rootNames(page), FIRST_VISIT.at(-1));
		}, returning([FIRST, SECOND]));
	});

	for (const [how, answer] of [
		[KEEP, (page) => offer(page).getByRole('button', { name: KEEP, exact: true }).click()],
		['Escape', (page) => page.keyboard.press('Escape')]
	])
		test(`${how} changes nothing, is remembered, and the next default is offered alone`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await offer(page).waitFor();
				await answer(page);
				await offer(page).waitFor({ state: 'detached' });
				assert.deepEqual(await rootNames(page), ['tail']);

				await app.reload();
				assert.equal(await offer(page).count(), 0);
				assert.deepEqual(await rootNames(page), ['tail']);

				await ship(page.context(), [FIRST, SECOND]);
				await app.reload();
				const text = await offer(page).locator('p').innerText();
				assert.ok(text.includes(SECOND) && !text.includes(FIRST), text);
			}, returning([FIRST]));
		});

	test('never to a device that starts on it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal(await offer(page).count(), 0);
			assert.equal(await seen(page), '2');

			await app.reload();
			assert.equal(await offer(page).count(), 0);
		}, { ...DESKTOP, home: null, prepare: (context) => ship(context, [FIRST, SECOND]) });
	});
});

// A first visit is marked with how many new defaults datum had shipped, so
// this is read against datum's OWN list — nothing shipped by the suite.
test('the tabs a first visit opens are the ones the last new default was announced with', async () => {
	await withApp(async (app) => {
		assert.deepEqual(await rootNames(app.page), FIRST_VISIT[Number(await seen(app.page))]);
	}, { ...DESKTOP, home: null });
});
