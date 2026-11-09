// The tutorial's «On and off» lesson (da-rru2t): a book, a TV and a phone
// that go on and off, several at once, logged by tap, played in the game box
// (da-uzyih.4864, .4866); after the round, its accuracy.
//
// At rest the book is closed and the TV and the phone are off. A round with
// overlapping on-periods is played, all three show as running at once, the
// record's blocks are the taps — each edge at its tap's instant — and
// the box shows one number. The clock is held and stepped past each edge, so
// each tap is logged at the instant the test set, however loaded the runner
// (da-uzyih.4907). The taps are on the collect widget's own buttons
// (da-uzyih.11), the three's alone in the lesson.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { isStatus, settled, lessonTab } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const DEN = ['phone', 'reading', 'tv'];

/** A short round: all three overlap from 2.4 s to 3.6 s. */
const ROUND = { length: 6, on: { reading: [[0.8, 3.6]], tv: [[1.6, 4.8]], phone: [[2.4, 4.2]] } };

/** Each start up to the next end of a field's rows, its TRACKED and IGNORED aside: its blocks, as ISO strings. */
const recorded = (rows) => {
	const blocks = [];
	for (const r of rows.filter((r) => !isStatus(r)).sort((a, b) => Date.parse(a.time) - Date.parse(b.time)))
		if (r.following_state === true) blocks.push({ from: new Date(r.time).toISOString(), to: '' });
		else blocks.at(-1).to = new Date(r.time).toISOString();
	return blocks;
};

test('at rest the book is closed and the TV and phone off; a round of all three overlapping: the record is the taps, and the box shows the accuracy', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.evaluate((round) => {
				const den = window.datum.tutorial.den;
				den.length = round.length;
				for (const thing of den.things) thing.on = round.on[thing.field];
			}, ROUND);
			await lessonTab(page, 'on-off').click();
			const buttons = page.locator('[data-pane$="-fields"] .fields button[data-field]');
			await buttons.nth(2).waitFor();
			assert.deepEqual(
				(await buttons.evaluateAll((bs) => bs.map((b) => b.dataset.field))).sort(),
				DEN,
				"the lesson's buttons are the fields widget's, for its three alone"
			);
			assert.deepEqual(
				await page.locator('.thing').evaluateAll((things) => things.map((t) => [t.dataset.on, t.getAttribute('aria-label')])),
				[
					['false', 'the book, closed'],
					['false', 'the TV, off'],
					['false', 'the phone, off']
				],
				'at rest: the book closed, the TV and the phone off'
			);
			const start = app.clock.at();
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, '[data-pane="on-off-lesson"]');

			// Play: a tap a tenth of a second after each edge, landed before the clock moves on.
			const LATE = 100;
			const edges = Object.entries(ROUND.on)
				.flatMap(([field, spans]) => spans.flatMap(([a, b]) => [{ field, on: true, at: a * 1000 }, { field, on: false, at: b * 1000 }]))
				.sort((x, y) => x.at - y.at);
			let all = null;
			for (const { field, on, at } of edges) {
				await app.clock.set(start + at + LATE);
				await page.locator(`.thing[data-field="${field}"][data-on="${on}"]`).waitFor();
				await page.locator(`[data-pane$="-fields"] .fields button[data-field="${field}"]`).click();
				await page.locator(`[data-pane$="-fields"] .fields button[data-field="${field}"]${on ? '.on' : ':not(.on)'}`).waitFor();
				if (field === 'phone' && on) {
					const pressed = await page.$$eval('[data-pane$="-fields"] .fields button[data-field].on', (on) => on.map((b) => b.dataset.field));
					const active = JSON.parse((await app.run(['now', '--json'])).stdout).active.map((a) => a.field);
					all = { pressed, active };
				}
			}
			assert.deepEqual(all?.pressed.sort(), DEN, 'all three show as logged on');
			assert.deepEqual(all?.active.sort(), DEN, 'all three run at once in the record');
			await app.clock.set(start + ROUND.length * 1000 + LATE);
			const result = page.locator('.game .result');
			await result.waitFor();

			// The record's blocks are the taps, each at the instant it was made.
			const iso = (ms) => new Date(start + ms + LATE).toISOString();
			for (const field of DEN)
				assert.deepEqual(
					recorded(JSON.parse((await app.run(['tail', field, '--json'])).stdout).rows),
					ROUND.on[field].map(([a, b]) => ({ from: iso(a * 1000), to: iso(b * 1000) })),
					`${field}'s blocks are its taps`
				);
			// One number: the share of the round each thing's record agreed with it, a tenth of a second off at each of its two edges.
			assert.equal(await result.locator('dt').allTextContents().then((t) => t.join()), 'accuracy');
			assert.equal(await result.locator('dd').innerText(), `${Math.round((1 - (2 * LATE) / (ROUND.length * 1000)) * 100)}%`);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// «End now» scores the time played, and Start then reads Reset. A game
// ended with all three on leaves them off, every entry still there: the
// IGNORED each that ends the game ends each (da-uzyih.4882). Reset then
// brings the neutral scene back and writes nothing (da-uzyih.4910).
test('«End now» shows the accuracy; a game over leaves all three off, and Reset the scene at rest, with nothing deleted', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const tap = (field) => page.locator(`[data-pane$="-fields"] .fields button[data-field="${field}"]`).click();
			const hids = async () =>
				(await Promise.all(DEN.map((f) => app.run(['tail', f, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows.map((row) => row.hid))))).flat();
			const active = () => app.run(['now', '--json']).then((r) => JSON.parse(r.stdout).active.map((a) => a.field).sort());
			await lessonTab(page, 'on-off').click();
			const game = page.locator('[data-pane="on-off-lesson"] .game');

			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await tap('tv');
			await until(active, (on) => on.includes('tv'));
			await page.getByRole('button', { name: 'End now', exact: true }).click();
			assert.match(await game.locator('.result').innerText(), /^accuracy\s+\d+%$/, 'one number, the accuracy');
			await page.getByRole('button', { name: 'Reset', exact: true }).click();
			await settled(page, '[data-pane="on-off-lesson"]');

			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, '[data-pane="on-off-lesson"]');
			for (const field of DEN) await tap(field);
			await until(active, (on) => on.length === 3);
			assert.deepEqual(await active(), DEN, 'all three on');
			const before = await hids();

			await page.getByRole('button', { name: 'End now', exact: true }).click();
			await page.locator('[data-pane="on-off-lesson"] .game[data-phase="done"][aria-busy="false"]').waitFor();
			assert.deepEqual(await active(), [], 'all three off');
			const after = await hids();
			assert.ok(before.every((h) => after.includes(h)), 'nothing deleted');
			assert.equal(after.length, before.length + 3, 'an IGNORED each');

			await page.getByRole('button', { name: 'Reset', exact: true }).click();
			await game.and(page.locator('[aria-busy="true"]')).waitFor({ state: 'detached' });
			assert.equal(await game.getAttribute('data-phase'), 'neutral');
			assert.deepEqual(await page.locator('.thing').evaluateAll((ts) => ts.map((t) => t.dataset.on)), ['false', 'false', 'false'], 'the scene at rest');
			assert.deepEqual(await hids(), after, 'Reset writes nothing more');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial', clock: null }
	);
});

// vremya, 2026-10-27, on a 390 px phone: the moment lesson's third button was
// cut mid-word, the row scrolling sideways. Here every field's button is
// wholly inside the widget's visible box, with nothing to scroll to.
test('at a 390 px phone width every field of the game is wholly in view, with no sideways scrolling', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await lessonTab(page, 'on-off').click();
			// Attached, not visible: one cut off is the assertion's to say.
			await page.locator('[data-pane$="-fields"] .fields button[data-field]').nth(2).waitFor({ state: 'attached' });
			const seen = await page.locator('[data-pane="on-off-fields"] .fields .bar').evaluate((bar) => {
				const box = bar.getBoundingClientRect();
				const inside = [...bar.querySelectorAll('button[data-field]')].map((b) => {
					const r = b.getBoundingClientRect();
					return [b.dataset.field, r.left >= box.left - 0.5 && r.right <= box.right + 0.5 && r.top >= box.top - 0.5 && r.bottom <= box.bottom + 0.5];
				});
				return { width: innerWidth, inside: inside.sort(), scrolls: bar.scrollWidth > bar.clientWidth };
			});
			assert.equal(seen.width, 390);
			assert.deepEqual(seen.inside, DEN.map((f) => [f, true]), 'each button inside the widget');
			assert.equal(seen.scrolls, false, 'nothing off to the side');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});
