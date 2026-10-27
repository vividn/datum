// A wall time a fall-back night repeats is asked about (da-6i7l1).
//
// vividn, 2026-10-26: «in datum.day it should pop up a dialog box with the
// different options as buttons». 02:30 on 2026-10-25 in Europe/Berlin is
// 00:30Z and 01:30Z: the CLI tells the page both (`DATUM_CHOOSE=`, exit 12)
// and writes nothing, the page asks with a button each, and the line runs
// again with the one pressed.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const FIRST = '-t 2026-10-25T02:30:00+02:00';
const SECOND = '-t 2026-10-25T02:30:00+01:00';

/** Type `line` into the bottom bar and press Enter, without waiting for it to land. */
async function type(page, line) {
	const input = page.locator('.prompt-bar input');
	await input.fill(line);
	await input.press('Enter');
}

test('an hour that happened twice is a dialog with a button each, and the second stores the second', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.declare('tea');
			await type(page, 'occur tea -d 2026-10-25 -t 02:30');
			const dialog = page.locator('dialog.confirm');
			await dialog.waitFor();
			assert.match(await dialog.locator('p').innerText(), /^2026-10-25 02:30 happens twice in Europe\/Berlin/);
			assert.deepEqual(await dialog.getByRole('button').allInnerTexts(), [FIRST, SECOND, 'cancel']);
			assert.deepEqual(await app.entries(), [], 'asked: nothing written');

			const before = await page.locator('.cli .echo').count();
			await dialog.getByRole('button', { name: SECOND, exact: true }).click();
			await page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length > n, before);
			const [tea] = await app.entries();
			assert.equal(tea.slug, 'tea:2026-10-25T01:30:00.000Z');
			assert.equal(tea.occur_time.timezone, 'Europe/Berlin');
			assert.deepEqual(app.errors, []);
		},
		{ timezone: 'Europe/Berlin' }
	);
});

// da-job98: a `*_time` key is asked about as `-t` is, and a line naming two
// is two dialogs: the line runs again with every button pressed so far.
test('two time keys are two dialogs, and each stores the reading pressed', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.declare('tea');
			await type(page, 'occur tea occur_time=2026-10-25T02:30 brew_time=2026-10-25T02:10');
			const dialog = page.locator('dialog.confirm');
			const occur = 'occur_time=2026-10-25T02:30:00+01:00';
			await dialog.getByRole('button', { name: occur, exact: true }).click();
			const brew = 'brew_time=2026-10-25T02:10:00+02:00';
			const before = await page.locator('.cli .echo').count();
			await dialog.getByRole('button', { name: brew, exact: true }).click();
			await page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length > n, before);
			const [tea] = await app.entries();
			assert.equal(tea.slug, 'tea:2026-10-25T01:30:00.000Z');
			assert.equal(tea.data.brew_time.utc, '2026-10-25T00:10:00.000Z');
			assert.deepEqual(app.errors, []);
		},
		{ timezone: 'Europe/Berlin' }
	);
});

// da-7bomd: a stale `_LAST` and then which reading of its new time are two
// questions in turn, and the page re-ran the line with the last answer
// alone: the reading's dialog got EOF for the yes, and the retime's run had
// no yes for the stale question and aborted. Every answer now goes again,
// in order, with the pin of the entry the confirm showed.
test('a stale _LAST retimed into a repeated hour asks both, and stores the reading pressed', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.declare('tea');
			await app.submit('occur tea -d 2026-10-25 -t 04:00');
			await app.clock.forward(16 * 60 * 1000);

			await type(page, 'retime -t 02:30');
			const dialog = page.locator('dialog.confirm');
			const asks = (re) =>
				page.waitForFunction((src) => new RegExp(src).test(document.querySelector('dialog.confirm[open] p')?.textContent ?? ''), re.source);
			await asks(/Retime this entry\? \[Y\/n\]$/);
			await dialog.getByRole('button', { name: 'retime', exact: true }).click();
			await asks(/happens twice in Europe\/Berlin/);
			assert.deepEqual(await dialog.getByRole('button').allInnerTexts(), [FIRST, SECOND, 'cancel']);

			const before = await page.locator('.cli .echo').count();
			await dialog.getByRole('button', { name: SECOND, exact: true }).click();
			await page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length > n, before);
			const [tea] = await app.entries();
			assert.equal(tea.slug, 'tea:2026-10-25T01:30:00.000Z');
			assert.deepEqual(app.errors, []);
		},
		{ timezone: 'Europe/Berlin' }
	);
});
