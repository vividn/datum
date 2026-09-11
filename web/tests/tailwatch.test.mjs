// The tail pane as `tail -w` (da-lan0, da-qrc6): its height decides how
// many rows it shows — exactly the rows that fit, no floor — its header is
// pinned at every scroll position, scrolling up pages older rows in, and a
// write while the reader is scrolled up keeps their place and says `↓ new`
// at the foot.
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

/** The tail pane's body and table, measured. */
const geometry = (page) =>
	page.evaluate(() => {
		const pane = document.querySelector('section.pane[data-title="tail"]');
		const body = pane.querySelector('.body');
		const style = getComputedStyle(body);
		const box = body.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
		const thead = pane.querySelector('thead');
		const head = thead?.getBoundingClientRect().height ?? 0;
		const trs = [...pane.querySelectorAll('tbody tr')];
		const row = Math.min(...trs.map((tr) => tr.getBoundingClientRect().height));
		const rect = body.getBoundingClientRect();
		const top = rect.top;
		// The content box's top, where the table starts when not scrolled.
		const content = top + parseFloat(style.paddingTop) - body.scrollTop;
		const headRect = thead?.getBoundingClientRect();
		return {
			box,
			head,
			row,
			n: Math.floor((box - head) / row),
			hids: trs.map((tr) => tr.dataset.hid),
			// Each row's position on screen, relative to the body.
			ys: Object.fromEntries(trs.map((tr) => [tr.dataset.hid, tr.getBoundingClientRect().top - top])),
			// The last row's bottom, from the top of the content box.
			last: trs.length ? trs.at(-1).getBoundingClientRect().bottom - content : 0,
			headInside: !!headRect && headRect.top >= rect.top - 0.5 && headRect.bottom <= rect.bottom + 0.5,
			headers: [...pane.querySelectorAll('thead th')].map((th) => th.textContent),
			start: [...pane.querySelectorAll('tbody tr.start')].map((tr) => tr.dataset.hid),
			scrollTop: body.scrollTop,
			scrolls: body.scrollHeight > body.clientHeight + 1,
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

/**
 * Wait until the pane is at rest: nothing in flight behind the seam, and
 * the rows on screen are the count its box holds now. Polled, with a
 * deadline: the box can still be settling after the first listing lands
 * (a phone's layout does), and a measurement taken then is of a pane that
 * is about to ask again.
 */
const atRest = (page) =>
	page.waitForFunction(
		() => {
			if (window.__datumInflight !== 0) return false;
			const pane = document.querySelector('section.pane[data-title="tail"]');
			const body = pane?.querySelector('.body');
			const trs = [...(pane?.querySelectorAll('tbody tr') ?? [])];
			if (!body || trs.length === 0) return false;
			const style = getComputedStyle(body);
			const box = body.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
			const head = pane.querySelector('thead').getBoundingClientRect().height;
			const row = Math.min(...trs.map((tr) => tr.getBoundingClientRect().height));
			return trs.length === Math.floor((box - head) / row);
		},
		null,
		{ timeout: 10_000 }
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

/**
 * Scroll the tail's body to its top — which is how the page above is asked
 * for — and say where each row was the moment it got there, before the
 * page (a worker's round trip away) can land.
 */
const toTop = (page) =>
	page.evaluate(() => {
		const body = document.querySelector('section.pane[data-title="tail"] .body');
		body.scrollTop = 0;
		const top = body.getBoundingClientRect().top;
		const trs = [...body.querySelectorAll('tbody tr')];
		return Object.fromEntries(trs.map((tr) => [tr.dataset.hid, tr.getBoundingClientRect().top - top]));
	});

/** Turn the wheel up over the tail's body. */
async function wheelUp(page) {
	const b = await page.locator('section.pane[data-title="tail"] .body').boundingBox();
	await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
	await page.mouse.wheel(0, -120);
}

/** The pane at rest, pinned at the newest: exactly the rows that fit, no gap, nothing to scroll. */
function assertFits(g, label) {
	assert.equal(g.hids.length, g.n, `${label}: ${g.hids.length} rows in a box of ${g.box}px that holds ${g.n}`);
	assert.ok(g.last <= g.box + 0.5, `${label}: the rows overflow the box (${g.last} of ${g.box}px)`);
	assert.ok(g.box - g.last < g.row, `${label}: a gap under the last row (${g.box - g.last}px, a row is ${g.row}px)`);
	assert.ok(!g.scrolls, `${label}: a scrollbar at rest`);
	assert.ok(g.headInside, `${label}: the header is not in the pane`);
}

describe('rows from height', () => {
	// About 3, 5, 8 and 20 rows of the default layout's tail pane: its
	// share of the height grows with the viewport. The count asserted is
	// read off the box, not these.
	// And a narrow one: at 768px the tail pane is about 240px wide, where
	// the header's `field` and `state` break onto two lines and the header
	// is taller than the one-line probe that first sizes the count.
	for (const [width, height] of [
		[1280, 720],
		[1280, 900],
		[1280, 1200],
		[1280, 2300],
		[768, 1200]
	]) {
		test(`at ${width}x${height} a pane shows exactly the rows its box holds, newest last, whole header words`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seed(app, 40);
					await atRest(page);
					const g = await geometry(page);
					assertFits(g, `${width}x${height}`);
					assert.ok(g.n < 40, `the pane holds all forty rows (n = ${g.n}), which tests nothing`);
					assert.equal(g.hids.at(-1), seedHid(39), 'the newest row is not last');
					assert.deepEqual(g.hids, g.hids.map((_, i) => seedHid(40 - g.n + i)), 'the rows are not the newest n');
					// The CLI shrinks `field` and `state` to `f` and `s` when no
					// row is wide — here none is — and the pane says the whole
					// words, as `tail -w` over a wide terminal would.
					assert.deepEqual(
						g.headers.map((h) => h.split(' ')[0]),
						['time', 'field', 'state', '_n', 'hid'],
						'the header words are not the whole words'
					);
				},
				{ viewport: { width, height } }
			);
		});
	}
});

describe('scrolling up', () => {
	test('the wheel pages older rows in, the top row stays put, the header stays pinned, to the start of the log', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 60);
				await atRest(page);
				const g0 = await geometry(page);
				assertFits(g0, 'at rest');
				const n = g0.n;

				// The table fits its box exactly, so there is nothing to
				// scroll: the wheel, turned up over it, asks for the page above.
				await wheelUp(page);
				await waitRows(page, 'n === arg', 2 * n);
				const g1 = await geometry(page);
				const top = g0.hids[0];
				assert.deepEqual(g1.hids, g1.hids.map((_, i) => seedHid(60 - 2 * n + i)), 'the page above is not contiguous');
				// To the pixel: scroll offsets are whole pixels and the rows are not.
				assert.ok(Math.abs(g1.ys[top] - g0.ys[top]) <= 1, `the row that was at the top moved: ${g0.ys[top]} → ${g1.ys[top]}`);
				assert.ok(g1.headInside, 'the header scrolled out of the pane');

				// Up to the top of that, and the page above it.
				const ys = await toTop(page);
				const top2 = g1.hids[0];
				await waitRows(page, 'n === arg', 3 * n);
				const g2 = await geometry(page);
				assert.ok(Math.abs(g2.ys[top2] - ys[top2]) <= 1, `the row that was at the top moved: ${ys[top2]} → ${g2.ys[top2]}`);
				assert.ok(g2.headInside, 'after two pages, the header is not in the pane');
				assert.ok(g2.scrollTop > 0, 'the page above landed with the reader at its top, not at their row');
				assert.equal(g2.start.length, 0, 'the start of the log marked before it was reached');

				// On to the start of the log: every row, oldest first.
				let count = g2.hids.length;
				for (let pages = 0; pages < 20 && count < 60; pages++) {
					await toTop(page);
					await waitRows(page, 'n !== arg', count);
					count = await rowCount(page);
				}
				assert.equal(count, 60, 'the whole log is not listed');
				// Once more, at the start: nothing comes, and the first row
				// says it is the first.
				await toTop(page);
				await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
				const g3 = await geometry(page);
				assert.equal(g3.hids.length, 60, 'rows came from before the start of the log');
				assert.deepEqual(g3.start, [seedHid(0)], 'the first row of the log is not marked as the start');
				assert.ok(g3.headInside, 'at the start of the log, the header is not in the pane');
			},
			{ viewport: { width: 1280, height: 1200 } }
		);
	});

	test('a touch pull at 390x844 pages the rows above in', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 60);
				await atRest(page);
				const g0 = await geometry(page);
				assertFits(g0, 'at rest');

				// A finger pulling down over the table, as the device's own
				// input — not a synthetic DOM event. From its middle: the
				// pane's tabs sit on its outline, over the body's top edge.
				const b = await page.locator('section.pane[data-title="tail"] .body').boundingBox();
				const cdp = await page.context().newCDPSession(page);
				const x = b.x + b.width / 2;
				const y = b.y + b.height / 2 - 30;
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
				for (let d = 10; d <= 60; d += 10) {
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y + d }] });
				}
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });

				await waitRows(page, 'n === arg', 2 * g0.n);
				const g1 = await geometry(page);
				const top = g0.hids[0];
				assert.deepEqual(g1.hids, g1.hids.map((_, i) => seedHid(60 - 2 * g0.n + i)), 'the page above is not contiguous');
				assert.ok(Math.abs(g1.ys[top] - g0.ys[top]) <= 1, `the row that was at the top moved: ${g0.ys[top]} → ${g1.ys[top]}`);
				assert.ok(g1.headInside, 'the header scrolled out of the pane');
			},
			{ viewport: { width: 390, height: 844 }, device: { hasTouch: true, isMobile: true } }
		);
	});

	// A log shorter than the pane is all on screen: the wheel has nothing
	// to bring, and what the reader sees is the first row marked as the
	// start of the log — a dotted edge over it, "the start of the log" on
	// hover — and nothing else moving.
	test('with the whole log on screen, the wheel brings nothing and the first row is marked', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 3);
				await waitRows(page, 'n === 3');
				await app.idle();
				const g0 = await geometry(page);
				assert.ok(g0.n > 3, `the pane holds ${g0.n} rows, which is not more than the log`);
				await wheelUp(page);
				await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
				await app.idle();
				const g1 = await geometry(page);
				assert.deepEqual(g1.hids, g0.hids, 'the rows changed');
				assert.deepEqual(g1.ys, g0.ys, 'the rows moved');
				assert.deepEqual(g1.start, [seedHid(0)]);
				assert.equal(
					await page.locator('section.pane[data-title="tail"] tbody tr.start').getAttribute('title'),
					'the start of the log'
				);
			},
			{ viewport: { width: 1280, height: 1200 } }
		);
	});

	test('stops at the cap: past 300 rows, the newest go as older ones come', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 320);
				await atRest(page);
				// The first page by the wheel: a table that fits has no scroll to set.
				let count = await rowCount(page);
				await wheelUp(page);
				await waitRows(page, 'n === arg', 2 * count);
				count = await rowCount(page);
				for (let pages = 0; pages < 60 && count < 300; pages++) {
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
			},
			{ viewport: { width: 1280, height: 2300 } }
		);
	});
});

describe('a write', () => {
	test('while scrolled up keeps the place and shows ↓ new, which returns to the live end', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 30);
				await atRest(page);
				const g0 = await geometry(page);
				// A page above, then up to its top: scrolled up, not at the bottom.
				await wheelUp(page);
				await waitRows(page, 'n === arg', 2 * g0.n);
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
				assert.ok(g2.headInside, 'the header is not in the pane');
				const [newest] = await app.entries(1);
				assert.equal(newest.field, 'tea');

				// The marker: to the bottom, fresh — the newest page, the new row last.
				await marker(page).click();
				await waitRows(page, 'n === arg', g0.n);
				await app.idle();
				const g3 = await geometry(page);
				assertFits(g3, 'after ↓ new');
				assert.equal(g3.hids.at(-1), newest.hid, 'the new row is not last');
				assert.equal(await marker(page).count(), 0, '↓ new is still up');
			},
			{ viewport: { width: 1280, height: 1200 } }
		);
	});

	// The start of the log is what the last pull found, not a fact: a
	// write older than the first row makes it wrong, so a write forgets it
	// and the next pull at the top asks again.
	test('a backdated write below the start of the log is found by the next pull at the top', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 20);
				await atRest(page);
				let count = await rowCount(page);
				await wheelUp(page);
				await waitRows(page, 'n > arg', count);
				count = await rowCount(page);
				for (let pages = 0; pages < 10 && count < 20; pages++) {
					await toTop(page);
					await waitRows(page, 'n !== arg', count);
					count = await rowCount(page);
				}
				assert.equal(count, 20, 'the whole log is not listed');
				await wheelUp(page);
				await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
				assert.deepEqual((await geometry(page)).start, [seedHid(0)]);

				// A day before the first seed, typed while at the top.
				await app.submit('occur tea -d 2026-09-09');
				await app.idle();
				const tea = (await app.entries(50)).find((entry) => entry.field === 'tea');
				assert.ok(tea, 'the tea was not stored');
				const g1 = await geometry(page);
				assert.equal(g1.start.length, 0, 'the old first row is still marked as the start after a write');

				await wheelUp(page);
				await waitRows(page, 'n === arg', 21);
				const g2 = await geometry(page);
				assert.equal(g2.hids[0], tea.hid, 'the backdated row is not first');
				await wheelUp(page);
				await page.waitForFunction(
					(hid) => document.querySelector('section.pane[data-title="tail"] tbody tr.start')?.dataset.hid === hid,
					tea.hid,
					{ timeout: 10_000 }
				);
			},
			{ viewport: { width: 1280, height: 1200 } }
		);
	});

	/**
	 * Every listing the pane asks the CLI for, from the first one on, with
	 * the pane's content box at the moment of asking. Watched at the
	 * worker's door, not through `spyOnRunner`: that installs once the app
	 * is up, after the first request — the one this is about.
	 */
	const watchAsks = (context) =>
		context.addInitScript(() => {
			window.__tailAsks = [];
			const post = Worker.prototype.postMessage;
			Worker.prototype.postMessage = function (message, ...rest) {
				if (message?.op === 'cli' && message.argv[0] === 'tail' && message.argv.includes('--json')) {
					const body = document.querySelector('section.pane[data-title="tail"] .body');
					const style = body && getComputedStyle(body);
					window.__tailAsks.push({
						n: Number(message.argv[message.argv.indexOf('-n') + 1]),
						box: body ? body.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom) : null
					});
				}
				return post.call(this, message, ...rest);
			};
		});

	for (const [width, height] of [
		[1280, 800],
		[390, 844]
	]) {
		test(`pinned at the bottom, a typed occur lands last with the count unchanged, at ${width}x${height}`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seed(app, 30);
					await atRest(page);
					const g0 = await geometry(page);
					assertFits(g0, 'at rest');
					assert.deepEqual(g0.hids, g0.hids.map((_, i) => seedHid(30 - g0.n + i)), 'not the newest rows');

					await app.submit('occur coffee');
					await page.waitForFunction(
						() => [...document.querySelectorAll('section.pane[data-title="tail"] tbody tr')].at(-1)?.textContent.includes('coffee'),
						null,
						{ timeout: 2000 }
					);
					await app.idle();
					const g1 = await geometry(page);
					assert.equal(g1.hids.length, g0.n, 'the count changed with the write');
					assert.deepEqual(g1.hids.slice(0, -1), g0.hids.slice(1), 'the screen is not the newest page');
					assertFits(g1, 'after the write');
					assert.equal(await marker(page).count(), 0, '↓ new for a row that is on screen');

					// Asked for what fits the box as it was at the asking — a
					// phone's box settles after the first listing lands, and the
					// pane asks again — and only once the box was a size.
					const asks = await page.evaluate(() => window.__tailAsks);
					assert.ok(asks.length > 0, 'no listing was asked for');
					for (const ask of asks) {
						const holds = Math.floor((ask.box - g0.head) / g0.row);
						assert.equal(ask.n, holds, `asked for ${ask.n} rows in a box that held ${holds}: ${JSON.stringify(asks)}`);
						assert.ok(ask.box > g0.head, `asked before the box was a size: ${JSON.stringify(asks)}`);
					}
				},
				{ viewport: { width, height }, prepare: watchAsks }
			);
		});
	}
});
