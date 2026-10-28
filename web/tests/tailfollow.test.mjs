// The tail pane follows the page's selection (da-lqz3j, vividn: "if you
// have a piece of data selected, the tail should jump to that piece of data
// to show surrounding data, similarly, if you select a time, the tail
// should jump to that time, show a white dotted line where the selected
// time is in the data. if you select a range of times, then only show the
// data that appears within that range in the tail").
//
// The selection is set through the stores the dayview and the nowview
// write — the `hid` pill a tap on a mark makes, and the cursor a press on
// a time makes — and every row is the CLI's, named by hid.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

const APP = sharedApp({ viewport: { width: 1280, height: 1200 } });

const COUNT = 400;
/** The i-th oldest seed's instant: one a minute, ending 2026-09-10 05:00 UTC. */
const at = (i) => new Date(Date.UTC(2026, 8, 10, 5, 0) - (COUNT - 1 - i) * 60_000);
const seedHid = (i) => 'seed' + i.toString().padStart(8, '0');
const seedIndex = (hid) => Number(hid.slice(4));

/** `COUNT` entries restored in one command, as tailwatch.test.mjs seeds them. */
async function seed(app) {
	const backup = await app.run(['db', 'backup', '-']);
	assert.equal(backup.code, 0, backup.stderr);
	const header = JSON.parse(backup.stdout.split('\n')[0]);
	const lines = [JSON.stringify({ ...header, backup_time: at(COUNT - 1).toISOString(), source_db: 'seed', entry_count: COUNT })];
	for (let i = 0; i < COUNT; i++) {
		const t = at(i).toISOString();
		lines.push(
			JSON.stringify({
				hid: seedHid(i),
				slug: `seed:${t}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field: 'seed',
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: null,
				occur_time: { utc: t, offset_minutes: 0, timezone: 'UTC' },
				create_time: t,
				modify_time: t,
				client_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: '%occur_time%',
				raw_input: 'occur seed',
				parser_version: 2
			})
		);
	}
	const result = await app.run(['db', 'restore', '-', '--merge'], lines.join('\n') + '\n');
	assert.equal(result.code, 0, result.stderr);
	await app.reload();
}

/** The tail's rows held, the ones on screen, the line's and where the body is. */
const tail = (page) =>
	page.evaluate(() => {
		const pane = document.querySelector('section.pane[data-title="tail"]');
		const body = pane.querySelector('.body:not(.behind .body)');
		const box = body.getBoundingClientRect();
		const head = pane.querySelector('thead')?.getBoundingClientRect().bottom ?? box.top;
		const trs = [...pane.querySelectorAll('tbody tr')];
		const onScreen = (tr) => {
			const r = tr.getBoundingClientRect();
			return r.top >= head - 1 && r.bottom <= box.bottom + 1;
		};
		return {
			inflight: window.__datumInflight,
			hids: trs.map((tr) => tr.dataset.hid),
			shown: trs.filter(onScreen).map((tr) => tr.dataset.hid),
			line: trs.filter((tr) => tr.classList.contains('line')).map((tr) => tr.dataset.hid),
			selected: trs.filter((tr) => tr.classList.contains('selected')).map((tr) => tr.dataset.hid),
			atBottom: body.scrollHeight - body.scrollTop - body.clientHeight <= 2
		};
	});

/** The pane at the live end: the newest row last and on screen, at the bottom. */
const atLiveEnd = (page) =>
	until(
		() => tail(page),
		(t) => t.inflight === 0 && t.hids.at(-1) === seedHid(COUNT - 1) && t.shown.includes(seedHid(COUNT - 1)) && t.atBottom
	);

describe('the tail follows the selection', () => {
	test('an entry selected is listed with the rows around it, and on screen; clearing it gives the live end back', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			const live = await atLiveEnd(page);
			assert.ok(!live.hids.includes(seedHid(50)), 'the entry is held already, which tests nothing');

			await page.evaluate((hid) => window.datum.bar.setPills([{ id: 'hid', kind: 'hid', text: hid }]), seedHid(50));
			const t = await until(
				() => tail(page),
				(t) => t.inflight === 0 && t.shown.includes(seedHid(50))
			);
			assert.deepEqual(t.selected, [seedHid(50)], 'the entry is not marked');
			const i = t.hids.indexOf(seedHid(50));
			assert.ok(i > 0 && i < t.hids.length - 1, `no rows both above and below it: ${t.hids[0]}…${t.hids.at(-1)}`);
			// The CLI's order, contiguous: the rows around it and none skipped.
			assert.deepEqual(
				t.hids.map(seedIndex),
				t.hids.map((_, k) => seedIndex(t.hids[0]) + k),
				'the rows around it are not the listing'
			);
			const shown = t.shown.map(seedIndex);
			assert.ok(shown[0] < 50 && shown.at(-1) > 50, `not in the middle of the pane: ${shown[0]}…${shown.at(-1)}`);

			// Scrolled to the bottom of what is held, the rows under it page in.
			const newest = seedIndex(t.hids.at(-1));
			await page.evaluate(() => {
				const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
				body.scrollTop = body.scrollHeight;
			});
			await until(
				() => tail(page),
				(t) => seedIndex(t.hids.at(-1)) > newest
			);

			await page.evaluate(() => window.datum.bar.setPills([]));
			await atLiveEnd(page);
		});
	});

	test('a time selected is listed around, with the dotted line over the first row after it', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			await atLiveEnd(page);
			// Half a minute past the 100th: the line falls between it and the 101st.
			await page.evaluate((at) => window.datum.cursor.point({ at: new Date(at), step: 1 }), +at(100) + 30_000);
			const t = await until(
				() => tail(page),
				(t) => t.inflight === 0 && t.line.length > 0 && t.shown.includes(seedHid(101))
			);
			assert.deepEqual(t.line, [seedHid(101)], 'the line is not over the first row after the time');
			assert.ok(t.shown.includes(seedHid(100)), 'the row before the time is not on screen');
			const line = await page.evaluate(
				() => getComputedStyle(document.querySelector('section.pane[data-title="tail"] tr.line td')).backgroundImage
			);
			assert.match(line, /rgb\(255, 255, 255\)/, `the line is not white: ${line}`);

			await page.evaluate(() => window.datum.cursor.set(null));
			const back = await atLiveEnd(page);
			assert.deepEqual(back.line, [], 'the line outlived the selection');
		});
	});

	test('a range selected shows only the rows inside it, until it is cleared', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			await atLiveEnd(page);
			// Ten minutes ending at the 220th: the 210th to the 220th, both edges in.
			await page.evaluate((at) => window.datum.cursor.point({ at: new Date(at), span: 10 }), +at(220));
			const want = Array.from({ length: 11 }, (_, k) => seedHid(210 + k));
			const t = await until(
				() => tail(page),
				(t) => t.inflight === 0 && t.hids.join() === want.join()
			);
			assert.deepEqual(t.line, [], 'a range draws no line');

			await page.evaluate(() => window.datum.cursor.set(null));
			await atLiveEnd(page);
		});
	});
});

// When the pane changes height — the keyboard opening — the tail keeps what
// the reader was looking at (da-vcdil, vividn: «if it's at "now", it should
// generally keep now visible … if there is a selected time visible … it
// should resize around the selected time»). The browser alone keeps the top.
describe('the tail resized keeps its place', () => {
	/** `main` as one tail pane, whose height is the page's: the stock tail is a strip of fixed height. */
	const ONLY_TAIL = JSON.stringify({ version: 1, name: 'main', root: widgetPane('only', 'tail') });
	/** The body's height, and where in it, as a share of it, the line and each row on screen start. */
	const place = (page) =>
		page.evaluate(() => {
			const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
			const box = body.getBoundingClientRect();
			const share = (y) => (y - box.top) / body.clientHeight;
			const line = body.querySelector('tr.line');
			const mid = [...body.querySelectorAll('tbody tr')].find((tr) => tr.getBoundingClientRect().bottom > box.top + body.clientHeight / 2);
			return {
				inflight: window.__datumInflight,
				height: body.clientHeight,
				line: line ? share(line.getBoundingClientRect().top) : null,
				mid: mid?.dataset.hid ?? null,
				shareOf: Object.fromEntries(
					[...body.querySelectorAll('tbody tr')]
						.map((tr) => [tr.dataset.hid, share(tr.getBoundingClientRect().top)])
						.filter(([, y]) => y >= 0 && y < 1)
				)
			};
		});

	/** Shrink the page by `by` px, and wait for the pane's new box to land and its listing to settle. */
	async function shrink(page, by) {
		const before = (await place(page)).height;
		const { width, height } = page.viewportSize();
		await page.setViewportSize({ width, height: height - by });
		await until(
			() => place(page),
			(p) => p.height < before - by / 4
		);
		// The box lands a settle after the resize (150 ms and more), and asks a new count.
		await new Promise((resolve) => setTimeout(resolve, 600));
		return until(
			() => place(page),
			(p) => p.inflight === 0
		);
	}

	test('at now, the newest row stays at the bottom', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			await atLiveEnd(page);
			await shrink(page, 400);
			const t = await tail(page);
			assert.ok(t.atBottom, 'not at the bottom');
			assert.ok(t.shown.includes(seedHid(COUNT - 1)), `the newest row is off screen: ${t.shown.at(0)}…${t.shown.at(-1)}`);
		}, ONLY_TAIL);
	});

	test('a selected time on screen keeps its share of the height', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			await atLiveEnd(page);
			await page.evaluate((at) => window.datum.cursor.point({ at: new Date(at), step: 1 }), +at(100) + 30_000);
			await until(
				() => tail(page),
				(t) => t.inflight === 0 && t.shown.includes(seedHid(101))
			);
			// A quarter down, not the middle: the middle row is kept anyway.
			const was = await page.evaluate(() => {
				const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
				const line = body.querySelector('tr.line').getBoundingClientRect().top - body.getBoundingClientRect().top;
				body.scrollTop += line - body.clientHeight / 4;
				return new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
			}).then(() => place(page));
			assert.ok(Math.abs(was.line - 0.25) < 0.05, `the line is not a quarter down: ${was.line}`);
			const now = await shrink(page, 400);
			assert.ok(now.line !== null && Math.abs(now.line - was.line) < 0.05, `the line moved from ${was.line} to ${now.line}`);
		}, ONLY_TAIL);
	});

	test('scrolled up with nothing selected, the middle row stays in the middle', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			await atLiveEnd(page);
			await page.evaluate(() => {
				const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
				body.scrollTop = (body.scrollHeight - body.clientHeight) / 2;
				return new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
			});
			const was = await place(page);
			const now = await shrink(page, 400);
			const share = now.shareOf[was.mid];
			assert.ok(share !== undefined && Math.abs(share - 0.5) < 0.1, `the middle row ${was.mid} is at ${share}`);
		}, ONLY_TAIL);
	});

	// Scrolled that far up, the page above is asked for, and lands a chunk a
	// task: a resize between two chunks once had the next put back the place
	// from before the resize, the top row, as on a loaded CI runner.
	test('resized while the page above lands, the middle row stays in the middle', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app);
			const held = (await atLiveEnd(page)).hids.length;
			await page.evaluate(() => {
				const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
				body.scrollTop = (body.scrollHeight - body.clientHeight) / 2;
			});
			await page.waitForFunction(
				(held) => document.querySelectorAll('section.pane[data-title="tail"] tbody tr').length > held,
				held
			);
			const was = await place(page);
			const now = await shrink(page, 400);
			const share = now.shareOf[was.mid];
			assert.ok(share !== undefined && Math.abs(share - 0.5) < 0.1, `the middle row ${was.mid} is at ${share}`);
		}, ONLY_TAIL);
	});
});
