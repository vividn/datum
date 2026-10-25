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
 * ($lib/nowpoint.ts), which held timers would freeze.
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
const geometry = (page, scope = NOWVIEW) =>
	page.locator(`${scope} svg.svg`).evaluate((svg) => {
		const day = svg.querySelector('[data-lo]');
		const num = (n) => Number(day.getAttribute(n));
		const [from, to] = [Date.parse(day.getAttribute('data-from')), Date.parse(day.getAttribute('data-to'))];
		const box = svg.getBoundingClientRect();
		const view = svg.viewBox.baseVal;
		const px = (u) => box.left + ((u - view.x) * box.width) / view.width;
		const [drawnLo, drawnHi] = [px(num('data-lo')), px(num('data-hi'))];
		const at = (x) => from + ((x - drawnLo) / (drawnHi - drawnLo)) * (to - from);
		// The WINDOW: the picture is drawn past its box either side
		// (`--surround`, da-61nm8), and slid when a pan stays inside it. A
		// picture drawn AHEAD of now (`--ahead`, da-hklaz) draws on past its
		// now-line, behind it, so on now the window ends at that line.
		const frame = svg.parentElement.getBoundingClientRect();
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
			y: box.top + box.height / 2,
			nowLine: line && shown(mid(line)) ? mid(line) : null,
			marks: [...svg.querySelectorAll('.gridline')].filter((l) => shown(mid(l))).map((l) => ({
				x: mid(l),
				back: Math.round((at(hi) - at(mid(l))) / 60000)
			}))
		};
	});

/** How many px one minute of the window is drawn as. */
const perMinute = (g) => (g.hi - g.lo) / ((g.to - g.from) / MINUTE);

/** A point on the band `back` minutes before the window's end. */
const at = (g, back) => ({ x: g.hi - back * perMinute(g), y: g.y });

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

/** The `-t` of the last nowview asked for; `null` while the window is on now. */
const asked = async (page) => {
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
	read(page).then((g) => g.from + ((px - g.lo) / (g.hi - g.lo)) * (g.to - g.from));

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

// ── a tap: a selection, and the window left alone ──────────────────────

describe('a tap on the band drops a selection where it fell', () => {
	test('the −10m gridline says -qq, the line lands on it, and no window is asked for', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x, mark.y);

			await saysPills(page, ['time:-qq']);
			// The window did not move: the seam was asked for no nowview at
			// all. That is the whole of "it should just drop a selection line
			// right there" rather than recentre.
			assert.deepEqual(await windows(page), [], 'the tap asked for a window');

			const { head } = await drawn(page, NOWVIEW);
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
			// pill is the minutes themselves.
			const five = (await geometry(page)).marks.find((m) => m.back === 5);
			const between = mark.x + (five.x - mark.x) * 0.6;
			await page.mouse.click(between, mark.y);
			await saysPills(page, ['time:-t -7']);
			const { head } = await drawn(page, NOWVIEW);
			assert.ok(Math.abs(head.x - between) <= 6, `the line at ${head.x}, the tap at ${between}`);
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
			// is taken down first, so there is a moment with none.
			await page.waitForFunction((sel) => {
				const day = document.querySelector(sel);
				return day && Date.parse(day.dataset.to) - Date.parse(day.dataset.from) <= 15 * 60_000;
			}, `${NOWVIEW} svg.svg [data-lo]`);
			await showBand(app);
			// 21 s back, which no gridline is near and five seconds rounds to 20.
			const g = await geometry(page);
			await page.mouse.click(at(g, 21 / 60).x, g.y);
			await saysPills(page, ['time:-t -20s']);

			assert.match(await barRun(page, 'occur coffee'), /^CREATE /);
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
			[`${NOWVIEW} svg.svg [data-lo]`, 4 * stepMs(history)]
		);
		await showBand(app);
		const g = await geometry(page);
		const middle = { x: (g.lo + g.hi) / 2, y: g.y };
		await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
		await page.locator(`${NOWVIEW} .nowview-jump.right`).waitFor();
		await app.idle();
		const panned = await until(
			() => geometry(page),
			(n) => n.nowLine === null && n.from <= target && target <= n.to
		);
		const x = panned.lo + ((target - panned.from) / (panned.to - panned.from)) * (panned.hi - panned.lo);
		const near = panned.marks.filter((m) => Math.abs(m.x - x) <= 12);
		assert.deepEqual(near, [], 'a gridline is near enough to snap the tap to it');
		await page.mouse.click(x, g.y);
		const pills = await until(
			() => pillsOf(page),
			(seen) => seen.length === 1
		);
		assert.match(await barRun(page, 'occur coffee'), /^CREATE /);
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

	test('thirty minutes: 14:31:20 is still -t 14:31, as it always was', async () => {
		await withBand(async (app, room) => {
			const { pills, logged } = await tapPanned(app, room, '30m', NOW - 28 * MINUTE - 40_000);
			assert.deepEqual(pills, ['time:-t 14:31']);
			assert.equal(new Date(logged).toISOString(), new Date(NOW - 29 * MINUTE).toISOString());
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
			// (`drawnStart` in $lib/hover.ts) — which, the picture being
			// drawn a window's worth before its box (da-61nm8), is the
			// block's own 14:00 rather than the window's 14:30.
			await saysPills(page, ['time:-t 14:00']);
			// And the window is the band's own: a selection made in here never
			// moves it, by the keyboard as by a finger.
			assert.deepEqual(await windows(page), [], 'Enter in the band asked for a window');
			assert.equal(await page.locator(`${NOWVIEW} .nowview-jump.right`).count(), 0);
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
				assert.match(await barRun(page, 'occur coffee'), /^CREATE /);
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
	test('the window moves with no nowview asked for, the selection stays put, and `now →` appears', async () => {
		await withBand(async (app) => {
			const { page } = app;
			// A selection first, so the pan can be seen to leave it alone.
			const mark = await gridline(page, 10);
			await page.mouse.click(mark.x, mark.y);
			await saysPills(page, ['time:-qq']);
			await app.spyOnRunner();
			assert.equal(await page.locator(`${NOWVIEW} .nowview-jump.right`).count(), 0, '`now →` while on now');

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
			// Inside what was drawn: the picture slid, and datum was not asked.
			assert.deepEqual(await windows(page), []);
			// The selection is untouched: the same pill, the same minute.
			assert.deepEqual(await pillsOf(page), ['time:-qq']);
			await page.locator(`${NOWVIEW} .nowview-jump.right`).waitFor();
		});
	});

	test('panned away: no yellow line, a tap is absolute, and `now →` takes it home', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			const middle = { x: (g.lo + g.hi) / 2, y: g.y };
			await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
			await page.locator(`${NOWVIEW} .nowview-jump.right`).waitFor();
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
			await page.locator(`${NOWVIEW} .nowview-jump.right`).click();
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
			assert.deepEqual(await pillsOf(page), [], '`now →` left the time pills behind');
			assert.equal(await page.locator(`${NOWVIEW} .nowview-jump.right`).count(), 0);
			assert.equal((await drawn(page, NOWVIEW)).head, null, 'the selection outlived `now →`');
		});
	});

	test('`now →` clears a pill that came from the DAYVIEW too, and brings the window home', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			// A click on the dayview centres this window on the instant — the
			// one thing a nowview tap does NOT do — so the window is off now
			// and `now →` is drawn. vividn, 17:01, on whether it should clear a
			// pill it did not make: "yes, that is what i want".
			await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 14, 20))));
			await until(
				() => asked(page),
				(t) => t !== null
			);
			await app.idle();
			assert.deepEqual(await pillsOf(page), ['time:-t 14:20']);

			await page.locator(`${NOWVIEW} .nowview-jump.right`).click();
			await until(
				() => asked(page),
				(t) => t === null
			);
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
					await page.locator(`${NOWVIEW} .nowview-jump.right`).count(),
					1,
					'`now →` is the only way home, so it is still there'
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
			await page.locator(`${NOWVIEW} .nowview-jump.right`).waitFor();
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
			const marks = await drawn(page, NOWVIEW);
			assert.ok(marks.span, 'no band was drawn for the range');
			assert.ok(marks.span.w > 20 * perMinute(g), `the band is ${marks.span.w}px, not 25 minutes of band`);
			assert.equal(marks.lines, 2, 'a range is drawn between two lines');
			assert.deepEqual(await windows(page), [], 'a range asked for a window');

			// The pills go into the argv after the verb, so the block ENDS at
			// the selection and its duration runs back from there (da-zwct).
			assert.match(await barRun(page, 'start posture'), /^CREATE /);
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
	// time pill ($lib/cursor.ts).
	test('a command spends BOTH of a range’s pills: nothing is left, and the NEXT entry is logged at now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			await drag(page, at(g, 5), at(g, 20), { shift: true });
			await saysPills(page, ['time:-q', 'dur:dur=15m']);

			assert.match(await barRun(page, 'start posture'), /^CREATE /);
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
			assert.match(await barRun(page, 'occur coffee'), /^CREATE /);
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
			assert.match(await barRun(page, 'occur water'), /^CREATE /);
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
			// and runs back twelve minutes, after the long press: «swiping
			// left/right must not select time» (vividn, da-xg1vi), so a plain
			// drag here pans like one anywhere.
			await drag(page, { x: g.nowLine + 2, y: g.y }, { x: g.nowLine - 12 * perMinute(g), y: g.y }, { hold: true });

			await saysPills(page, ['dur:dur=12m']);
			const marks = await drawn(page, NOWVIEW);
			assert.ok(marks.span && marks.span.w > 9 * perMinute(g), `the band is ${marks.span?.w}px`);

			assert.match(await barRun(page, 'start posture'), /^CREATE /);
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

			// Over the instant it names, and clear of the band.
			const [pill, head, band] = await Promise.all([
				margin.boundingBox(),
				drawn(page, NOWVIEW).then((d) => d.head),
				page.locator(BAND).boundingBox()
			]);
			assert.ok(Math.abs(pill.x + pill.width / 2 - head.x) <= 2, `the pill at ${pill.x}, the line at ${head.x}`);
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
				// And it panned instead, which is the other half of the same rule.
				await until(
					() => asked(page),
					(t) => t !== null
				);
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
				assert.equal(await page.locator(`${NOWVIEW} .nowview-jump.right`).count(), 0, 'the scrub panned the window off now');
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
				for (const over of ['.nowview-handles', '.nowview-jump.right']) {
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
					// `now →` is only drawn off now, and the assertions above
					// want it on screen: pan the window away first.
					const g = await geometry(app.page);
					await drag(app.page, { x: (g.lo + g.hi) / 2, y: g.y }, { x: (g.lo + g.hi) / 2 + 40, y: g.y });
					await app.page.locator(`${NOWVIEW} .nowview-jump.right`).waitFor();
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
	page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
		const day = svg.querySelector('[data-lo]');
		const num = (n) => Number(day.getAttribute(n));
		const [from, to] = [Date.parse(day.getAttribute('data-from')), Date.parse(day.getAttribute('data-to'))];
		const box = svg.getBoundingClientRect();
		const view = svg.viewBox.baseVal;
		const px = (u) => box.top + ((u - view.y) * box.height) / view.height;
		const [drawnLo, drawnHi] = [px(num('data-lo')), px(num('data-hi'))];
		const at = (y) => from + ((y - drawnLo) / (drawnHi - drawnLo)) * (to - from);
		// The window, as `geometry` reads it.
		const frame = svg.parentElement.getBoundingClientRect();
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
	});

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

	test('a drag DOWN pans the window earlier, and `now ↓` brings it home', async () => {
		await withBand(async (app, room) => {
			const { page } = app;
			await turn(app, room);
			await app.spyOnRunner();
			const g = await vgeometry(page);
			const middle = { x: g.x, y: (g.lo + g.hi) / 2 };
			await drag(page, middle, { x: g.x, y: middle.y + 10 * perMinute(g) });
			const centred = await until(
				() => instantUnder(page, middle.y, vgeometry),
				(t) => Math.abs(t - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000
			);
			assert.ok(
				Math.abs(centred - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000,
				`centred on ${new Date(centred).toISOString()}, not ~14:35`
			);
			const home = page.locator(`${NOWVIEW} .nowview-jump.right`);
			assert.equal(await home.innerText(), 'now ↓');
			await home.click();
			await until(
				() => vgeometry(page).then((n) => n.nowLine),
				(line) => line !== null
			);
		});
	});
});
