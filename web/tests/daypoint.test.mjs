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
// the picture's `getScreenCTM` — the same reading `$lib/hover.ts` makes —
// so the times the pills are expected to say are the times the pixels the
// test pressed ARE, and never a guess at what a minute is worth in px.
// The state machine the gestures run is `nowpoint-pure.test.mjs`, in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE, settled, swipe, titles } from './support/layout.mjs';
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
const NOWVIEW = 'section.pane[data-title="nowview"]';
const BAR = 'section.pane[data-title="fields"] .bar';
/** How long `$lib/nowpoint.ts` waits before a press is a range, with room to spare. */
const HOLD = 650;

/**
 * The app with a mark on today's band, the strip drawn, and the clock
 * pinned on both sides of the seam.
 *
 * `clock` is a FIXED time rather than an installed one: every gesture here
 * is a real press over real milliseconds and the long press is a TIMER, so
 * an installed clock would freeze the very thing under test.
 */
async function withStrip(fn, options = {}) {
	await withApp(async (app) => {
		assert.equal((await app.run(['occur', 'coffee', '-t', '08:00'])).code, 0);
		await app.reload();
		await app.page.locator(`${TODAY} svg.svg`).waitFor();
		await stackOfDays(app.page);
		await app.idle();
		await fn(app);
	}, { clock: NOW, ...DESKTOP, ...options });
}

/**
 * A day height HELD, short enough that YESTERDAY's whole band is on screen
 * — at the desktop size these tests use and at `PHONE` alike.
 *
 * ⚑ SINCE da-r0ydu A DAY FOLLOWS THE WIDGET until the reader sets one, so
 * a strip nobody has touched draws exactly ONE day and there is no band
 * but today's — and half this file points into YESTERDAY's. Nothing here
 * is about how tall a day is: this is how a reader gets a stack (the `+`
 * handle, whose value then holds), and 40 is `MIN_DAY_HEIGHT`, the most
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
	const box = await page.locator('button.day-grip').boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + 300, { steps: 8 });
	await page.mouse.up();
	await page.waitForFunction(
		() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 40
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
/** To the minute, as every selection is ($lib/cursor.ts). */
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

/** Every field button the bar draws, in the order it draws them. */
const shownFields = async (page) => (await listed(page.locator(BAR), '[data-field]')).map((el) => el.field);

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

/** A press, a drag and a lift with the mouse, `hold` ms before the first move. */
async function drag(page, from, to, { hold = 0, shift = false } = {}) {
	await page.mouse.move(from.x, from.y);
	if (shift) await page.keyboard.down('Shift');
	await page.mouse.down();
	if (hold) await page.waitForTimeout(hold);
	await page.mouse.move(to.x, to.y, { steps: 8 });
	await page.mouse.up();
	if (shift) await page.keyboard.up('Shift');
}

/** Type a line into the bottom bar and wait for its receipt — the pills ride with it. */
const receiptNow = () => document.querySelector('.prompt-bar .doc .receipt')?.textContent ?? '';

async function barRun(page, line) {
	const before = await page.evaluate(receiptNow);
	await page.locator('.prompt-bar input').fill(line);
	await page.locator('.prompt-bar input').press('Enter');
	await page.waitForFunction(
		`(${receiptNow})() !== ${JSON.stringify(before)} && (${receiptNow})() !== ''`
	);
	return page.locator('.prompt-bar .doc .receipt').last().innerText();
}

/**
 * The settings the strip widget is DRAWN from — what its owner last heard,
 * with whatever THIS SESSION holds over it. Since da-vhwq a move of the
 * strip's own (the zoom below is one) lands in the session and reaches the
 * tree only on «Save options with layout», so the tree alone is no longer
 * what the strip was told.
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
	return walk(window.datum.layout.current().root);
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

// ── zoomed all the way out: a drag IS a range ──────────────────────────

describe('the whole day on screen: a drag selects, with no long press', () => {
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

			await drag(page, a, b);

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
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)));
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
			await drag(page, b, a);
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
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)));
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

			await drag(page, a, b);
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
			// has travelled far further along than across. A mostly
			// VERTICAL finger is the stack's scroll and never a range —
			// which is the test below.
			const [a, b] = [
				spot(yesterday, msAt(yesterday, 8)),
				{ x: xOf(today, msAt(today, 20)), y: today.y }
			];
			await saysPills(page, []);
			await drag(page, a, b);
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
	// INSPECT and only a travelling one is a range ($lib/nowpoint.ts) — so
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
			await page.waitForTimeout(HOLD);
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

	// The other half of vividn's sentence, on the strip: «as long as it's not
	// a long press drag». The same wait, ended by a LIFT instead of a drag,
	// selects the entry under the finger — which is `inspect.test.mjs`'s to
	// prove; here it only has to not be a range.
	test('a long press that lifts where it fell selects no TIME at all', async () => {
		await withStrip(async (app) => {
			const { page } = app;
			await zoomIn(app, await geom(page));
			const g = await geom(page);
			// Above the lanes, on the band's own ground: no entry to inspect,
			// so the gesture falls through to the click the strip has always
			// taken, which is one instant and no range.
			const a = { x: Math.round(g.left + g.width * 0.3), y: g.y };
			await page.mouse.move(a.x, a.y);
			await page.mouse.down();
			await page.waitForTimeout(HOLD);
			await page.mouse.up();
			await saysPills(page, [`time:-t ${hhmm(instantOf(g, a.x))}`]);
			assert.equal((await drawn(page, TODAY)).lines, 1, 'a still long press made a range');
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

			await drag(page, a, b);
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
			await drag(page, spot(g, msAt(g, 9)), spot(g, msAt(g, 10, 30)));
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
		await withApp(
			async (app) => {
				const { page } = app;
				// One field of each shape the filter cares about: a point, an
				// open block and a switch.
				for (const argv of [
					['occur', 'coffee', '-t', '08:00'],
					['start', 'sleep', '-t', '07:00'],
					['switch', 'mood', 'happy', '-t', '09:30']
				])
					assert.equal((await app.run(argv)).code, 0);
				await app.reload();
				await page.locator(`${BAR} [data-field="sleep"]`).waitFor();
				await app.idle();
				// The fields on now first — sleep and mood, as high up as each
				// is drawn — then coffee (da-vu7pi).
				await saysFields(page, ['sleep', 'mood', 'coffee']);

				const g = await geom(page);
				await drag(page, spot(g, msAt(g, 11)), spot(g, msAt(g, 12, 30)));
				await until(
					() => pillsOf(page),
					(seen) => seen.length === 2
				);

				// `hasDuration` asks the pill's KIND, and the strip's
				// `start_time=` wears the same one the nowview's `dur=` does
				// — so the rule da-qu16 wrote needs no second clause for it.
				await saysFields(page, ['sleep', 'mood']);

				await page.locator('.prompt-bar .pill.dur button.remove').last().click();
				await saysFields(page, ['sleep', 'mood', 'coffee']);
			},
			{ clock: NOW, ...DESKTOP }
		);
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
				await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 0, 300, 10);
				await until(
					() => at(),
					(top) => top <= rest - 200
				);
				assert.deepEqual(await pillsOf(page), [], 'the scroll left a selection behind');
			},
			PHONE
		);
	});

	test('a swipe that starts OFF the bands still changes the layout; one on a band does not', async () => {
		await withStrip(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['main', 'tail']);

				// On a band: the strip's, so the layout stays put — and it
				// panned or selected instead, which the tests above are about.
				const band = await page.locator(TODAY).boundingBox();
				await swipe(page, band.x + band.width * 0.6, band.y + band.height / 2, -140, 0, 10);
				await settled(page);
				assert.ok((await titles(page)).includes('dayview'), 'a drag on a band moved the layout');

				// The date column beside it is the frame's room, as the
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

			// A press that begins on the `+` handle is the handle's: it sizes
			// the day and selects nothing. (Whatever the gestures above left
			// on the page — a click on a day points it at that time, da-wnt2
			// — the slate is wiped first.)
			await page.evaluate(() => window.datum.cursor.set(null));
			await saysPills(page, []);
			const grip = await page.locator('button.day-grip').boundingBox();
			const was = Number(await page.locator('button.day-grip').getAttribute('aria-valuenow'));
			await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
			await page.mouse.down();
			await page.mouse.move(grip.x + grip.width / 2, grip.y - 20, { steps: 5 });
			await page.mouse.up();
			await until(
				() => page.locator('button.day-grip').getAttribute('aria-valuenow'),
				(now) => Number(now) > was
			);
			assert.deepEqual(await pillsOf(page), [], 'the handle made a selection');
			assert.deepEqual(app.errors, []);
		});
	});
});
