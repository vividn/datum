// The nowview's two axes (da-b9glf). vividn, 2026-10-29: «Let's expand the
// nowview image on the top to fill that room. Let's put absolute time stamps
// on the top (the gradation should depend on the zoom level) with dotted
// lines going through for those, and relative timestamps on the bottom. You
// should be able to scrub an absolute pill on the top axis and a relative
// time pill on the bottom time axis. … Let's only have the furthest out axis
// label be the handle/button, the others then are not.» And the lines: the
// clock's solid, as the dayview's are, and the relative ones dotted.
//
// A selection is ONE pill, absolute (`-t 14:50`) or relative (`-qq`), by how
// it was made: the axis scrubbed; a press in the picture snapped to a mark of
// either axis; a long press snapped to a dot or a state change, absolute; and
// a scrub of the clock axis stopping a few px either side of those.
//
// On a phone, both ways round: time across the band, and time down it, where
// the clock's axis stands right of the band and the relative one left.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, maximizeButton, pane, pastTheWindow, pickOption } from './support/layout.mjs';
import { finger } from './support/press.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** 15:02:30: the relative marks fall half a minute off the clock's, so a snap says which it took. */
const NOW = Date.UTC(2026, 8, 10, 15, 2, 30);
const NOWVIEW = 'section.pane[data-title="nowview"]';
/** Off every whole minute, so a snap to it is told from a press rounded near it. */
const [CHANGE, DOT] = ['14:48:30', '14:53:30'];

/**
 * The phone's nowview, `vertical` or not, over `work` since 14:00, `mood`
 * calm then busy at CHANGE, and a coffee at DOT.
 */
async function withAxes(vertical, fn) {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const line of [
				['start', 'work', '-t', '14:00'],
				['switch', 'mood', 'calm', '-t', '14:35'],
				['switch', 'mood', 'busy', '-t', CHANGE],
				['occur', 'coffee', '-t', DOT]
			])
				assert.equal((await app.run(line)).code, 0, line.join(' '));
			await app.redraw();
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
			if (vertical) {
				await pickOption(page, 'side', 'orientation', 'vertical');
				await maximizeButton(page, 'side').click();
			}
			// The picture of the box it is in: a maximized one is tall.
			await page.waitForFunction(
				([sel, y]) => {
					const svg = document.querySelector(sel);
					return svg?.querySelector(`[data-lo][data-axis="${y ? 'y' : 'x'}"]`) && (!y || svg.viewBox.baseVal.height > 400);
				},
				[`${NOWVIEW} svg.svg`, vertical]
			);
			await page.locator(`${NOWVIEW} .handle[data-axis="abs"]`).waitFor();
			await app.idle();
			await pastTheWindow(page);
			await fn(app);
		},
		{ ...PHONE, clock: NOW }
	);
}

/**
 * The picture as it is drawn: which way time runs, the boxes, where each axis's
 * strip is across the band, a place across the band no mark covers, and the
 * instants and places along time of the clock's marks, the relative gridlines,
 * the dot and the state change.
 */
const read = (page) =>
	page.locator(NOWVIEW).evaluate((root, [change, dot, now]) => {
		const svg = root.querySelector('svg.svg');
		const box = (e) => e.getBoundingClientRect();
		const y = !!svg.querySelector('[data-lo][data-axis="y"]');
		const along = (e) => (y ? box(e).top + box(e).height / 2 : box(e).left + box(e).width / 2);
		const [frame, rel, abs, picture] = [svg.closest('.picture'), svg.querySelector('path.axis'), svg.querySelector('path.clock-axis'), svg].map(box);
		const inside = (p) => (y ? p >= frame.top && p <= frame.bottom : p >= frame.left && p <= frame.right);
		// Across: each strip's middle, and the band between them.
		const relRow = y ? (picture.left + rel.right) / 2 : (rel.top + picture.bottom) / 2;
		const absRow = y ? (abs.left + picture.right) / 2 : (picture.top + abs.bottom) / 2;
		const [bandLo, bandHi] = y ? [rel.right + 2, abs.left - 2] : [abs.bottom + 2, rel.top - 2];
		const covered = [...svg.querySelectorAll('rect.block, [data-field][data-at]')].map((m) => (y ? [box(m).left, box(m).right] : [box(m).top, box(m).bottom]));
		let empty = null;
		for (let c = bandLo + 4; c < bandHi - 4 && empty === null; c += 2)
			if (!covered.some(([a, b]) => c >= a - 6 && c <= b + 6)) empty = c;
		const at = (e) => Date.parse(e.getAttribute('data-at'));
		// Not those still to come, drawn ahead behind the clip at the now-line.
		const clock = [...svg.querySelectorAll('.clock-mark')].map((m) => ({ at: at(m), p: along(m) })).filter((m) => inside(m.p) && m.at <= now);
		const local = (iso) => {
			const [h, m, s] = iso.split(':').map(Number);
			const d = new Date(clock[0].at);
			return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), h, m, s);
		};
		// Along time by the clock's own marks: two of them make the map.
		const [a, b] = clock;
		const pos = (t) => a.p + ((t - a.at) * (b.p - a.p)) / (b.at - a.at);
		const coffee = svg.querySelector('[data-field="coffee"][data-at]');
		const busy = [...svg.querySelectorAll('rect.block[data-field="mood"]')].find((r) => Date.parse(r.getAttribute('data-from')) === local(change));
		return {
			y,
			widget: box(root.querySelector('.nowview')),
			band: box(root.querySelector('.nowview-band')),
			handles: [...root.querySelectorAll('.nowview-handles .handle')].map((h) => (y ? [box(h).top, box(h).bottom] : [box(h).left, box(h).right])),
			picture,
			frame,
			relRow,
			absRow,
			empty,
			clock,
			grid: [...svg.querySelectorAll('.gridline')].map(along).filter(inside),
			pos: { change: pos(local(change)), dot: pos(local(dot)) },
			drawn: { change: busy ? (y ? box(busy).top : box(busy).left) : null, dot: coffee ? along(coffee) : null },
			instants: { change: local(change), dot: local(dot) }
		};
	}, [CHANGE, DOT, NOW]);

/** Whether a place along time is clear of every handle, which a press there would take. */
const clear = (g, p) => g.handles.every(([a, b]) => p < a - 4 || p > b + 4);

/** A point `along` time and `across` the band, whichever way time runs. */
const pt = (g, along, across) => (g.y ? { x: across, y: along } : { x: along, y: across });

/** The pills as `kind:text`. */
const pillsOf = (page) => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/** Wait for the time pill to say `want` (a string, or a test of it); returns what it said. */
async function timePill(page, want) {
	const ok = (t) => (typeof want === 'string' ? t === want : !!t && want(t));
	const said = await until(
		async () => (await pillsOf(page)).find((p) => p.startsWith('time:'))?.slice(5),
		ok
	);
	assert.ok(ok(said), `the time pill says ${said}`);
	return said;
}

/** Wait for the picture a time pill draws: its dashed line at the instant. */
async function drewPill(app) {
	await app.page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'attached' });
	await app.idle();
}

/** HH:MM[:SS] of an instant, on the harness's clock (UTC). */
const hhmm = (t) => new Date(t).toISOString().slice(11, new Date(t).getUTCSeconds() ? 19 : 16);

for (const vertical of [false, true]) {
	const way = vertical ? 'time down the band' : 'time across the band';

	describe(`the nowview's axes, ${way}`, () => {
		test('no pill row: the picture fills the widget, the clock written over the band and the relative times under it', async () => {
			await withAxes(vertical, async ({ page }) => {
				assert.equal(await page.locator(`${NOWVIEW} .nowview-margin`).count(), 0, 'the pill row is still drawn');
				const g = await read(page);
				// The band starts under the frame's own titles' 8px and runs to the
				// widget's foot, and the picture fills it: as little room over it
				// as under it, the pane's own inset.
				assert.ok(Math.abs(g.band.top - g.widget.top - 8) <= 1, `the band starts ${g.band.top - g.widget.top}px down the widget`);
				assert.ok(Math.abs(g.widget.bottom - g.band.bottom) <= 1, `${g.widget.bottom - g.band.bottom}px under the band`);
				const [over, under] = [g.picture.top - g.band.top, g.band.bottom - g.picture.bottom];
				assert.ok(over <= under + 1 && under <= 8, `${over}px over the picture, ${under}px under it`);

				const lines = await page.locator(NOWVIEW).evaluate((root) => {
					const svg = root.querySelector('svg.svg');
					const y = !!svg.querySelector('[data-lo][data-axis="y"]');
					const box = (e) => e.getBoundingClientRect();
					const across = (e) => (y ? box(e).left + box(e).width / 2 : box(e).top + box(e).height / 2);
					const shown = (e) => getComputedStyle(e).visibility !== 'hidden' && box(e).width > 0;
					const dash = (e) => getComputedStyle(e).strokeDasharray;
					const [rel, abs] = ['path.axis', 'path.clock-axis'].map((s) => across(svg.querySelector(s)));
					return {
						rel,
						abs,
						clock: [...svg.querySelectorAll('.clock-label')].filter(shown).map((l) => ({ text: l.textContent, at: across(l) })),
						ticks: [...svg.querySelectorAll('.tick-label')].filter(shown).map((l) => ({ text: l.textContent, at: across(l) })),
						clockDash: [...svg.querySelectorAll('.clock-mark')].map(dash),
						gridDash: [...svg.querySelectorAll('.gridline')].map(dash)
					};
				});
				// The clock above the band, or right of it; the relative times below it, or left.
				assert.ok(lines.clock.length >= 2, JSON.stringify(lines.clock));
				for (const l of lines.clock) {
					assert.match(l.text, /^(\d\d:\d\d|:\d\d)$/);
					assert.ok(vertical ? l.at > lines.abs : l.at < lines.abs, `${l.text} is not on the clock's side`);
				}
				assert.ok(lines.ticks.length >= 2, JSON.stringify(lines.ticks));
				for (const l of lines.ticks) {
					assert.match(l.text, /^[-+]\d/);
					assert.ok(vertical ? l.at < lines.rel : l.at > lines.rel, `${l.text} is not on the relative side`);
				}
				assert.ok(lines.clockDash.every((d) => d === 'none'), `a clock line is dashed: ${lines.clockDash}`);
				assert.ok(lines.gridDash.length && lines.gridDash.every((d) => d !== 'none'), `a relative line is solid: ${lines.gridDash}`);
			});
		});

		test('one handle on each axis, its outermost label, and none on the others', async () => {
			await withAxes(vertical, async ({ page }) => {
				const seen = await page.locator(NOWVIEW).evaluate((root) => {
					const svg = root.querySelector('svg.svg');
					const y = !!svg.querySelector('[data-lo][data-axis="y"]');
					const box = (e) => e.getBoundingClientRect();
					const frame = box(svg.closest('.picture'));
					const [lo, hi] = y ? [frame.top, frame.bottom] : [frame.left, frame.right];
					const ends = (e) => (y ? [box(e).top, box(e).bottom] : [box(e).left, box(e).right]);
					const whole = (e) => getComputedStyle(e).visibility !== 'hidden' && ends(e)[0] >= lo - 1 && ends(e)[1] <= hi + 1;
					const first = (s) => [...svg.querySelectorAll(s)].filter(whole).sort((a, b) => ends(a)[0] - ends(b)[0])[0]?.textContent;
					return {
						handles: [...root.querySelectorAll('.nowview-handles .handle')].map((h) => `${h.dataset.axis}:${h.dataset.at}`).sort(),
						far: first('.tick-label'),
						clock: first('.clock-label'),
						labels: [svg.querySelectorAll('.tick-label').length, svg.querySelectorAll('.clock-label').length]
					};
				});
				assert.ok(seen.labels.every((n) => n > 1), `${seen.labels} labels: nothing to leave unhandled`);
				assert.deepEqual(seen.handles, [`abs:${seen.clock}`, `rel:${seen.far}`]);
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
				g = await read(page);
				await finger(page, pt(g, from, g.relRow), pt(g, to, g.relRow));
				await timePill(page, (t) => /^(-q+|-t -\d+(m\d+s|s)?)$/.test(t));
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

				// The relative gridline ten minutes back, which says -qq: while there
				// is no pill, as the relative ticks count from a pill's instant.
				const [a, b] = g.clock;
				const back10 = a.p + ((NOW - 10 * 60_000 - a.at) * (b.p - a.p)) / (b.at - a.at);
				const grid = g.grid.reduce((x, y) => (Math.abs(y - back10) < Math.abs(x - back10) ? y : x));
				assert.ok(Math.abs(grid - back10) < 2, `no gridline at −10m: ${grid} ${back10}`);
				assert.ok(reach(g, grid) >= 1.5, `the −10m gridline has a mark ${reach(g, grid)}px off`);
				await finger(page, pt(g, grid + reach(g, grid), g.empty));
				await timePill(page, '-qq');

				// And the clock mark with the most room around it, back on now: a
				// pill's ticks count from its instant, and may stand on the clock's.
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
				assert.ok(clear(g, g.pos.dot - 40) && clear(g, g.pos.dot + 9), 'a handle is in the way');
				await finger(page, pt(g, g.pos.dot - 40, g.absRow));
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
}
