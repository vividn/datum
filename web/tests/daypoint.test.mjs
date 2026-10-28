// A drag on the day strip selects a RANGE (da-ku7y).
//
// vividn, 2026-09-17: "when dragging in the dayview, if already zoomed out
// to the max then just automatically do a selection, if zoomed in, then
// pan and select with long press like nowview. For selections in dayview,
// since they are longer, let's use the alternative duration selection
// pills start_time and end_time. That way they are also discoverable."
//
// So there are three claims here and a trap under each: WHICH gesture a
// drag is, which depends on the window; WHAT the range says, which is the
// CLI's own pair of keys and not `dur=`; and WHOSE the drag was, over a
// stack that scrolls and inside a frame that swipes.
//
// Delta-blind, as `nowpoint.test.mjs` is: every instant is read off the
// band's own `data-from` / `data-to` over `data-lo` / `data-hi` through
// the picture's `getScreenCTM` — the same reading `#lib/hover.ts` makes —
// so the times the pills are expected to say are the times the pixels the
// test pressed ARE, and never a guess at what a minute is worth in px.
// The state machine the gestures run is `nowpoint-pure.test.mjs`, in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, sharedApp, todayAt } from './support/app.mjs';
import { PHONE, settled, swipe, titles } from './support/layout.mjs';
import { drag, finger, longPressed, noWait, swipeNoWait } from './support/press.mjs';
import { until } from './support/until.mjs';
import { listed } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** 15:00 sharp: a minute boundary, and room on both sides of it in the day. */
const NOW = todayAt(15);
const MINUTE = 60_000;
const DESKTOP = { viewport: { width: 1280, height: 900 } };
const STRIP = '.strip';
const TODAY = '.band[data-days-ago="0"]';
const YESTERDAY = '.band[data-days-ago="1"]';
const NOWVIEW = 'section.pane[data-title="nowview"]';
const BAR = 'section.pane[data-title="fields"] .bar';
/** How long `#lib/nowpoint.ts` waits before a press is a range, with room to spare. */
const HOLD = 650;

/**
 * The app with a mark on today's band, the strip drawn, and the clock
 * pinned on both sides of the seam.
 *
 * `clock` is a FIXED time rather than an installed one: a gesture here is
 * a real press over real milliseconds and the long press is a TIMER, so
 * an installed clock would freeze the very thing under test. The slow
 * finger alone holds the timers (`holdTimers`), and fires the long press
 * itself.
 */
async function withStrip(fn, options) {
	const body = async (app) => {
		assert.equal((await app.run(['occur', 'coffee', '-t', '08:00'])).code, 0);
		await app.redraw();
		await app.page.locator(`${TODAY} svg.svg`).waitFor();
		await stackOfDays(app.page);
		await app.idle();
		await fn(app);
	};
	// The desktop's tests share ONE app (sharedApp in support/app.mjs), each
	// in a room of its own: a zoomed or panned strip is the strip's own
	// state, so a test must not inherit the last one's. A phone is the
	// context's.
	if (!options) return desktop.fresh(body);
	await withApp(body, { clock: NOW, ...DESKTOP, ...options });
}

const desktop = sharedApp({ clock: NOW, ...DESKTOP });

/**
 * A day height HELD, short enough that YESTERDAY's whole band is on screen
 * — at the desktop size these tests use and at `PHONE` alike.
 *
 * ⚑ SINCE da-r0ydu A DAY FOLLOWS THE WIDGET until the reader sets one, so
 * a strip nobody has touched draws exactly ONE day and there is no band
 * but today's — and half this file points into YESTERDAY's. Nothing here
 * is about how tall a day is: this is how a reader gets a stack (the day
 * line over the ruler, whose value then holds), and 40 is `MIN_DAY_HEIGHT`, the most
 * past a strip's own box can show. Only the SETTING is touched, never a
 * pane's size: a strip given a bigger cell is a different layout, and two
 * of the tests below are about what a swipe does to the layout.
 */
const stackOfDays = async (page) => {
	// DRAGGED, not written into the tree. A setting the reader moves lands
	// in THIS SESSION (da-vhwq) and the layout never hears of it; writing
	// the tree instead lights the layout's modified dot, and two of the
	// tests below read the root tab's name. Down is shorter (da-hsba), and
	// 300px of it clamps at `MIN_DAY_HEIGHT`.
	const box = await page.locator('.today-line').boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + 300, { steps: 8 });
	await page.mouse.up();
	await page.waitForFunction(
		() => Number(document.querySelector('.today-line')?.getAttribute('aria-valuenow')) === 40
	);
	await page.locator('.band[data-days-ago="1"] svg.svg').waitFor();
};

/**
 * A band's own geometry: the day it covers, and where that day's axis
 * begins and ends on screen.
 *
 * Through `getScreenCTM`, which carries the window's CSS transform with
 * it — so the same arithmetic reads a zoomed strip as reads a whole day.
 */
const geom = (page, daysAgo = 0) =>
	page.locator(`.band[data-days-ago="${daysAgo}"]`).evaluate((band) => {
		const svg = band.querySelector('svg.svg');
		const day = svg.querySelector('[data-lo]');
		const ctm = svg.getScreenCTM();
		const px = (u) => new DOMPoint(u, 0).matrixTransform(ctm).x;
		// The BAND's box, not the picture's: zoomed in, the picture is
		// several bands wide and hangs off both ends of the one that clips
		// it, so its middle is nowhere a finger can land.
		const box = band.getBoundingClientRect();
		return {
			from: Date.parse(day.getAttribute('data-from')),
			to: Date.parse(day.getAttribute('data-to')),
			lo: px(Number(day.getAttribute('data-lo'))),
			hi: px(Number(day.getAttribute('data-hi'))),
			y: box.top + box.height / 2,
			left: box.left,
			width: box.width
		};
	});

/** The instant `h`:`m` of the day `g` draws. The page's zone is UTC. */
const msAt = (g, h, m = 0) => g.from + (h * 60 + m) * MINUTE;
/** Where along the band an instant falls, to the pixel a pointer can land on. */
const xOf = (g, ms) => Math.round(g.lo + ((ms - g.from) / (g.to - g.from)) * (g.hi - g.lo));
/** And back: the instant a pixel IS, which is what the app will read there. */
const instantOf = (g, x) => g.from + ((x - g.lo) / (g.hi - g.lo)) * (g.to - g.from);
/** To the minute, as every selection is (#lib/cursor.ts). */
const toMinute = (ms) => Math.round(ms / MINUTE) * MINUTE;
/** `mins` as the ISO duration the CLI stores. */
const isoDur = (mins) =>
	`PT${mins >= 60 ? `${Math.floor(mins / 60)}H` : ''}${mins % 60 ? `${mins % 60}M` : ''}`;
/** `HH:MM` in the page's zone, as `timeArg` writes it for today. */
const hhmm = (ms) => new Date(toMinute(ms)).toISOString().slice(11, 16);
/** `YYYY-MM-DDTHH:MM`, as `timeArg` writes it for any other day. */
const dated = (ms) => new Date(toMinute(ms)).toISOString().slice(0, 16);

/** A point on a band, at the pixel nearest an instant. */
const spot = (g, ms) => ({ x: xOf(g, ms), y: g.y });

/** The pills as `kind:text` — what each PUTS on the command line, which is what it means. */
const pillsOf = (page) =>
	page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/**
 * The middle of the widest mark naming an entry on a band, and its id: a
 * block where there is one, so a finger cannot miss it.
 */
const markAt = (page, band) =>
	page.locator(`${band} svg.svg`).evaluate((svg) => {
		const marks = [...svg.querySelectorAll('[data-id]')].map((m) => ({ m, r: m.getBoundingClientRect() }));
		const { m, r } = marks.sort((a, b) => b.r.width - a.r.width)[0];
		const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
		if (!document.elementFromPoint(x, y)?.closest('[data-id]')) throw new Error('the middle of the mark is not on it');
		return { id: m.getAttribute('data-id'), x, y };
	});


/** Wait until the pills say exactly `want`, and report what they said if they never do. */
const saysPills = (page, want) =>
	until(
		() => pillsOf(page),
		(seen) => seen.join('|') === want.join('|')
	);

/** The two pills a strip range is, in the order the bar draws them. */
const rangePills = (from, to, day = hhmm) => [
	`dur:start_time=${day(from)}`,
	`time:end_time=${day(to)}`
];

/** Which field buttons the bar draws — WHICH, since where each stands is fieldsrows.test.mjs's. */
const shownFields = async (page) => (await listed(page.locator(BAR), '[data-field]')).map((el) => el.field).sort();

/** Wait until the bar reads exactly `want`, and say what it read if it never does. */
const saysFields = (page, want) =>
	until(
		() => shownFields(page),
		(seen) => seen.join() === want.join()
	);

/** Where the selection is DRAWN in `scope`: the arrow at its head, the band behind it. */
const drawn = (page, scope) =>
	page.locator(scope).evaluate((el) => {
		const mid = (s) => {
			const m = el.querySelector(`svg.svg ${s}`);
			if (!m) return null;
			const r = m.getBoundingClientRect();
			return { x: r.left + r.width / 2, w: r.width, left: r.left };
		};
		return {
			head: mid('.cursor-head'),
			span: mid('.cursor-span'),
			lines: el.querySelectorAll('svg.svg line.cursor').length
		};
	});

/**
 * Type a line into the bottom bar and wait for its receipt — the pills ride with it.
 *
 * The RUN is `bar.last()`, a fresh object for every command, and not the
 * receipt's text (da-k082e): a text wait cannot tell a command that failed,
 * or drew no document, from one still running, and sat its whole 30 s cap
 * three times in a row on CI with nothing to say. Now each says what it saw.
 */
const receiptNow = () => document.querySelector('.prompt-bar .group:last-child .doc .receipt')?.textContent ?? '';

async function barRun(page, line) {
	const before = await page.evaluate(() => {
		window.__barRun = window.datum.bar.last();
		return document.querySelector('.prompt-bar .group:last-child .doc .receipt')?.textContent ?? '';
	});
	await page.locator('.prompt-bar input').fill(line);
	await page.locator('.prompt-bar input').press('Enter');
	const ran = await until(
		() =>
			page.evaluate(() => {
				const last = window.datum.bar.last();
				if (last === window.__barRun) return { box: document.querySelector('.prompt-bar input')?.value };
				return { line: last.line, code: last.result.code, stderr: last.result.stderr };
			}),
		(seen) => 'code' in seen
	);
	assert.equal(ran.code, 0, `\`${ran.line}\` exited ${ran.code}: ${ran.stderr}`);
	await until(
		() => page.evaluate(receiptNow),
		(now) => now !== before && now !== ''
	);
	return page.locator('.prompt-bar .group:last-child .doc .receipt').last().innerText();
}

/**
 * The settings the strip widget is DRAWN from — what its owner last heard,
 * with whatever THIS SESSION holds over it. Since da-vhwq a move of the
 * strip's own (the zoom below is one) lands in the session and reaches the
 * tree only on «Save options with layout», so the tree alone is no longer
 * what the strip was told.
 *
 * The strip of the root tab IN FRONT: a test's room stands before `main`,
 * which has a strip of its own (da-s4i98).
 */
const stripSettings = () => {
	const walk = (pane) => {
		for (const tab of pane.tabs) {
			if ('children' in tab.content) {
				const hit = tab.content.children.map(walk).find(Boolean);
				if (hit) return hit;
			} else if (tab.content.kind === 'daystrip') return window.datum.layout.settings(tab.id);
		}
	};
	const { root } = window.datum.layout.current();
	return walk({ tabs: [root.tabs[root.active]] });
};

const windowHeard = ({ page }) =>
	page.evaluate(`(() => { const s = (${stripSettings})(); return [s.hourFrom, s.hourTo]; })()`);

/** Wait until the window the owner has heard satisfies `expr`, written about `w`. */
const untilWindow = ({ page }, expr) =>
	page.waitForFunction(
		`(() => { const s = (${stripSettings})(); const w = [s.hourFrom, s.hourTo]; return ${expr}; })()`
	);

/** Narrow the strip onto an hour range, as a reader does on a desk. */
async function zoomIn(app, g) {
	const { page } = app;
	await page.mouse.move(g.left + g.width / 2, g.y);
	await page.keyboard.down('Control');
	await page.mouse.wheel(0, -300);
	await untilWindow(app, 'w[1] - w[0] < 20');
	await page.keyboard.up('Control');
	await app.idle();
}

// ── a long press and a drag: a period (da-xg1vi) ───────────────────────

describe('a long press and a drag selects a period', () => {
	// vividn, 2026-10-22: «swiping left/right must not select time». Until
	// da-xg1vi a strip with the whole day on screen read any sideways drag
	// as a range, since it had nowhere to pan to; now that drag is the
	// pane's (the swipe below), and selects nothing.
	test('with the whole day on screen, a plain drag selects nothing and pans nothing', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			await page.evaluate(() => window.datum.cursor.set(null));
			await saysPills(page, []);
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 13)));
			assert.equal(await page.locator(STRIP).getAttribute('data-mode'), 'press');
			assert.deepEqual(await pillsOf(page), [], 'a swipe selected time');
			assert.deepEqual(await windowHeard(app), [0, 24], 'the whole day panned');
		});
	});

	test('two pills, a white band, and a block that ends where the drag ended', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const [a, b] = [spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30))];
			// What the two pixels ARE, which is what the pills will say: the
			// band is a day wide, so the nearest pixel to 09:00 is a few
			// dozen seconds either side of it.
			const [from, to] = [instantOf(g, a.x), instantOf(g, b.x)];
			assert.ok(Math.abs(from - msAt(g, 9)) < MINUTE, 'the 09:00 pixel is not within a minute of 09:00');

			await drag(page, a, b, { hold: true });

			await saysPills(page, rangePills(from, to));
			const marks = await drawn(page, TODAY);
			assert.ok(marks.span, 'no band was drawn for the range');
			assert.ok(
				Math.abs(marks.span.w - (b.x - a.x)) <= 2,
				`the band is ${marks.span.w}px, the drag ${b.x - a.x}px`
			);
			assert.equal(marks.lines, 2, 'a range is drawn between two lines');

			// The pair is lifted onto occur_time and dur (da-mjwq): the block
			// ENDS at `end_time=` and runs back to `start_time=`.
			assert.match(await barRun(page, 'start posture'), /^CREATE /);
			const [posture] = (await app.entries()).filter((e) => e.field === 'posture');
			assert.equal(
				new Date(Date.parse(posture.occur_time.utc)).toISOString(),
				new Date(toMinute(to)).toISOString()
			);
			assert.equal(posture.dur, isoDur((toMinute(to) - toMinute(from)) / MINUTE), JSON.stringify(posture));
		});
	});

	test('a plain click goes back to one instant, and the band goes with it', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)), { hold: true });
			await saysPills(page, rangePills(instantOf(g, xOf(g, msAt(g, 9))), instantOf(g, xOf(g, msAt(g, 10, 30)))));

			const one = spot(g, msAt(g, 12));
			await page.mouse.click(one.x, one.y);
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, one.x))}`]);
			await until(
				() => drawn(page, TODAY).then((d) => d.span),
				(span) => span === null
			);
		});
	});

	test('dragged either way it is the same range: it ENDS at the later instant', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const [a, b] = [spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30))];
			const want = rangePills(instantOf(g, a.x), instantOf(g, b.x));
			await drag(page, b, a, { hold: true });
			await saysPills(page, want);
		});
	});

	// vremya on PR #48, and the trap this bead's new pills walk straight
	// into: a command spends BOTH of a range's pills in one update, and the
	// reconciler answering them in the wrong order put the cursor back and
	// logged the NEXT plain entry at the spent range's time.
	test('a command spends BOTH pills: nothing is left, and the next entry is logged at now', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)), { hold: true });
			await until(
				() => pillsOf(page),
				(seen) => seen.length === 2
			);

			assert.match(await barRun(page, 'start posture'), /^CREATE /);
			await saysPills(page, []);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
			await until(
				() => page.locator('line.cursor').count(),
				(n) => n === 0
			);

			assert.match(await barRun(page, 'occur water'), /^CREATE /);
			const [water] = (await app.entries()).filter((e) => e.field === 'water');
			assert.equal(
				new Date(Date.parse(water.occur_time.utc)).toISOString(),
				new Date(NOW).toISOString(),
				'the next entry took the spent range’s time'
			);
		});
	});
});

// ── another day ────────────────────────────────────────────────────────

describe('a range on a day that is not today', () => {
	// MEASURED against the CLI before it was built: `start nap
	// start_time=2026-09-16T13:00 end_time=2026-09-16T14:30` stores the
	// block 13:00–14:30 on the 16th. The dated value is read exactly as
	// `-t` reads one — a lifted key goes through the `-t` slot (da-mjwq,
	// commands/log_core.rs) — so the pills say what `timeArg` already
	// wrote for the cursor, and no date pill of its own is needed.
	test('the pills carry the date, and the block lands on that day', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page, 1);
			const [a, b] = [spot(g, msAt(g, 13)), spot(g, msAt(g, 14, 30))];
			const [from, to] = [instantOf(g, a.x), instantOf(g, b.x)];

			await drag(page, a, b, { hold: true });
			await saysPills(page, rangePills(from, to, dated));

			assert.match(await barRun(page, 'start nap'), /^CREATE /);
			const [nap] = (await app.entries()).filter((e) => e.field === 'nap');
			assert.equal(
				new Date(Date.parse(nap.occur_time.utc)).toISOString(),
				new Date(toMinute(to)).toISOString(),
				'the block did not land on yesterday'
			);
		});
	});

	// A range never spans two days: the day is the day the press BEGAN in,
	// and the x is the time, so a finger that wanders down into the band
	// below — which is what a drag along a 40px band mostly does — still
	// selects on its own day, at the hours it crossed. Drifting into the
	// neighbour costs nothing, and there is no midnight to fall over.
	test('a drag that wanders into the band below is still the day it began in', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const yesterday = await geom(page, 1);
			const today = await geom(page, 0);
			// Mostly SIDEWAYS: a band is a few dozen pixels tall and the
			// day is a thousand wide, so a drag that drifts one band down
			// has traveled far further along than across. A mostly
			// VERTICAL finger is the stack's scroll and never a range —
			// which is the test below.
			const [a, b] = [
				spot(yesterday, msAt(yesterday, 8)),
				{ x: xOf(today, msAt(today, 20)), y: today.y }
			];
			await saysPills(page, []);
			await drag(page, a, b, { hold: true });
			await saysPills(
				page,
				rangePills(instantOf(yesterday, a.x), instantOf(yesterday, b.x), dated)
			);
		});
	});
});

// ── zoomed in: the drag pans, the long press selects ───────────────────

describe('zoomed in, a drag pans and a long press selects', () => {
	test('a drag moves the hour window and makes no selection', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			await zoomIn(app, await geom(page));
			const before = await windowHeard(app);
			assert.ok(before[1] - before[0] < 20, `the strip did not zoom in: ${before}`);

			const g = await geom(page);
			const middle = { x: g.left + g.width / 2, y: g.y };
			// Rightward: the content follows the finger, so the window is
			// EARLIER after it — what shift+wheel does, by pointer.
			await drag(page, middle, { x: middle.x + g.width / 4, y: g.y });
			await untilWindow(app, `w[0] < ${before[0] - 0.25}`);
			const after = await windowHeard(app);
			assert.ok(
				Math.abs(after[1] - after[0] - (before[1] - before[0])) < 0.05,
				`the pan changed the span: ${before} → ${after}`
			);
			assert.deepEqual(await pillsOf(page), [], 'the pan selected something');
		});
	});

	// da-pzzoz moved the wait's FEEDBACK. The bar used to drop the moment
	// the press was long enough, because the press committed to a range
	// there and then; it no longer commits — a still long press is an
	// INSPECT and only a travelling one is a range (#lib/nowpoint.ts) — so
	// what the finger is shown at 400ms is the `.hold-mark`, and the bar
	// drops on the first pixel of travel after it.
	test('a long press marks the wait before any movement; the drag after it is a range', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			await zoomIn(app, await geom(page));
			const g = await geom(page);
			const a = { x: Math.round(g.left + g.width * 0.3), y: g.y };
			const b = { x: Math.round(g.left + g.width * 0.7), y: g.y };
			const [from, to] = [instantOf(g, a.x), instantOf(g, b.x)];

			await page.mouse.move(a.x, a.y);
			await page.mouse.down();
			// The wait's own mark is down before the finger has moved at all:
			// how a finger knows it has stopped panning. Nothing is selected
			// yet — the gesture is still undecided.
			// `attached`, not `visible`: the mark is a 0x0 anchor whose ring and
			// stem are drawn AROUND it, so it has no box of its own to be
			// visible by.
			await page.locator(`${STRIP} .hold-mark.held`).waitFor({ state: 'attached' });
			assert.deepEqual(await pillsOf(page), [], 'the wait alone selected something');

			await page.mouse.move(b.x, b.y, { steps: 8 });
			// And the first pixel of travel is what drops the bar: one
			// instant, the range's leading edge, before the finger arrives.
			await saysPills(page, rangePills(from, to));
			await page.mouse.up();
			await saysPills(page, rangePills(from, to));
			assert.equal((await drawn(page, TODAY)).lines, 2);
			await page.locator(`${STRIP} .hold-mark`).waitFor({ state: 'detached' });
		});
	});

	// The other half: the same wait, ended by a LIFT instead of a drag, is a
	// TIME — vividn, da-xg1vi: «long tap / right click → select a time» —
	// on a mark as much as off one, where a tap would take the entry.
	test('a long press that lifts where it fell is the time, on a mark too', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const mark = await markAt(page, TODAY);
			await page.mouse.move(mark.x, mark.y);
			await page.mouse.down();
			await longPressed(page);
			await page.mouse.up();
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, mark.x))}`]);
			assert.equal((await drawn(page, TODAY)).lines, 1, 'a still long press made a range');
		});
	});

	test('a right click is the time, on a mark too', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const mark = await markAt(page, TODAY);
			await page.mouse.click(mark.x, mark.y, { button: 'right' });
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, mark.x))}`]);
		});
	});

	test('shift+drag is the same range without the wait', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			await zoomIn(app, await geom(page));
			const g = await geom(page);
			const a = { x: Math.round(g.left + g.width * 0.3), y: g.y };
			const b = { x: Math.round(g.left + g.width * 0.7), y: g.y };
			await drag(page, a, b, { shift: true });
			await saysPills(page, rangePills(instantOf(g, a.x), instantOf(g, b.x)));
			assert.deepEqual(await windowHeard(app), await windowHeard(app));
		});
	});
});

// ── one selection, two views ───────────────────────────────────────────

describe('a range made in the strip is the page’s one selection', () => {
	test('the nowview draws it as its band, and does not re-spell its pills', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			await page.locator(`${NOWVIEW} svg.svg .gridline`).first().waitFor({ state: 'attached' });
			const g = await geom(page);
			// Inside the nowview's own half-hour window, so it has it to draw.
			const [a, b] = [spot(g, NOW - 24 * MINUTE), spot(g, NOW - 2 * MINUTE)];
			const want = rangePills(instantOf(g, a.x), instantOf(g, b.x));

			await drag(page, a, b, { hold: true });
			await saysPills(page, want);

			await until(
				() => drawn(page, NOWVIEW).then((d) => d.span),
				(span) => span !== null && span.w > 0
			);
			// The pills are the view the range was MADE in's, whoever draws it.
			assert.deepEqual(await pillsOf(page), want);
		});
	});

	test('the pills are in the strip’s own margin too, over their marks, and ✕ there clears both', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			const g = await geom(page);
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)), { hold: true });
			await until(
				() => pillsOf(page),
				(seen) => seen.length === 2
			);

			const margin = page.locator(`${STRIP} .daystrip-margin`);
			await margin.locator('.pill.time').waitFor();
			assert.equal(await page.locator('.prompt-bar .pill.time').count(), 1, 'the bar lost its copy');

			// BRACKETING the range: the start pill's right edge against the
			// band's leading edge, the end pill's left edge against the
			// arrow at its head. Two pills this wide centred on their marks
			// would be drawn one over the other.
			const [end, start, marks, strip] = await Promise.all([
				margin.locator('.pill.time').boundingBox(),
				margin.locator('.pill.dur').boundingBox(),
				drawn(page, TODAY),
				page.locator(STRIP).boundingBox()
			]);
			assert.ok(
				Math.abs(start.x + start.width - marks.span.left) <= 2,
				`the start pill ends at ${start.x + start.width}, the band begins at ${marks.span.left}`
			);
			assert.ok(
				Math.abs(end.x - (marks.span.left + marks.span.w)) <= 2,
				`the end pill begins at ${end.x}, the band ends at ${marks.span.left + marks.span.w}`
			);
			assert.ok(start.x >= strip.x - 1, 'the start pill hangs off the strip');
			assert.ok(end.x + end.width <= strip.x + strip.width + 1, 'the end pill hangs off the strip');

			// × on the START pill takes the LENGTH away and leaves the
			// instant, which is a single-instant selection again — and a
			// single instant says `-t`, not a keyed edge.
			await margin.locator('.pill.dur button.remove').click();
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, xOf(g, msAt(g, 10, 30))))}`]);
			await until(
				() => drawn(page, TODAY).then((d) => d.span),
				(span) => span === null
			);

			// And × on the one left takes the selection away entirely.
			await page.locator(`${STRIP} .daystrip-margin .pill.time button.remove`).click();
			await saysPills(page, []);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
			await until(
				() => page.locator(`${STRIP} .daystrip-margin .pill`).count(),
				(n) => n === 0
			);
		});
	});
});

// ── the fields widget answers it, as it answers a nowview range ────────

describe('a strip range is a DURATION to the fields widget (da-qu16)', () => {
	test('the bar filters down to the blocks, and dismissing `start_time=` brings the rest back', async () => {
		await desktop.fresh(async (app) => {
			const { page } = app;
			// One field of each shape the filter cares about: a point, an
			// open block and a switch.
			for (const argv of [
				['occur', 'coffee', '-t', '08:00'],
				['start', 'sleep', '-t', '07:00'],
				['switch', 'mood', 'happy', '-t', '09:30']
			])
				assert.equal((await app.run(argv)).code, 0);
			await app.redraw();
			await page.locator(`${BAR} [data-field="sleep"]`).waitFor();
			await app.idle();
			await saysFields(page, ['coffee', 'mood', 'sleep']);

			const g = await geom(page);
			await drag(page, spot(g, msAt(g, 11)), spot(g, msAt(g, 12, 30)), { hold: true });
			await until(
				() => pillsOf(page),
				(seen) => seen.length === 2
			);

			// `hasDuration` asks the pill's KIND, and the strip's
			// `start_time=` wears the same one the nowview's `dur=` does
			// — so the rule da-qu16 wrote needs no second clause for it.
			await saysFields(page, ['mood', 'sleep']);

			await page.locator('.prompt-bar .pill.dur button.remove').last().click();
			await saysFields(page, ['coffee', 'mood', 'sleep']);
		});
	});
});

// ── vividn's table, on a phone (da-xg1vi) ──────────────────────────────

describe('a finger on the strip, row by row', () => {
	/** The strip on a phone, with a block on today to tap. */
	const onPhone = (fn) =>
		withStrip(async (app) => {
			assert.equal((await app.run(['start', 'walk', '-t', '12:00', '3h'])).code, 0);
			await app.redraw();
			await app.page.locator(`${TODAY} svg.svg [data-id]`).first().waitFor();
			await app.page.evaluate(() => window.datum.cursor.set(null));
			await fn(app);
		}, PHONE);

	test('a tap on a mark is its entry; a tap on the ground is the time', async () => {
		await onPhone(async (app) => {
			const { page } = app;
			const mark = await markAt(page, TODAY);
			await page.touchscreen.tap(mark.x, mark.y);
			await saysPills(page, [`hid:${mark.id}`]);

			// 13:30: after the walk, before now, and no mark of its own.
			const g = await geom(page);
			const ground = spot(g, msAt(g, 13, 30));
			// The time joins the entry rather than replacing it: an entry and a
			// time are what a `retime` is spelt with. Only a second tap on the
			// entry itself takes it back (the next test).
			await page.touchscreen.tap(ground.x, ground.y);
			await saysPills(page, [`hid:${mark.id}`, `time:-t ${hhmm(instantOf(g, ground.x))}`]);
		});
	});

	// vividn, 15:04: «first tap selects the piece of data, second tap then
	// selects time instead».
	test('a second tap on the entry selected is the time there instead', async () => {
		await onPhone(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const mark = await markAt(page, TODAY);
			await page.touchscreen.tap(mark.x, mark.y);
			await saysPills(page, [`hid:${mark.id}`]);
			await page.touchscreen.tap(mark.x, mark.y);
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, mark.x))}`]);
		});
	});

	test('a long tap is the time, on a mark too; a long tap and a drag is a period', async () => {
		await onPhone(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const mark = await markAt(page, TODAY);
			await finger(page, mark, mark, { hold: true });
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, mark.x))}`]);

			const [a, b] = [spot(g, msAt(g, 14)), spot(g, msAt(g, 17))];
			await finger(page, a, b, { hold: true });
			await saysPills(page, rangePills(instantOf(g, a.x), instantOf(g, b.x)));
		});
	});

	// vividn, 14:40: «if you touch in the time axis and drag around, in this
	// case, do not pan but rather be able to precisely choose a time».
	test('the ruler is the time: a tap there, and a drag along it that follows the finger and pans nothing', async () => {
		await onPhone(async (app) => {
			const { page } = app;
			const g = await geom(page);
			const ruler = await page.locator(`${STRIP} .axis`).boundingBox();
			const y = ruler.y + ruler.height / 2;
			const [a, b] = [xOf(g, msAt(g, 9)), xOf(g, msAt(g, 15))];

			await page.touchscreen.tap(a, y);
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, a))}`]);

			const titles0 = await titles(page);
			await finger(page, { x: b, y }, { x: a + 5, y });
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, a + 5))}`]);
			assert.deepEqual(await windowHeard(app), [0, 24], 'the scrub panned');
			assert.deepEqual(await titles(page), titles0, 'the scrub swiped the layout');
		});
	});
});

// ── whose the gesture is ───────────────────────────────────────────────

describe('what the bands take, and what they leave', () => {
	test('a band declares the axis and the date column does not', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			// THE RULE the swipe below is a consequence of: `pannersUnder`
			// (PaneFrame.svelte) walks from the finger up to the frame and
			// yields the gesture to anything declaring the axis. Read at the
			// pixel a finger would land on, which is the half a passing
			// swipe test cannot show.
			const declared = (selector) =>
				page.locator(selector).evaluate((el) => {
					const box = el.getBoundingClientRect();
					const out = [];
					for (
						let e = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
						e && e.tagName !== 'BODY';
						e = e.parentElement
					) {
						const pans = e.getAttribute?.('data-pans');
						if (pans) out.push(pans);
						if (e.tagName === 'SECTION') break;
					}
					return out;
				});
			assert.deepEqual(await declared(`${TODAY} svg.svg`), ['x'], 'the band did not take the axis');
			assert.deepEqual(await declared(`${STRIP} .row.pinned .stamp`), [], 'the date column took the axis');
		});
	});

	test('a finger down a band still scrolls the stack back through the days', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				const at = () => page.locator('.stack').evaluate((el) => el.scrollTop);
				const rest = await at();
				assert.ok(rest > 200, `the phone's strip holds nothing to scroll: ${rest}`);

				// A band in the SCROLLER: today's is pinned below it and is
				// not what the stack scrolls.
				const box = await page.locator('.stack .band').last().boundingBox();
				await swipeNoWait(page, box.x + box.width / 2, box.y + box.height / 2, 0, 300, 10);
				await until(
					() => at(),
					(top) => top <= rest - 200
				);
				assert.deepEqual(await pillsOf(page), [], 'the scroll left a selection behind');
			},
			PHONE
		);
	});

	test('a slow finger down a band is still the scroll once the long press has passed', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				const at = () => page.locator('.stack').evaluate((el) => el.scrollTop);
				const rest = await at();
				assert.ok(rest > 200, `the phone's strip holds nothing to scroll: ${rest}`);

				const box = await page.locator('.stack .band').last().boundingBox();
				const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
				// «press» is the strip at rest as well as a press it dropped, so
				// the touch has to be one it takes: over a band with a day drawn,
				// which is what `begin` looks for.
				assert.ok(
					await page.evaluate(
						([x, y]) => !!document.elementFromPoint(x, y)?.closest('.band')?.querySelector('svg.svg [data-lo]'),
						[x, y]
					),
					'the finger is not over a day'
				);
				const cdp = await page.context().newCDPSession(page);
				const touch = (type, dy) =>
					cdp.send('Input.dispatchTouchEvent', { type, touchPoints: dy === undefined ? [] : [{ x, y: y + dy }] });
				// The fast finger above is the browser's long before the long
				// press, whatever the strip does. This one goes past the slop
				// (SLOP_PX, 6) and rests through the wait, which is the strip's
				// own vertical guard's to drop: a press it kept is a hold by
				// now, and a hold keeps the moves that follow from the stack.
				// The page's timers are held, so the long press cannot come
				// between the touch and its first move however late the move
				// is: it fires in `runFor`, on a press the guard has had.
				await touch('touchStart', 0);
				await touch('touchMove', 8);
				await page.clock.runFor(HOLD);
				assert.equal(await page.locator(STRIP).getAttribute('data-mode'), 'press', 'the strip kept a vertical press');
				for (let i = 1; i <= 10; i++) await touch('touchMove', 8 + (292 * i) / 10);
				await touch('touchEnd');
				await cdp.detach();
				await until(
					() => at(),
					(top) => top <= rest - 200
				);
				assert.deepEqual(await pillsOf(page), [], 'the scroll left a selection behind');
			},
			{ ...PHONE, holdTimers: true }
		);
	});

	// vividn, 2026-10-22 (da-xg1vi): «on dayview, let's only have the lines
	// that are between the day labels (not the rest of the day) be handles
	// for adjusting day height. Right now it's way too easy to accidentally
	// resize days on mobile when you're just trying to scroll.»
	test('a finger on the line between two days sizes them across the dates, and scrolls over the bands', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				const height = async () => Number(await page.locator('.today-line').getAttribute('aria-valuenow'));
				const at = () => page.locator('.stack').evaluate((el) => el.scrollTop);
				// The divider's line, over today: the stack's own lines lie under
				// the pane's title on this screen, which a finger's touch
				// adjustment snaps to.
				const middle = async () => {
					const box = await page.locator('.divider > .day-line').boundingBox();
					return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
				};

				// Across the dates, the line is the handle: up is taller.
				const was = await height();
				const dates = await middle();
				await swipe(page, dates.x, dates.y, 0, -20, 6);
				await until(height, (now) => now === was + 20);

				// Over the bands it is no handle: three pixels above it, where the
				// handle reached across the whole width until da-xg1vi, is the
				// band of the day above, and a finger down it is the stack's scroll.
				await page.waitForFunction(
					(want) => Number(document.querySelector('.today-line')?.getAttribute('aria-valuenow')) === want,
					was + 20
				);
				// Every band drawn again at the new height, and the stack pinned
				// back to today: each picture that lands grows its band and
				// re-pins the stack, so a rest read before the last is no rest.
				// On a loaded runner one landed as the swipe began, the stack
				// went 300 to its new bottom, and the swipe's 300 brought it
				// back to the stale rest (da-fgb1e).
				await app.idle();
				await page.waitForFunction(() => {
					const el = document.querySelector('.stack');
					return el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
				});
				const band = await page.locator('.stack .band').last().boundingBox();
				const line = await page.locator('.divider').boundingBox();
				const [x, y] = [band.x + band.width / 2, line.y - 3];
				assert.equal(
					await page.evaluate(([x, y]) => document.elementFromPoint(x, y)?.closest('.sizer, .band')?.className.split(' ')[0], [x, y]),
					'band',
					'the line is a handle over the bands'
				);
				const rest = await at();
				assert.ok(rest > 200, `the phone's strip holds nothing to scroll: ${rest}`);
				await swipeNoWait(page, x, y, 0, 300, 10);
				await until(at, (top) => top <= rest - 200);
				assert.equal(await height(), was + 20, 'a swipe down the line over a band sized the days');
				assert.deepEqual(await pillsOf(page), [], 'the scroll left a selection behind');
			},
			PHONE
		);
	});

	// vividn, 2026-10-22 (da-xg1vi): «in general panning around shouldn't
	// cause data to be selected or any tool tips to pop up».
	test('a scroll that starts on a mark leaves no label and no selection', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				// Yesterday's: today is pinned under the scroller on a phone.
				assert.equal((await app.run(['start', 'walk', '-y', '-t', '12:00', '3h'])).code, 0);
				await app.redraw();
				await page.locator(`${YESTERDAY} svg.svg [data-id]`).waitFor();
				const { x, y } = await markAt(page, YESTERDAY);
				const at = () => page.locator('.stack').evaluate((el) => el.scrollTop);
				const rest = await at();
				await swipeNoWait(page, x, y, 0, 200, 10);
				await until(at, (top) => top <= rest - 100);
				assert.equal(await page.locator('.tip:not([hidden])').count(), 0, 'the scroll labeled a mark');
				assert.deepEqual(await pillsOf(page), [], 'the scroll left a selection behind');
			},
			PHONE
		);
	});

	test('a pan of the nowview shows no label on the way, and selects nothing', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['start', 'walk', '-t', '-20'])).code, 0);
			await app.redraw();
			await page.locator(`${NOWVIEW} svg.svg [data-id]`).first().waitFor();
			// A mark on screen and clear of the now region, whose left third
			// the press lands on: the picture is drawn past the band either side.
			const { x, y } = await page.locator(`${NOWVIEW} .nowview-band`).evaluate((band) => {
				const frame = band.getBoundingClientRect();
				const r = [...band.querySelectorAll('svg.svg [data-id]')]
					.map((m) => m.getBoundingClientRect())
					.map((r) => ({ r, left: Math.max(r.left, frame.left), right: Math.min(r.right, frame.right) }))
					.find(({ left, right }) => right - left > 90);
				return { x: r.left + 30, y: r.r.top + r.r.height / 2 };
			});
			assert.ok(
				await page.evaluate(([x, y]) => !!document.elementFromPoint(x, y)?.closest('[data-id]'), [x, y]),
				'the press is not on a mark'
			);
			await page.evaluate(() => window.datum.cursor.set(null));
			await saysPills(page, []);
			await page.mouse.move(x, y);
			await page.evaluate(noWait);
			await page.mouse.down();
			// The picture follows the hand, so the mark stays under it all the way.
			await page.mouse.move(x - 40, y, { steps: 8 });
			assert.equal(await page.locator(`${NOWVIEW} .nowview-band`).getAttribute('data-mode'), 'pan');
			assert.equal(await page.locator('.tip:not([hidden])').count(), 0, 'the pan labeled a mark');
			await page.mouse.up();
			assert.equal(await page.locator('.tip:not([hidden])').count(), 0, 'the pan labeled a mark');
			// The press focused the mark, and a redraw after the lift keeps
			// that focus: a loaded runner drew one there by chance (da-tke7i).
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			await app.idle();
			assert.equal(await page.locator('.tip:not([hidden])').count(), 0, 'the redraw after the pan labeled a mark');
			assert.deepEqual(await pillsOf(page), [], 'the pan selected something');
		});
	});

	// vividn, da-xg1vi: «simple swipe or drag → pan; or, if already at the
	// edge, pass the swipe to the container (so it switches tabs)». The
	// whole day is at both edges.
	test('a swipe on a zoomed band pans it; across the whole day it is the layout’s, as off the bands', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['main', 'tail']);

				// Zoomed in, with hours either side: the band's, so the layout
				// stays put and the window moves.
				await zoomIn(app, await geom(page));
				const window0 = await windowHeard(app);
				const band = await page.locator(TODAY).boundingBox();
				await swipe(page, band.x + band.width * 0.6, band.y + band.height / 2, -140, 0, 10);
				await settled(page);
				assert.ok((await titles(page)).includes('dayview'), 'a pan on a band moved the layout');
				await untilWindow(app, `w[0] > ${window0[0] + 0.25}`);
				assert.deepEqual(await pillsOf(page), [], 'the pan selected something');

				// The whole day: nothing to pan to, so the swipe is the root's.
				await page.locator('.strip button.unzoom').click();
				await untilWindow(app, 'w[0] === 0 && w[1] === 24');
				assert.equal(await page.locator(TODAY).getAttribute('data-pan-stop'), 'start end');
				// Held first, the same drag is a period, and the layout's no more
				// than it was: the strip took the move before the pane could.
				// Measured once the whole day is drawn again, not the zoomed one.
				await app.idle();
				const g = await geom(page);
				const [a, b] = [spot(g, msAt(g, 14)), spot(g, msAt(g, 11))];
				await finger(page, a, b, { hold: true });
				await saysPills(page, rangePills(instantOf(g, b.x), instantOf(g, a.x)));
				await settled(page);
				assert.ok((await titles(page)).includes('dayview'), 'a long-press drag moved the layout');
				await page.evaluate(() => window.datum.cursor.set(null));
				await saysPills(page, []);

				await swipe(page, band.x + band.width * 0.6, band.y + band.height / 2, -140, 0, 10);
				await until(
					() => titles(page),
					(seen) => seen.join() === 'tail'
				);
				assert.deepEqual(await pillsOf(page), [], 'the swipe selected time');
			},
			{
				...PHONE,
				prepare: (context) =>
					context.addInitScript(
						(json) => localStorage.setItem('datum.layout.root.v1', json),
						JSON.stringify({ names: ['main', 'tail'], active: 0 })
					)
			}
		);
	});

	test('a swipe that starts OFF the bands changes the layout', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				// The date column beside the band is the frame's room, as the
				// nowview's margin is: the swipe there reaches the root.
				const stamp = await page.locator(`${STRIP} .row.pinned .stamp`).boundingBox();
				await swipe(page, stamp.x + stamp.width / 2, stamp.y + stamp.height / 2, -140, 0, 10);
				await until(
					() => titles(page),
					(seen) => seen.join() === 'tail'
				);
			},
			{
				...PHONE,
				prepare: (context) =>
					context.addInitScript(
						(json) => localStorage.setItem('datum.layout.root.v1', json),
						JSON.stringify({ names: ['main', 'tail'], active: 0 })
					)
			}
		);
	});

	test('the pinch, the zoom reset and the controls are untouched', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			// ctrl+wheel still zooms and the `⊖` in the corner still gives
			// the day back (da-8rqb9, which took that from the double tap):
			// the one-pointer gesture is a new tenant of the strip, not a
			// new landlord.
			await zoomIn(app, await geom(page));
			await page.locator('.strip button.unzoom').click();
			await untilWindow(app, 'w[0] === 0 && w[1] === 24');

			// A press that begins on a day line is the line's: it sizes
			// the day and selects nothing. (Whatever the gestures above left
			// on the page — a click on a day points it at that time, da-wnt2
			// — the slate is wiped first.)
			await page.evaluate(() => window.datum.cursor.set(null));
			await saysPills(page, []);
			const grip = await page.locator('.today-line').boundingBox();
			const was = Number(await page.locator('.today-line').getAttribute('aria-valuenow'));
			await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
			await page.mouse.down();
			await page.mouse.move(grip.x + grip.width / 2, grip.y - 20, { steps: 5 });
			await page.mouse.up();
			await until(
				() => page.locator('.today-line').getAttribute('aria-valuenow'),
				(now) => Number(now) > was
			);
			assert.deepEqual(await pillsOf(page), [], 'the handle made a selection');
			assert.deepEqual(app.errors, []);
		});
	});
});
