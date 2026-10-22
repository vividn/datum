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
