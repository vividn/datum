// The nowview's two axes (da-b9glf). vividn, 2026-10-29: «Let's expand the
// nowview image on the top to fill that room. Let's put absolute time stamps
// on the top (the gradation should depend on the zoom level) with dotted
// lines going through for those, and relative timestamps on the bottom. You
// should be able to scrub an absolute pill on the top axis and a relative
// time pill on the bottom time axis.» And the lines: the clock's solid, as the
// dayview's are, and the relative ones dotted.
//
// And on #1418 (da-tglm6): «rather than having the the left most axis label be
// the handleand clickable like right now, let's just put a protruding circle
// handle on the very far side of the axis … That way if you tap on an axis
// label it always puts a selection time right there, which is more intuitive.»
//
// A selection is ONE pill, absolute (`-t 14:50`) or relative (`-qq`), by how
// it was made: the axis scrubbed; a press in the picture snapped to a mark of
// either axis; a long press snapped to a dot or a state change, absolute; and
// a scrub of the clock axis stopping a few px either side of those.
//
// On a phone, both ways round: time across the band, and time down it, where
// the clock's axis stands right of the band and the relative one left.
//
// The helpers of the four nowaxes*.test.mjs files, split by what they press so
// each stays well inside test.mjs's 300 s file cap (da-9utsv).

import { describe } from 'node:test';
import assert from 'node:assert/strict';
import { withApp } from './app.mjs';
import { PHONE, maximizeButton, pane, pastTheWindow, pickOption } from './layout.mjs';
import { until } from './until.mjs';

/** 15:02:30: the relative marks fall half a minute off the clock's, so a snap says which it took. */
export const NOW = Date.UTC(2026, 8, 10, 15, 2, 30);
export const NOWVIEW = 'section.pane[data-title="nowview"]';
/** Off every whole minute, so a snap to it is told from a press rounded near it. */
const [CHANGE, DOT] = ['14:48:30', '14:53:30'];

/**
 * The phone's nowview, `vertical` or not, over `work` since 14:00, `mood`
 * calm then busy at CHANGE, and a coffee at DOT.
 */
export async function withAxes(vertical, fn) {
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
export const read = (page) =>
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
		// Past where a press takes an axis (`AXIS_REACH_PX`).
		const [bandLo, bandHi] = y ? [rel.right + 10, abs.left - 10] : [abs.bottom + 10, rel.top - 10];
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
export const clear = (g, p) => g.handles.every(([a, b]) => p < a - 4 || p > b + 4);

/** A point `along` time and `across` the band, whichever way time runs. */
export const pt = (g, along, across) => (g.y ? { x: across, y: along } : { x: along, y: across });

/** The pills as `kind:text`. */
export const pillsOf = (page) => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/** Wait for the time pill to say `want` (a string, or a test of it); returns what it said. */
export async function timePill(page, want) {
	const ok = (t) => (typeof want === 'string' ? t === want : !!t && want(t));
	const said = await until(
		async () => (await pillsOf(page)).find((p) => p.startsWith('time:'))?.slice(5),
		ok
	);
	assert.ok(ok(said), `the time pill says ${said}`);
	return said;
}

/** Wait for the picture a time pill draws: its dashed line at the instant. */
export async function drewPill(app) {
	await app.page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'attached' });
	// A pill from outside the nowview flies it to show the pill (da-2v3mr), and the picture
	// of the window it lands on is asked once it lands: the first with the line is the one before.
	await app.page.locator(`${NOWVIEW} .nowview-band:not([data-flying])`).waitFor({ state: 'attached' });
	await app.idle();
}

/** A `--history` as minutes. */
export const minutesOf = (h) => (h.endsWith('s') ? Number(h.slice(0, -1)) / 60 : h.endsWith('h') ? Number(h.slice(0, -1)) * 60 : Number(h.slice(0, -1)));

/** HH:MM[:SS] of an instant, on the harness's clock (UTC). */
export const hhmm = (t) => new Date(t).toISOString().slice(11, new Date(t).getUTCSeconds() ? 19 : 16);

/** Each test of `fn(vertical)` both ways round: time across the band, and down it. */
export function bothWays(fn) {
	for (const vertical of [false, true])
		describe(`the nowview's axes, ${vertical ? 'time down the band' : 'time across the band'}`, () => fn(vertical));
}
