// One data layer by instant, shared by the day strip and the nowview (da-m6i2a).
//
// vividn, 2026-10-27: «we could still do cache by day, just batch the data
// layer write when scrolling up, so generate 2 weeks at a time of data and cut
// it into days after generation». Both widgets draw a day from the same
// `dayview --data-layer` picture, held by #lib/dayLayers.ts: so a day either
// asks is asked once for both, and a day the clock changed in is drawn by
// instant in the nowview and as the CLI lived it in the strip.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt, ON_COLLECT } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';

before(startApp);
after(stopApp);

const NOWVIEW = 'section[data-pane="nowview"] .nowview-band';
const band = (end) => `.band[data-days-ago="${end}"] svg.svg`;

/** The dates a data layer run draws (`--pictures`) or checks (`--digests`), by its `--end-date`. */
function layerDates(argv, kind = '--pictures') {
	if (argv[0] !== 'dayview' || !argv.includes('--data-layer') || !argv.includes(kind)) return [];
	const flag = (name) => argv[argv.indexOf(name) + 1];
	const list = flag(kind);
	const end = Date.parse(flag('--end-date'));
	return list.split(',').map((back) => new Date(end - Number(back) * 86_400_000).toISOString().slice(0, 10));
}

test('the strip and the nowview draw a day from one data layer, asked once for both', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.idle();
			const seen = await app.spyOnRunner();
			// Ten minutes past midnight: both widgets draw yesterday's end and today.
			await seedLines(app, ['switch mood calm -t -20', 'occur coffee -t -5']);
			for (const at of [`${band(1)} rect.block[data-state="calm"]`, `${band(0)} [data-field="coffee"]`, `${NOWVIEW} svg.svg rect.block[data-state="calm"]`, `${NOWVIEW} svg.svg [data-field="coffee"]`])
				await page.locator(at).first().waitFor({ state: 'attached' });
			await app.idle();

			// Each day drawn once, and checked at most once, for both widgets.
			const runs = await seen();
			const [dates, checked] = ['--pictures', '--digests'].map((kind) => runs.flatMap((argv) => layerDates(argv, kind)));
			for (const list of [dates, checked])
				assert.deepEqual(list.filter((date, i) => list.indexOf(date) !== i), [], `a day asked twice: ${dates} | ${checked}`);
			const [today, yesterday] = [0, 1].map((back) => new Date(todayAt(0) - back * 86_400_000).toISOString().slice(0, 10));
			assert.ok(dates.includes(today) && dates.includes(yesterday), `not today and yesterday asked: ${dates}`);
			assert.deepEqual(app.errors, []);
		},
		{ ...DESKTOP, home: ON_COLLECT, clock: todayAt(0, 10) }
	);
});

// 2026-10-25 in Berlin: the clock went back from 03:00 to 02:00 at 01:00 UTC.
const FALL_BACK = { ...DESKTOP, home: ON_COLLECT, timezone: 'Europe/Berlin', clock: Date.UTC(2026, 9, 25, 1, 10) };

test('a day the clock went back in: the nowview by instant, the strip as it was lived', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// Ten minutes either side of the change, as a Berlin clock read them.
			for (const line of ['switch mood calm -t 2026-10-25T02:45:00+02:00', 'occur coffee -t 2026-10-25T02:55:00+02:00', 'occur tea -t 2026-10-25T02:05:00+01:00'])
				assert.equal((await app.runLine(line)).code, 0, line);
			await app.redraw();
			await page.locator(`${NOWVIEW} svg.svg [data-field="tea"][data-at]`).first().waitFor({ state: 'attached' });
			await page.locator(`${band(0)} [data-field="tea"]`).first().waitFor({ state: 'attached' });
			await app.idle();

			// The nowview: ten minutes apart is ten minutes apart across the
			// change, read off the overlay's own axis, and the state is one
			// block across it, a lane's full height.
			const now = await page.locator(NOWVIEW).evaluate((band) => {
				const svg = band.querySelector('svg.svg');
				const x = (el) => {
					const r = el.getBoundingClientRect();
					return (r.left + r.right) / 2;
				};
				const axis = [...svg.querySelectorAll('[data-lo]')].find((g) => g.closest('.plot') && !g.closest('.crop'));
				const ctm = axis.getScreenCTM();
				const at = (name) => new DOMPoint(Number(axis.getAttribute(name)), 0).matrixTransform(ctm).x;
				const msPerPx = (Date.parse(axis.getAttribute('data-to')) - Date.parse(axis.getAttribute('data-from'))) / (at('data-hi') - at('data-lo'));
				const dot = (field) => x(svg.querySelector(`[data-field="${field}"].point-fg, [data-field="${field}"] .point-fg`));
				const calm = [...svg.querySelectorAll('.block[data-state="calm"]')];
				return { apart: (dot('tea') - dot('coffee')) * msPerPx, calm: calm.map((b) => b.localName), heights: calm.map((b) => Math.round(b.getBoundingClientRect().height)) };
			});
			assert.ok(Math.abs(now.apart - 600_000) < 30_000, `coffee and tea ${now.apart / 60_000} minutes apart`);
			assert.deepEqual([...new Set(now.calm)], ['rect'], `the state is drawn as ${now.calm}`);
			assert.equal(new Set(now.heights).size, 1, `the state narrows: ${now.heights}`);

			// The strip: today is the CLI's own picture, a stretch a clock,
			// the repeated hour in lanes; yesterday, one clock, the layer.
			const strip = await page.evaluate(
				([today, yesterday]) => ({
					stretches: document.querySelectorAll(`${today} .day-data`).length,
					labels: [...document.querySelectorAll(`${today} .zone-label`)].map((t) => t.textContent),
					lanes: document.querySelectorAll(`${today} .lane`).length,
					yesterday: [...document.querySelectorAll(`${yesterday} .day-data`)].map((g) => g.getAttribute('data-offset'))
				}),
				[band(0), band(1)]
			);
			assert.equal(strip.stretches, 2, JSON.stringify(strip));
			assert.ok(strip.labels.includes('UTC+2') && strip.labels.includes('UTC+1'), JSON.stringify(strip));
			assert.ok(strip.lanes > 0, JSON.stringify(strip));
			assert.deepEqual(strip.yesterday, ['7200']);
			assert.deepEqual(app.errors, []);
		},
		FALL_BACK
	);
});
