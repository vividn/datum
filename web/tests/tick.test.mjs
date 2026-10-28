// The minute tick (da-r61i): dayview is a picture of NOW, and so is nowview.
//
// dayview reads the clock inside the wasm process and draws what it says —
// the now-line — so run once by its pane it shows the minute it was run in
// and then stands still. One shared minute clock (#lib/clock.ts) re-runs it
// on each wall-clock minute, and nothing else: the tail is not a view of
// now and neither is the widget bar.
//
// nowview ticked too — every ten seconds (da-fprk0) — until da-hklaz drew
// it AHEAD of now (`--ahead`) and had the widget slide it along with the
// clock: so a tick runs no nowview at all, and what the clock does to it is
// held to here as a slide.
//
// Delta-blind on purpose: what moved is read off the CLI's own attributes
// (`data-from` / `data-to`, the now-line's x) and the runs are counted at
// the seam, so a picture that merely changed is not enough.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/**
 * Today at 15:00 sharp — a minute boundary, so one minute of clock is one
 * tick, and today, so the seeds below and the clock move together
 * (`todayAt`).
 */
const AT = todayAt(15);
const MINUTE = 60_000;
const TEN_SECONDS = 10_000;
/** Long enough for a box to settle (#lib/layout/resizing.ts), short of a minute. */
const SETTLE = 200;
const NOWVIEW = 'section.pane[data-title="nowview"]';
const TODAY = '.band[data-days-ago="0"] svg.svg';

/**
 * The app held at `AT`, its timers with it (`holdTimers`): the tick is a
 * timer, and the clock is the test's to move — nothing the page schedules
 * runs until `app.clock.forward` (or `runFor`) says so.
 *
 * The web decides WHEN to ask again; the CLI decides what now IS, from
 * the machine clock inside the wasm process in the database worker. A
 * test that moved only the page's would tick and be handed the same
 * picture back; one that moved only the worker's would never tick at all
 * — which is why `app.clock` moves both.
 */
const HELD = { clock: AT, holdTimers: true };

/**
 * Take the document's visibility over, and hand back the switch.
 *
 * There is no playwright call for "this tab is in the background", and it
 * is the browser's own `document.hidden` the clock reads — so the
 * property is redefined and the event fired, which is what the browser
 * does when a reader moves to another tab.
 */
async function visibility(page) {
	await page.evaluate(() => {
		window.__hidden = false;
		Object.defineProperty(document, 'hidden', { configurable: true, get: () => window.__hidden });
		Object.defineProperty(document, 'visibilityState', {
			configurable: true,
			get: () => (window.__hidden ? 'hidden' : 'visible')
		});
	});
	return (hidden) =>
		page.evaluate((h) => {
			window.__hidden = h;
			document.dispatchEvent(new Event('visibilitychange'));
		}, hidden);
}

/**
 * The instant the nowview shows under its now line: the instant the CLI drew
 * there (its day's axis read at the line's `data-now`), and as far on as
 * the widget has slid the picture's `.flow` groups since (da-hklaz).
 */
const underNow = (page) =>
	page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
		const day = svg.querySelector('[data-lo]');
		const [from, to] = [Date.parse(day.dataset.from), Date.parse(day.dataset.to)];
		const msPerUnit = (to - from) / (Number(day.dataset.hi) - Number(day.dataset.lo));
		const drawn = from + (Number(svg.querySelector('.current-time-line').dataset.now) - Number(day.dataset.lo)) * msPerUnit;
		const flow = svg.querySelector('.flow');
		return drawn - (flow ? new DOMMatrixReadOnly(getComputedStyle(flow).transform).e : 0) * msPerUnit;
	});

/** An open block's ends, as the CLI stamped them on its rect. */
const span0 = (block) => block.evaluate((r) => [r.dataset.from, r.dataset.to]);

/** The commands of one name the seam was handed, since the spy was armed. */
const ranOf = async (seen, name) => (await seen()).filter((argv) => argv[0] === name);

// Every test here opens an app of its own (da-6b3dj): each installs a
// clock in the page and the worker and moves it, which a shared app would
// hand on to the next test already moved.
describe('the minute tick', () => {
	test("nowview's open block runs on under the now line: a minute slides it a minute along, and runs nothing", async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:50'])).code, 0);
				await app.reload();
				const block = page.locator(`${NOWVIEW} svg.svg rect.block[data-field="work"]`);
				await block.waitFor();
				// Settled first, and SEEN to be settled: a pane still finding
				// its box re-runs on the box. A held clock does not settle by
				// itself — the box arrives through a 150ms timer
				// (#lib/layout/resizing.ts) — so the clock is run out to it,
				// well short of the minute.
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();
				const [from, to] = await span0(block);
				// Drawn ahead: open past now, behind the line — to the end of
				// its day, the day's data layer (da-0e06p).
				assert.ok(Date.parse(to) >= AT + 30 * MINUTE, 'the block is not drawn on past now');
				const before = await underNow(page);
				await app.clock.forward(MINUTE);
				await app.idle();

				// A 30-minute window is slid once a second, a fraction of a
				// pixel each time, so `before` may be up to a second behind the
				// clock; after the minute it is the clock's to the millisecond.
				const after = await underNow(page);
				assert.ok(Math.abs(after - (await page.evaluate(() => Date.now()))) < 50, 'the picture is not slid to the clock');
				assert.ok(after - before >= MINUTE && after - before <= MINUTE + 1000, `the picture slid ${after - before}ms in a minute`);
				assert.deepEqual(await span0(block), [from, to], 'the block was drawn again');
				assert.equal((await ranOf(seen, 'nowview')).length, 0, 'nowview was run by the clock');
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test("dayview's now-line moves on by one minute of the day", async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// Attached, not visible: a vertical line is a box of no
				// width, which playwright counts as hidden.
				const line = page.locator(`${TODAY} line.current-time-line`);
				await line.waitFor({ state: 'attached' });
				const x = () => line.evaluate((l) => Number(l.getAttribute('x1')));

				const before = await x();
				await app.clock.forward(MINUTE);
				const after = await until(x, (seen) => seen !== before);

				// One minute of the band's own axis: `data-lo` to `data-hi`
				// is the whole day, which is 1440 of them (da-cgyh's bands
				// are asked for with no margin and no label column).
				const [lo, hi] = await page
					.locator(`${TODAY} .day-data`)
					.evaluate((g) => [Number(g.dataset.lo), Number(g.dataset.hi)]);
				const step = (hi - lo) / 1440;
				assert.ok(
					Math.abs(after - before - step) < 0.5,
					`the now-line moved ${after - before}px, a minute is ${step}px`
				);
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test("nowview moves on every ten seconds and more, with nothing written (da-fprk0)", async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:50'])).code, 0);
				await app.reload();
				await page.locator(`${NOWVIEW} svg.svg rect.block[data-field="work"]`).waitFor();
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();
				const before = await underNow(page);
				// The beat da-fprk0 asked for, and finer: a second of clock is
				// a second's worth of slide.
				for (const ms of [TEN_SECONDS, 1000]) {
					const was = await underNow(page);
					await page.clock.runFor(ms);
					assert.ok(Math.abs((await underNow(page)) - was - ms) < 50, `${ms}ms of clock did not slide ${ms}ms`);
				}
				assert.ok((await underNow(page)) > before);
				assert.equal((await ranOf(seen, 'nowview')).length, 0, 'nowview was run by the clock');
				// The minute has not turned: the strip keeps its picture.
				assert.equal((await ranOf(seen, 'dayview')).length, 0, 'the strip re-drew on the beat');
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test('a tick runs the views of now and nothing else', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const seen = await app.spyOnRunner();
				await app.idle();

				await app.clock.forward(MINUTE);
				await until(() => ranOf(seen, 'dayview'), (ran) => ran.length > 0);
				await app.idle();

				// dayview once, the geometry, which says what day it is: today's
				// band is cut at the new now from the day's data layer it holds
				// (#lib/dayLayers.ts, da-m6i2a), and the strip's other bands are
				// finished days and keep the pictures they have, however deep
				// the stack is. Whatever the depth, and whatever is in the tail.
				// nowview not at all: it slides (da-hklaz).
				assert.equal((await ranOf(seen, 'nowview')).length, 0, 'nowview was run by the clock');
				assert.deepEqual(
					(await ranOf(seen, 'dayview')).map((argv) => argv.slice(-2).join(' ')),
					['--show json'],
					'the strip re-drew more than the geometry'
				);
				// The tick is not the frame's epoch: a pane whose render does
				// not depend on now is not re-run, and the widget bar — one
				// `fields --json` per refresh (#lib/layout/engine.ts) — is not
				// rebuilt.
				assert.equal((await ranOf(seen, 'tail')).length, 0, 'the tail was re-run by the clock');
				assert.equal((await ranOf(seen, 'fields')).length, 0, 'the widget bar was rebuilt by the clock');
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test('a hidden document runs nothing, however many minutes pass, and one run on return', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const show = await visibility(page);
				const seen = await app.spyOnRunner();
				await app.idle();

				await show(true);
				await app.clock.forward(MINUTE);
				await app.clock.forward(MINUTE);
				await app.idle();
				assert.equal(
					(await ranOf(seen, 'dayview')).length,
					0,
					'a backgrounded tab woke up to redraw a picture nobody can see'
				);

				await show(false);
				await until(() => ranOf(seen, 'dayview'), (ran) => ran.length > 0);
				await app.idle();
				// Today's band and its geometry, once: one tick's worth.
				assert.equal(
					(await ranOf(seen, 'dayview')).length,
					1,
					'coming back cost one run per minute away, not one run'
				);
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test('a nowview pinned to the time cursor does not tick', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// A press on the day band sets the time cursor (#lib/cursor.ts)
				// and the nowview is centred on that instant from then on
				// (`-t`): a picture with no now in it, and so nothing for a
				// minute to move.
				await page.locator(TODAY).click();
				// The cursor's own dotted line, drawn by datum where the
				// instant falls, is the picture saying it is pinned.
				await page.locator(`${NOWVIEW} svg.svg line.cursor`).waitFor({ state: 'attached' });
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();

				await app.clock.forward(MINUTE);
				// The strip is not pinned, so its band is how we know the
				// minute really turned.
				await until(() => ranOf(seen, 'dayview'), (ran) => ran.length > 0);
				await app.idle();
				assert.equal(
					(await ranOf(seen, 'nowview')).length,
					0,
					'a picture of an instant the reader chose was re-drawn by the clock'
				);
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});

	test('a tick while a write is still refreshing costs the pane one more run, not one each', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const seen = await app.spyOnRunner();
				await app.idle();

				// The tick lands while the write's own refresh is out. A
				// BOUND, as da-s3pt's own test is: two things asking the
				// pane to re-draw cost at most one run out and one waiting,
				// which is the pane's rule for any burst (da-eoq3) — never
				// a run each per pane per event, which is what a tick
				// bumping the frame's epoch would have cost.
				const written = app.submit('occur coffee');
				await app.clock.forward(MINUTE);
				await written;
				await app.idle();

				// The nowview's runs: its overlay, and its day's data layer, which is what a write re-draws (da-0e06p).
				const ran = [...(await ranOf(seen, 'nowview')), ...(await seen()).filter((argv) => argv.includes('--data-layer'))];
				assert.ok(ran.length >= 1, 'neither the write nor the tick re-drew nowview');
				assert.ok(ran.length <= 2, `${ran.length} nowview processes for a write and a tick`);
				// And what is on screen is of the minute the clock is in, not
				// of the one the write was in: slid there, if not drawn there.
				await page.clock.runFor(100);
				assert.ok(Math.abs((await underNow(page)) - (AT + MINUTE + 100)) < 1000, 'the picture on screen is a minute behind');
				assert.deepEqual(app.errors, []);
			},
			HELD
		);
	});
});
