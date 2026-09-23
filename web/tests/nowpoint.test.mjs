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
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';
import { PHONE, activeTab, maximizeButton, pane, pickOption, settled, swipe } from './support/layout.mjs';
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
 * the CLI reads it — with `setFixedTime` rather than `install`: every
 * gesture here is a real press over real milliseconds, and the long press
 * is a TIMER ($lib/nowpoint.ts), which an installed clock would freeze.
 */
async function withBand(fn, { ready, ...options } = {}) {
	const clock = workerClock(NOW);
	await withApp(
		async (app) => {
			const logged = await app.run(['start', 'work', '-t', '14:00']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.reload();
			// `ready` brings the nowview to the front where a layout keeps it
			// behind a tab — a pane's `data-title` is its ACTIVE tab, so on a
			// phone there is no `nowview` pane until something selects it.
			if (ready) await ready(app);
			else await showBand(app);
			await fn(app);
		},
		{
			...options,
			prepare: async (context) => {
				await context.clock.setFixedTime(new Date(NOW));
				await clock.prepare(context);
				await options.prepare?.(context);
			}
		}
	);
}

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
		const [lo, hi] = [px(num('data-lo')), px(num('data-hi'))];
		const mid = (el) => {
			const r = el.getBoundingClientRect();
			return r.left + r.width / 2;
		};
		const line = svg.querySelector('.current-time-line');
		return {
			from,
			to,
			lo,
			hi,
			y: box.top + box.height / 2,
			nowLine: line ? mid(line) : null,
			marks: [...svg.querySelectorAll('.gridline')].map((l) => ({
				x: mid(l),
				back: Math.round((to - (from + ((mid(l) - lo) / (hi - lo)) * (to - from))) / 60000)
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
			// (`drawnStart` in $lib/hover.ts), which is this window's 14:30.
			// The same instant vremya measured on main.
			await saysPills(page, ['time:-t 14:30']);
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
		const worker = workerClock(NOW);
		let now = NOW;
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:00'])).code, 0);
				await app.reload();
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
				for (let i = 0; i < 2; i++) {
					now += MINUTE;
					await worker.set(page, now);
					await page.clock.fastForward(MINUTE);
				}
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
					new Date(now - 7 * MINUTE).toISOString()
				);
			},
			{
				prepare: async (context) => {
					// Installed rather than fixed: the tick is a timer, and
					// this test moves the clock. Only a TAP is made here, and
					// a tap needs no timer of its own.
					await context.clock.install({ time: new Date(NOW) });
					await worker.prepare(context);
				}
			}
		);
	});
});

// ── a drag pans ────────────────────────────────────────────────────────

describe('a drag along the band pans the window', () => {
	test('the seam is asked for a window elsewhere, the selection stays put, and `now →` appears', async () => {
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
			// EARLIER by the ten minutes of band it travelled.
			await drag(page, middle, { x: middle.x + 10 * perMinute(g), y: g.y });

			await until(
				() => asked(page),
				(t) => t !== null
			);
			const centred = Date.parse(await asked(page));
			assert.ok(
				Math.abs(centred - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000,
				`centred on ${new Date(centred).toISOString()}, not ~14:35`
			);
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
			await until(
				() => asked(page),
				(t) => t !== null
			);
			await app.idle();

			// The yellow line is NOW's alone and now is not in this window
			// (da-5flt): the CLI draws none.
			await until(
				() => geometry(page).then((n) => n.nowLine),
				(line) => line === null
			);

			// A tap here is the absolute pill the dayview has always made —
			// vividn, 17:01: "off-now, yes use absolute time".
			const panned = await geometry(page);
			await page.mouse.click((panned.lo + panned.hi) / 2, panned.y);
			await saysPills(page, ['time:-t 14:30']);
			await page.locator(`${NOWVIEW} .nowview-jump.right`).click();
			await until(
				() => asked(page),
				(t) => t === null
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
		const worker = workerClock(NOW);
		let now = NOW;
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:00'])).code, 0);
				await app.reload();
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
				await drag(page, { x: (g.lo + g.hi) / 2, y: g.y }, { x: (g.lo + g.hi) / 2 + (g.hi - g.lo) / 2, y: g.y });
				await until(
					() => asked(page),
					(t) => t !== null && t !== '2026-09-10T12:00:00.000Z'
				);
				const panned = await asked(page);
				await app.idle();

				// A minute of clock on both sides of the seam, and a beat for
				// anything the tick sets off.
				now += MINUTE;
				await worker.set(page, now);
				await page.clock.fastForward(MINUTE);
				await app.idle();
				await page.waitForTimeout(250);
				await app.idle();

				assert.equal(await asked(page), panned, 'the minute brought the window home');
				assert.equal(
					await page.locator(`${NOWVIEW} .nowview-jump.right`).count(),
					1,
					'`now →` is the only way home, so it is still there'
				);
			},
			{
				prepare: async (context) => {
					await context.clock.install({ time: new Date(NOW) });
					await worker.prepare(context);
				}
			}
		);
	});

	test('a pan survives a write: the window is re-asked for where it was, not back at now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const g = await geometry(page);
			const middle = { x: (g.lo + g.hi) / 2, y: g.y };
			await drag(page, middle, { x: middle.x + (g.hi - g.lo) / 2, y: g.y });
			await until(
				() => asked(page),
				(t) => t !== null
			);
			await app.idle();
			const held = await asked(page);
			const span = await geometry(page).then((n) => [n.from, n.to]);

			assert.equal((await app.run(['occur', 'tea', '-t', '14:10'])).code, 0);
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			await app.idle();
			assert.equal(await asked(page), held, 'the write snapped the window back');
			assert.deepEqual(await geometry(page).then((n) => [n.from, n.to]), span);
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
			await drag(page, at(g, 5), at(g, 30), { hold: 600 });

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

describe('the now region: a drag LEFT is a duration ending NOW', () => {
	test('one `dur=` pill and no time pill; a line logs a block that ends now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const g = await geometry(page);
			// The press lands on the now-line — the now panel's own edge —
			// and runs back twelve minutes. No long press: on now there is
			// nothing to pan to on that side, so this gesture is free.
			await drag(page, { x: g.nowLine + 2, y: g.y }, { x: g.nowLine - 12 * perMinute(g), y: g.y });

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
			assert.equal((await drawn(page, NOWVIEW)).head, null, 'the selection is drawn in a window without it');

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

/** `30m` down to `0` on NOWVIEW_STEPS — 30 → 20 → 15 → 10 → 5 → 0. */
const STEPS_TO_SNAPSHOT = 5;

describe('the snapshot says «a selected time, not now»', () => {
	test('a selection crosses its chips with the white dotted line; with none, nothing is drawn', async () => {
		await withBand(async (app) => {
			const { page } = app;
			const shorter = page.locator(`${NOWVIEW} button[aria-label="shorter window"]`);
			for (let i = 0; i < STEPS_TO_SNAPSHOT; i++) {
				await shorter.click();
				await app.idle();
			}
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
			await until(
				() => drawn(page, NOWVIEW).then((d) => d.lines),
				(n) => n === 1
			);
			const line = await page
				.locator(`${NOWVIEW} svg.svg line.cursor`)
				.evaluate((l) => [getComputedStyle(l).stroke, l.getAttribute('stroke-dasharray')]);
			assert.deepEqual(line, ['rgb(255, 255, 255)', '3 3']);
		});
	});
});

// ── the phone ──────────────────────────────────────────────────────────

/** Bring the nowview tab of `day`'s phone column to the front, and wait for it. */
async function toNowviewTab(app) {
	await pane(app.page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
	await showBand(app, BAND);
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
				const from = async (selector) => {
					const b = await page.locator(selector).boundingBox();
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
					await swipe(page, b.x - dx / 2, b.y, dx, 0, 10);
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
				// steps).
				for (const over of ['.nowview-steps', '.nowview-jump.right']) {
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
				await swipe(page, (g.lo + g.hi) / 2, g.y, 60, 0);
				await until(
					() => asked(page),
					(t) => t !== null
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

	test('the tiny nowview in `mobile` shows the lines and no margin', async () => {
		await withBand(
			async (app) => {
				const { page } = app;
				const tiny = page.locator('section[data-pane="now"] .nowview');
				const box = await tiny.boundingBox();
				assert.ok(box.width < 200, `the tiny nowview is ${box.width}px — the margin's floor is 200`);
				assert.equal(await tiny.locator('.nowview-margin').count(), 0, 'the margin ate the tiny nowview');
				assert.equal(await tiny.locator('.nowview-jump').count(), 0);
				// And it still draws the line: the selection is the one thing
				// there is always room for.
				await page.evaluate(() => window.datum.cursor.set(new Date(Date.UTC(2026, 8, 10, 14, 40))));
				await until(
					() => page.locator('section[data-pane="now"] svg.svg line.cursor').count(),
					(n) => n === 1
				);
			},
			{
				...PHONE,
				home: 'mobile',
				ready: async (app) => {
					// `mobile`'s tiny nowview is the SNAPSHOT (`history: '0'`):
					// no axis, no gridline — its chips are what it draws.
					await app.page.locator('section[data-pane="now"] svg.svg .now-chunk').first().waitFor({ state: 'attached' });
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
 */
async function turn(app) {
	const { page } = app;
	await pickOption(page, 'nowview', 'orientation', 'vertical');
	await maximizeButton(page, 'nowview').click();
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
		const [lo, hi] = [px(num('data-lo')), px(num('data-hi'))];
		const mid = (el) => {
			const r = el.getBoundingClientRect();
			return r.top + r.height / 2;
		};
		const line = svg.querySelector('.current-time-line');
		return {
			axis: day.getAttribute('data-axis'),
			from,
			to,
			lo,
			hi,
			x: box.left + box.width * 0.75,
			nowLine: line ? mid(line) : null,
			marks: [...svg.querySelectorAll('.gridline')].map((l) => ({
				y: mid(l),
				back: Math.round((to - (from + ((mid(l) - lo) / (hi - lo)) * (to - from))) / 60000)
			}))
		};
	});

/** A point on the vertical band `back` minutes before the window's end. */
const vat = (g, back) => ({ x: g.x, y: g.hi - back * perMinute(g) });

describe('turned on its side, the band is pointed into along its own axis', () => {
	test('a tap on the −10m gridline says -qq; a long press dragged UP is a range', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await turn(app);
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
			await drag(page, vat(g, 10), vat(g, 30), { hold: 600 });
			await saysPills(page, ['time:-qq', 'dur:dur=20m']);
		});
	});

	test('from the now strip, a drag UP is a duration ending now', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await turn(app);
			const g = await vgeometry(page);
			assert.ok(12 * perMinute(g) > 24 + 12, `twelve minutes is ${12 * perMinute(g)}px`);
			await drag(page, { x: g.x, y: g.nowLine + 2 }, { x: g.x, y: g.nowLine - 12 * perMinute(g) });
			await saysPills(page, ['dur:dur=12m']);
		});
	});

	test('a drag DOWN pans the window earlier, and `now ↓` brings it home', async () => {
		await withBand(async (app) => {
			const { page } = app;
			await turn(app);
			await app.spyOnRunner();
			const g = await vgeometry(page);
			const middle = { x: g.x, y: (g.lo + g.hi) / 2 };
			await drag(page, middle, { x: g.x, y: middle.y + 10 * perMinute(g) });
			await until(
				() => asked(page),
				(t) => t !== null
			);
			const centred = Date.parse(await asked(page));
			assert.ok(
				Math.abs(centred - ((g.from + g.to) / 2 - 10 * MINUTE)) <= 90_000,
				`centred on ${new Date(centred).toISOString()}, not ~14:35`
			);
			const home = page.locator(`${NOWVIEW} .nowview-jump.right`);
			assert.equal(await home.innerText(), 'now ↓');
			await home.click();
			await until(
				() => asked(page),
				(t) => t === null
			);
		});
	});
});
