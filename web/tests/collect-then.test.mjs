// Collect at a selected time (da-0qdsp). vividn, 2026-11-04: «if a time is
// selected show the active state at the selected time both in the graphical
// render and in the active field outline. if the selected time is exactly when
// a moment dot occurs show the moment dot in the center of the render and the
// moment field in the active state box, tapping that field should toggle it in
// that case, so i guess the moment field should be some how filled in a bit
// more».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const ROOM = 'section.pane[data-title="collect"] .bar .collect .room';
const AT = { ...PHONE, home: ON_COLLECT, clock: todayAt(15) };
const iso = (t) => new Date(t).toISOString();

/** A field with its band set, as a spec sets it. */
const at = (field, verb, y) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y, height: 0.05 })}'`;

// work ran 09:00–10:00 and is off now; coffee is a moment at 09:30.
const SEED = [at('work', 'start', 0.2), at('coffee', 'occur', 0.8), `start work -t ${iso(todayAt(9))}`, `end work -t ${iso(todayAt(10))}`, `moment coffee -t ${iso(todayAt(9, 30))}`];

async function seeded(app) {
	for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
	await app.reload();
	await app.page.locator(`${ROOM} .spot.shelf`).first().waitFor();
	await app.idle();
}

/** Each field's section at rest. */
const sections = (page) =>
	page.locator(`${ROOM} .spot.shelf`).evaluateAll((spots) => Object.fromEntries(spots.map((s) => [s.querySelector('[data-field]').dataset.field, s.dataset.section])));

/** `field`'s marks on the strip: where each one's middle is, and the strip's width. */
const marks = (page, field) =>
	page.locator(`${ROOM} svg.now-strip`).evaluate((svg, field) => {
		const box = svg.getBoundingClientRect();
		return [...svg.querySelectorAll(`[data-mark="${field}"]`)].map((el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left - box.left, right: r.right - box.left, mid: (r.left + r.right) / 2 - box.left, of: box.width };
		});
	}, field);

/** coffee's entries, by when. */
const coffees = async (app) => {
	const { code, stdout } = await app.runLine('tail coffee --json');
	return code === 0 ? JSON.parse(stdout).rows.map((r) => Date.parse(r.time)) : [];
};

const point = (page, t) => page.evaluate((t) => window.datum.cursor.set(new Date(t)), t);

describe('collect at a selected time', () => {
	test('inside a block of a field off now: the strip and the active section show it on', async () => {
		await withApp(async (app) => {
			await seeded(app);
			const { page } = app;
			assert.equal((await sections(page)).work, 'recent', 'work is off now');
			await point(page, todayAt(9, 15));
			await until(() => sections(page), (s) => s.work === 'active' && s.coffee === 'recent');
			await until(
				() => marks(page, 'work'),
				(ms) => ms.some((m) => m.left <= 0.5 && m.right >= m.of - 0.5)
			);
			// Cleared, it is now again.
			await point(page, null);
			await until(() => sections(page), (s) => s.work === 'recent');
		}, AT);
	});

	test('a selected instant moved at every step asks `now -t` once it rests, for where it rests (da-63yb6)', async () => {
		await withApp(async (app) => {
			await seeded(app);
			const { page } = app;
			const seen = await app.spyOnRunner();
			// A drag's steps, a frame apart: 10:30 back to 09:15, five minutes a step.
			const steps = Array.from({ length: 16 }, (_, i) => todayAt(10, 30) - i * 5 * 60_000);
			const gaps = await page.evaluate(async (steps) => {
				const gaps = [];
				let last = performance.now();
				for (const t of steps) {
					window.datum.cursor.set(new Date(t));
					await new Promise((r) => setTimeout(r, 16));
					gaps.push(performance.now() - last);
					last = performance.now();
				}
				return gaps;
			}, steps);
			await until(() => sections(page), (s) => s.work === 'active');
			await app.idle();
			const asked = (await seen()).filter((argv) => argv[0] === 'now' && argv.includes('-t')).map((argv) => argv[argv.indexOf('-t') + 1]);
			// A step the box held past the settle is a rest of its own.
			const rests = gaps.filter((g) => g >= 150).length;
			assert.ok(asked.length >= 1 && asked.length <= rests + 1, `asked ${asked.length} times over ${steps.length} steps, ${rests} rests: ${asked}`);
			assert.equal(asked.at(-1), iso(steps.at(-1)), 'asked for where it rests');
		}, AT);
	});

	test("at a moment's instant: its dot is centered, its field is in the active section filled, and a tap takes it away and logs it back", async () => {
		await withApp(async (app) => {
			await seeded(app);
			const { page } = app;
			const coffee = page.locator(`${ROOM} [data-field="coffee"]`);
			const unfilled = await coffee.evaluate((b) => getComputedStyle(b).backgroundColor);
			await point(page, todayAt(9, 30));
			await until(() => sections(page), (s) => s.coffee === 'active' && s.work === 'active');
			await until(
				() => marks(page, 'coffee'),
				(ms) => ms.length > 0 && ms.every((m) => Math.abs(m.mid - m.of / 2) < 1)
			);
			assert.notEqual(await coffee.evaluate((b) => getComputedStyle(b).backgroundColor), unfilled, 'filled a little');

			await coffee.click();
			await until(() => coffees(app), (ts) => ts.length === 0);
			await until(() => sections(page), (s) => s.coffee === 'recent');
			assert.equal(await page.evaluate(() => window.datum.cursor.at()?.toISOString()), iso(todayAt(9, 30)), 'the time stays selected');

			await coffee.click();
			await until(() => coffees(app), (ts) => ts.length === 1 && ts[0] === todayAt(9, 30));
		}, AT);
	});
});
