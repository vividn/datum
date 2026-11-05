// The tail pane's now (da-cizxo, vividn: «right now there is an extra line of
// space at the bottom of the tail widget. let's remove it / draw a yellow
// dotted line where now is in the tail view (so at the very bottom if all
// entries are in the past) / if there are entries in the future, show up to
// 3 future entries and then 2.5 entries if there are more than 3 future
// entries … if now is ever not on the screen show a little yellow [now ↓]
// or … [now ↑] button, that resets the view to its default»).
//
// Every row is the CLI's, named by hid; what is asserted is where the line
// is among them and where the rows stand in the pane's body.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, sharedApp, startApp, stopApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const APP = sharedApp({ viewport: { width: 1280, height: 1200 } });
/** The same, with a touchscreen: a finger by CDP (da-c3oqn). */
const TOUCH = sharedApp({ viewport: { width: 1280, height: 1200 }, device: { hasTouch: true } });
const BODY = 'section.pane[data-title="tail"] .body:not(.behind .body)';

const MINUTE = 60_000;
const hidOf = (i) => 'seed' + i.toString().padStart(8, '0');

/**
 * `past` entries one a minute up to a minute before now, then `future`
 * entries ten minutes apart from ten minutes after it, restored in one
 * command as tailwatch.test.mjs seeds. `hidOf(i)` is the i-th oldest, so
 * `hidOf(past)` is the first after now.
 */
async function seed(app, past, future) {
	const times = [
		...Array.from({ length: past }, (_, i) => HELD_AT - (past - i) * MINUTE),
		...Array.from({ length: future }, (_, j) => HELD_AT + (j + 1) * 10 * MINUTE)
	].map((ms) => new Date(ms).toISOString());
	const backup = await app.run(['db', 'backup', '-']);
	assert.equal(backup.code, 0, backup.stderr);
	const header = JSON.parse(backup.stdout.split('\n')[0]);
	const lines = [JSON.stringify({ ...header, backup_time: new Date(HELD_AT).toISOString(), source_db: 'seed', entry_count: times.length })];
	times.forEach((t, i) =>
		lines.push(
			JSON.stringify({
				hid: hidOf(i),
				slug: `seed:${t}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field: 'seed',
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: null,
				at: { utc: t, offset_minutes: 0, timezone: 'UTC' },
				create_time: t,
				modify_time: t,
				client_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: '%at%',
				raw_input: 'occur seed',
				parser_version: 2
			})
		)
	);
	const result = await app.run(['db', 'restore', '-', '--merge'], lines.join('\n') + '\n');
	assert.equal(result.code, 0, result.stderr);
	await app.reload();
}

/** The tail's rows, where each is on screen, which the now line is over, and the button. */
const tail = (page) =>
	page.evaluate(() => {
		const pane = document.querySelector('section.pane[data-title="tail"]');
		const body = pane.querySelector('.body:not(.behind .body)');
		const box = body.getBoundingClientRect();
		const trs = [...pane.querySelectorAll('tbody tr')];
		return {
			inflight: window.__datumInflight,
			hids: trs.map((tr) => tr.dataset.hid),
			rows: trs.map((tr) => {
				const r = tr.getBoundingClientRect();
				return { top: r.top, bottom: r.bottom };
			}),
			now: trs.findIndex((tr) => tr.classList.contains('now')),
			under: trs.findIndex((tr) => tr.classList.contains('now-under')),
			marks: trs.filter((tr) => tr.matches('.now, .now-under')).length,
			head: pane.querySelector('thead').getBoundingClientRect().bottom,
			bottom: box.top + body.clientTop + body.clientHeight,
			button: pane.querySelector('button.home')?.textContent ?? null
		};
	});

/** Row `i` whole on screen: under the header and over the body's bottom. */
const whole = (t, i) => t.rows[i].top >= t.head - 1 && t.rows[i].bottom <= t.bottom + 1;

/** The default view with the first future row `first`, of `future` (da-cizxo). */
function home(t, first, future) {
	if (t.inflight !== 0 || t.hids.indexOf(hidOf(first)) < 0 || t.button !== null) return false;
	const k = t.hids.indexOf(hidOf(first));
	if (future === 0) return t.under === t.hids.length - 1 && t.now === -1 && Math.abs(t.rows.at(-1).bottom - t.bottom) <= 1;
	if (t.now !== k || !whole(t, k - 1)) return false;
	if (future <= 3) return Math.abs(t.rows.at(-1).bottom - t.bottom) <= 1 && t.hids.length - k === future;
	const third = t.rows[k + 2];
	return whole(t, k) && whole(t, k + 1) && Math.abs((third.top + third.bottom) / 2 - t.bottom) <= 1;
}

const atHome = (page, first, future) => until(() => tail(page), (t) => home(t, first, future));

const scrollBody = (page, to) =>
	page.evaluate((to) => {
		const body = document.querySelector('section.pane[data-title="tail"] .body:not(.behind .body)');
		body.scrollTop = to === 'bottom' ? body.scrollHeight : to;
	}, to);

describe('the tail pane at now', () => {
	test('every row past: the line under the last, which ends at the body’s bottom edge', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 0);
			const t = await atHome(page, 0, 0);
			assert.equal(t.marks, 1, 'more than one now line');
			assert.ok(t.rows.at(-1).bottom <= t.bottom + 0.5, 'the last row runs past the bottom');
			assert.ok(t.bottom - t.rows.at(-1).bottom < 1, `${t.bottom - t.rows.at(-1).bottom}px under the last row`);
		});
	});

	test('two rows after now: both shown, under the line', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 2);
			const t = await atHome(page, 40, 2);
			assert.ok(whole(t, t.now) && whole(t, t.now + 1), 'a future row is not whole on screen');
		});
	});

	test('five rows after now: two and a half shown, the third cut through its middle', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 5);
			const t = await atHome(page, 40, 5);
			assert.ok(t.rows[t.now + 3].top >= t.bottom - 1, 'the fourth future row is on screen');
		});
	});

	test('scrolled up past now, `now ↓` puts the default view back', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 60, 5);
			await atHome(page, 60, 5);
			await scrollBody(page, 0);
			await until(() => tail(page), (t) => t.button === 'now ↓');
			await page.locator('section.pane[data-title="tail"] button.home').click();
			await atHome(page, 60, 5);
		});
	});

	test('scrolled down a long future, `now ↑` puts the default view back', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 30);
			await atHome(page, 40, 30);
			await scrollBody(page, 'bottom');
			await until(() => tail(page), (t) => t.button === 'now ↑');
			await page.locator('section.pane[data-title="tail"] button.home').click();
			await atHome(page, 40, 30);
		});
	});

	test('more rows after now than a listing holds: the rows around now', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 20, 150);
			await atHome(page, 20, 150);
		});
	});

	test('the line moves as the clock passes a row', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 5);
			const before = await atHome(page, 40, 5);
			await app.clock.forward(11 * MINUTE);
			const after = await until(() => tail(page), (t) => t.now !== before.now);
			assert.equal(after.hids[after.now], hidOf(41), 'the line is not under the row the clock passed');
		});
	});
});

// vividn, 2026-11-05: «let's make the yellow dotted line thinner. Also there
// are some odd breaks in it where the dot pattern is irregular, please fix
// that. When trying to scroll down past the bottom (or the top if you've
// reached the very beginning of the data) add a little elastic effect like
// in other places in the app» (da-c3oqn).
describe('the tail pane’s now line and its ends', () => {
	test('the now line is 1px high, its dots one steady 6px pattern across the row’s cells', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 0);
			await atHome(page, 0, 0);
			const row = page.locator('section.pane[data-title="tail"] tbody tr.now-under');
			const style = await row.evaluate((tr) => ({
				size: getComputedStyle(tr).backgroundSize,
				cells: [...tr.cells].map((td) => getComputedStyle(td).backgroundImage),
				edges: [...tr.cells].map((td) => td.getBoundingClientRect().left)
			}));
			assert.match(style.size, /^6px 1px\b/, `the now line is not 6px a period and 1px high: ${style.size}`);
			assert.deepEqual([...new Set(style.cells)], ['none'], 'a cell paints a pattern of its own');
			// The row's bottom pixels as drawn: a column is a dot where yellow shows in any of them.
			const png = await row.screenshot();
			const dots = await page.evaluate(async (b64) => {
				const img = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
				const ctx = new OffscreenCanvas(img.width, img.height).getContext('2d');
				ctx.drawImage(img, 0, 0);
				const h = Math.min(3, img.height);
				const px = ctx.getImageData(0, img.height - h, img.width, h).data;
				const yellow = (x) => {
					for (let y = 0; y < h; y++) {
						const i = (y * img.width + x) * 4;
						if (px[i] - px[i + 2] > 60 && px[i + 1] - px[i + 2] > 40) return true;
					}
					return false;
				};
				const starts = [];
				for (let x = 0; x < img.width; x++) if (yellow(x) && !yellow(x - 1)) starts.push(x);
				return { starts, width: img.width };
			}, png.toString('base64'));
			const box = await row.boundingBox();
			assert.ok(style.edges.length > 2, 'the row has no cell boundary to cross');
			assert.ok(dots.starts.length >= Math.floor(dots.width / 6) - 1, `${dots.starts.length} dots over ${dots.width}px`);
			const off = dots.starts.filter((x) => (x - dots.starts[0]) % 6 !== 0);
			assert.deepEqual(
				off,
				[],
				`dots off the row's 6px period, at ${off.join(', ')}; cells start at ${style.edges.map((e) => Math.round(e - box.x)).join(', ')}`
			);
		});
	});

	/** Where the first row stands in the body's content, so a scroll moves nothing; its transform; whether it is the log's start. */
	const rest = (page) =>
		page.evaluate((body) => {
			const el = document.querySelector(body);
			const tbody = el.querySelector('tbody');
			return {
				top: tbody.rows[0].getBoundingClientRect().top + el.scrollTop,
				transform: tbody.style.transform,
				start: tbody.querySelector('tr.start') !== null
			};
		}, BODY);

	/** A finger by CDP from the body's middle, `dy` px in steps; `during` reads before the lift. */
	async function pull(page, dy, during) {
		const box = await page.locator(BODY).boundingBox();
		const x = box.x + box.width / 2;
		const y = box.y + box.height / 2;
		const cdp = await page.context().newCDPSession(page);
		const touch = (type, at) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: at ? [at] : [] });
		await touch('touchStart', { x, y });
		for (let i = 1; i <= 10; i++) await touch('touchMove', { x, y: y + (dy * i) / 10 });
		const held = await during();
		await touch('touchEnd');
		await cdp.detach();
		return held;
	}

	const springsBack = (page, at) =>
		until(
			() => rest(page),
			(r) => r.transform === '' && Math.abs(r.top - at.top) < 0.5
		);

	test('a finger past the newest row, and past the log’s start: the rows give less than the drag, and spring back', async () => {
		await TOUCH.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 0);
			await atHome(page, 0, 0);
			// Every row fits, but the log's start is known only once asked for:
			// the first pull at the top asks, and gives nothing.
			const first = await pull(page, 150, () => rest(page));
			assert.equal(first.transform, '', 'the rows gave before the log’s start was reached');
			await page.locator('section.pane[data-title="tail"] tbody tr.start').waitFor({ state: 'attached' });
			for (const dy of [-150, 150]) {
				await scrollBody(page, dy < 0 ? 'bottom' : 0);
				const at = await rest(page);
				const held = await pull(page, dy, () => rest(page));
				const moved = held.top - at.top;
				assert.ok(Math.sign(moved) === Math.sign(dy) && Math.abs(moved) > 4, `a ${dy}px drag moved the rows ${moved}px`);
				assert.ok(Math.abs(moved) < Math.abs(dy) / 2, `a ${dy}px drag moved the rows ${moved}px, with no resistance`);
				await springsBack(page, at);
			}
		});
	});

	test('a wheel past the newest row gives, and springs back', async () => {
		await APP.solo(async (app) => {
			const { page } = app;
			await seed(app, 40, 0);
			await atHome(page, 0, 0);
			await scrollBody(page, 'bottom');
			const before = await rest(page);
			// Every transform the rows are given, as it is given: the spring back is a timer's.
			await page.evaluate((body) => {
				window.__given = [];
				new MutationObserver((changes) => {
					for (const { target } of changes) if (target.localName === 'tbody') window.__given.push(target.style.transform);
				}).observe(document.querySelector(body), { subtree: true, attributeFilter: ['style'] });
			}, BODY);
			// On a row, as a wheel is: Playwright's own wheel reached no listener here.
			await page.evaluate(
				(body) => document.querySelector(`${body} tbody td`).dispatchEvent(new WheelEvent('wheel', { deltaY: 200, bubbles: true })),
				BODY
			);
			await springsBack(page, before);
			const given = await page.evaluate(() => window.__given);
			const moved = Math.min(0, ...given.map((t) => Number(/translateY\((-?[\d.]+)px\)/.exec(t)?.[1] ?? 0)));
			assert.ok(moved < -4 && moved > -100, `a 200px wheel moves the rows ${moved}px (${given.join(' ')})`);
		});
	});

	test('short of the log’s start, a finger pulling at the top gives nothing', async () => {
		await TOUCH.solo(async (app) => {
			const { page } = app;
			await seed(app, 2000, 0);
			await atHome(page, 1999, 0);
			await scrollBody(page, 0);
			const held = await pull(page, 150, () => rest(page));
			assert.equal(held.start, false, 'the log’s start was reached before the pull');
			assert.equal(held.transform, '', 'the rows gave short of the log’s start');
		});
	});
});
