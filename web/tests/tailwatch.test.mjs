// The tail pane as `tail -w` (da-lan0): its height decides how many rows
// it shows, scrolling up pages older rows in, and a write while the
// reader is scrolled up keeps their place and says `↓ new` at the foot.
//
// Every row is the CLI's (`tail --json`); what is asserted here is the
// geometry — the count the pane's box holds, by the pane's own header and
// row heights — and identity, by hid. Never "more rows than before": the
// count is computed from the box and compared to it, and the oldest and
// newest rows are named.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** Short, so that thirty rows are more than the tail holds. */
const SHORT = { viewport: { width: 1280, height: 600 } };
const DESKTOP = { viewport: { width: 1280, height: 900 } };

/** The tail pane's body and table, measured. */
const geometry = (page) =>
	page.evaluate(() => {
		const pane = document.querySelector('section.pane[data-title="tail"]');
		const body = pane.querySelector('.body');
		const style = getComputedStyle(body);
		const box = body.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
		const head = pane.querySelector('thead')?.getBoundingClientRect().height ?? 0;
		const trs = [...pane.querySelectorAll('tbody tr')];
		const row = Math.min(...trs.map((tr) => tr.getBoundingClientRect().height));
		const top = body.getBoundingClientRect().top;
		return {
			box,
			head,
			row,
			n: Math.max(1, Math.floor((box - head) / row)),
			hids: trs.map((tr) => tr.dataset.hid),
			// Each row's position on screen, relative to the body.
			ys: Object.fromEntries(trs.map((tr) => [tr.dataset.hid, tr.getBoundingClientRect().top - top])),
			scrollTop: body.scrollTop,
			atBottom: body.scrollHeight - body.scrollTop - body.clientHeight <= 2
		};
	});

const rowCount = (page) => page.evaluate(() => document.querySelectorAll('section.pane[data-title="tail"] tbody tr').length);

/** Wait until the tail's row count `n` satisfies `expr` (a JS expression in `n` and `arg`). */
const waitRows = (page, expr, arg = 0) =>
	page.waitForFunction(
		([expr, arg]) => {
			const n = document.querySelectorAll('section.pane[data-title="tail"] tbody tr').length;
			return new Function('n', 'arg', `return ${expr}`)(n, arg);
		},
		[expr, arg]
	);

/** Wait until the tail's first row is not `hid`. */
const waitTopNot = (page, hid) =>
	page.waitForFunction(
		(hid) => document.querySelector('section.pane[data-title="tail"] tbody tr')?.dataset.hid !== hid,
		hid
	);

/** The tail pane's `↓ new` marker. */
const marker = (page) => page.locator('section.pane[data-title="tail"] button.new');

/**
 * `count` entries of field `seed`, one a minute ending at 2026-09-10 05:00
 * UTC, restored in one command (`db restore -`) rather than logged one at
 * a time: a page of 300 rows is not a thing to log through the REPL.
 * The hids are the minute's ordinal, so `seed(i)` names the i-th oldest.
 */
async function seed(app, count) {
	const at = (i) => new Date(Date.UTC(2026, 8, 10, 5, 0) - (count - 1 - i) * 60_000).toISOString();
	// The header the CLI itself writes, so the schema version is its own.
	const backup = await app.run(['db', 'backup', '-']);
	assert.equal(backup.code, 0, backup.stderr);
	const header = JSON.parse(backup.stdout.split('\n')[0]);
	const lines = [JSON.stringify({ ...header, backup_time: at(count - 1), source_db: 'seed', entry_count: count })];
	for (let i = 0; i < count; i++) {
		lines.push(
			JSON.stringify({
				hid: seedHid(i),
				slug: `seed:${at(i)}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field: 'seed',
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: null,
				occur_time: { utc: at(i), offset_minutes: 0, timezone: 'UTC' },
				create_time: at(i),
				modify_time: at(i),
				device_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: '%occur_time%',
				raw_input: 'occur seed',
				parser_version: 2
			})
		);
	}
	const result = await app.run(['db', 'restore', '-'], lines.join('\n') + '\n');
	assert.equal(result.code, 0, result.stderr);
	// A run through the seam is not a command the REPL ran, so the panes
	// do not know; a fresh visit shows the listing as it now stands.
	await app.reload();
}

const seedHid = (i) => 'seed' + i.toString().padStart(8, '0');
const seedIndex = (hid) => Number(hid.slice(4));

/** Scroll the tail's body to its top — which is how the page above is asked for. */
const toTop = (page) =>
	page.evaluate(() => {
		document.querySelector('section.pane[data-title="tail"] .body').scrollTop = 0;
	});

describe('rows from height', () => {
	test('a pane shows exactly the rows its box holds, newest last, and a taller one shows more of the past', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, 30);
			await waitRows(page, 'n > 0');
			await app.idle();
			const g1 = await geometry(page);
			assert.ok(g1.n < 30, `the pane holds all thirty rows at 600px tall (n = ${g1.n}), which tests nothing`);
			assert.equal(g1.hids.length, g1.n, `${g1.hids.length} rows in a box of ${g1.box}px that holds ${g1.n}`);
			assert.equal(g1.hids.at(-1), seedHid(29), 'the newest row is not last');
			assert.deepEqual(g1.hids, g1.hids.map((_, i) => seedHid(30 - g1.n + i)), 'the rows are not the newest n');
			// The count is the count that fits: one more row would not.
			assert.ok(g1.head + g1.n * g1.row <= g1.box + 0.5, 'the rows overflow the box');
			assert.ok(g1.head + (g1.n + 1) * g1.row > g1.box, 'a gap: one more row would fit');

			// Taller: the viewport grows, the layout's shares with it, and the
			// box is re-measured — the count follows, and the oldest shown is
			// older than before.
			await page.setViewportSize({ width: 1280, height: 1000 });
			await waitRows(page, 'n > 0 && n !== arg', g1.n);
			await app.idle();
			const g2 = await geometry(page);
			assert.ok(g2.box > g1.box, `the box did not grow (${g1.box} → ${g2.box})`);
			assert.equal(g2.hids.length, g2.n, `${g2.hids.length} rows in a box of ${g2.box}px that holds ${g2.n}`);
			assert.ok(g2.n > g1.n, `a taller pane holds no more rows (${g1.n} → ${g2.n})`);
			assert.equal(g2.hids.at(-1), seedHid(29), 'the newest row is not last after the resize');
			assert.ok(seedIndex(g2.hids[0]) < seedIndex(g1.hids[0]), 'the taller pane shows no more of the past');
			assert.ok(g2.head + g2.n * g2.row <= g2.box + 0.5, 'the rows overflow the taller box');
			assert.ok(g2.head + (g2.n + 1) * g2.row > g2.box, 'a gap in the taller box');
		}, SHORT);
	});
});

describe('scrolling up', () => {
	test('pages older rows in above, keeps the row that was at the top where it was, and stops at the cap', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, 320);
			await waitRows(page, 'n > 0');
			await app.idle();
			const g0 = await geometry(page);
			assert.equal(g0.hids.length, g0.n);
			assert.equal(g0.hids.at(-1), seedHid(319));

			// The table fits its box exactly, so there is nothing to scroll:
			// the wheel, turned up over it, asks for the page above.
			const body = page.locator('section.pane[data-title="tail"] .body');
			const b = await body.boundingBox();
			await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
			await page.mouse.wheel(0, -120);
			await waitRows(page, 'n > arg', g0.n);
			const g1 = await geometry(page);
			const top = g0.hids[0];
			assert.equal(g1.hids.length, 2 * g0.n, `one page of ${g0.n} above ${g0.n}: ${g1.hids.length}`);
			// To the pixel: scroll offsets are whole pixels and the rows are not.
			assert.ok(Math.abs(g1.ys[top] - g0.ys[top]) < 1, `the row that was at the top moved: ${g0.ys[top]} → ${g1.ys[top]}`);
			assert.ok(seedIndex(g1.hids[0]) < seedIndex(top), 'the oldest shown is not older');
			assert.deepEqual(g1.hids, g1.hids.map((_, i) => seedHid(320 - 2 * g0.n + i)), 'the page above is not contiguous');

			// Again and again, to the cap: 300 rows, and then no more —
			// the newest go as older ones come.
			let count = g1.hids.length;
			for (let pages = 0; pages < 120 && count < 300; pages++) {
				await toTop(page);
				await waitRows(page, 'n !== arg', count);
				count = await rowCount(page);
			}
			assert.equal(count, 300, 'the cap');
			const before = await geometry(page);
			await toTop(page);
			await waitTopNot(page, before.hids[0]);
			await app.idle();
			const after = await geometry(page);
			assert.equal(after.hids.length, 300, 'grew past the cap');
			assert.ok(seedIndex(after.hids[0]) < seedIndex(before.hids[0]), 'past the cap, older rows no longer come');
			assert.ok(seedIndex(after.hids.at(-1)) < seedIndex(before.hids.at(-1)), 'past the cap, the newest rows stayed');
			// The live end has gone off the bottom, and the foot says so.
			await marker(page).waitFor();
		}, DESKTOP);
	});
});

describe('a write while scrolled up', () => {
	test('keeps the place and shows ↓ new; at the bottom the new row is simply there', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, 30);
			await waitRows(page, 'n > 0');
			await app.idle();
			const g0 = await geometry(page);
			// A page above, then up to its top: scrolled up, not at the bottom.
			const body = page.locator('section.pane[data-title="tail"] .body');
			const b = await body.boundingBox();
			await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
			await page.mouse.wheel(0, -120);
			await waitRows(page, 'n > arg', g0.n);
			// Up the older rows, a little short of the top — the top asks
			// for the page above, and this is about a write, not a page.
			await page.evaluate(() => {
				document.querySelector('section.pane[data-title="tail"] .body').scrollTop = 10;
			});
			const g1 = await geometry(page);
			assert.equal(g1.scrollTop, 10);
			assert.ok(!g1.atBottom, 'still at the bottom after scrolling up');
			assert.equal(await marker(page).count(), 0, '↓ new before anything is new');

			await app.submit('occur tea');
			await app.idle();
			await marker(page).waitFor();
			const g2 = await geometry(page);
			assert.equal(g2.hids[0], g1.hids[0], 'the top row changed');
			assert.ok(Math.abs(g2.ys[g1.hids[0]] - g1.ys[g1.hids[0]]) < 1, `the table jumped: ${g1.ys[g1.hids[0]]} → ${g2.ys[g1.hids[0]]}`);
			assert.ok(!g2.atBottom, 'the table scrolled to the bottom on its own');
			const [newest] = await app.entries(1);
			assert.equal(newest.field, 'tea');

			// The marker: to the bottom, fresh — the newest page, the new row last.
			await marker(page).click();
			await waitRows(page, 'n === arg', g0.n);
			await app.idle();
			const g3 = await geometry(page);
			assert.equal(g3.hids.length, g3.n, 'the fresh page is not the count the box holds');
			assert.equal(g3.hids.at(-1), newest.hid, 'the new row is not last');
			assert.ok(g3.atBottom, 'not at the bottom after ↓ new');
			assert.equal(await marker(page).count(), 0, '↓ new is still up');

			// At the bottom, a write is simply shown: pinned, no marker, no click.
			await app.submit('occur coffee');
			await app.idle();
			const [coffee] = await app.entries(1);
			assert.equal(coffee.field, 'coffee');
			await page.waitForFunction(
				(hid) => [...document.querySelectorAll('section.pane[data-title="tail"] tbody tr')].at(-1)?.dataset.hid === hid,
				coffee.hid
			);
			const g4 = await geometry(page);
			assert.equal(g4.hids.length, g4.n);
			assert.ok(g4.atBottom, 'not pinned at the bottom');
			assert.equal(await marker(page).count(), 0, '↓ new for a row that is on screen');
		}, DESKTOP);
	});
});
