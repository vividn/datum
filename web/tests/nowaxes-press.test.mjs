// The nowview's axes: a press or a scrub snaps to a mark, a dot or a state change.
// Its story and helpers are support/nowaxes.mjs; one of four files, so each stays
// well inside test.mjs's 300 s file cap (da-9utsv).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { finger } from './support/press.mjs';
import { until } from './support/until.mjs';
import { bothWays, NOW, NOWVIEW, withAxes, read, clear, pt, pillsOf, timePill, drewPill, hhmm } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

bothWays((vertical) => {
	test('a press just off an axis’s strip is the axis’s, and on a mark there it is the mark’s', async () => {
		await withAxes(vertical, async ({ page }) => {
			const g = await read(page);
			const [a, b] = g.clock.slice(1);
			const line = await page.locator(`${NOWVIEW} svg.svg path.clock-axis`).boundingBox();
			const across = vertical ? line.x - 6 : line.y + line.height + 6;
			await finger(page, pt(g, (a.p + b.p) / 2, across));
			await timePill(page, (t) => t.startsWith('-t '));
		});
	});

	test('a scrub on the clock axis is -t HH:MM, and on the relative axis a relative -t', async () => {
		await withAxes(vertical, async ({ page }) => {
			let g = await read(page);
			// Between two clock marks, clear of the handles.
			const [a, b] = g.clock.slice(1);
			const [from, to] = [a.p + (b.p - a.p) * 0.3, a.p + (b.p - a.p) * 0.6];
			assert.ok(clear(g, from) && clear(g, to), 'a handle is in the way');
			await finger(page, pt(g, from, g.absRow), pt(g, to, g.absRow));
			await timePill(page, (t) => /^-t \d\d:\d\d$/.test(t));
			// With none, and with a relative one: a relative time, and no length.
			await page.locator('.prompt-bar .pill.time button.remove').click();
			g = await read(page);
			// The second from the relative pill, which a still press would dismiss.
			for (const [lo, hi] of [[from, to], [to, from]]) {
				await finger(page, pt(g, lo, g.relRow), pt(g, hi, g.relRow));
				await timePill(page, (t) => /^(-q+|-t -\d+(m\d+s|s)?)$/.test(t));
				assert.ok(!(await pillsOf(page)).some((p) => p.startsWith('dur:')), 'a relative press took a length');
			}
		});
	});

	// da-f4sto, vividn 2026-11-03: «when an absolute time is selected and
	// you press in the relative time axis it adds a dur automatically».
	test('with an absolute pill, a press on the relative axis before it keeps it and adds the length back to it, on the relative axis', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			const T = Date.UTC(2026, 8, 10, 15, 0);
			await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), T);
			await drewPill(app);
			const g = await read(page);
			const [a, b] = g.clock;
			const at = (t) => pt(g, a.p + ((t - a.at) * (b.p - a.p)) / (b.at - a.at), g.relRow);
			const [ten, quarter] = [T - 10 * 60_000, T - 25 * 60_000];
			assert.ok(clear(g, vertical ? at(quarter).y : at(quarter).x), 'a handle is in the way');
			const lengthIs = async (want) => {
				const said = await until(() => pillsOf(page), (p) => p.includes(`dur:${want}`));
				assert.deepEqual(said.filter((p) => /^(time|dur):/.test(p)), ['time:-t 15:00', `dur:${want}`]);
			};
			await finger(page, at(ten));
			await lengthIs('10m');
			await finger(page, at(ten), at(quarter));
			await lengthIs('25m');
			await page.locator(`${NOWVIEW} svg.svg .cursor-span`).waitFor({ state: 'attached' });
			await app.idle();
			// Each pill on its axis's strip: the length on the relative one.
			const across = await page.locator(`${NOWVIEW} .axis-pill`).evaluateAll(
				(els, y) =>
					els.map((e) => {
						const r = e.getBoundingClientRect();
						return [e.querySelector('.pill').classList.contains('dur') ? 'dur' : 'time', y ? r.left + r.width / 2 : r.top + r.height / 2];
					}),
				vertical
			);
			const of = Object.fromEntries(across);
			assert.ok(Math.abs(of.dur - g.relRow) <= 2, `the length pill is at ${of.dur}, the relative axis at ${g.relRow}`);
			assert.ok(Math.abs(of.time - g.absRow) <= 2, `the time pill is at ${of.time}, the clock axis at ${g.absRow}`);
		});
	});

	// da-vfxgp, the mirror of da-f4sto, vividn 2026-11-03: «rel first then
	// abs -> hmmm spawn a dur pill down on the rel axis I suppose?».
	test('with a relative pill, a press on the clock axis before it keeps it and adds the length back to it, on the relative axis', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.cursor.point({ rel: 10, from: 'nowview' }));
			await drewPill(app);
			const g = await read(page);
			const [a, b] = g.clock;
			const at = (t) => pt(g, a.p + ((t - a.at) * (b.p - a.p)) / (b.at - a.at), g.absRow);
			// On clock marks, whose time a press near takes whole, before the
			// pill's own instant, which a relative pill reads off the minute's clock.
			const P = await page.evaluate(() => window.datum.cursor.at().getTime());
			const marks = g.clock.filter((m) => m.at < P - 60_000 && m.at > P - 60 * 60_000 && clear(g, m.p)).sort((x, y) => y.at - x.at);
			assert.ok(marks.length >= 2, `no two clock marks before the pill: ${JSON.stringify(g.clock)}`);
			const [ten, twenty, later] = [marks[0].at, marks[1].at, P + 5 * 60_000];
			const [m1, m2] = [ten, twenty].map((t) => `${(P - t) / 60_000}m`);
			assert.ok(clear(g, vertical ? at(later).y : at(later).x), 'a handle is in the way');
			const timed = (p) => p.filter((x) => /^(time|dur):/.test(x));
			const lengthIs = async (want) => {
				const said = await until(() => pillsOf(page), (p) => p.includes(`dur:${want}`));
				assert.deepEqual(timed(said), ['time:-qq', `dur:${want}`]);
			};
			await finger(page, at(ten));
			await lengthIs(m1);
			await finger(page, at(ten), at(twenty));
			await lengthIs(m2);
			await page.locator(`${NOWVIEW} svg.svg .cursor-span`).waitFor({ state: 'attached' });
			await app.idle();
			const dur = await page.locator(`${NOWVIEW} .axis-pill:has(.pill.dur)`).boundingBox();
			const across = vertical ? dur.x + dur.width / 2 : dur.y + dur.height / 2;
			assert.ok(Math.abs(across - g.relRow) <= 2, `the length pill is at ${across}, the relative axis at ${g.relRow}`);
			// Later than the pill, not decided yet (dur_forward): an absolute time, as before.
			await finger(page, at(later));
			const said = await until(() => pillsOf(page), (p) => timed(p).some((x) => /^time:-t \d\d:\d\d$/.test(x)));
			assert.equal(timed(said).length, 1, `a later press kept a length: ${said}`);
		});
	});

	test('a press near a mark of either axis takes it: the relative one’s relative, the clock’s absolute', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			let g = await read(page);
			assert.notEqual(g.empty, null, 'no room across the band clear of every mark');
			// Off a mark by as much as keeps it the nearest, up to 5px: a mark of
			// the other axis on it leaves it none.
			const reach = (g, p) => {
				const others = [...g.grid, ...g.clock.map((m) => m.p)];
				others.splice(others.indexOf(p), 1);
				return Math.min(5, Math.min(...others.map((q) => Math.abs(q - p))) / 2 - 0.5);
			};

			// The relative gridline ten minutes back, which says -qq.
			const [a, b] = g.clock;
			const back10 = a.p + ((NOW - 10 * 60_000 - a.at) * (b.p - a.p)) / (b.at - a.at);
			const grid = g.grid.reduce((x, y) => (Math.abs(y - back10) < Math.abs(x - back10) ? y : x));
			assert.ok(Math.abs(grid - back10) < 2, `no gridline at −10m: ${grid} ${back10}`);
			assert.ok(reach(g, grid) >= 1.5, `the −10m gridline has a mark ${reach(g, grid)}px off`);
			await finger(page, pt(g, grid + reach(g, grid), g.empty));
			await timePill(page, '-qq');

			// And the clock mark with the most room around it, back on now.
			await drewPill(app);
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'detached' });
			await app.idle();
			g = await read(page);
			const best = g.clock.map((m) => ({ ...m, r: reach(g, m.p) })).sort((x, y) => y.r - x.r)[0];
			assert.ok(best.r >= 1.5, `every clock mark has another mark beside it: ${JSON.stringify(g)}`);
			await finger(page, pt(g, best.p - best.r, g.empty));
			await timePill(page, `-t ${hhmm(best.at)}`);
		});
	});

	for (const [what, key] of [
		['an occur dot', 'dot'],
		['a state change', 'change']
	])
		test(`a long press near ${what} takes its instant, absolute`, async () => {
			await withAxes(vertical, async ({ page }) => {
				const g = await read(page);
				assert.ok(Math.abs(g.drawn[key] - g.pos[key]) < 1.5, `${what} is drawn at ${g.drawn[key]}, its instant at ${g.pos[key]}`);
				await finger(page, pt(g, g.pos[key] + 6, g.empty), undefined, { hold: true });
				await timePill(page, `-t ${hhmm(g.instants[key])}`);
			});
		});

	test('a scrub of the clock axis stops on the dot within a few px, and not past them', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			// A pill first, so the picture under the scrub is the one a pill draws.
			let g = await read(page);
			assert.ok(clear(g, g.pos.dot - 36) && clear(g, g.pos.dot + 9), 'a handle is in the way');
			await finger(page, pt(g, g.pos.dot - 36, g.absRow));
			await timePill(page, (t) => t.startsWith('-t '));
			await drewPill(app);
			g = await read(page);
			const exact = `-t ${hhmm(g.instants.dot)}`;
			await finger(page, pt(g, g.pos.dot - 30, g.absRow), pt(g, g.pos.dot + 2, g.absRow));
			await timePill(page, exact);
			await finger(page, pt(g, g.pos.dot - 30, g.absRow), pt(g, g.pos.dot + 9, g.absRow));
			const said = await timePill(page, (t) => t !== exact && /^-t \d\d:\d\d$/.test(t));
			assert.notEqual(said, exact, 'the scrub stopped on the dot from 9px away');
		});
	});
});
