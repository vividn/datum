// The tail pane as `tail -w` (da-lan0, da-qrc6): its height decides how
// many rows it shows — exactly the rows that fit, no floor — its header is
// pinned at every scroll position, a page of history is held above the
// screen from the first listing so the first scroll up moves at once
// (da-gtk48), scrolling up pages older rows in, and a write while the
// reader is scrolled up keeps their place and says `↓ new` at the foot.
//
// Every row is the CLI's (`tail --json`); what is asserted here is the
// geometry — the count the pane's box holds, by the pane's own header and
// row heights — and identity, by hid. Never "more rows than before": the
// count is computed from the box and compared to it, and the oldest and
// newest rows are named.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// ONE APP per screen that more than one test is measured at (da-6b3dj,
// sharedApp in support/app.mjs): 1280x1200 and 1280x2300. `solo` empties
// the database between them, and `seed`'s reload draws the pane afresh
// over what it restored. A screen only one test uses opens its own, as
// does a test that watches the worker from before the first page
// (`watchAsks`).
const TALL = sharedApp({ viewport: { width: 1280, height: 1200 } });
const TALLEST = sharedApp({ viewport: { width: 1280, height: 2300 } });
/** `fn` in the shared app at `width`×`height` if there is one, or an app of its own. */
const at = (width, height, fn) =>
	width === 1280 && height === 1200
		? TALL.solo(fn)
		: width === 1280 && height === 2300
			? TALLEST.solo(fn)
			: withApp(fn, { viewport: { width, height } });

/** The tail pane's body and table, measured. */
const geometry = (page) =>
	page.evaluate(() => {
		const pane = document.querySelector('section.pane[data-title="tail"]');
		const body = pane.querySelector('.body:not(.behind .body)');
		const style = getComputedStyle(body);
		// Exact, as the pane counts it (da-zs3t9): `clientHeight` is rounded,
		// and the page lays a padding out in 64ths of a pixel, truncated —
		// `5.28px` is 5.265625 — and either is a row off at the edge of one.
		const pad = (px) => Math.trunc(parseFloat(px) * 64) / 64;
		const box = body.getBoundingClientRect().height - pad(style.paddingTop) - pad(style.paddingBottom);
		const thead = pane.querySelector('thead');
		const head = thead?.getBoundingClientRect().height ?? 0;
		const trs = [...pane.querySelectorAll('tbody tr')];
		const row = Math.min(...trs.map((tr) => tr.getBoundingClientRect().height));
		const rect = body.getBoundingClientRect();
		const top = rect.top;
		// The content box's top on screen, where the header stands.
		const content = top + parseFloat(style.paddingTop);
		const headRect = thead?.getBoundingClientRect();
		const end = content + box;
		// A row on screen: whole, under the header and inside the box.
		const onScreen = (tr) => {
			const r = tr.getBoundingClientRect();
			return r.top >= (headRect?.bottom ?? content) - 1 && r.bottom <= end + 1;
		};
		const first = trs.find(onScreen);
		return {
			box,
			head,
			row,
			n: Math.floor((box - head) / row),
			// Every row held, the history above the screen with them.
			hids: trs.map((tr) => tr.dataset.hid),
			// The rows on screen: whole, under the header and inside the box.
			shown: trs.filter(onScreen).map((tr) => tr.dataset.hid),
			// Each row's position on screen, relative to the body.
			ys: Object.fromEntries(trs.map((tr) => [tr.dataset.hid, tr.getBoundingClientRect().top - top])),
			// Where the last row's bottom is, from the top of the content box,
			// with the screen's rows laid under the header: the header and
			// their heights, so a scroll offset's rounding is not in it.
			last: first ? head + trs.at(-1).getBoundingClientRect().bottom - first.getBoundingClientRect().top : 0,
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
 * Wait until the pane is at rest: nothing in flight behind the seam, at
 * the bottom, and the rows on screen are the count its box holds now.
 * Polled, with a deadline: the box can still be settling after the first
 * listing lands (a phone's layout does), and a measurement taken then is
 * of a pane that is about to ask again.
 */
const atRest = (page, timeout) =>
	until(
		async () => ({ inflight: await page.evaluate(() => window.__datumInflight), ...(await geometry(page).catch(() => null)) }),
		(g) => g.inflight === 0 && g.hids?.length > 0 && g.atBottom && g.shown.length === g.n,
		timeout
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
async function seed(app, count, timeless = [], field = () => 'seed') {
	const at = (i) => new Date(Date.UTC(2026, 8, 10, 5, 0) - (count - 1 - i) * 60_000).toISOString();
	// The header the CLI itself writes, so the schema version is its own.
	const backup = await app.run(['db', 'backup', '-']);
	assert.equal(backup.code, 0, backup.stderr);
	const header = JSON.parse(backup.stdout.split('\n')[0]);
	const lines = [JSON.stringify({ ...header, backup_time: at(count - 1), source_db: 'seed', entry_count: count })];
	for (let i = 0; i < count; i++) {
		const none = timeless.includes(i);
		lines.push(
			JSON.stringify({
				hid: seedHid(i),
				slug: `${field(i)}:${none ? 'c' : ''}${at(i)}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field: field(i),
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: null,
				occur_time: none ? null : { utc: at(i), offset_minutes: 0, timezone: 'UTC' },
				create_time: at(i),
				modify_time: at(i),
				client_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: none ? '%create_time%' : '%occur_time%',
				raw_input: `occur ${field(i)}`,
				parser_version: 2
			})
		);
	}
	const result = await app.run(['db', 'restore', '-', '--merge'], lines.join('\n') + '\n');
	assert.equal(result.code, 0, result.stderr);
	// A run through the seam is not a command the REPL ran, so the panes
	// do not know; a fresh visit shows the listing as it now stands.
	await app.reload();
}

/**
 * The rows a page above brings with `n` on screen (TailTable.svelte's
 * `pageSize`): four screens, at least 100, and with the lead and the
 * screen, within the cap of 300.
 */
const pageOf = (n) => Math.max(n, Math.min(Math.max(100, 4 * n), Math.floor((300 - n) / 2)));

/**
 * The rows a listing for a screen of `n` asks for: the screen and the page
 * above it (da-gtk48). Compared forward, never inverted: it never falls
 * as `n` grows, but from 34 up two screens can ask the same.
 */
const askFor = (n) => n + pageOf(n);

const seedHid = (i) => 'seed' + i.toString().padStart(8, '0');
const seedIndex = (hid) => Number(hid.slice(4));

/**
 * Scroll the tail's body to its top — which is how the page above is asked
 * for — and say where each row was the moment it got there, before the
 * page (a worker's round trip away) can land.
 */
const toTop = (page) =>
	page.evaluate(() => {
		const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
		body.scrollTop = 0;
		const top = body.getBoundingClientRect().top;
		const trs = [...body.querySelectorAll('tbody tr')];
		return Object.fromEntries(trs.map((tr) => [tr.dataset.hid, tr.getBoundingClientRect().top - top]));
	});

/** The header's columns, by key, and the width each is drawn at. */
const columns = (page) =>
	page.evaluate(() =>
		[...document.querySelectorAll('section.pane[data-title="tail"] thead th')].map((th) => ({
			key: th.dataset.key,
			width: th.getBoundingClientRect().width
		}))
	);

/** Turn the wheel up over the tail's body. */
async function wheelUp(page) {
	const b = await page.locator('section.pane[data-title="tail"] .body:not(.behind .body)').boundingBox();
	await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
	await page.mouse.wheel(0, -120);
}

/**
 * The pane at rest, pinned at the newest: exactly the rows that fit on
 * screen, no gap, and above them a page of the `total` rows logged — or
 * all of them — held for the first scroll up (da-gtk48).
 */
function assertFits(g, label, total) {
	assert.equal(g.shown.length, g.n, `${label}: ${g.shown.length} rows on screen in a box of ${g.box}px that holds ${g.n}`);
	assert.deepEqual(g.shown, g.hids.slice(-g.n), `${label}: the rows on screen are not the newest`);
	assert.equal(g.hids.length, Math.min(total, g.n + pageOf(g.n)), `${label}: not a page held above the screen`);
	assert.ok(g.atBottom, `${label}: not at the bottom`);
	assert.ok(g.last <= g.box + 0.5, `${label}: the rows overflow the box (${g.last} of ${g.box}px)`);
	assert.ok(g.box - g.last < g.row, `${label}: a gap under the last row (${g.box - g.last}px, a row is ${g.row}px)`);
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
			await at(width, height, async (app) => {
				const { page } = app;
				await seed(app, 40);
				await atRest(page);
				const g = await geometry(page);
				assertFits(g, `${width}x${height}`, 40);
				assert.ok(g.n < 40, `the pane holds all forty rows (n = ${g.n}), which tests nothing`);
				assert.equal(g.hids.at(-1), seedHid(39), 'the newest row is not last');
				assert.deepEqual(g.shown, g.shown.map((_, i) => seedHid(40 - g.n + i)), 'the rows are not the newest n');
				// The CLI shrinks `field` and `state` to `f` and `s` when no
				// row is wide — here none is — and the pane says the whole
				// words, as `tail -w` over a wide terminal would. The 240px
				// pane at 768 gives up `hid` and `_n` to keep them whole
				// (da-kk0d; tailcolumns.test.mjs).
				assert.deepEqual(
					g.headers.map((h) => h.split(' ')[0]),
					width > 768 ? ['_n', 'time', 'field', 'state', 'hid'] : ['time', 'field', 'state'],
					'the header words are not the whole words'
				);
			});
		});
	}

	// A box of n rows and a hair draws n (da-zs3t9): the box was floored to
	// whole pixels, so in the fraction of a pixel past the edge where the
	// nth row fits, the pane counted one fewer and left a row's gap. A
	// single height proves nothing about an edge, so this sweeps a pixel at
	// a time — the box moves under half a pixel a step — across more than
	// a row, asserts at every step, and asserts that a step landed within
	// half a pixel past an edge, or the sweep missed what it is for.
	test('at every height across a row boundary a pane shows exactly the rows its box holds', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 40);
				let hair = Infinity;
				for (let height = 790; height <= 850; height++) {
					await page.setViewportSize({ width: 1280, height });
					// Two frames: the box is laid out at the new size, so the
					// pane is not found at rest in the box it is leaving.
					await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
					// A pane that never settles fails on the count below, which says why.
					await atRest(page, 5000).catch(() => {});
					const g = await geometry(page);
					assertFits(g, `1280x${height}`, 40);
					hair = Math.min(hair, g.box - g.head - g.n * g.row);
				}
				assert.ok(hair < 0.5, `no step landed within half a pixel past an edge (closest ${hair}px)`);
			},
			{ viewport: { width: 1280, height: 790 } }
		);
	});
});

describe('scrolling up', () => {
	test('the wheel moves at once, pages older rows in, the top row stays put, the header stays pinned, to the start of the log', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			// Under the cap, and room for two whole pages above the screen.
			const total = 250;
			await seed(app, total);
			await atRest(page);
			const g0 = await geometry(page);
			assertFits(g0, 'at rest', total);
			const n = g0.n;
			const size = pageOf(n);
			assert.ok(n + 2 * size < total, `${total} rows is not two pages of ${size} above ${n}`);
			assert.deepEqual(g0.hids, g0.hids.map((_, i) => seedHid(total - n - size + i)), 'the page above the screen is not contiguous');

			// A page is held above the screen (da-gtk48): the wheel scrolls
			// it, at once, and asks for nothing yet.
			await wheelUp(page);
			const g1 = await until(() => geometry(page), (g) => g.scrollTop < g0.scrollTop);
			assert.deepEqual(g1.hids, g0.hids, 'the wheel changed the rows held');
			const bottom = g0.shown.at(-1);
			assert.ok(
				Math.abs(g1.ys[bottom] - g0.ys[bottom] - (g0.scrollTop - g1.scrollTop)) <= 1,
				`the rows did not move with the scroll: ${g0.ys[bottom]} → ${g1.ys[bottom]}`
			);
			assert.ok(g1.headInside, 'the header scrolled out of the pane');

			// Up to the top of that, and the page above it.
			const ys = await toTop(page);
			const top2 = g1.hids[0];
			await waitRows(page, 'n === arg', n + 2 * size);
			const g2 = await geometry(page);
			assert.deepEqual(g2.hids, g2.hids.map((_, i) => seedHid(total - n - 2 * size + i)), 'the page above is not contiguous');
			assert.ok(Math.abs(g2.ys[top2] - ys[top2]) <= 1, `the row that was at the top moved: ${ys[top2]} → ${g2.ys[top2]}`);
			assert.ok(g2.headInside, 'after two pages, the header is not in the pane');
			assert.ok(g2.scrollTop > 0, 'the page above landed with the reader at its top, not at their row');
			assert.equal(g2.start.length, 0, 'the start of the log marked before it was reached');

			// On to the start of the log: every row, oldest first.
			let count = g2.hids.length;
			for (let pages = 0; pages < 20 && count < total; pages++) {
				await toTop(page);
				await waitRows(page, 'n !== arg', count);
				count = await rowCount(page);
			}
			assert.equal(count, total, 'the whole log is not listed');
			// Once more, at the start: nothing comes, and the first row
			// says it is the first.
			await toTop(page);
			await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
			const g3 = await geometry(page);
			assert.equal(g3.hids.length, total, 'rows came from before the start of the log');
			assert.deepEqual(g3.start, [seedHid(0)], 'the first row of the log is not marked as the start');
			assert.ok(g3.headInside, 'at the start of the log, the header is not in the pane');
		});
	});

	// vividn, 2026-10-22 (da-gtk48): «I try to scroll up and nothing happens
	// at first except a bit of jitter, and then all of a sudden the hid
	// column disappears entirely». The columns were decided again from
	// every row held as each part of a page was drawn, so an older row
	// whose words did not fit took `hid` and `_n` with it, and the others
	// widened and narrowed with what landed. Now they are the screen's, and
	// held at their widths while rows land above.
	test('past the first page, no column goes or changes width, and the reader stays where they were', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			const total = 250;
			const LONG = 'commute_by_bicycle_through_the_park_on_a_rainy_morning';
			// The oldest hundred are of a field too long for every column.
			await seed(app, total, [], (i) => (i < 100 ? LONG : 'seed'));
			await atRest(page);
			const g0 = await geometry(page);
			assertFits(g0, 'at rest', total);
			const n = g0.n;
			const size = pageOf(n);
			assert.ok(total - n - size >= 100, 'a long row is in the first listing: not the case');
			assert.ok(total - n - 2 * size < 100, 'the page above brings no long row: not the case');
			const c0 = await columns(page);
			assert.deepEqual(c0.map((c) => c.key), ['_n', 'time', 'field', 'state', 'hid'], 'not every column at rest');

			const at = await toTop(page);
			await waitRows(page, 'n === arg', n + 2 * size);
			await app.idle();
			const g1 = await geometry(page);
			assert.ok(
				await page.locator('section.pane[data-title="tail"] tbody td.field', { hasText: LONG }).count(),
				'the long rows did not land'
			);
			assert.deepEqual(await columns(page), c0, 'a column went or changed width as the page landed');
			const top = g0.hids[0];
			assert.ok(Math.abs(g1.ys[top] - at[top]) <= 1, `the reader's row moved: ${at[top]} → ${g1.ys[top]}`);
			assert.ok(g1.headInside, 'the header scrolled out of the pane');
		});
	});

	// A timeless row (`add`, `-T`) is listed at the stamp it was created at,
	// as `tail` lists it (da-2vgj1), and pages in there: the page above is
	// asked for by a row's place in the listing, not by a time (da-mhcla).
	// The rows are the ones the CLI lists, in its order.
	test('in a log longer than a page, a timeless row pages in at the stamp it was created at', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			const total = 253;
			const timeless = [3, 4, 120];
			await seed(app, total, timeless);
			await atRest(page);
			const listed = await app.run(['tail', '--json', '-n', '1000']);
			assert.equal(listed.code, 0, listed.stderr);
			const rows = JSON.parse(listed.stdout).rows;
			const want = rows.map((row) => row.hid);
			assert.deepEqual(want, Array.from({ length: total }, (_, i) => seedHid(i)), '`tail` does not list a timeless row at its create time');
			assert.deepEqual(
				timeless.map((i) => rows[i].time_metric),
				['create', 'create', 'create']
			);
			const g0 = await geometry(page);
			assert.ok(g0.n + pageOf(g0.n) < total - timeless[0], `the timeless rows are not more than a page above the screen`);

			// Up until the pane says it is at the start, wherever it says so.
			await wheelUp(page);
			const start = page.locator('section.pane[data-title="tail"] tbody tr.start');
			for (let pages = 0, count = g0.n; pages < 20 && !(await start.count()); pages++) {
				await page.waitForFunction((arg) => {
					const pane = document.querySelector('section.pane[data-title="tail"]');
					return pane.querySelectorAll('tbody tr').length !== arg || pane.querySelector('tbody tr.start');
				}, count);
				count = await rowCount(page);
				await toTop(page);
			}
			const g1 = await geometry(page);
			assert.deepEqual(g1.hids, want, 'the pane, scrolled to its top, does not list what `tail` lists');
			assert.deepEqual(g1.start, [want[0]], 'the start of the log is not its first row');

			// A write while they are on screen keeps them.
			await app.submit('occur seed');
			await app.idle();
			await marker(page).waitFor();
			assert.deepEqual((await geometry(page)).hids.slice(0, 6), want.slice(0, 6), 'a write below dropped the rows the reader was at');
		});
	});

	test('a touch pull at 390x844 moves the rows above at once', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seed(app, 250);
				await atRest(page);
				const g0 = await geometry(page);
				assertFits(g0, 'at rest', 250);

				// A finger pulling down over the table, as the device's own
				// input — not a synthetic DOM event. From its middle: the
				// pane's tabs sit on its outline, over the body's top edge.
				const b = await page.locator('section.pane[data-title="tail"] .body:not(.behind .body)').boundingBox();
				const cdp = await page.context().newCDPSession(page);
				const x = b.x + b.width / 2;
				const y = b.y + b.height / 2 - 30;
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
				for (let d = 10; d <= 60; d += 10) {
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y + d }] });
				}
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });

				// The page held above the screen (da-gtk48) is scrolled by the
				// finger itself: nothing to wait for.
				const g1 = await until(() => geometry(page), (g) => g.scrollTop < g0.scrollTop);
				assert.deepEqual(g1.hids, g0.hids, 'the pull changed the rows held');
				const bottom = g0.shown.at(-1);
				assert.ok(
					Math.abs(g1.ys[bottom] - g0.ys[bottom] - (g0.scrollTop - g1.scrollTop)) <= 1,
					`the rows did not move with the scroll: ${g0.ys[bottom]} → ${g1.ys[bottom]}`
				);
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
		await TALL.solo(async (app) => {
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
		});
	});

	// vividn (da-j6frl): «rows should load long before you get to them … The
	// scrolling should just feel smooth and effortless.» A steady scroll up
	// a long log, past the cap: the page above is asked for while the
	// reader is still a screen or more below the top, several screens at a
	// time, and the reader is never at the top with a page in flight.
	test('a steady scroll up asks ahead, several screens a page, and never waits at the top', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			await seed(app, 3000);
			await atRest(page);
			const { n, row } = await geometry(page);
			// How far ahead of the top a page is asked for.
			const lead = (pageOf(n) - n) * row;
			// Each page asked for, where the reader was when it was, and how
			// many are in flight.
			await page.evaluate(() => {
				const pages = (window.__pages = { asked: [], inflight: 0 });
				const previous = window.datum.setRunner(async (argv, ...rest) => {
					if (argv[0] !== 'tail' || !argv.includes('--above')) return previous(argv, ...rest);
					const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
					const ask = { top: body.scrollTop, screen: body.clientHeight, t: performance.now() };
					pages.asked.push(ask);
					pages.inflight++;
					try {
						return await previous(argv, ...rest);
					} finally {
						pages.inflight--;
						ask.ms = performance.now() - ask.t;
					}
				});
				window.__restore = () => window.datum.setRunner(previous);
			});
			try {
				// The first listing's time, for the pace until a page has
				// been timed: as long as a page, whatever its size.
				const t0 = performance.now();
				await app.run(['tail', '--json', '-n', String(n + pageOf(n))]);
				const first = performance.now() - t0;
				// Steady, and at a speed the look-ahead is for: the reader
				// takes four times the slowest page yet to cover the lead it
				// is asked for with. Asked for at the top instead, as it was,
				// a page is in flight there at any speed.
				let scrolled = 0;
				let screen = 0;
				const stalls = [];
				let most = 0;
				for (let step = 0; step < 100; step++) {
					const s = await page.evaluate(
						([px, lead, first]) =>
							new Promise((resolve) => {
								const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
								const before = body.scrollTop;
								body.scrollTop = before - px;
								const moved = before - body.scrollTop;
								const slowest = Math.max(first, ...window.__pages.asked.map((a) => a.ms ?? performance.now() - a.t));
								const pause = (4 * slowest * px) / lead;
								requestAnimationFrame(() => {
									const sample = {
										moved,
										top: body.scrollTop,
										screen: body.clientHeight,
										inflight: window.__pages.inflight,
										rows: body.querySelectorAll('tbody tr').length
									};
									setTimeout(() => resolve(sample), pause);
								});
							}),
						[60, lead, first]
					);
					scrolled += s.moved;
					screen = s.screen;
					most = Math.max(most, s.rows);
					if (s.top <= 0 && s.inflight > 0) stalls.push(step);
				}
				const { asked } = await page.evaluate(() => window.__pages);
				const screens = scrolled / screen;
				// Every page: the first is held from the start (da-gtk48).
				const ahead = asked;
				assert.ok(screens > 15, `scrolled ${screens.toFixed(1)} screens, not a long scroll`);
				assert.deepEqual(stalls, [], 'the reader sat at the top with a page in flight');
				assert.ok(most > 300 - 1 && most <= 300, `held ${most} rows: the scroll did not reach the cap, or passed it`);
				assert.ok(
					ahead.every((a) => a.top >= a.screen),
					`a page asked for less than a screen from the top: ${JSON.stringify(ahead)}`
				);
				assert.ok(
					asked.length * 3 <= screens,
					`${asked.length} pages for ${screens.toFixed(1)} screens scrolled: not several screens a page`
				);
			} finally {
				await page.evaluate(() => window.__restore?.());
			}
		});
	});

	test('stops at the cap: past 300 rows, the newest go as older ones come', async () => {
		await TALLEST.solo(async (app) => {
			const { page } = app;
			await seed(app, 600);
			await atRest(page);
			let count = await rowCount(page);
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
		});
	});

	// vividn, 2026-10-27: «if you scroll sufficiently far up in the tail
	// widget, you become unable to scroll down all the way anymore». The cap
	// let the newest rows go and nothing paged them back: `↓ new` waited on
	// a write. Scrolling down alone brings them back now (da-r88ur).
	test('past the cap, scrolling down pages the newest back in, to the live end, with no write', async () => {
		await TALLEST.solo(async (app) => {
			const { page } = app;
			const total = 600;
			await seed(app, total);
			await atRest(page);
			const newest = seedHid(total - 1);
			const [before] = await app.entries(1);
			for (let pages = 0; pages < 60 && (await geometry(page)).hids.includes(newest); pages++) {
				const top = (await geometry(page)).hids[0];
				await toTop(page);
				await waitTopNot(page, top);
			}
			await app.idle();
			assert.ok(!(await geometry(page)).hids.includes(newest), 'the cap never let the newest row go');
			// Not held, the live end is one press away, write or none.
			await marker(page).waitFor();

			const lastRow = () =>
				page.evaluate(() => document.querySelector('section.pane[data-title="tail"] tbody tr:last-child')?.dataset.hid);
			for (let i = 0; i < 20 && !(await geometry(page)).shown.includes(newest); i++) {
				const last = await lastRow();
				await page.evaluate(() => {
					const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
					body.scrollTop = body.scrollHeight;
				});
				if (last !== newest)
					await page.waitForFunction(
						(last) => document.querySelector('section.pane[data-title="tail"] tbody tr:last-child')?.dataset.hid !== last,
						last
					);
			}
			const g = await geometry(page);
			assert.ok(g.shown.includes(newest), `scrolling down stopped at ${g.hids.at(-1)}, short of ${newest}`);
			assert.equal(g.hids.at(-1), newest, 'the newest row is not last');
			assert.ok(g.hids.length <= 300, `held ${g.hids.length} rows, past the cap`);
			await page.waitForFunction(() => !document.querySelector('section.pane[data-title="tail"] button.new'));
			const [after] = await app.entries(1);
			assert.equal(after.hid, before.hid, 'a write landed: not the case this is about');
		});
	});

	// One scroll event for two scrolls (da-gaiko). A body raises its scroll
	// events a frame at a time, so a reader who reaches the top in the frame
	// the widget put them back at their row shares the one event with it.
	// Read against where the body was at the event BEFORE, the top, that
	// event was not a scroll up: no page was asked for, and the reader sat
	// at the top of a log with a past — as this file's own loops (`toTop`,
	// then wait for the rows to change) did once, on a busy lane. Built on
	// purpose here: wherever the widget puts the body, the reader is at the
	// top again before the task is out, so every event is such a one.
	test('a reader at the top in the frame a page lands is brought the page above', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			const total = 250;
			await seed(app, total);
			await atRest(page);
			await page.evaluate(() => {
				const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
				const { get, set } = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop');
				Object.defineProperty(body, 'scrollTop', {
					configurable: true,
					get: () => get.call(body),
					set(top) {
						set.call(body, top);
						// A microtask: after the widget has read where it landed,
						// before any frame.
						if (top > 0) queueMicrotask(() => set.call(body, 0));
					}
				});
				window.__release = () => (delete body.scrollTop, delete window.__release);
			});
			try {
				// The one scroll that is this test's.
				await toTop(page);
				await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
				const g = await geometry(page);
				assert.equal(g.hids.length, total, 'the whole log is not listed');
				assert.deepEqual(g.start, [seedHid(0)], 'the first row of the log is not marked as the start');
				assert.equal(g.scrollTop, 0, 'the reader is not at the top: not the case this is about');
			} finally {
				await page.evaluate(() => window.__release?.());
			}
		});
	});
});

describe('a write', () => {
	test('while scrolled up keeps the place and shows ↓ new, which returns to the live end', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			await seed(app, 30);
			await atRest(page);
			assertFits(await geometry(page), 'at rest', 30);
			// Up the older rows held above the screen, a little short of
			// the top: scrolled up, not at the bottom.
			await page.evaluate(() => {
				document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)').scrollTop = 10;
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
			const g3 = await atRest(page);
			assert.equal(g3.shown.at(-1), newest.hid, 'the new row is not last');
			assert.equal(await marker(page).count(), 0, '↓ new is still up');
		});
	});

	// A page the reader asked for survives a refresh (da-nkkjb): landing
	// while the page was in flight, a refresh once replaced the screen and
	// the page was thrown away; landing after, it replaced the page. Then
	// the screen fitted its box exactly, so the reader's row stayed at the
	// live end; now a page is held above it from the first listing
	// (da-gtk48), and the reader asks for the next at the top.
	const TOTAL = 250;
	const pageKept = async (page, g0, at, label) => {
		// Both have come back; two frames for them to be drawn.
		await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
		const g = await geometry(page);
		const held = g0.n + 2 * pageOf(g0.n);
		const want = Array.from({ length: held }, (_, i) => seedHid(TOTAL - held + i));
		assert.deepEqual(g.hids.slice(0, want.length), want, `${label}: the page above is gone`);
		const top = g0.hids[0];
		assert.ok(Math.abs(g.ys[top] - at[top]) <= 1, `${label}: the reader's row moved: ${at[top]} → ${g.ys[top]}`);
		return g;
	};

	test('a refresh landing while the page above is in flight keeps the page and the place (da-nkkjb)', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			await seed(app, TOTAL);
			await atRest(page);
			const g0 = await geometry(page);
			assertFits(g0, 'at rest', TOTAL);
			// The page's answer is held until released: the refresh lands first.
			await page.evaluate(() => {
				let release;
				const hold = new Promise((resolve) => (release = resolve));
				const previous = window.datum.setRunner(async (argv, ...rest) => {
					const result = await previous(argv, ...rest);
					if (argv[0] === 'tail' && argv.includes('--above') && !window.__paged) {
						window.__paged = hold.then(() => result);
						return window.__paged;
					}
					return result;
				});
				window.__release = () => {
					window.datum.setRunner(previous);
					release();
					delete window.__release;
				};
			});
			try {
				const at = await toTop(page);
				await page.waitForFunction(() => !!window.__paged);
				await app.submit('occur tea');
				await app.idle();
				const [newest] = await app.entries(1);
				await page.waitForFunction(
					(hid) => document.querySelector(`section.pane[data-title="tail"] tbody tr[data-hid="${hid}"]`),
					newest.hid
				);
				await page.evaluate(() => (window.__release(), window.__paged));
				const g = await pageKept(page, g0, at, 'refreshed mid page');
				assert.equal(g.hids.at(-1), newest.hid, 'the new row is not last');
				await marker(page).waitFor();
			} finally {
				await page.evaluate(() => (window.__release?.(), delete window.__paged));
			}
		});
	});

	test('a refresh landing after the page above keeps the page and the place (da-nkkjb)', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			await seed(app, TOTAL);
			await atRest(page);
			const g0 = await geometry(page);
			assertFits(g0, 'at rest', TOTAL);
			const at = await toTop(page);
			await waitRows(page, 'n === arg', g0.n + 2 * pageOf(g0.n));
			await app.submit('occur tea');
			await app.idle();
			const [newest] = await app.entries(1);
			await page.waitForFunction(
				(hid) => document.querySelector(`section.pane[data-title="tail"] tbody tr[data-hid="${hid}"]`),
				newest.hid
			);
			await pageKept(page, g0, at, 'refreshed after the page');
			await marker(page).waitFor();
		});
	});

	// The start of the log is what the last pull found, not a fact: a
	// write older than the first row makes it wrong, so a write forgets it.
	// The refresh brings the backdated row, the page above the screen being
	// in every listing (da-gtk48), and the next pull at the top finds it is
	// the start.
	test('a backdated write below the start of the log is listed, and the next pull at the top marks it the start', async () => {
		await TALL.solo(async (app) => {
			const { page } = app;
			await seed(app, 20);
			await atRest(page);
			// The whole log is held from the first listing.
			assert.equal(await rowCount(page), 20, 'the whole log is not listed');
			await toTop(page);
			await wheelUp(page);
			await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor();
			assert.deepEqual((await geometry(page)).start, [seedHid(0)]);

			// A day before the first seed, typed while at the top. The
			// field is declared first so the write is one row (da-c8mh).
			await app.declare('tea');
			await app.submit('occur tea -d 2026-09-09');
			await app.idle();
			const tea = (await app.entries(50)).find((entry) => entry.field === 'tea');
			assert.ok(tea, 'the tea was not stored');
			const g1 = await geometry(page);
			assert.equal(g1.start.length, 0, 'the old first row is still marked as the start after a write');

			await wheelUp(page);
			await waitRows(page, 'n === arg', 22);
			const g2 = await geometry(page);
			assert.equal(g2.hids[0], tea.hid, 'the backdated row is not first');
			await wheelUp(page);
			await page.waitForFunction(
				(hid) => document.querySelector('section.pane[data-title="tail"] tbody tr.start')?.dataset.hid === hid,
				tea.hid
			);
		});
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
				// The PANE's listing, which never names a field: `tail --json`
				// with the field's slot empty. The layout store asks `tail
				// layout --json` of the data db for what a kit shipped
				// (da-gg0v), and that is not a listing this pane asked for.
				if (message?.op === 'cli' && message.argv[0] === 'tail' && message.argv[1] === '--json') {
					const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
					const style = body && getComputedStyle(body);
					const pad = (px) => Math.trunc(parseFloat(px) * 64) / 64;
					window.__tailAsks.push({
						n: Number(message.argv[message.argv.indexOf('-n') + 1]),
						box: body ? body.getBoundingClientRect().height - pad(style.paddingTop) - pad(style.paddingBottom) : null
					});
				}
				return post.call(this, message, ...rest);
			};
		});

	// A state long enough to wrap in a narrow pane is lines the one-line
	// probe does not count: the first ask is the probe's, the screen as
	// drawn overflows, and the count comes down to what fits — a second
	// ask, and nothing to scroll at rest.
	test('long states in a narrow pane: the drawn screen settles the count', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// Four entries, and the TRACK document each of the two fields
				// is declared by (da-c8mh): six rows, which is what this pane
				// must bring down to what its box draws.
				const LONG = [
					'switch mood contemplative-about-the-weather -t 0700',
					'switch project datum-day-tail-table-that-wraps -t 0800',
					'switch mood restless-in-the-afternoon-light -t 0900',
					'switch project another-name-that-wraps-in-a-column -t 1200'
				];
				await seedLines(app, LONG);
				await page.waitForFunction(() =>
					[...document.querySelectorAll('section.pane[data-title="tail"] tbody tr')].at(-1)?.textContent.includes('another-name')
				);
				// Settled: the last ask's screen is what is shown, at the bottom.
				const g = await until(
					async () => ({
						...(await geometry(page)),
						inflight: await page.evaluate(() => window.__datumInflight),
						asks: await page.evaluate(() => window.__tailAsks)
					}),
					(g) =>
						g.inflight === 0 &&
						g.atBottom &&
						g.hids.at(-1) &&
						g.asks.at(-1).n === askFor(g.shown.length)
				);
				assert.ok(g.shown.length > 0 && g.shown.length < g.hids.length, `${g.shown.length} rows on screen: the pane shows all ${g.hids.length}, which tests nothing`);
				// An ask for more than `askFor` of the screen was for a bigger screen.
				assert.ok(g.asks.some((ask) => ask.n > askFor(g.shown.length)), `no ask beyond what fits was corrected: ${JSON.stringify(g.asks)}`);
				assert.ok(g.last <= g.box + 0.5, `the rows overflow the box (${g.last} of ${g.box}px)`);
				assert.ok(g.headInside, 'the header is not in the pane');
			},
			{ viewport: { width: 768, height: 1200 }, prepare: watchAsks }
		);
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
					// A declaration already in the listing, so the `action`
					// column (da-c8mh) is there before the write and the count
					// does not move when the write brings one of its own.
					await app.declare('coffee');
					await app.reload();
					await atRest(page);
					const g0 = await geometry(page);
					assertFits(g0, 'at rest', 31);
					const seeds = g0.shown.slice(0, -1);
					assert.deepEqual(
						seeds,
						seeds.map((_, i) => seedHid(30 - seeds.length + i)),
						'not the newest rows'
					);

					await app.submit('occur coffee');
					await page.waitForFunction(() =>
						[...document.querySelectorAll('section.pane[data-title="tail"] tbody tr')].at(-1)?.textContent.includes('coffee')
					);
					await app.idle();
					const g1 = await geometry(page);
					assert.equal(g1.shown.length, g0.n, 'the count changed with the write');
					assert.deepEqual(g1.shown.slice(0, -1), g0.shown.slice(1), 'the screen is not the newest page');
					assertFits(g1, 'after the write', 32);
					assert.equal(await marker(page).count(), 0, '↓ new for a row that is on screen');

					// Asked for what fits the box as it was at the asking — a
					// phone's box settles after the first listing lands, and the
					// pane asks again — and only once the box was a size.
					const asks = await page.evaluate(() => window.__tailAsks);
					assert.ok(asks.length > 0, 'no listing was asked for');
					for (const ask of asks) {
						const holds = Math.floor((ask.box - g0.head) / g0.row);
						assert.equal(ask.n, askFor(holds), `asked for ${ask.n} rows in a box that held ${holds}: ${JSON.stringify(asks)}`);
						assert.ok(ask.box > g0.head, `asked before the box was a size: ${JSON.stringify(asks)}`);
					}
				},
				{ viewport: { width, height }, prepare: watchAsks }
			);
		});
	}
});
