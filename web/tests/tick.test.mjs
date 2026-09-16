// The minute tick (da-r61i): nowview and dayview are pictures of NOW.
//
// Both read the clock inside the wasm process and draw what it says — the
// elapsed duration of an open block, the now-line — so run once by their
// pane they show the minute they were run in and then stand still. One
// shared minute clock ($lib/clock.ts) re-runs them on each wall-clock
// minute, and nothing else: the tail is not a view of now and neither is
// the widget bar.
//
// Delta-blind on purpose: what moved is read off the CLI's own attributes
// (`data-from` / `data-to`, the now-line's x) and the runs are counted at
// the seam, so a picture that merely changed is not enough.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp, workerClock } from './support/app.mjs';
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
/** Long enough for a box to settle ($lib/layout/resizing.ts), short of a minute. */
const SETTLE = 200;
const NOWVIEW = 'section.pane[data-title="nowview"]';
const TODAY = '.band[data-days-ago="0"] svg.svg';

/**
 * The two clocks the app reads, moved together.
 *
 * The web decides WHEN to ask again; the CLI decides what now IS, from
 * the machine clock inside the wasm process — which runs in the database
 * worker, where playwright's clock does not reach (`workerClock`). A test
 * that moved only the page's would tick and be handed the same picture
 * back; one that moved only the worker's would never tick at all.
 */
function clocks(from) {
	const worker = workerClock(from);
	let at = from;
	return {
		prepare: async (context) => {
			// Installed rather than fixed: the tick is a timer, and the
			// clock is the test's to move — nothing the page schedules runs
			// until `fastForward` (or `runFor`) says so.
			await context.clock.install({ time: new Date(from) });
			await worker.prepare(context);
		},
		/** The CLI's clock first, so the run the tick starts sees the new minute. */
		async forward(page, ms = MINUTE) {
			at += ms;
			await worker.set(page, at);
			await page.clock.fastForward(ms);
			return at;
		}
	};
}

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

/** An open block's ends, as the CLI stamped them on its rect. */
const span0 = (block) => block.evaluate((r) => [r.dataset.from, r.dataset.to]);

/** The commands of one name the seam was handed, since the spy was armed. */
const ranOf = async (seen, name) => (await seen()).filter((argv) => argv[0] === name);

describe('the minute tick', () => {
	test("nowview's open block grows by a minute, with nothing written", async () => {
		const clock = clocks(AT);
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.run(['start', 'work', '-t', '14:50'])).code, 0);
				await app.reload();
				const block = page.locator(`${NOWVIEW} svg.svg rect.block[data-field="work"]`);
				await block.waitFor();
				// Settled first, and SEEN to be settled: a pane still finding
				// its box re-runs on the box, and a test that read the
				// picture that came back would be holding the tick to a run
				// it did not start. A held clock does not settle by itself —
				// the box arrives through a 150ms timer
				// ($lib/layout/resizing.ts) — so the clock is run out to it,
				// well short of the minute. From here the seam is watched,
				// and the tick's own run is the one counted below.
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();
				const [from, to] = await span0(block);
				assert.equal(Date.parse(to), AT, 'the block does not reach to now');
				await clock.forward(page);

				const [grown, later] = await until(
					() => span0(block),
					([, t]) => Date.parse(t) !== Date.parse(to)
				);
				assert.equal(
					(await ranOf(seen, 'nowview')).length,
					1,
					'the picture came from something other than the one tick'
				);
				assert.equal(grown, from, 'the block moved its start rather than growing');
				assert.equal(
					Date.parse(later) - Date.parse(to),
					MINUTE,
					'the open block did not grow by exactly the minute that passed'
				);
				assert.deepEqual(app.errors, []);
			},
			{ prepare: clock.prepare }
		);
	});

	test("dayview's now-line moves on by one minute of the day", async () => {
		const clock = clocks(AT);
		await withApp(
			async (app) => {
				const { page } = app;
				// Attached, not visible: a vertical line is a box of no
				// width, which playwright counts as hidden.
				const line = page.locator(`${TODAY} line.current-time-line`);
				await line.waitFor({ state: 'attached' });
				const x = () => line.evaluate((l) => Number(l.getAttribute('x1')));

				const before = await x();
				await clock.forward(page);
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
			{ prepare: clock.prepare }
		);
	});

	test('a tick runs the views of now and nothing else', async () => {
		const clock = clocks(AT);
		await withApp(
			async (app) => {
				const { page } = app;
				const seen = await app.spyOnRunner();
				await app.idle();

				await clock.forward(page);
				await until(() => ranOf(seen, 'nowview'), (ran) => ran.length > 0);
				await app.idle();

				// nowview once; dayview twice, which is today's band and the
				// geometry beside it — the strip's other bands are finished
				// days and keep the pictures they have, however deep the
				// stack is. Whatever the depth, and whatever is in the tail.
				assert.equal((await ranOf(seen, 'nowview')).length, 1, 'nowview did not run exactly once');
				assert.equal((await ranOf(seen, 'dayview')).length, 2, 'the strip re-drew more than today');
				// The tick is not the frame's epoch: a pane whose render does
				// not depend on now is not re-run, and the widget bar — one
				// `fields --json` per refresh ($lib/layout/engine.ts) — is not
				// rebuilt.
				assert.equal((await ranOf(seen, 'tail')).length, 0, 'the tail was re-run by the clock');
				assert.equal((await ranOf(seen, 'fields')).length, 0, 'the widget bar was rebuilt by the clock');
				assert.deepEqual(app.errors, []);
			},
			{ prepare: clock.prepare }
		);
	});

	test('a hidden document runs nothing, however many minutes pass, and one run on return', async () => {
		const clock = clocks(AT);
		await withApp(
			async (app) => {
				const { page } = app;
				const show = await visibility(page);
				const seen = await app.spyOnRunner();
				await app.idle();

				await show(true);
				await clock.forward(page);
				await clock.forward(page);
				await app.idle();
				assert.equal(
					(await ranOf(seen, 'nowview')).length,
					0,
					'a backgrounded tab woke up to redraw a picture nobody can see'
				);

				await show(false);
				await until(() => ranOf(seen, 'nowview'), (ran) => ran.length > 0);
				await app.idle();
				assert.equal(
					(await ranOf(seen, 'nowview')).length,
					1,
					'coming back cost one run per minute away, not one run'
				);
				assert.deepEqual(app.errors, []);
			},
			{ prepare: clock.prepare }
		);
	});

	test('a nowview pinned to the time cursor does not tick', async () => {
		const clock = clocks(AT);
		await withApp(
			async (app) => {
				const { page } = app;
				// A press on the day band sets the time cursor ($lib/cursor.ts)
				// and the nowview is centred on that instant from then on
				// (`--at`): a picture with no now in it, and so nothing for a
				// minute to move.
				await page.locator(TODAY).click();
				// The cursor's own dotted line, drawn by datum where the
				// instant falls, is the picture saying it is pinned.
				await page.locator(`${NOWVIEW} svg.svg line.cursor`).waitFor({ state: 'attached' });
				await page.clock.runFor(SETTLE);
				await app.idle();
				const seen = await app.spyOnRunner();

				await clock.forward(page);
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
			{ prepare: clock.prepare }
		);
	});

	test('a tick while a write is still refreshing costs the pane one more run, not one each', async () => {
		const clock = clocks(AT);
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
				await clock.forward(page);
				await written;
				await app.idle();

				const ran = await ranOf(seen, 'nowview');
				assert.ok(ran.length >= 1, 'neither the write nor the tick re-drew nowview');
				assert.ok(ran.length <= 2, `${ran.length} nowview processes for a write and a tick`);
				// And what is on screen is of the minute the tick asked for,
				// not of the one the write was in.
				const to = await page
					.locator(`${NOWVIEW} svg.svg .day-data`)
					.evaluate((g) => g.dataset.to);
				assert.equal(Date.parse(to), AT + MINUTE, 'the picture on screen is a minute behind');
				assert.deepEqual(app.errors, []);
			},
			{ prepare: clock.prepare }
		);
	});
});
