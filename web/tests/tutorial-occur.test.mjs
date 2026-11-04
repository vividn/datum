// The tutorial's moment lesson (da-nl1co, da-uzyih.4865): a scripted café
// table, a plate on it where things appear as Google's Noto emoji (the fork
// and knife beside it are drawn shapes), a field a kind, one tap to log,
// played in the game box (da-uzyih.4864).
//
// A round with a seeded scene, some things tapped and some missed, and the
// box's one result is what was tapped: the accuracy and the mean reaction
// time, from a thing appearing to its tap.
// The clock is held, so each tap is logged at the instant the test set:
// the timing the box says is the offset the test chose.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { isStatus, settled } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The café's fields, sorted. */
const FIELDS = ['bagel', 'cake', 'coffee', 'croissant', 'tea'];
/** `field`'s entries, its TRACKED and IGNORED aside (da-uzyih.4882). */
const rows = (app, field, db) =>
	app.run(['tail', field, '--json', '-n', '50', ...(db ? ['--db', db] : [])]).then((r) => JSON.parse(r.stdout).rows.filter((row) => !isStatus(row)));
const MOMENT = '[data-pane="moment-lesson"]';
const fieldButton = (field) => `.tutorial .fields button[data-field="${field}"]`;
/** The box's result, name to value. */
const result = async (page) => {
	await page.locator('.game .result').waitFor();
	return Object.fromEntries(
		await page.locator('.game .result > div').evaluateAll((ds) => ds.map((d) => [d.querySelector('dt').textContent, d.querySelector('dd').textContent]))
	);
};

test('a round of the moment game: each thing appears on the café table and goes, and the box shows the accuracy and the mean reaction time', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			const { items, duration, STAND_MS, GOING_MS, kinds } = await page.evaluate(() => {
				const { cafe } = window.datum.tutorial;
				cafe.round.seed = 7;
				return { ...cafe.scene(7), STAND_MS: cafe.STAND_MS, GOING_MS: cafe.GOING_MS, kinds: cafe.KINDS };
			});
			const fields = [...new Set(items.map((item) => item.field))];
			assert.deepEqual([...fields].sort(), FIELDS, `seed 7 sets every kind on the table: ${fields}`);

			// At rest, the neutral scene: the table, the plate, nothing on it.
			const thing = page.locator('.game svg.cafe .thing');
			await page.locator('.game svg.cafe .plate').first().waitFor();
			assert.equal(await thing.count(), 0, 'the plate is empty before Start');

			// Every fourth thing missed; the rest tapped a little after it appears.
			const offsets = [300, 700, 450];
			const tapped = (i) => i % 4 !== 3;
			// And one tap with nothing of its kind near: half a second before the end.
			const lone = fields.find((f) => f !== items[items.length - 1].field);

			const start = app.clock.at();
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, MOMENT);
			const logged = {};
			const tap = async (field, at) => {
				await app.clock.set(start + at);
				await page.locator(fieldButton(field)).click();
				logged[field] = (logged[field] ?? 0) + 1;
				// Landed before the clock moves on, so it is stamped where the test set it.
				await until(
					() => page.evaluate((f) => window.datum.runQuiet(['tail', f, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows.filter((row) => row.state !== 'TRACKED' && row.state !== 'IGNORED').length), field),
					(n) => n === logged[field]
				);
			};
			for (const [i, item] of items.entries()) {
				await app.clock.set(start + item.at + 100);
				await page.locator(`.game svg.cafe .thing[data-field="${item.field}"]`).waitFor();
				assert.equal(await thing.count(), 1, 'one thing at a time');
				// Drawn as its kind's Noto emoji, vendored, the same picture on every device (da-uzyih.4873).
				const href = await thing.locator('image').getAttribute('href');
				const { glyph } = kinds.find((k) => k.field === item.field);
				assert.equal(href, `/noto-emoji/emoji_u${glyph.codePointAt(0).toString(16)}.svg`);
				assert.equal(await page.evaluate((h) => fetch(h).then((r) => r.status), href), 200, `${href} is served`);
				if (tapped(i)) await tap(item.field, item.at + offsets[i % 3]);
				// Eaten or drunk, then gone.
				await app.clock.set(start + item.at + STAND_MS + GOING_MS + 50);
				await thing.waitFor({ state: 'detached' });
			}
			await tap(lone, duration - 500);
			await app.clock.set(start + duration + 100);

			// Accuracy: the things logged over the things that appeared and the taps that matched none.
			const hits = items.map((_, i) => i).filter(tapped);
			const mean = hits.reduce((s, i) => s + offsets[i % 3], 0) / hits.length;
			assert.deepEqual(await result(page), {
				accuracy: `${Math.round((hits.length / (items.length + 1)) * 100)}%`,
				'mean reaction time': `${(mean / 1000).toFixed(1)} s`
			});
			assert.equal(await page.locator('.lesson ul.summary, .lesson .timeline').count(), 0, 'no summary or timeline beside it');

			// The record is the database's: each tap a moment entry at its instant, in the tutorial alone.
			const plan = [...hits.map((i) => ({ field: items[i].field, at: items[i].at + offsets[i % 3] })), { field: lone, at: duration - 500 }];
			for (const field of fields) {
				const times = (await rows(app, field)).map((r) => Date.parse(r.time) - start);
				assert.deepEqual(
					[...times].sort((a, b) => a - b),
					plan.filter((p) => p.field === field).map((p) => p.at),
					`${field}'s entries are the taps`
				);
				assert.deepEqual(await rows(app, field, 'datum'), [], `no ${field} in datum`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// vremya, on #1231's preview: the third café field was cut mid-word on a
// 390 px phone, the row scrolling sideways. Every field's button is whole
// inside the widget's visible box, the rows wrapped, nothing to scroll.
test('on a phone every café field is in view, nothing scrolled sideways', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			for (const field of FIELDS) await page.locator(fieldButton(field)).waitFor();
			const { box, scroll, buttons } = await page.locator('.tutorial .lesson .controls .bar').evaluate((bar) => ({
				box: bar.getBoundingClientRect().toJSON(),
				scroll: bar.scrollWidth - bar.clientWidth,
				buttons: [...bar.querySelectorAll('button[data-field]')].map((b) => ({ field: b.dataset.field, ...b.getBoundingClientRect().toJSON() }))
			}));
			assert.equal(scroll, 0, 'nothing to scroll sideways');
			assert.deepEqual(buttons.map((b) => b.field).sort(), FIELDS);
			for (const b of buttons)
				assert.ok(
					b.left >= box.left - 0.5 && b.right <= box.right + 0.5 && b.top >= box.top - 0.5 && b.bottom <= box.bottom + 0.5,
					`${b.field} at ${JSON.stringify(b)} is whole inside ${JSON.stringify(box)}`
				);
			assert.ok(new Set(buttons.map((b) => Math.round(b.top))).size > 1, 'wrapped onto more rows');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.12, vividn: «add an undo button for mis taps». The app's own
// `undo`, and the scoring reads the record after it: a mis-tap undone is in
// neither the database nor the result. Ended early with «End now», the
// game is scored on what passed before it: the first thing alone.
// A mis-tap is the bar's `undo`, as anywhere: the game has no Undo of its own (da-uzyih.4910).
test('a mis-tap undone in the bar is gone from the record, and «End now» scores what passed', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			const { items } = await page.evaluate(() => {
				window.datum.tutorial.cafe.round.seed = 7;
				return window.datum.tutorial.cafe.scene(7);
			});
			const [first, second] = items;
			const wrong = FIELDS.find((f) => f !== first.field);
			const count = (field) =>
				page.evaluate(
					(f) =>
						window.datum
							.runQuiet(['tail', f, '--json', '-n', '50'])
							.then((r) => JSON.parse(r.stdout).rows.filter((row) => row.state !== 'TRACKED' && row.state !== 'IGNORED').length),
					field
				);
			const landed = (field, n) => until(() => count(field), (k) => k === n);

			const start = app.clock.at();
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, MOMENT);

			await app.clock.set(start + first.at + 300);
			await page.locator(fieldButton(first.field)).click();
			await landed(first.field, 1);
			await page.locator(fieldButton(wrong)).click();
			await landed(wrong, 1);
			// Typed into the bar, as a reader does: the tab has no CLI widget for `submit` to wait on.
			const input = page.locator('.prompt-bar input');
			await input.fill('undo');
			await input.press('Enter');
			const ran = await until(() => page.evaluate(() => window.datum.bar.last()), (last) => last?.line === 'undo');
			assert.equal(ran.result.code, 0, 'the bar ran undo');
			await landed(wrong, 0);
			assert.equal(await count(first.field), 1, 'the tap before it stays');

			// Mid-game the start button reads «End now», the time gone filling it.
			assert.ok(first.at + 600 < second.at, 'the second thing is still to come');
			await app.clock.set(start + first.at + 600);
			const end = page.getByRole('button', { name: 'End now', exact: true });
			await page.waitForFunction(() => parseFloat(document.querySelector('.game .go .spent')?.style.inlineSize) > 0);
			await end.click();
			assert.deepEqual(await result(page), { accuracy: '100%', 'mean reaction time': '0.3 s' });
			assert.ok(await page.getByRole('button', { name: 'Reset', exact: true }).isEnabled(), 'and Start reads Reset');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.17, vividn: «on the tutorial have the glyphs in the nowview for
// the croissants and stuff actually be their emojis». Each kind's glyph in
// the kit is the scene's own (`KINDS`), so the two cannot drift: tapped one
// at a time, each draws in the nowview as its emoji.
test('each thing on the table draws in the nowview as the scene’s emoji', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			const kinds = await page.evaluate(() => window.datum.tutorial.cafe.KINDS);
			assert.deepEqual(kinds.map((k) => k.field).sort(), FIELDS, 'the kit’s café fields are the scene’s');
			const drawn = () => page.locator('[data-pane="moment-now"] text.point-fg').allTextContents();
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, MOMENT);
			await app.clock.forward(1000);
			const want = [];
			for (const { field, glyph } of kinds) {
				await page.locator(fieldButton(field)).click();
				want.push(glyph);
				await page.waitForFunction(
					(n) => document.querySelectorAll('[data-pane="moment-now"] text.point-fg').length === n,
					want.length
				);
				assert.deepEqual((await drawn()).sort(), [...want].sort(), `${field} draws as ${glyph}`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4891, vividn: «Fork on left side of plate, knife on right. Match
// the table lean here, so custom svg for these two», and on its PR: «let's go
// ahead and position those better», and «center the plate and silverware
// horizontally on the placemat move the tea and coffee to the upper right
// corner of the placemat». Drawn as shapes, not an emoji: the plate centered
// on the mat, the fork wholly left of the plate's center and the knife right
// of it; each baked good centered on the plate, its picture ending above the
// plate's near edge, and each cup in the mat's far right quarter, beyond the
// knife's tip. da-uzyih.4896, vividn on #1314: «move the plate and silverware
// a little lower on the placemat. Add some subtle shadows under the plate,
// silverware, and emojis», and then «Remove the shadows under the food, they
// look terrible»: the plate's middle below the mat's, a shadow under the plate
// and each piece, and none under a thing on the table.
test('the plate is centered on the mat, the fork left of it and the knife right; a baked good stands on the plate and a cup in the mat’s far right corner', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			const items = await page.evaluate(() => {
				const { cafe } = window.datum.tutorial;
				cafe.round.seed = 7;
				return cafe.scene(7).items;
			});
			const box = (selector) => page.locator(`.game svg.cafe ${selector}`).first().evaluate((e) => e.getBoundingClientRect().toJSON());
			await page.locator('.game svg.cafe .plate').first().waitFor();
			const [plate, mat, fork, knife] = [await box('.plate'), await box('.mat'), await box('polygon.silverware.fork'), await box('polygon.silverware.knife')];
			const center = plate.left + plate.width / 2;
			assert.ok(Math.abs(mat.left + mat.width / 2 - center) < 1, `the plate ${JSON.stringify(plate)} is centered on the mat ${JSON.stringify(mat)}`);
			assert.ok(fork.right < center, `the fork ${JSON.stringify(fork)} is left of the plate's center ${center}`);
			assert.ok(knife.left > center, `the knife ${JSON.stringify(knife)} is right of the plate's center ${center}`);
			assert.ok(plate.top + plate.height / 2 > mat.top + mat.height / 2, `the plate ${JSON.stringify(plate)} sits low on the mat ${JSON.stringify(mat)}`);
			assert.equal(await page.locator('.game svg.cafe .shadow').count(), 3, 'a shadow under the plate, the fork and the knife');

			const start = app.clock.at();
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, MOMENT);
			const seen = new Set();
			for (const item of items) {
				if (seen.has(item.field)) continue;
				seen.add(item.field);
				await app.clock.set(start + item.at + 100);
				await page.locator(`.game svg.cafe .thing[data-field="${item.field}"]`).waitFor();
				const picture = await box('.thing image');
				const at = `${item.field} ${JSON.stringify(picture)}`;
				assert.equal(await page.locator('.game svg.cafe .shadow').count(), 3, `no shadow under the ${item.field}`);
				if (item.field === 'coffee' || item.field === 'tea') {
					assert.ok(picture.left + picture.width / 2 > center && picture.right <= mat.right, `the ${at} stands right of the plate, on the mat ${JSON.stringify(mat)}`);
					assert.ok(picture.bottom < plate.top + plate.height / 2, `the ${at} stands in the mat's far half, beyond the plate's middle`);
					assert.ok(picture.bottom <= knife.top, `the ${at} stands beyond the knife's tip ${JSON.stringify(knife)}`);
				} else {
					assert.ok(Math.abs(picture.left + picture.width / 2 - center) < 2, `the ${at} is centered on the plate at ${center}`);
					assert.ok(picture.bottom < plate.bottom, `the ${at} ends above the plate's near edge ${plate.bottom}`);
				}
			}
			assert.deepEqual([...seen].sort(), FIELDS, 'every kind placed');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});
