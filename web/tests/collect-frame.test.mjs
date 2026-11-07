// Collect's now section frames the selection (da-xqae2). vividn, 2026-11-07:
// «if one entry is selected show the time in the middle (if moment or no dur).
// if it has a dur, or there are multiple entries selected, zoom this now
// section out to show all the selected data (or dur range) in the inner 80%
// (so 10% time margin on each side). Similarly when you have a time pill show
// that time exactly in the center, and with a dur pill also then zoom this now
// section out to see the context.»

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const STRIP = 'section.pane[data-title="collect"] [data-strip] svg.now-strip';
const iso = (t) => new Date(t).toISOString();
const at = (field, verb, y) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y, height: 0.05 })}'`;

// coffee: moments at 09:00 and 11:00; work: a block 09:30–10:00, its time its end.
const SEED = [
	at('work', 'start', 0.2),
	at('coffee', 'occur', 0.8),
	`moment coffee -t ${iso(todayAt(9))}`,
	`start work 30m -t ${iso(todayAt(10))}`,
	`moment coffee -t ${iso(todayAt(11))}`
];

/** Each entry's hid by field and time. */
async function seeded(app) {
	for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
	await app.reload();
	await app.page.locator(STRIP).first().waitFor();
	await app.idle();
	const rows = JSON.parse((await app.runLine('tail -n 10 --json')).stdout).rows;
	const hid = (field, t) => rows.find((r) => r.field === field && Date.parse(r.time) === t).hid;
	return { coffee9: hid('coffee', todayAt(9)), work: hid('work', todayAt(10)), coffee11: hid('coffee', todayAt(11)) };
}

/** `field`'s marks on the strip, the lane's group left out: each one's edges and middle as a fraction of the strip's width. */
const marks = (page, field) =>
	page.locator(STRIP).first().evaluate((svg, field) => {
		const box = svg.getBoundingClientRect();
		return [...svg.querySelectorAll(`[data-mark="${field}"]:not(g)`)].map((el) => {
			const r = el.getBoundingClientRect();
			const f = (x) => (x - box.left) / box.width;
			return { left: f(r.left), right: f(r.right), mid: f((r.left + r.right) / 2), px: box.width };
		});
	}, field);

/** Within a pixel of `want`, as a fraction of a strip `px` wide. */
const near = (got, want, px) => Math.abs(got - want) * px <= 1;

const selectEntries = (page, hids) => page.evaluate((text) => window.datum.bar.setPills([{ id: 'hid', kind: 'hid', text }]), hids.join(','));
const point = (page, sel) => page.evaluate(({ at, span }) => window.datum.cursor.point({ at: new Date(at), span }), sel);

const OPTS = { ...PHONE, home: ON_COLLECT, clock: todayAt(15) };

describe('collect frames the selection on its now strip', () => {
	test('one moment selected: its instant at the center', async () => {
		await withApp(async (app) => {
			const { coffee9 } = await seeded(app);
			await selectEntries(app.page, [coffee9]);
			await until(
				() => marks(app.page, 'coffee'),
				(ms) => ms.length === 1 && near(ms[0].mid, 0.5, ms[0].px)
			);
		}, OPTS);
	});

	test('one entry 30m long: its start at 10% of the width, its end at 90%', async () => {
		await withApp(async (app) => {
			const { work } = await seeded(app);
			await selectEntries(app.page, [work]);
			await until(
				() => marks(app.page, 'work'),
				(ms) => ms.length === 1 && near(ms[0].left, 0.1, ms[0].px) && near(ms[0].right, 0.9, ms[0].px)
			);
		}, OPTS);
	});

	test('three entries: the earliest start at 10%, the latest end at 90%', async () => {
		await withApp(async (app) => {
			const { coffee9, work, coffee11 } = await seeded(app);
			await selectEntries(app.page, [work, coffee11, coffee9]);
			// 09:00 to 11:00 in the inner 80%: work's 09:30–10:00 is 30% to 50%.
			await until(
				() => Promise.all([marks(app.page, 'coffee'), marks(app.page, 'work')]),
				([cs, [w]]) => {
					const mids = cs.map((c) => c.mid).sort((a, b) => a - b);
					return mids.length === 2 && near(mids[0], 0.1, cs[0].px) && near(mids[1], 0.9, cs[0].px) && !!w && near(w.left, 0.3, w.px) && near(w.right, 0.5, w.px);
				}
			);
		}, OPTS);
	});

	test('a time pill: that instant at the center', async () => {
		await withApp(async (app) => {
			await seeded(app);
			await point(app.page, { at: todayAt(11) });
			await until(
				() => marks(app.page, 'coffee'),
				(ms) => ms.some((m) => near(m.mid, 0.5, m.px))
			);
		}, OPTS);
	});

	test('a time pill with a dur pill: the span at 10% and 90%', async () => {
		await withApp(async (app) => {
			await seeded(app);
			// 10:00 back 30 minutes: work's block exactly.
			await point(app.page, { at: todayAt(10), span: 30 });
			await until(
				() => marks(app.page, 'work'),
				(ms) => ms.length === 1 && near(ms[0].left, 0.1, ms[0].px) && near(ms[0].right, 0.9, ms[0].px)
			);
		}, OPTS);
	});
});
