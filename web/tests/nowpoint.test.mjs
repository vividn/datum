// The nowview is a place you point INTO (da-rskp).
//
// vividn, 2026-09-17: "Clicking on the nowview shouldn't recenter that time
// like clicking on the dayview does, rather it should just drop a selection
// line right there. If the -5 -10 -15 or -20 lines is clicked, use -q, -qq,
// -qqq, -qqqq as the pill. for other times use relative time as the pill …
// it would be nice to be able to scroll around in the nowview by clicking
// and dragging. However a long press and a drag should let you select a
// range of time for entering in a duration command … let's show the pills
// in the margin of the nowview … when nowview is not on "now", let's put a
// little button on the right hand side that says now->, this should also
// clear the time based pills."
//
// Delta-blind on purpose: every instant is read off the picture's own
// `data-` geometry and its own drawn gridlines, every pill off the bar's
// store, every window off the argv the seam was handed, and every stored
// row off what the CLI wrote — never off "something moved".
//
// The state machine the gestures run is `nowpoint-pure.test.mjs`, in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, sharedApp } from './support/app.mjs';
import { PHONE, activeTab, inRoom, maximizeButton, pane, pastTheWindow, pickOption, settled, swipe, toggleOption } from './support/layout.mjs';
import { drag, finger, swipeNoWait } from './support/press.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** 15:00 sharp on 2026-09-10: a minute boundary, so one minute of clock is one tick. */
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const MINUTE = 60_000;
const NOWVIEW = 'section.pane[data-title="nowview"]';
const BAND = `${NOWVIEW} .nowview-band`;
const MARGIN = `${NOWVIEW} .nowview-margin`;
const TODAY = '.band[data-days-ago="0"]';

/**
 * The app with `work` running since 14:00, so the nowview's 30-minute
 * window has a lane to draw and a block to close.
 *
 * The clock is pinned on both sides — the page's and the worker's, where
 * the CLI reads it — without `holdTimers`: every gesture here is a real
 * press over real milliseconds, and the long press is a TIMER
 * (#lib/nowpoint.ts), which held timers would freeze.
 */
async function withBand(fn, { ready, ...options } = {}) {
	const body = async (app, room) => {
		const logged = await app.run(['start', 'work', '-t', '14:00']);
		assert.equal(logged.code, 0, logged.stderr);
		await app.redraw();
		// `ready` brings the nowview to the front where a layout keeps it
		// behind a tab — a pane's `data-title` is its ACTIVE tab, so on a
		// phone there is no `nowview` pane until something selects it.
		if (ready) await ready(app);
		else await showBand(app);
		await fn(app, room);
	};
	// The desktop's tests share ONE app (sharedApp in support/app.mjs), each
	// in a room of its own: a pan is the nowview's own state, so a test
	// must not inherit the last one's. A phone is the context's.
	if (!Object.keys(options).length) return desktop.fresh(body);
	await withApp(body, { ...options, clock: NOW });
}

const desktop = sharedApp({ clock: NOW });

/** Wait until the nowview's band is drawn and every pane has settled. */
async function showBand(app, scope = NOWVIEW) {
	await app.page.locator(`${scope} svg.svg .gridline`).first().waitFor({ state: 'attached' });
	await app.idle();
}

/**
 * The picture's geometry, read off its own attributes and marks: the
 * window's span, where the axis is drawn, the yellow now-line if it drew
 * one, and every gridline with the minutes back from the window's end that
 * it marks.
 *
 * Independent of the app's own mapping on purpose — the same `data-lo` /
 * `data-hi` over `data-from` / `data-to` the CLI wrote, arithmetic done
 * here — so a test that agrees with the app is agreeing about the picture.
 */
// One round trip, the picture found and read in it: a pan's answer replaces
// the picture (da-6csum), and between a locator's finding and its reading
// that left the found one out of the page.
const geometry = (page, scope = NOWVIEW) =>
	page.evaluate((sel) => {
		const svg = document.querySelector(sel);
		const box = svg.getBoundingClientRect();
		const view = svg.viewBox.baseVal;
		const px = (u) => box.left + ((u - view.x) * box.width) / view.width;
		// Each stretch of the axis: a strip at a selected instant cuts it in
		// two there (da-zccjs), the time after it drawn a strip further on,
		// and a point is read off the stretch it is over, or the nearest.
		const stretches = [...svg.querySelectorAll('[data-lo]')].map((d) => ({
			lo: px(Number(d.getAttribute('data-lo'))),
			hi: px(Number(d.getAttribute('data-hi'))),
			from: Date.parse(d.getAttribute('data-from')),
			to: Date.parse(d.getAttribute('data-to'))
		}));
		const [drawnLo, drawnHi] = [stretches[0].lo, stretches.at(-1).hi];
		const off = (s, x) => Math.max(s.lo - x, x - s.hi);
		const at = (x) => {
			const s = stretches.reduce((a, b) => (off(b, x) < off(a, x) ? b : a));
			return s.from + ((x - s.lo) / (s.hi - s.lo)) * (s.to - s.from);
		};
		// The WINDOW: the picture is drawn past its box either side
		// (`--surround`, da-61nm8), and slid when a pan stays inside it. A
		// picture drawn AHEAD of now (`--ahead`, da-hklaz) draws on past its
		// now-line, behind it, so on now the window ends at that line.
		const frame = svg.closest('.picture').getBoundingClientRect();
		const ahead = svg.querySelector('.flow') && svg.querySelector('.current-time-line')?.getAttribute('data-now');
		const end = ahead ? px(Number(ahead)) : Infinity;
		const [lo, hi] = [Math.max(drawnLo, frame.left), Math.min(drawnHi, end, frame.right)];
		const mid = (el) => {
			const r = el.getBoundingClientRect();
			return r.left + r.width / 2;
		};
		const shown = (x) => x >= frame.left - 1 && x <= frame.right + 1;
		const line = svg.querySelector('.current-time-line');
		return {
			from: at(lo),
			to: at(hi),
			lo,
			hi,
			stretches,
			y: box.top + box.height / 2,
			nowLine: line && shown(mid(line)) ? mid(line) : null,
			marks: [...svg.querySelectorAll('.gridline')].filter((l) => shown(mid(l))).map((l) => ({
				x: mid(l),
				back: Math.round((at(hi) - at(mid(l))) / 60000)
			}))
		};
	}, `${scope} svg.svg`);

/** How many px one minute of the window is drawn as. */
const perMinute = (g) => {
	const s = g.stretches?.[0] ?? g;
	return (s.hi - s.lo) / ((s.to - s.from) / MINUTE);
};

/** A point on the band `back` minutes before the window's end. */
const at = (g, back) => ({ x: xOf(g, g.to - back * MINUTE), y: g.y });

/** Where on screen the instant `t` is drawn: on the stretch holding it, or the nearest. */
const xOf = (g, t) => {
	const off = (s) => Math.max(s.from - t, t - s.to);
	const s = g.stretches.reduce((a, b) => (off(b) < off(a) ? b : a));
	return s.lo + ((t - s.from) / (s.to - s.from)) * (s.hi - s.lo);
};

/** Where on screen the gridline `back` minutes before the window's end is drawn. */
async function gridline(page, back) {
	const g = await geometry(page);
	const mark = g.marks.find((m) => m.back === back);
	assert.ok(mark, `no gridline at −${back}m among ${JSON.stringify(g.marks.map((m) => m.back))}`);
	return { x: mark.x, y: g.y, g };
}

/** The pills as `kind:text` — what each PUTS on the command line, which is what it means. */
const pillsOf = (page) =>
	page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/** Wait until the pills say exactly `want`, and report what they said if they never do. */
const saysPills = (page, want) =>
	until(
		() => pillsOf(page),
		(seen) => seen.join('|') === want.join('|')
	);

/** Where the selection is DRAWN in `scope`: the arrow at its head, the band behind it, the lines. */
const drawn = (page, scope) =>
	page.locator(scope).evaluate((el) => {
		const mid = (s) => {
			const m = el.querySelector(`svg.svg ${s}`);
			if (!m) return null;
			const r = m.getBoundingClientRect();
			return { x: r.left + r.width / 2, w: r.width };
		};
		return {
			head: mid('.cursor-head'),
			span: mid('.cursor-span'),
			lines: el.querySelectorAll('svg.svg line.cursor').length
		};
	});

/** Every nowview argv the seam has been handed since the spy was armed, newest last. */
const windows = (page) => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview'));

/** The `--center` of the last nowview asked for; `null` while the window is on now, where it is none or relative. */
const asked = async (page) => {
	const argv = (await windows(page)).at(-1);
	const i = argv ? argv.indexOf('--center') : -1;
	return i < 0 || /^[+-]/.test(argv[i + 1]) ? null : argv[i + 1];
};

/**
 * Wait for the nowview on screen to be the picture drawn about `ms`, a whole
 * minute: its dashed line there (da-dv45l), and the picture the one last
 * asked for.
 */
const instantDrawn = (page, ms) =>
	page.waitForFunction(
		([sel, hhmm]) => {
			const svg = document.querySelector(sel);
			return svg?.querySelector('.instant-line')?.getAttribute('data-at')?.slice(11, 16) === hhmm && !svg.closest('.behind');
		},
		[`${NOWVIEW} svg.svg`, new Date(ms).toISOString().slice(11, 16)]
	);

/** The `-t` of the last nowview asked for — the instant its dashed line is at (da-dv45l); `null` for now. */
const instantAsked = async (page) => {
	const argv = (await windows(page)).at(-1);
	const i = argv ? argv.indexOf('-t') : -1;
	return i < 0 ? null : argv[i + 1];
};

/**
 * The instant drawn at `px` along the band — a pan that stays inside what
 * the picture drew slides the picture and asks datum for nothing
 * (da-61nm8), so where the window is is read off the screen, not off the
 * seam.
 */
const instantUnder = (page, px, read = geometry) =>
	read(page).then((g) => {
		const off = (s) => Math.max(s.lo - px, px - s.hi);
		const s = g.stretches?.reduce((a, b) => (off(b) < off(a) ? b : a)) ?? g;
		return s.from + ((px - s.lo) / (s.hi - s.lo)) * (s.to - s.from);
	});

/** Type a line into the bottom bar and wait for its receipt — the pills ride with it. */
const receiptNow = () => document.querySelector('.prompt-bar .group:last-child .doc .receipt')?.textContent ?? '';

async function barRun(page, line) {
	const before = await page.evaluate(receiptNow);
	await page.locator('.prompt-bar input').fill(line);
	await page.locator('.prompt-bar input').press('Enter');
	await page.waitForFunction(
		`(${receiptNow})() !== ${JSON.stringify(before)} && (${receiptNow})() !== ''`
	);
	// What the command did, CREATE or EXISTS: the label is the entry's shape, and this its title (da-hxjrk).
	return page.locator('.prompt-bar .group:last-child .doc .action').last().getAttribute('title');
}

// ── a tap: a selection, and the window left alone ──────────────────────

describe('a tap on the band drops a selection where it fell', () => {
	test('the −10m gridline says -qq, the line and the section land on it, and the window stays on now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x, mark.y);

			await saysPills(page, ['time:-qq']);
			// The window did not move — "it should just drop a selection line
			// right there" rather than recentre — but datum draws the pill's
			// dashed line (da-dv45l): asked for the same window, on now, about
			// the pill's instant.
			await until(
				() => instantAsked(page),
				(t) => t !== null && Date.parse(t) === NOW - 10 * MINUTE
			);
			assert.equal(await asked(page), null, 'the tap moved the window off now');

			const { head } = await until(
				() => drawn(page, NOWVIEW),
				(d) => d.head && page.locator(`${NOWVIEW} svg.svg .instant-line`).count().then((n) => n > 0)
			);
			assert.ok(head && Math.abs(head.x - mark.x) <= 1, `the line at ${head?.x}, the −10m gridline at ${mark.x}`);
		});
	});

	test('within a finger’s width of the gridline it is still -qq; well off it, the relative -t', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x + 8, mark.y);
			await saysPills(page, ['time:-qq']);

			// Three minutes along, to −7m: past the snap either way, and the
			// pill is the minutes themselves — read off the picture drawn
			// about the first pill, its line at −10m (da-dv45l).
			await instantDrawn(page, NOW - 10 * MINUTE);
			const between = xOf(await geometry(page), NOW - 7 * MINUTE);
			await page.mouse.click(between, mark.y);
			await saysPills(page, ['time:-t -7']);
			await instantDrawn(page, NOW - 7 * MINUTE);
			const { head } = await drawn(page, NOWVIEW);
			const seven = xOf(await geometry(page), NOW - 7 * MINUTE);
			assert.ok(Math.abs(head.x - seven) <= 2, `the line at ${head.x}, −7m at ${seven}`);
		});
	});

	test('the four -q marks are the four -q marks, and −25m is not one of them', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			for (const [back, want] of [
				[5, '-q'],
				[10, '-qq'],
				[15, '-qqq'],
				[20, '-qqqq'],
				[25, '-t -25']
			]) {
				const mark = g.marks.find((m) => m.back === back);
				assert.ok(mark, `no gridline at −${back}m`);
				await page.mouse.click(mark.x, g.y);
				await saysPills(page, [`time:${want}`]);
			}
		});
	});
});

// ── a short window says seconds ────────────────────────────────────────
//
// vividn, 2026-10-22, on a two-minute window: «you still can only use
// -t -[integer] taps, which is not nearly high enough resolution at this
// level. you should be able to do 5 seconds at this scale». The 30-minute
// window's whole minutes are the tests above, unchanged.

describe('a tap on a two-minute window is to the nearest five seconds', () => {
	test('20 s back says -t -20s, and the entry lands 20 s before the run', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			await pickOption(page, inRoom(room, 'nowview'), 'history', '2m');
			// The two-minute picture, asked for and drawn: the one before it
			// is taken down first, so there is a moment with none. It draws
			// some 20 minutes, eight windows before it (da-6fdaf), where the
			// 30-minute one drew 300.
			await page.waitForFunction((sel) => {
				const day = document.querySelector(sel);
				return day && Date.parse(day.dataset.to) - Date.parse(day.dataset.from) <= 30 * 60_000;
			}, `${NOWVIEW} svg.svg [data-lo]`);
			await showBand(app);
			// 21 s back, which no gridline is near and five seconds rounds to 20.
			const g = await geometry(page);
			await page.mouse.click(at(g, 21 / 60).x, g.y);
			await saysPills(page, ['time:-t -20s']);

			assert.match(await barRun(page, 'occur coffee'), /^CREATE$/);
			const [coffee] = (await app.entries()).filter((e) => e.field === 'coffee');
			assert.equal(
				new Date(Date.parse(coffee.occur_time.utc)).toISOString(),
				new Date(app.clock.at() - 20_000).toISOString()
			);
		});
	});
});

// da-9f8ne: panned OFF now the selection is absolute, and to the same step.
describe('a tap on a window panned off now is to the same step, said as a clock', () => {
	/**
	 * Pan the window back half of itself, tap where `target` is drawn, and
	 * run `occur coffee` at the pill it made: the pills and the entry's time.
	 */
	async function tapPanned(app, room, history, target) {
		const { page } = app;
		await pickOption(page, inRoom(room, 'nowview'), 'history', history);
		await page.waitForFunction(
			([sel, ms]) => {
				const day = document.querySelector(sel);
				return day && Date.parse(day.dataset.to) - Date.parse(day.dataset.from) <= ms;
			},
			// Drawn on now: eight windows before it (da-6fdaf), more on a narrow
			// band (da-vnfr3), and one ahead; the default's is longer.
			[`${NOWVIEW} svg.svg [data-lo]`, 40 * stepMs(history)]
		);
		await showBand(app);
		const g = await geometry(page);
		const middle = { x: (g.lo + g.hi) / 2, y: g.y };
		await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
		await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
		await app.idle();
		const panned = await until(
			() => geometry(page),
			(n) => n.nowLine === null && n.from <= target && target <= n.to
		);
		const x = xOf(panned, target);
		const near = panned.marks.filter((m) => Math.abs(m.x - x) <= 12);
		assert.deepEqual(near, [], 'a gridline is near enough to snap the tap to it');
		await page.mouse.click(x, g.y);
		const pills = await until(
			() => pillsOf(page),
			(seen) => seen.length === 1
		);
		assert.match(await barRun(page, 'occur coffee'), /^CREATE$/);
		const [coffee] = (await app.entries()).filter((e) => e.field === 'coffee');
		return { pills, logged: Date.parse(coffee.occur_time.utc) };
	}

	/** A window's length as the ladder spells it, in ms. */
	const stepMs = (step) => Number(step.slice(0, -1)) * MINUTE;

	test('two minutes: 14:58:21 says -t 14:58:20, and the entry lands there', async () => {
		await withBand(async (app, room) => {
			const { pills, logged } = await tapPanned(app, room, '2m', NOW - 98_500);
			assert.deepEqual(pills, ['time:-t 14:58:20']);
			assert.equal(new Date(logged).toISOString(), new Date(NOW - 100_000).toISOString());
		});
	});

	// 14:32:20, not 14:31:20 as it was: the now strip's room is taken from
	// a panned window too (da-6csum, da-dv45l), and on a desktop's narrow nowview that
	// put 14:31:20 within a fingertip of the 14:30 gridline.
	test('thirty minutes: 14:32:20 is still -t 14:32, as it always was', async () => {
		await withBand(async (app, room) => {
			const { pills, logged } = await tapPanned(app, room, '30m', NOW - 27 * MINUTE - 40_000);
			assert.deepEqual(pills, ['time:-t 14:32']);
			assert.equal(new Date(logged).toISOString(), new Date(NOW - 28 * MINUTE).toISOString());
		});
	});
});

// ── the keyboard ───────────────────────────────────────────────────────
//
// vremya on PR #48: main made `-t 14:30` on Enter and the band's first head
// made no pill, because the `picture` action's `enter` returned early under
// `data-points="own"` along with its click.

describe('the keyboard can still select in the band', () => {
	test('Enter on a focused mark is the mark’s own time, and does not move the window', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await page.locator(`${BAND} svg.svg [data-state]`).first().focus();
			await page.keyboard.press('Enter');

			// A mark's time is a fact of the drawing, so the pill is absolute
			// even while the window is on now — and for a block that began
			// before the window it is where the block is DRAWN from
			// (`drawnStart` in #lib/hover.ts) — which, the picture being
			// drawn a window's worth before its box (da-61nm8), is the
			// block's own 14:00 rather than the window's 14:30.
			await saysPills(page, ['time:-t 14:00']);
			// And the window is the band's own: a selection made in here never
			// moves it, by the keyboard as by a finger. The pill's line is
			// drawn (da-dv45l), and the button on now's «now» says «reset».
			await instantDrawn(page, Date.parse('2026-09-10T14:00:00Z'));
			assert.equal(await asked(page), null, 'Enter in the band moved the window');
			assert.equal(await page.locator(`${NOWVIEW} .nowview-now`).innerText(), 'reset');
		});
	});
});

// ── relative means relative ────────────────────────────────────────────
//
// The trap this test exists for: `-q` pressed at 14:40 and run at 14:47
// logs at 14:42, and the white line was on the −5 gridline the whole time.

describe('a relative selection is measured from NOW, not from the press', () => {
	test('two minutes on: the nowview’s line has not moved, the dayview’s has, and a line logs at now − 7', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:00'])).code, 0);
				await app.redraw();
				await page.locator(`${NOWVIEW} svg.svg .gridline`).first().waitFor({ state: 'attached' });
				await page.locator(`${TODAY} svg.svg`).waitFor();
				await app.idle();

				// Seven minutes back, between the gridlines: `-t -7`.
				const g = await geometry(page);
				await page.mouse.click(at(g, 7).x, g.y);
				await saysPills(page, ['time:-t -7']);
				await until(
					() => drawn(page, NOWVIEW).then((d) => d.head),
					(head) => head !== null
				);

				const before = {
					now: (await drawn(page, NOWVIEW)).head,
					day: (await drawn(page, TODAY)).head
				};
				assert.ok(before.now, 'no line on the nowview');
				assert.ok(before.day, 'the same selection is not drawn on the dayview');

				// Two minutes of clock, on both sides of the seam: the CLI's
				// first, so the run the tick starts sees the new minute.
				for (let i = 0; i < 2; i++) await app.clock.forward(MINUTE);
				await app.idle();
				// Seven minutes back from a clock two minutes on is two
				// minutes LATER in absolute time, so on the dayview's
				// absolute axis the line moves forward.
				await until(
					() => drawn(page, TODAY).then((d) => d.head?.x ?? null),
					(x) => x !== null && x > before.day.x + 0.1
				);
				// The nowview is drawn again a minute on, its line where it was.
				await instantDrawn(page, NOW + 2 * MINUTE - 7 * MINUTE);
				const after = {
					now: (await drawn(page, NOWVIEW)).head,
					day: (await drawn(page, TODAY)).head
				};
				assert.ok(
					Math.abs(after.now.x - before.now.x) <= 1,
					`the nowview's line left its gridline: ${before.now.x} → ${after.now.x}`
				);
				assert.ok(after.day.x > before.day.x, `the dayview's line stood still: ${before.day.x} → ${after.day.x}`);

				// And a line typed with the pill is logged seven minutes
				// before the RUN, not before the press: 15:02 − 7 = 14:55.
				assert.match(await barRun(page, 'occur coffee'), /^CREATE$/);
				const [coffee] = (await app.entries()).filter((e) => e.field === 'coffee');
				assert.equal(
					new Date(Date.parse(coffee.occur_time.utc)).toISOString(),
					new Date(app.clock.at() - 7 * MINUTE).toISOString()
				);
			},
			// Held timers: the tick is a timer, and this test moves the
			// clock. Only a TAP is made here, and a tap needs no timer of
			// its own.
			{ clock: NOW, holdTimers: true }
		);
	});
});

// ── a drag pans ────────────────────────────────────────────────────────

describe('a drag along the band pans the window', () => {
	test('the window moves, the selection stays put, and «now» appears', async () => {
		await withBand(async (app) => {
			const { page } = app;
			// A selection first, so the pan can be seen to leave it alone.
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x, mark.y);
			await saysPills(page, ['time:-qq']);
			await instantDrawn(page, NOW - 10 * MINUTE);
			// On now, the button with a pill is «reset» (da-zccjs).
			assert.equal(await page.locator(`${NOWVIEW} .nowview-now`).innerText(), 'reset');

			const g = await geometry(page);
			const middle = { x: (g.lo + g.hi) / 2, y: g.y };
			// Rightward: the content follows the finger, so the window looks
			// EARLIER by the ten minutes of band it traveled.
			await drag(page, middle, { x: middle.x + 10 * perMinute(g), y: g.y });

			const centred = await until(
				() => instantUnder(page, middle.x),
				(t) => Math.abs(t - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000
			);
			assert.ok(
				Math.abs(centred - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000,
				`centred on ${new Date(centred).toISOString()}, not ~14:35`
			);
			// The picture slides, now's section off its right edge with now
			// (da-dv45l), and the button off now is the way back to it. The
			// selection is untouched: the same pill, the same minute.
			assert.deepEqual(await pillsOf(page), ['time:-qq']);
			assert.equal(await page.locator(`${NOWVIEW} .nowview-now`).innerText(), 'now →');
		});
	});

	test('a pan that keeps now in the window slides the picture and asks for nothing (da-61nm8)', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			let g = await geometry(page);
			// Back ten minutes: now is still in the window.
			await drag(page, { x: (g.lo + g.hi) / 2, y: g.y }, { x: (g.lo + g.hi) / 2 + 10 * perMinute(g), y: g.y });
			await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			await app.idle();
			g = await geometry(page);
			const x = (g.lo + g.hi) / 2;
			const was = await instantUnder(page, x);
			// On four: now stays inside, and so does the window in what was drawn.
			await drag(page, { x, y: g.y }, { x: x - 4 * perMinute(g), y: g.y });
			await until(
				() => instantUnder(page, x),
				(t) => Math.abs(t - (was + 4 * MINUTE)) <= 30_000
			);
			await app.idle();
			assert.deepEqual(await windows(page), [], 'a slide inside the drawing asked for a window');
		});
	});

	test('panned away: no yellow line, a tap is absolute, and «now» takes it home', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			const middle = { x: (g.lo + g.hi) / 2, y: g.y };
			await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
			await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			await app.idle();

			// The yellow line is NOW's alone and now is not in this window
			// (da-5flt): the CLI draws none.
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line === null
			);

			// A tap here is the absolute pill the dayview has always made —
			// vividn, 17:01: "off-now, yes use absolute time".
			// Where 14:45 was, 14:30 is now.
			await page.mouse.click(middle.x, middle.y);
			await saysPills(page, ['time:-t 14:30']);
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
			assert.deepEqual(await pillsOf(page), [], '«now» left the time pills behind');
			assert.equal(await page.locator(`${NOWVIEW} .nowview-now`).count(), 0);
			assert.equal((await drawn(page, NOWVIEW)).head, null, 'the selection outlived «now»');
		});
	});

	test('«now» clears a pill that came from the DAYVIEW too, and brings the window home', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			// A click on the dayview centres this window on the instant — the
			// one thing a nowview tap does NOT do — so the window is off now
			// and «now» is drawn. vividn, 17:01, on whether it should clear a
			// pill it did not make: "yes, that is what i want".
			await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 14, 20))));
			await until(
				() => asked(page),
				(t) => t !== null
			);
			await app.idle();
			assert.deepEqual(await pillsOf(page), ['time:-t 14:20']);

			// On now at once, from the picture kept drawn there: nothing is asked (da-y6jzs).
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
			assert.equal(await asked(page), '2026-09-10T14:20:00.000Z', '«now» asked for a picture');
			assert.deepEqual(await pillsOf(page), []);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
		});
	});

	// vremya on PR #48: asked at 12:00, panned to 11:45, and back at 12:00 a
	// minute later. `pointed` is derived over the minute clock and a Svelte
	// store calls its subscribers for every value, so the window followed
	// the selection again on each tick.
	test('a pan survives the MINUTE, when the window was pointed by the dayview first', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:00'])).code, 0);
				await app.redraw();
				await page.locator(`${NOWVIEW} svg.svg .gridline`).first().waitFor({ state: 'attached' });
				await app.idle();
				await app.spyOnRunner();

				// As a click on the dayview does: the window centres on it.
				await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 12, 0))));
				await until(
					() => asked(page),
					(t) => t === '2026-09-10T12:00:00.000Z'
				);
				await app.idle();

				const g = await geometry(page);
				const x = (g.lo + g.hi) / 2;
				await drag(page, { x, y: g.y }, { x: x + (g.hi - g.lo) / 2, y: g.y });
				const panned = await until(
					() => instantUnder(page, x),
					(t) => t < Date.UTC(2026, 8, 10, 11, 55)
				);
				await app.idle();

				// A minute of clock on both sides of the seam, and a beat for
				// anything the tick sets off.
				await app.clock.forward(MINUTE);
				await app.idle();
				await page.waitForTimeout(250);
				await app.idle();

				assert.ok(Math.abs((await instantUnder(page, x)) - panned) < 1000, 'the minute brought the window home');
				assert.equal(
					await page.locator(`${NOWVIEW} .nowview-now`).count(),
					1,
					'«now» is the only way home, so it is still there'
				);
			},
			{ clock: NOW, holdTimers: true }
		);
	});

	test('a pan survives a write: the window is re-asked for where it was, not back at now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			const middle = { x: (g.lo + g.hi) / 2, y: g.y };
			await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
			await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			await app.idle();
			const held = await instantUnder(page, middle.x);

			assert.equal((await app.run(['occur', 'tea', '-t', '14:10'])).code, 0);
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			await app.idle();
			assert.ok(Math.abs((await instantUnder(page, middle.x)) - held) < 1000, 'the write snapped the window back');
		});
	});
});

// ── the two range gestures ─────────────────────────────────────────────

describe('a range: two pills, and a block that ends where the range ends', () => {
	test('long-press then drag → a white band, `-q` and `dur=25m`; a line logs that block', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			// −5m back to −30m: twenty-five minutes inside a thirty-minute
			// window, ending on the `-q` mark.
			await drag(page, at(g, 5), at(g, 30), { hold: true });

			await saysPills(page, ['time:-q', 'dur:dur=25m']);
			// Its line stands at its end (da-dv45l), the window on now.
			await instantDrawn(page, NOW - 5 * MINUTE);
			const marks = await until(
				() => drawn(page, NOWVIEW),
				(m) => m.span && m.lines === 2
			);
			assert.ok(marks.span, 'no band was drawn for the range');
			assert.ok(marks.span.w > 20 * perMinute(g), `the band is ${marks.span.w}px, not 25 minutes of band`);
			assert.equal(marks.lines, 2, 'a range is drawn between two lines');
			assert.equal(await asked(page), null, 'a range moved the window');

			// The pills go into the argv after the verb, so the block ENDS at
			// the selection and its duration runs back from there (da-zwct).
			assert.match(await barRun(page, 'start posture'), /^CREATE$/);
			const [posture] = (await app.entries()).filter((e) => e.field === 'posture');
			assert.equal(posture.dur, 'PT25M', JSON.stringify(posture));
			assert.equal(
				new Date(Date.parse(posture.occur_time.utc)).toISOString(),
				new Date(NOW - 5 * MINUTE).toISOString()
			);
		});
	});

	// vremya on PR #48: after `start posture` the `-q` pill was BACK, the
	// cursor was still set, a white line was still drawn, and the next plain
	// `occur` was logged at it. A command spends both of a range's pills at
	// once, and the reconciler answered the missing `dur=` before the missing
	// time pill (#lib/cursor.ts).
	test('a command spends BOTH of a range’s pills: nothing is left, and the NEXT entry is logged at now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			await drag(page, at(g, 5), at(g, 20), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);

			assert.match(await barRun(page, 'start posture'), /^CREATE$/);
			const [posture] = (await app.entries()).filter((e) => e.field === 'posture');
			assert.equal(posture.dur, 'PT15M');
			assert.equal(
				new Date(Date.parse(posture.occur_time.utc)).toISOString(),
				new Date(NOW - 5 * MINUTE).toISOString()
			);

			// Both pills spent means the SELECTION is spent: no pill of either
			// kind, no cursor, and no line on any picture.
			await saysPills(page, []);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
			await until(
				() => page.locator('line.cursor').count(),
				(n) => n === 0
			);

			// And the proof of what the bug cost: a SECOND plain entry, with
			// no selection made for it, is logged NOW.
			assert.match(await barRun(page, 'occur coffee'), /^CREATE$/);
			const [coffee] = (await app.entries()).filter((e) => e.field === 'coffee');
			assert.equal(
				new Date(Date.parse(coffee.occur_time.utc)).toISOString(),
				new Date(NOW).toISOString(),
				'the next entry took the spent range’s time'
			);
		});
	});

	// The control vremya ran beside it: a tap's single pill behaved already,
	// and it has to go on behaving.
	test('a tap’s one pill is spent the same way, and leaves nothing behind', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x, mark.y);
			await saysPills(page, ['time:-qq']);
			assert.match(await barRun(page, 'occur water'), /^CREATE$/);
			await saysPills(page, []);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
		});
	});

	test('shift+drag does the same without the wait — a long press with a mouse is nobody’s habit', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			await drag(page, at(g, 5), at(g, 20), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);
		});
	});

	test('the range reads the same dragged either way: it ENDS at the later instant', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			await drag(page, at(g, 20), at(g, 5), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);
		});
	});

	test('a plain tap clears the range back to one instant; the dur pill’s × does too', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			await drag(page, at(g, 5), at(g, 20), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);

			await page.locator(`${MARGIN} .pill.dur button.remove`).click();
			await saysPills(page, ['time:-q']);
			await until(
				() => drawn(page, NOWVIEW).then((d) => d.span),
				(span) => span === null
			);

			await drag(page, at(g, 5), at(g, 20), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);
			const ten = await gridline(page, 10);
			await page.mouse.click(ten.x, ten.y);
			await saysPills(page, ['time:-qq']);
		});
	});
});

// ── the flow to make easiest ───────────────────────────────────────────

describe('the now region: a long press dragged LEFT is a duration ending NOW', () => {
	test('one `dur=` pill and no time pill; a line logs a block that ends now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			// The press lands on the now-line — the now panel's own edge —
			// and runs back to twelve minutes before now, which is the
			// window's end and stands a gap before the line (da-6csum), after
			// the long press: «swiping left/right must not select time»
			// (vividn, da-xg1vi), so a plain drag here pans like one anywhere.
			await drag(page, { x: g.nowLine + 2, y: g.y }, { x: g.hi - 12 * perMinute(g), y: g.y }, { hold: true });

			await saysPills(page, ['dur:dur=12m']);
			const marks = await drawn(page, NOWVIEW);
			assert.ok(marks.span && marks.span.w > 9 * perMinute(g), `the band is ${marks.span?.w}px`);

			assert.match(await barRun(page, 'start posture'), /^CREATE$/);
			const [posture] = (await app.entries()).filter((e) => e.field === 'posture');
			assert.equal(posture.dur, 'PT12M', JSON.stringify(posture));
			assert.equal(new Date(Date.parse(posture.occur_time.utc)).toISOString(), new Date(NOW).toISOString());
		});
	});
});

// ── the margin ─────────────────────────────────────────────────────────

describe('the pills live in the band’s margin too', () => {
	test('drawn in the margin over the mark it names AND in the bar; × in either clears both', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const mark = await gridline(page, 15);
			await page.mouse.click(mark.x, mark.y);
			await saysPills(page, ['time:-qqq']);

			const margin = page.locator(`${MARGIN} .pill.time`);
			await margin.waitFor();
			assert.equal(await page.locator('.prompt-bar .pill.time').count(), 1, 'the bar lost its copy');

			// Over the dashed line its instant is drawn as (da-dv45l), and
			// clear of the band.
			await instantDrawn(page, NOW - 15 * MINUTE);
			const section = page.locator(`${NOWVIEW} svg.svg line.instant-line`).last();
			const [pill, label, band] = await until(
				() => Promise.all([margin.boundingBox(), section.boundingBox(), page.locator(BAND).boundingBox()]),
				([p, l]) => Math.abs(p.x + p.width / 2 - (l.x + l.width / 2)) <= 2
			);
			const over = label.x + label.width / 2;
			assert.ok(Math.abs(pill.x + pill.width / 2 - over) <= 2, `the pill at ${pill.x}, the section at ${over}`);
			assert.ok(pill.y + pill.height <= band.y + 1, `the pill lies over the band: ${pill.y + pill.height} / ${band.y}`);

			// × in the margin takes the bar's copy and the line with it.
			await margin.locator('button.remove').click();
			await saysPills(page, []);
			assert.equal(await page.locator('.prompt-bar .pill.time').count(), 0);
			await until(
				() => drawn(page, NOWVIEW).then((d) => d.head),
				(h) => h === null
			);

			// And the other way about.
			await page.mouse.click(mark.x, mark.y);
			await saysPills(page, ['time:-qqq']);
			await page.locator('.prompt-bar .pill.time button.remove').click();
			await saysPills(page, []);
			await until(
				() => page.locator(`${MARGIN} .pill`).count(),
				(n) => n === 0
			);
		});
	});
});

// ── the way back ───────────────────────────────────────────────────────

describe('a selection the window has left behind', () => {
	test('the return button appears, and pressing it brings the window onto it', async () => {
		await withBand(async (app) => {
			const { page } = app;
			// An absolute selection, as a click on the dayview makes one: it
			// centres the nowview on the instant, which is what that click
			// has always done.
			await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 12, 0))));
			await app.idle();
			await until(
				() => drawn(page, NOWVIEW).then((d) => d.head),
				(h) => h !== null
			);
			// A whole window of pan takes it off screen.
			const g = await geometry(page);
			await drag(page, { x: (g.lo + g.hi) / 2, y: g.y }, { x: (g.lo + g.hi) / 2 + (g.hi - g.lo), y: g.y });
			await app.idle();

			const back = page.locator(`${NOWVIEW} .nowview-jump.selection`);
			await back.waitFor();
			// Drawn, if at all, in the surroundings past the window (da-61nm8).
			const head = (await drawn(page, NOWVIEW)).head;
			const shown = await geometry(page);
			assert.ok(!head || head.x < shown.lo || head.x > shown.hi, `the selection is drawn in a window without it, at ${head?.x}`);

			await back.click();
			await app.idle();
			await until(
				() => drawn(page, NOWVIEW).then((d) => d.head),
				(h) => h !== null
			);
			const onto = await geometry(page);
			const instant = Date.UTC(2026, 8, 10, 12, 0);
			assert.ok(onto.from <= instant && instant <= onto.to, `12:00 is not in ${new Date(onto.from).toISOString()}…`);
			assert.equal(await page.locator(`${NOWVIEW} .nowview-jump.selection`).count(), 0);
		});
	});
});

// ── the snapshot ───────────────────────────────────────────────────────

describe('the snapshot says «a selected time, not now»', () => {
	test('a selection crosses its chips with the white dotted line; with none, nothing is drawn', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			// The snapshot is «show history» turned off (da-hklaz).
			await toggleOption(page, inRoom(room, 'nowview'), 'show history');
			await app.idle();
			await until(
				() => page.locator(`${NOWVIEW} svg.svg .now-chunk`).count(),
				(n) => n > 0
			);
			await until(
				() => geometry(page).then((g) => g.marks.length),
				(n) => n === 0
			);
			assert.equal((await drawn(page, NOWVIEW)).lines, 0, 'a line with nothing selected');

			await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 14, 40))));
			// Found and read in ONE evaluation: the line is drawn again after
			// the selection lands, and one found first and read after can be
			// the detached copy, whose computed stroke is '' (da-bctg0).
			const lines = await until(
				() =>
					page
						.locator(NOWVIEW)
						.evaluate((el) =>
							[...el.querySelectorAll('svg.svg line.cursor')].map((l) => [
								getComputedStyle(l).stroke,
								l.getAttribute('stroke-dasharray')
							])
						),
				(ls) => ls.length === 1
			);
			assert.deepEqual(lines, [['rgb(255, 255, 255)', '3 3']]);
		});
	});
});

// ── the phone ──────────────────────────────────────────────────────────

/** Bring the nowview tab of `main`'s phone column to the front, and wait for it. */
async function toNowviewTab(app) {
	await pane(app.page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
	await showBand(app, BAND);
	// A drag on the band right after the switch is the frame's (da-d9p5m).
	await pastTheWindow(app.page);
}

describe('on a phone', () => {
	// vremya drove this on PR #48 and it DOES work — my own attempt started
	// the touch off the margin's centre line and read nothing. His call,
	// kept: the swipe is centred ON the strip, ten steps, and the tab is
	// read off the pane. This is the spec's own sentence, end to end.
	test('a swipe from the MARGIN changes the tab, either way; the same swipe on the band does not', async () => {
		await withBand(
			async (app) => {
				const { page } = app;
				// Re-read until there is a box: a swipe on the band pans the
				// window and redraws it, and `boundingBox()`, which waits for
				// the element to be attached but not for it to be drawn, is
				// null for one caught mid-redraw (da-kptka; the shape
				// bada9af4 fixed in inspect).
				const from = async (selector) => {
					const b = await until(
						() => page.locator(selector).boundingBox(),
						(box) => box !== null
					);
					return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
				};

				// The margin is the pane's room (da-um7y's "a little padding so
				// that swiping is still possible"). `side` holds tail | nowview
				// | custom with nowview in front, so a leftward swipe is the
				// next tab and a rightward one the tab before — and the nowview
				// is brought back in front between them, because a pane's
				// `data-title` IS its active tab and the margin goes with it.
				for (const [dx, want] of [
					[-160, 'custom'],
					[160, 'tail']
				]) {
					const m = await from(MARGIN);
					await swipe(page, m.x - dx / 2, m.y, dx, 0, 10);
					await until(
						() => activeTab(page, 'side'),
						(tab) => tab === want
					);
					await toNowviewTab(app);
				}

				// The band is the window's: the same swipe leaves the tab alone.
				await app.spyOnRunner();
				for (const dx of [-160, 160]) {
					const b = await from(`${BAND} svg.svg`);
					await swipeNoWait(page, b.x - dx / 2, b.y, dx, 0, 10);
					await settled(page);
					assert.equal(await activeTab(page, 'side'), 'nowview', `a ${dx}px drag on the band moved the tab`);
				}
				// And it panned instead, which is the other half of the same rule:
				// on toward the future it stops at now (da-v8gzq), and back it
				// leaves now, with the button home.
				await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			},
			{ ...PHONE, ready: toNowviewTab }
		);
	});

	// vividn's table, 2026-10-22 (da-xg1vi), «let's also do that on the
	// nowview for consistency»: a tap on data is the entry, the time axis
	// and a long tap are the time, and a drag along the axis scrubs it.
	test('a finger, row by row: tap a mark, tap the axis, long tap, long tap and drag', async () => {
		await withBand(
			async (app) => {
				const { page } = app;
				// A mark's on-screen part, clipped to the band: the picture is
				// drawn past it either side. Its left end, clear of the now region.
				const work = await page.locator(BAND).evaluate((band) => {
					const frame = band.getBoundingClientRect();
					const hit = [...band.querySelectorAll('svg.svg [data-id]')]
						.map((m) => ({ m, r: m.getBoundingClientRect() }))
						.map(({ m, r }) => ({ m, r, left: Math.max(r.left, frame.left), right: Math.min(r.right, frame.right) }))
						.find(({ left, right }) => right - left > 60);
					return { id: hit.m.getAttribute('data-id'), x: hit.left + 20, y: hit.r.top + hit.r.height / 2 };
				});
				await page.touchscreen.tap(work.x, work.y);
				await saysPills(page, [`hid:${work.id}`]);

				const axis = await page.locator(`${BAND} svg.svg path.axis`).boundingBox();
				const y = axis.y + axis.height / 2;
				// The axis between two labels: each label is a handle (da-57vj4),
				// and the axis scrubs in the gaps between them.
				const gap = await page.locator(NOWVIEW).evaluate((p) => {
					const hs = [...p.querySelectorAll('.nowview-handles .handle')].map((h) => h.getBoundingClientRect());
					return (hs[0].right + hs[1].left) / 2;
				});
				await page.touchscreen.tap(gap, y);
				// The time joins the entry: the two are a `retime`.
				await until(
					() => pillsOf(page),
					(seen) => seen.length === 2 && seen[0] === `hid:${work.id}` && seen[1].startsWith('time:')
				);

				await page.evaluate(() => {
					window.datum.cursor.set(null);
					window.datum.bar.removePill('hid');
				});
				await saysPills(page, []);
				await finger(page, work, work, { hold: true });
				await until(
					() => pillsOf(page),
					(seen) => seen.length === 1 && seen[0].startsWith('time:')
				);

				await finger(page, work, { x: work.x + 80, y: work.y }, { hold: true });
				await until(
					() => pillsOf(page),
					(seen) => seen.length === 2 && seen.some((p) => p.startsWith('dur:dur='))
				);
			},
			{ ...PHONE, ready: toNowviewTab }
		);
	});

	// vividn, 14:40: «if you touch in the time axis and drag around, in this
	// case, do not pan but rather be able to precisely choose a time».
	test('a drag along the time axis scrubs the time under the finger, to the minute, and pans nothing', async () => {
		await withBand(
			async (app) => {
				const { page } = app;
				const g = await geometry(page);
				const axis = await page.locator(`${BAND} svg.svg path.axis`).boundingBox();
				const y = axis.y + axis.height / 2;
				// 7½ and 13 minutes back: off every five-minute gridline, which a
				// tap on the band would snap to and a scrub does not.
				const [a, b] = [at(g, 7.5), at(g, 13)];
				await finger(page, { x: a.x, y }, { x: b.x, y });
				const want = await instantUnder(page, b.x);
				const got = await until(
					() => page.evaluate(() => +(window.datum.cursor.of()?.at ?? NaN)),
					(t) => Math.abs(t - want) <= MINUTE
				);
				assert.ok(Math.abs(got - want) <= MINUTE, 'the time is not where the finger stopped');
				// The window stays on now, the section at the scrubbed time.
				assert.notEqual((await geometry(page)).nowLine, null, 'the scrub panned the window off now');
				assert.equal(await activeTab(page, 'side'), 'nowview', 'the scrub swiped the tab');
			},
			{ ...PHONE, ready: toNowviewTab }
		);
	});

	test('the MARGIN declares no axis and the band and its controls do; a drag on the band pans', async () => {
		await withBand(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();

				// THE RULE the test above is a consequence of, kept beside it
				// at vremya's word: `pannersUnder` (PaneFrame.svelte) walks
				// from the finger up to the frame and yields the gesture to
				// anything declaring the axis. So the margin is the pane's room
				// exactly as long as nothing between it and its pane says
				// `data-pans` — read here at the pixel a finger would land on,
				// which is the half a swipe test cannot show when it passes.
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
				assert.deepEqual(await declared(MARGIN), [], 'the margin took the axis from the pane');
				assert.deepEqual(await declared(`${BAND} svg.svg`), ['x'], 'the band did not take the axis');
				// And the two controls drawn OVER the band, which are its
				// siblings in the DOM and would otherwise hand a drag that
				// starts on them to the pane (da-ndzl's test says so of the
				// steps, which the handles replaced, da-57vj4).
				for (const over of ['.nowview-handles', '.nowview-now']) {
					assert.equal(
						await page.locator(`${NOWVIEW} ${over}`).evaluate((el) => el.getAttribute('data-pans')),
						'x',
						`${over} does not declare the band's axis`
					);
				}

				// And end to end, with a real finger: a drag on the band pans
				// the window. That it does NOT move the tab is
				// `layout-swipe.test.mjs`'s, which drags the band on a phone.
				const g = await geometry(page);
				const was = await instantUnder(page, (g.lo + g.hi) / 2);
				await swipeNoWait(page, (g.lo + g.hi) / 2, g.y, 60, 0);
				await until(
					() => instantUnder(page, (g.lo + g.hi) / 2),
					(t) => t < was - MINUTE
				);
			},
			{
				...PHONE,
				ready: async (app) => {
					await toNowviewTab(app);
					// «now» is only drawn off now, and the assertions above
					// want it on screen: pan the window away first.
					const g = await geometry(app.page);
					await drag(app.page, { x: (g.lo + g.hi) / 2, y: g.y }, { x: (g.lo + g.hi) / 2 + 40, y: g.y });
					await app.page.locator(`${NOWVIEW} .nowview-now`).waitFor();
					await app.idle();
				}
			}
		);
	});
});

// ── turned on its side (da-78evn) ──────────────────────────────────────
//
// vividn, 2026-09-22: «I think others will want to visualize it vertically
// (most calendars are already vertically oriented for example)». The same
// gestures, measured along the axis the picture says it runs: DOWN.

/**
 * The nowview turned vertical, maximized, drawn, and settled.
 *
 * Maximized because a vertical window's minutes are the pane's HEIGHT,
 * and the desktop pane is short: at ~2px a minute every gesture below is
 * inside the 12px a press snaps by. The tall box is what a vertical
 * picture is for, and each test still checks the room it needs.
 *
 * `room` is the one `withBand` drew the test in: its panes' ids are
 * `main`'s under the room's name (`inRoom`).
 */
async function turn(app, room) {
	const { page } = app;
	await pickOption(page, inRoom(room, 'nowview'), 'orientation', 'vertical');
	await maximizeButton(page, inRoom(room, 'nowview')).click();
	await page.waitForFunction(
		(sel) => {
			const svg = document.querySelector(sel);
			return svg?.querySelector('g.day-data[data-axis="y"]') && svg.viewBox.baseVal.height > 400;
		},
		`${NOWVIEW} svg.svg`
	);
	await showBand(app);
}

/** `geometry` with time running down: `lo`/`hi` and each mark are screen y. */
const vgeometry = (page) =>
	page.evaluate((sel) => {
		const svg = document.querySelector(sel);
		const day = svg.querySelector('[data-lo]');
		const num = (n) => Number(day.getAttribute(n));
		const [from, to] = [Date.parse(day.getAttribute('data-from')), Date.parse(day.getAttribute('data-to'))];
		const box = svg.getBoundingClientRect();
		const view = svg.viewBox.baseVal;
		const px = (u) => box.top + ((u - view.y) * box.height) / view.height;
		const [drawnLo, drawnHi] = [px(num('data-lo')), px(num('data-hi'))];
		const at = (y) => from + ((y - drawnLo) / (drawnHi - drawnLo)) * (to - from);
		// The window, as `geometry` reads it.
		const frame = svg.closest('.picture').getBoundingClientRect();
		const ahead = svg.querySelector('.flow') && svg.querySelector('.current-time-line')?.getAttribute('data-now');
		const end = ahead ? px(Number(ahead)) : Infinity;
		const [lo, hi] = [Math.max(drawnLo, frame.top), Math.min(drawnHi, end, frame.bottom)];
		const mid = (el) => {
			const r = el.getBoundingClientRect();
			return r.top + r.height / 2;
		};
		const shown = (y) => y >= frame.top - 1 && y <= frame.bottom + 1;
		const line = svg.querySelector('.current-time-line');
		return {
			axis: day.getAttribute('data-axis'),
			from: at(lo),
			to: at(hi),
			lo,
			hi,
			x: box.left + box.width * 0.75,
			nowLine: line && shown(mid(line)) ? mid(line) : null,
			marks: [...svg.querySelectorAll('.gridline')].filter((l) => shown(mid(l))).map((l) => ({
				y: mid(l),
				back: Math.round((at(hi) - at(mid(l))) / 60000)
			}))
		};
	}, `${NOWVIEW} svg.svg`);

/** A point on the vertical band `back` minutes before the window's end. */
const vat = (g, back) => ({ x: g.x, y: g.hi - back * perMinute(g) });

describe('turned on its side, the band is pointed into along its own axis', () => {
	test('a tap on the −10m gridline says -qq; a long press dragged UP is a range', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			await app.spyOnRunner();
			await turn(app, room);
			const argv = (await windows(page)).at(-1);
			assert.deepEqual(argv.slice(argv.indexOf('--orientation'), argv.indexOf('--orientation') + 2), [
				'--orientation',
				'vertical'
			]);
			const g = await vgeometry(page);
			assert.equal(g.axis, 'y');
			assert.ok(g.hi > g.lo, 'time runs down the page');

			const mark = g.marks.find((m) => m.back === 10);
			assert.ok(mark, `no gridline at −10m among ${JSON.stringify(g.marks.map((m) => m.back))}`);
			await page.mouse.click(g.x, mark.y);
			await saysPills(page, ['time:-qq']);

			// From −10m: well clear of the 24px the now region reaches back.
			assert.ok(10 * perMinute(g) > 24 + 6, `ten minutes is ${10 * perMinute(g)}px`);
			await drag(page, vat(g, 10), vat(g, 30), { hold: true });
			await saysPills(page, ['time:-qq', 'dur:dur=20m']);
		});
	});

	test('from the now strip, a long press dragged UP is a duration ending now', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			await turn(app, room);
			const g = await vgeometry(page);
			assert.ok(12 * perMinute(g) > 24 + 12, `twelve minutes is ${12 * perMinute(g)}px`);
			await drag(page, { x: g.x, y: g.nowLine + 2 }, { x: g.x, y: g.nowLine - 12 * perMinute(g) }, { hold: true });
			await saysPills(page, ['dur:dur=12m']);
		});
	});

	test('a drag DOWN pans the window earlier, and «now» brings it home', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			await turn(app, room);
			await app.spyOnRunner();
			const g = await vgeometry(page);
			const middle = { x: g.x, y: (g.lo + g.hi) / 2 };
			await drag(page, middle, { x: g.x, y: middle.y + 10 * perMinute(g) });
			// The picture slides, now's section with it (da-dv45l).
			await app.idle();
			const centred = await until(
				() => instantUnder(page, middle.y, vgeometry),
				(t) => Math.abs(t - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000
			);
			assert.ok(
				Math.abs(centred - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000,
				`centred on ${new Date(centred).toISOString()}, not ~14:35`
			);
			// Now is below the window, and the button held to that edge says so.
			const home = page.locator(`${NOWVIEW} .nowview-now`);
			assert.equal(await home.innerText(), 'now ↓');
			await home.click();
			await until(
				() => vgeometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
		});
	});
});

// ── the now section (da-6csum, da-dv45l) ─────────────────────────────────
//
// vividn, 2026-10-27: «the pinned now on the side is kind of weird and I
// don't end up liking it, let's remove it. Keep the yellow now button along
// the axis, add a little -> to it.» «increase the size of the now section
// some too, maybe twice as big». «I meant when panning, so it is no longer
// pinned on the side, but rather it just disappears when panning around.
// When you are reset to now, then the now section still exists».

/** The now section as drawn: the band's two ends, the strip's chunk, the «now» written under it and the button. */
const section = (page) =>
	page.locator(NOWVIEW).evaluate((root) => {
		const ends = (r) => r && [r.left, r.right];
		const middle = (el) => el && ((r) => (r.left + r.right) / 2)(el.getBoundingClientRect());
		// The picture's own box, which the band holds, as `geometry` reads it.
		const band = root.querySelector('svg.svg').closest('.picture').getBoundingClientRect();
		// The chunk where it is on screen at all: one slid off the band is none.
		const chunk = root.querySelector('svg.svg .now-chunk')?.getBoundingClientRect();
		const word = root.querySelector('svg.svg .now-label');
		const button = root.querySelector('.nowview-now');
		return {
			band: ends(band),
			chunk: chunk && chunk.right > band.left + 0.5 && chunk.left < band.right - 0.5 ? ends(chunk) : null,
			word: word && { at: middle(word), shown: getComputedStyle(word).visibility !== 'hidden' },
			button: button && {
				at: middle(button),
				half: button.getBoundingClientRect().width / 2,
				text: button.textContent.trim(),
				fill: getComputedStyle(button).backgroundColor,
				ink: getComputedStyle(button).color
			}
		};
	});

/** A drag along the band by `minutes` of the window: positive drags right, toward the past. */
async function pan(page, minutes) {
	const g = await geometry(page);
	const x = g.lo + (g.hi - g.lo) * (minutes > 0 ? 0.25 : 0.75);
	await drag(page, { x, y: g.y }, { x: x + minutes * perMinute(g), y: g.y });
}

describe('the now section belongs to now (da-dv45l)', () => {
	for (const [size, options] of [
		['a desktop', {}],
		['a phone', { ...PHONE, ready: toNowviewTab }]
	]) {
		test(`on ${size}: a fifth of the band at the end, at now as it pans, gone with now, and «now →» brings the default back`, async () => {
			await withBand(async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				// The plot stands its margin, 2px, inside the band.
				const near = (a, b, what) => assert.ok(Math.abs(a - b) <= 3, `${what}: ${a} is not ${b}`);
				const width = (s) => s.band[1] - s.band[0];

				// The default: the strip at the right edge, a fifth of the band
				// (twice the tenth it was), «now» written in yellow under it,
				// and no button.
				const home = await section(page);
				near(home.chunk[1], home.band[1], 'on now, the strip ends');
				const strip = (width(home) - 4) / 5;
				assert.ok(home.chunk[1] - home.chunk[0] >= strip - 8, `a strip of ${home.chunk[1] - home.chunk[0]}px, not ~${strip}`);
				assert.equal(home.button, null, 'a button on the default window');
				assert.equal(home.word?.shown, true, '«now» is not written under the strip');

				// Three minutes into the past: now is still on screen, and the
				// strip with it, moved off toward the right edge, the yellow
				// button on the axis saying the way.
				await pan(page, 3);
				const edging = await until(
					() => section(page),
					(s) => s.button && s.chunk && s.chunk[0] < s.band[1] - 5 && s.chunk[1] > s.band[1] + 5
				);
				assert.deepEqual(
					[edging.button.text, edging.button.fill, edging.button.ink],
					['now →', 'rgb(255, 204, 0)', 'rgb(0, 0, 0)']
				);

				// Into the past: now off the right edge, and the strip gone with
				// it — no section at the side — the button held at that edge.
				await pan(page, 12);
				const past = await until(
					() => section(page),
					(s) => s.button && !s.chunk
				);
				assert.equal(past.chunk, null, `a section in a window now is not in: ${JSON.stringify(past)}`);
				assert.equal(past.button.text, 'now →');
				near(past.button.at + past.button.half, past.band[1], 'the button is not at the right edge');

				// On past now (da-v8gzq): the window stops at now, the default
				// again, with no button.
				await pan(page, -30);
				const stopped = await until(
					() => section(page),
					(s) => !s.button && s.chunk && Math.abs(s.chunk[1] - s.band[1]) <= 3
				);
				assert.equal(await asked(page), null, 'a pan on past now left the window off now');
				assert.equal(stopped.word?.shown, true);

				// Back again, and the button: the default window again, and no button.
				await pan(page, 12);
				await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
				await page.locator(`${NOWVIEW} .nowview-now`).click();
				const back = await until(
					() => section(page),
					(s) => !s.button && s.chunk && Math.abs(s.chunk[1] - s.band[1]) <= 3
				);
				assert.equal(await asked(page), null, 'the button left the window off now');
				assert.equal(back.word?.shown, true);
			}, options);
		});
	}

	// vividn, 15:14: «no, leave the scaling be, users can change scaling by
	// tapping the handles and might have different preferences, stretching
	// should always leave now where it is» — a handle's stretch, which scales
	// about now. A ctrl+wheel does the same on now, and zooms about the
	// pointer only once the window is panned off it (da-89vsn, vividn,
	// 2026-10-28: «if the nowview is reset to now, then I would just change
	// the scaling of the time axis and leave it on now. If you're panned
	// somewhere else, then zoom on the mouse cursor»).
	const history = async (page) => {
		const argv = (await windows(page)).at(-1);
		return argv?.[argv.indexOf('--history') + 1];
	};

	/** One notch of ctrl+wheel in at `x`: out would bring the time past now into the window, and it stops at now (da-v8gzq). */
	async function wheelIn(page, x, y) {
		await page.mouse.move(x, y);
		await page.keyboard.down('Control');
		await page.mouse.wheel(0, -200);
		await page.keyboard.up('Control');
	}

	test('on now, a ctrl+wheel stretch over the middle scales the window and leaves it on now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			await wheelIn(page, (g.lo + g.hi) / 2, g.y);
			await until(() => history(page), (h) => h !== undefined && h !== '30m');
			await app.idle();
			assert.equal(await asked(page), null, 'the stretch on now asked for a --center');
			const s = await until(
				() => section(page),
				(s) => s.chunk && Math.abs(s.chunk[1] - s.band[1]) <= 3
			);
			assert.equal(s.button, null, 'the stretch on now drew the «now» button');
		});
	});

	test('panned off now, a ctrl+wheel stretch keeps the instant under the pointer, and «now» keeps the stretch', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await pan(page, 10);
			await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			await app.idle();
			const g = await geometry(page);
			const x = g.lo + (g.hi - g.lo) * 0.3;
			const under = await instantUnder(page, x);
			await wheelIn(page, x, g.y);
			const stretched = await until(() => history(page), (h) => h !== undefined && h !== '30m');
			// The live rule's ask names the window before the wheel's pause commits it (da-gy4xz),
			// and «now» drops a stretch still in hand: wait for the gesture to be over.
			await page.waitForFunction((band) => document.querySelector(band).style.getPropertyValue('--zoom') === '1', BAND);
			await app.idle();
			assert.notEqual(await asked(page), null, 'the stretch panned off now went home');
			const after = await geometry(page);
			near(xOf(after, under), x, `the instant under the pointer moved on a stretch to ${stretched}`);

			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
			// The picture on now it shows is the one kept drawn at the stretch's window (da-y6jzs).
			const kept = await page.evaluate(() => window.__datumStandby.filter((a) => a[0] === 'nowview').at(-1));
			assert.equal(kept?.[kept.indexOf('--history') + 1], stretched, '«now» undid the stretch');
		});
	});
});

// ── a selected time (da-zccjs, da-dv45l) ────────────────────────────────
//
// vividn, 2026-10-27: «For selected times, don't display the outset block,
// just have the nowview as it would be with a dashed line and continuous in
// time around it». And of da-zccjs, still: «the yellow button should in this
// case say "reset", which then resets to being now, just like clearing the
// pill … when the view is panned away, scale about where the "now" would
// actually be, not the center of the window. The time handles should be
// relative to whatever the pill or now is».

/** `work` since 14:00 (the band's), `mood` calm at 14:20 and focused at 14:40, and a coffee at 14:50. */
async function withDay(fn) {
	await withBand(async (app) => {
		for (const line of ['switch mood calm -t 14:20', 'switch mood focused -t 14:40', 'occur coffee -t 14:50']) {
			const done = await app.run(line.split(' '));
			assert.equal(done.code, 0, done.stderr);
		}
		await app.redraw();
		await app.idle();
		await fn(app);
	});
}

/** Point the page at `ms` as a tap in the nowview does: the window stays where it is. */
const point = (page, ms) => page.evaluate((ms) => window.datum.cursor.point({ at: new Date(ms), from: 'nowview' }), ms);

/** The picture about a selected time, and what stands over and under it, in px across the page. */
const atInstant = (page) =>
	page.locator(NOWVIEW).evaluate((root) => {
		const svg = root.querySelector('svg.svg');
		const band = svg.closest('.picture').getBoundingClientRect();
		const mid = (el) => (el ? ((r) => (r.left + r.right) / 2)(el.getBoundingClientRect()) : null);
		const onScreen = (r) => r.width > 0 && r.right > band.left + 0.5 && r.left < band.right - 0.5;
		const rects = (sel) => [...svg.querySelectorAll(sel)].map((el) => el.getBoundingClientRect()).filter(onScreen);
		const state = (s) => rects(`.block[data-field="mood"][data-state="${s}"]`);
		const button = root.querySelector('.nowview-now');
		return {
			band: [band.left, band.right],
			chunks: rects('.now-chunk').map((r) => [r.left, r.right]),
			// Where calm ends and focused begins, on the timeline.
			calmEnds: Math.max(...state('calm').map((r) => r.right)),
			line: mid([...svg.querySelectorAll('line.instant-line')].at(-1)),
			label: svg.querySelector('.instant-label') !== null,
			dots: [...svg.querySelectorAll('.point-bg[data-field="coffee"]')].map((d) => ({ at: Date.parse(d.getAttribute('data-at')), x: mid(d) })),
			pill: mid(root.querySelector('.nowview-margin .margin-pill')),
			button: button && { text: button.textContent.trim(), at: mid(button) },
			ticks: Object.fromEntries(
				[...svg.querySelectorAll('.tick-label')]
					.filter((t) => getComputedStyle(t).visibility !== 'hidden')
					.map((t) => [t.textContent, mid(t)])
			)
		};
	});

const near = (a, b, what, by = 3) => assert.ok(Math.abs(a - b) <= by, `${what}: ${a} is not ${b}`);

describe('a selected time is a dashed line in a continuous timeline (da-dv45l)', () => {
	test('at a transition: no section there, the line where calm meets focused, the pill over it, «reset», and «reset» takes it all home', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await point(page, NOW - 20 * MINUTE);
			await instantDrawn(page, NOW - 20 * MINUTE);
			const s = await until(
				() => atInstant(page),
				(s) => s.line !== null && s.pill !== null && s.button !== null
			);
			// The timeline runs on through the instant: calm ends at the
			// line and focused begins there, with no section between.
			near(s.calmEnds, s.line, 'calm does not end at the line');
			assert.equal(s.label, false, 'a time written for the instant');
			// The one section is now's, at the end of the window on now.
			assert.equal(s.chunks.length, 2, `sections: ${JSON.stringify(s.chunks)}`);
			assert.ok(s.chunks.every(([l]) => l > s.line + 20), 'a section at the instant');
			near(s.pill, s.line, 'the pill');
			assert.equal(s.button.text, 'reset');

			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await saysPills(page, []);
			const home = await until(
				() => atInstant(page),
				(s) => !s.button && s.line === null && s.chunks.length > 0
			);
			near(Math.max(...home.chunks.map(([, r]) => r)), home.band[1], 'on now, the section ends the window');
		});
	});

	test('at a dot: the dot stays on the timeline, under the line', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await point(page, NOW - 10 * MINUTE);
			await instantDrawn(page, NOW - 10 * MINUTE);
			const s = await until(
				() => atInstant(page),
				(s) => s.line !== null && s.dots.some((d) => d.at === NOW - 10 * MINUTE)
			);
			near(s.dots.find((d) => d.at === NOW - 10 * MINUTE).x, s.line, 'the coffee');
		});
	});

	test('at a plain instant: the ticks counting from it, and a stretch about it', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const at = NOW - 22 * MINUTE;
			await point(page, at);
			await instantDrawn(page, at);
			const s = await until(
				() => atInstant(page),
				(s) => s.line !== null && s.ticks['-5m'] !== undefined && s.ticks['+5m'] !== undefined
			);
			// The ticks count from 14:38: five minutes either side of the
			// line stand as far from it.
			near(s.line - s.ticks['-5m'], s.ticks['+5m'] - s.line, '−5m and +5m from the line');

			// A ctrl+wheel keeps the line where it is wherever the pointer
			// is: a window on now with a pill scales about the pill, as a
			// handle's stretch does (da-zccjs, da-89vsn), not about the
			// pointer or its middle, and leaves now with the pill.
			const history = async () => {
				const argv = (await windows(page)).at(-1);
				return argv?.[argv.indexOf('--history') + 1];
			};
			const g = await geometry(page);
			await page.mouse.move(g.lo + (g.hi - g.lo) * 0.15, g.y);
			await page.keyboard.down('Control');
			// In: out would bring the time past now into the window, and it
			// stops at now (da-v8gzq).
			await page.mouse.wheel(0, -200);
			await page.keyboard.up('Control');
			const stretched = await until(history, (h) => h !== undefined && h !== '30m');
			await instantDrawn(page, at);
			await app.idle();
			const after = await atInstant(page);
			near(after.line, s.line, `the line moved on a stretch to ${stretched}`);
			const centre = Date.parse(await asked(page));
			const by = (Number.parseFloat(stretched) * (stretched.endsWith('h') ? 60 : 1)) / 30;
			near(centre, at + 7 * MINUTE * by, 'the window about 14:38', 1000);
		});
	});

	test('panned back: the line with its instant, no section at the side, and «now →» takes it home', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await point(page, NOW - 20 * MINUTE);
			await instantDrawn(page, NOW - 20 * MINUTE);
			// Back into the past: 14:20–14:50, the line at 14:40 inside it and
			// now past its end.
			await pan(page, 10);
			const back = await until(
				() => atInstant(page),
				(s) => s.line !== null && s.chunks.length === 0 && s.button?.text === 'now →'
			);
			near(back.calmEnds, back.line, 'calm does not end at the line');
			near(back.pill, back.line, 'the pill');
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await saysPills(page, []);
			await until(
				() => atInstant(page),
				(s) => !s.button && s.line === null && s.chunks.length > 0 && Math.abs(Math.max(...s.chunks.map(([, r]) => r)) - s.band[1]) <= 3
			);
		});
	});
});

// vividn, 2026-10-27 20:29:09: «no field names on nowview. Those keep sneaking
// back in and they don't look good». The CLI writes one only under
// `--labels`, which datum.day never asks for.
describe('the nowview names no field', () => {
	test('on now, panned and at a selected time: no text in the picture is a field’s name, and no --labels asked', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const fields = ['work', 'mood', 'coffee'];
			const named = () =>
				page
					.locator(`${NOWVIEW} svg.svg text`)
					.allTextContents()
					.then((texts) => texts.filter((t) => fields.some((f) => t.trim() === f || t.trim().startsWith(`${f}:`))));
			const look = async (what) => {
				await app.idle();
				assert.ok((await page.locator(`${NOWVIEW} svg.svg text`).count()) > 0, `${what}: no text at all to read`);
				assert.deepEqual(await named(), [], what);
			};
			await look('on now');
			await pan(page, 10);
			await until(
				() => atInstant(page),
				(s) => s.button?.text === 'now →'
			);
			await look('panned');
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await point(page, NOW - 20 * MINUTE);
			await instantDrawn(page, NOW - 20 * MINUTE);
			await look('at a selected time');
			const asked = await windows(page);
			assert.ok(asked.length > 0, 'no nowview asked');
			assert.deepEqual(
				asked.filter((argv) => argv.includes('--labels')),
				[]
			);
		});
	});
});
