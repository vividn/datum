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

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, maximizeButton, pane, pastTheWindow, pickOption, placeBar } from './support/layout.mjs';
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

/** A `--history` as minutes. */
const minutesOf = (h) => (h.endsWith('s') ? Number(h.slice(0, -1)) / 60 : h.endsWith('h') ? Number(h.slice(0, -1)) * 60 : Number(h.slice(0, -1)));

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

		// vividn, on #1445: «move them to the middle of the axis … small white
		// diamonds, 2:1 ratio so they are longer along the axis than sticking out».
		test('one white 2:1 diamond on each axis’s line in its middle, a press on it no scrub, and no label under it', async () => {
			await withAxes(vertical, async ({ page }) => {
				const seen = await page.locator(NOWVIEW).evaluate((root) => {
					const svg = root.querySelector('svg.svg');
					const y = !!svg.querySelector('[data-lo][data-axis="y"]');
					const box = (e) => e.getBoundingClientRect();
					const frame = box(svg.closest('.picture'));
					const shown = [...svg.querySelectorAll('.tick-label, .clock-label')].filter((l) => getComputedStyle(l).visibility !== 'hidden');
					return {
						stroke: getComputedStyle(svg.querySelector('path.axis')).stroke,
						cut: shown.filter((l) => {
							const b = box(l);
							return y ? b.top < frame.top && b.bottom > frame.top : b.left < frame.left && b.right > frame.left;
						}).map((l) => l.textContent),
						handles: [...root.querySelectorAll('.nowview-handles .handle')].map((h) => {
							const [r, d] = [box(h), getComputedStyle(h, '::before')];
							const [cx, cy] = [r.left + r.width / 2, r.top + r.height / 2];
							const line = box(svg.querySelector(h.dataset.axis === 'rel' ? 'path.axis' : 'path.clock-axis'));
							const [w, t] = [parseFloat(d.width), parseFloat(d.height)];
							return {
								axis: h.dataset.axis,
								at: h.dataset.at,
								along: y ? t : w,
								out: y ? w : t,
								clip: d.clipPath,
								fill: d.backgroundColor,
								place: y ? (cy - frame.top) / frame.height : (cx - frame.left) / frame.width,
								off: h.dataset.axis === 'rel' ? (y ? cx - line.right : cy - line.top) : y ? cx - line.left : cy - line.bottom,
								under: shown.filter((l) => {
									const b = box(l);
									return b.left < cx + w / 2 && cx - w / 2 < b.right && b.top < cy + t / 2 && cy - t / 2 < b.bottom;
								}).map((l) => l.textContent)
							};
						}).sort((a, b) => a.axis.localeCompare(b.axis))
					};
				});
				assert.deepEqual(seen.cut, [], 'the window\'s far end cuts a label in two');
				assert.deepEqual(seen.handles.map((h) => [h.axis, h.at]), [['abs', '14:47:30'], ['rel', '-15m']]);
				for (const h of seen.handles) {
					assert.equal(h.along, 2 * h.out, `${h.axis}: ${h.along} along, ${h.out} out`);
					assert.ok(h.out <= 8, `${h.axis} sticks out ${h.out}px`);
					assert.match(h.clip, /^polygon/);
					assert.equal(h.fill, seen.stroke === 'white' ? 'rgb(255, 255, 255)' : seen.stroke);
					assert.ok(h.place > 0.3 && h.place < 0.55, `${h.axis} stands ${h.place} along`);
					assert.ok(Math.abs(h.off) <= 1, `${h.axis} is ${h.off}px off its line`);
					assert.deepEqual(h.under, []);
				}
				const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
				await finger(page, { x: h.x + h.width / 2, y: h.y + h.height / 2 });
				await page.locator('.nowview-wheel .notch.held').waitFor();
				assert.deepEqual(await pillsOf(page), [], 'a press on the handle made a pill');
			});
		});

		// «It should be possible to still put a time pill by clicking above the diamond».
		for (const kind of ['rel', 'abs'])
			test(`a press just out of the ${kind} diamond puts a time pill`, async () => {
				await withAxes(vertical, async ({ page }) => {
					const h = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
					const [x, y] = [h.x + h.width / 2, h.y + h.height / 2];
					// Out of the band: the relative axis stands under it, or left of it. A
					// touch within about 3px of the 12px target is the browser's to give to it.
					const by = kind === 'rel' ? 10 : -10;
					await finger(page, vertical ? { x: x - by, y } : { x, y: y + by });
					await until(() => pillsOf(page), (p) => p.length === 1);
					assert.equal(await page.locator('.nowview-wheel').count(), 0, 'the press opened the numbers');
				});
			});

		test('with an absolute pill centered, the handle stands short of it and still scales', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const l = await page.locator(`${NOWVIEW} svg.svg .clock-label`, { hasText: /^14:50$|^14:45$/ }).first();
				const b = await l.boundingBox();
				const word = await l.textContent();
				await finger(page, { x: b.x + b.width / 2, y: b.y + b.height / 2 });
				await timePill(page, `-t ${word}`);
				await drewPill(app);
				await app.spyOnRunner();
				const handle = page.locator(`${NOWVIEW} .handle[data-axis="rel"]`);
				const at = await until(() => handle.boundingBox(), (r) => r !== null);
				const g = await read(page);
				const [start, length] = g.y ? [g.frame.top, g.frame.height] : [g.frame.left, g.frame.width];
				const [from, across] = g.y ? [at.y + at.height / 2, at.x + at.width / 2] : [at.x + at.width / 2, at.y + at.height / 2];
				// On now it stands about 0.4 along: halfway to an anchor nearer the far end, here.
				assert.ok((from - start) / length > 0.1 && (from - start) / length < 0.3, `it stands ${(from - start) / length} along`);
				await finger(page, pt(g, from, across), pt(g, from + 30, across));
				await until(
					() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
					(a) => a && a[a.indexOf('--history') + 1] !== '30m'
				);
			});
		});

		// vividn, on #1445: «Don't show the new extant over the dragged handle … adjust
		// the axis labels as you are dragging … the axis labels that you will have as
		// soon as you let go, so that it is seamless».
		test('mid-drag the axis wears the labels the release will, and the handle no word', async () => {
			await withAxes(vertical, async ({ page }) => {
				const labels = (sel) =>
					page.locator(NOWVIEW).evaluate((root, sel) => {
						const svg = root.querySelector('svg.svg');
						const y = !!svg.querySelector('[data-lo][data-axis="y"]');
						const f = svg.closest('.picture').getBoundingClientRect();
						return [...svg.querySelectorAll(sel)]
							.filter((l) => {
								const b = l.getBoundingClientRect();
								const [a, z] = y ? [b.top, f.bottom] : [b.left, f.right];
								return getComputedStyle(l).visibility !== 'hidden' && a >= (y ? f.top : f.left) && a < z;
							})
							.map((l) => l.textContent)
							.sort();
					}, sel);
				const before = await labels('.plot > .tick-label');
				const h = await page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).boundingBox();
				const [x, y] = [h.x + h.width / 2, h.y + h.height / 2];
				await page.mouse.move(x, y);
				await page.mouse.down();
				// Toward now: a longer window.
				const g = await read(page);
				const from = g.y ? y : x;
				const to = from + ((g.y ? g.frame.bottom : g.frame.right) - from) * 0.35;
				await page.mouse.move(g.y ? x : to, g.y ? to : y, { steps: 8 });
				await page.locator(`${NOWVIEW} svg.svg.axis-live .live-axis .tick-label`).first().waitFor({ state: 'attached' });
				assert.equal(await page.locator('.handle.dragged').textContent(), '', 'the dragged handle says something');
				const during = await until(() => labels('.live-axis .tick-label'), (l) => l.length > 0);
				assert.notDeepEqual(during, before, 'the labels did not follow the drag');
				await page.mouse.up();
				await page.waitForFunction((sel) => !document.querySelector(`${sel} svg.svg.axis-live`), NOWVIEW);
				const after = await until(() => labels('.plot > .tick-label'), (l) => l.length > 0);
				assert.deepEqual(during, after, 'the release moved the labels');
			});
		});

		// Tabs hang 8px into the widget from a bar beside them (CI on #1445).
		for (const edge of ['top', 'left'])
			test(`with the bar on the ${edge}, every tab keeps its taps`, async () => {
				await withAxes(vertical, async ({ page }) => {
					await placeBar(page, 'side', edge);
					const seen = await until(
						() =>
							page.locator('section[data-pane="side"]').evaluate((p) => {
								const box = (e) => e.getBoundingClientRect();
								const taken = [...p.querySelectorAll('[role="tab"]')].flatMap((tab) => {
									const r = box(tab);
									return [1, 3, 5, 7, 9].flatMap((i) =>
										[1, 3, 5, 7, 9].map((j) => document.elementFromPoint(r.left + (r.width * i) / 10, r.top + (r.height * j) / 10)?.closest('.handle') && tab.textContent.trim())
									);
								});
								return { tabs: p.querySelectorAll('[role="tab"]').length, handles: p.querySelectorAll('.nowview-handles .handle').length, taken: taken.filter(Boolean) };
							}),
						(s) => s.handles === 2
					);
					assert.ok(seen.tabs > 1, 'no tabs to tap');
					assert.deepEqual(seen.taken, [], 'a handle took a tap on a tab');
				});
			});

		// vividn, on #1445: «make it smooth and continuous rather than having jumps».
		for (const kind of ['rel', 'abs'])
			test(`a drag of the ${kind} handle scales the window by where the finger goes, unrounded`, async () => {
				await withAxes(vertical, async (app) => {
					const { page } = app;
					await app.spyOnRunner();
					const h = await page.locator(`${NOWVIEW} .handle[data-axis="${kind}"]`).boundingBox();
					const g = await read(page);
					const from = pt(g, vertical ? h.y + h.height / 2 : h.x + h.width / 2, vertical ? h.x + h.width / 2 : h.y + h.height / 2);
					const start = vertical ? from.y : from.x;
					const anchor = g.pos.dot + ((NOW - g.instants.dot) * (g.pos.dot - g.pos.change)) / (g.instants.dot - g.instants.change);
					await finger(page, from, pt(g, start + 37, vertical ? from.x : from.y));
					const seen = await until(
						() => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1)),
						(a) => a?.[a.indexOf('--history') + 1] !== '30m'
					);
					const want = (30 * (anchor - start)) / (anchor - start - 37);
					const got = minutesOf(seen[seen.indexOf('--history') + 1]);
					assert.ok(Math.abs(got - want) / want < 0.03, `the window came to ${got}m, the finger to ${want}m`);
				});
			});

		test('a tap on an axis label selects that label’s time, on either axis', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const label = (sel, text) =>
					page.locator(`${NOWVIEW} svg.svg ${sel}`, { hasText: new RegExp(`^${text}$`) }).first().boundingBox();
				const middle = (b) => ({ x: b.x + b.width / 2, y: b.y + b.height / 2 });
				// Off its middle, as a finger is: it is still the label's time.
				const l = await label('.clock-label', vertical ? '14:45' : '14:50');
				await finger(page, { x: middle(l).x + 2, y: middle(l).y + 1 });
				await timePill(page, `-t ${vertical ? '14:45' : '14:50'}`);
				await drewPill(app);
				await page.locator(`${NOWVIEW} .nowview-now`).click();
				await page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'detached' });
				await app.idle();
				await finger(page, middle(await label('.tick-label', '-10m')));
				await timePill(page, '-qq');
			});
		});

		test('with a time selected the relative labels read t-5m and t+5m, and a tap on one counts from the selection', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				// 14:33, a whole minute, as a time set from outside is.
				await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), NOW - 29.5 * 60_000);
				await drewPill(app);
				const after = page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t\+15m$/ }).first();
				await after.waitFor({ state: 'attached' });
				const words = await page.locator(`${NOWVIEW} svg.svg .tick-label`).allTextContents();
				for (const w of words) assert.match(w, /^t[-+]\d/);
				const at = await after.boundingBox();
				await finger(page, { x: at.x + at.width / 2, y: at.y + at.height / 2 });
				// Fifteen after the selection: 14:48, 14m30s back from now.
				await timePill(page, '-t -14m30s');
			});
		});

		test('a relative pill leaves the relative axis as it was, in words and in place, and its handle on now', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const ruler = () =>
					page.evaluate((sel) => {
						const of = (e) => e.getBoundingClientRect();
						const home = of(document.querySelector(`${sel} .nowview-band`));
						return [...document.querySelectorAll(`${sel} svg.svg .tick-label`)]
							.filter((l) => l.style.visibility !== 'hidden' && l.textContent)
							.map((l) => [l.textContent, of(l).left - home.left, of(l).top - home.top])
							.filter(([, x, y]) => x >= 0 && y >= 0 && x <= home.width && y <= home.height)
							.map(([w, x, y]) => [w, Math.round(x), Math.round(y)]);
					}, NOWVIEW);
				const handle = () => page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).getAttribute('data-at');
				const was = await ruler();
				assert.ok(was.some(([w]) => /^-\d/.test(w)), 'no relative labels to keep');
				const washeld = await handle();
				const g = await read(page);
				await finger(page, pt(g, (g.clock[1].p + g.clock[2].p) / 2, g.relRow));
				await timePill(page, (t) => /^(-t -|-q)/.test(t));
				await drewPill(app);
				const on = await ruler();
				assert.deepEqual(on.map(([w]) => w), was.map(([w]) => w), 'the words moved');
				for (const [i, [w, x, y]] of on.entries()) assert.ok(Math.abs(x - was[i][1]) <= 1 && Math.abs(y - was[i][2]) <= 1, `${w} moved: ${[x, y]} from ${was[i].slice(1)}`);
				assert.equal(await handle(), washeld);
			});
		});

		test('an absolute pill turns the relative axis to t±, and dismissed it reads from now again', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				const g = await read(page);
				const [a, b] = g.clock.slice(1);
				await finger(page, pt(g, (a.p + b.p) / 2, g.absRow));
				await timePill(page, (t) => /^-t \d\d:\d\d$/.test(t));
				await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t[-+]/ }).first().waitFor({ state: 'attached' });
				await page.locator('.prompt-bar .pill.time button.remove').click();
				await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^-\d/ }).first().waitFor({ state: 'attached' });
				assert.equal(await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t[-+]/ }).count(), 0);
			});
		});

		test('a tap on an axis’s pill dismisses it, and a drag from it scrubs its axis', async () => {
			await withAxes(vertical, async (app) => {
				const { page } = app;
				let g = await read(page);
				const [a, b] = g.clock.slice(1);
				await finger(page, pt(g, (a.p + b.p) / 2, g.absRow));
				const first = await timePill(page, (t) => /^-t \d\d:\d\d$/.test(t));
				await drewPill(app);
				const pill = await page.locator(`${NOWVIEW} .axis-pill`).boundingBox();
				const on = { x: pill.x + pill.width / 2, y: pill.y + pill.height / 2 };
				g = await read(page);
				const hit = await page.evaluate(({ x, y }) => !!document.elementFromPoint(x, y)?.closest('.axis-pill'), on);
				assert.ok(hit, 'the press does not reach the pill');
				const along = vertical ? on.y : on.x;
				const across = vertical ? on.x : on.y;
				await finger(page, on, pt(g, along + 40, across));
				const moved = await timePill(page, (t) => t !== first && /^-t /.test(t));
				assert.notEqual(moved, first, 'the drag from the pill did not scrub');
				await drewPill(app);
				const again = await page.locator(`${NOWVIEW} .axis-pill`).boundingBox();
				await finger(page, { x: again.x + again.width / 2, y: again.y + again.height / 2 });
				await until(
					() => pillsOf(page),
					(p) => !p.some((x) => x.startsWith('time:'))
				);
			});
		});

		// What «scrubbing is a bit inconsistent» came to on a phone: past the
		// now-line the clock's ran on into the future, and the relative one's
		// dropped its pill and brought it back minutes before now.
		for (const row of ['absRow', 'relRow'])
			test(`a scrub of the ${row === 'absRow' ? 'clock' : 'relative'} axis into the now strip says no time, and no earlier one`, async () => {
				await withAxes(vertical, async ({ page }) => {
					const g = await read(page);
					const [lo, hi] = vertical ? [g.frame.top, g.frame.bottom] : [g.frame.left, g.frame.right];
					const cdp = await page.context().newCDPSession(page);
					const said = [];
					const at = (p) => pt(g, p, g[row]);
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [at(lo + 0.6 * (hi - lo))] });
					for (let i = 0; i <= 10; i++) {
						await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [at(lo + (0.6 + (0.38 * i) / 10) * (hi - lo))] });
						// A frame, for the page to have read the move.
						await page.evaluate(() => new Promise(requestAnimationFrame));
						said.push(await page.evaluate(() => window.datum.cursor.at()?.getTime() ?? null));
					}
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
					assert.ok(said.at(-1) === null && said.some((t) => t !== null), `the scrub said ${said}`);
					const first = said.indexOf(null);
					assert.ok(said.slice(first).every((t) => t === null), `a time came back past now: ${said}`);
					const before = said.slice(0, first);
					assert.ok(before.every((t, i) => t <= NOW && (!i || t >= before[i - 1])), `the scrub went back or on past now: ${said}`);
					const times = await page.evaluate(() => window.datum.bar.pills().filter((p) => p.kind === 'time').map((p) => p.text));
					assert.deepEqual(times, [], 'the lift in the strip left a time');
				});
			});

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
}
