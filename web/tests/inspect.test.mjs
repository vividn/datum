// Inspecting a mark in nowview and dayview (da-pzzoz).
//
// vividn, 2026-09-21: "hover will get some info on desktop. double tap to
// select (maybe long press could as well, as long as it's not a long press
// drag, which should be for selecting a time range 🤔)" · "Yes long press.
// on desktop right click or shift click or also long click" · "it selects
// the entry that generated that block" · "double clicking or tapping in
// dayview or nowview selects an existing piece of data and creates an hid
// pill rather than a time pill. locking the hid pill lets you select other
// pieces of data" · "when a piece of data is selected through other means
// (like clicking in tail), then it should be highlighted in the dayview
// (and nowview if that particular piece of data is represented there)".
//
// da-xg1vi turned the tap around. vividn, 2026-10-22: «single tap → select
// a piece of data; single tap where there is no data, or in the time axis →
// select a time; long tap / right click → select a time … For quickly
// selecting a time, a user can touch in the time axis, so that won't slow
// them down.» So a tap on a mark is the inspect, and the time is the long
// tap's, the right click's and the axis's — asserted beside it here.
//
// Delta-blind, as `nowpoint.test.mjs` is: the entry a gesture is expected
// to select is read off the mark's OWN `data-id` (da-6hg6v) and compared
// with the hid the bar's pill says, never with a row the test counted to.
// The state machine is `nowpoint-pure.test.mjs`, in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, sharedApp, todayAt, withApp } from './support/app.mjs';
import { pinch } from './support/daystrip.mjs';
import { longPressed } from './support/press.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** 15:00 sharp: a minute boundary, with room on both sides of it in the day. */
const NOW = todayAt(15);
const DESKTOP = { viewport: { width: 1280, height: 900 } };
const NOWVIEW = 'section.pane[data-title="nowview"]';
const BAND = `${NOWVIEW} .nowview-band`;
const TAIL = 'section.pane[data-title="tail"]';
const TODAY = '.band[data-days-ago="0"]';

/**
 * The app with two entries the pictures both draw: a CLOSED block on
 * today, and a state that is still OPEN — which is what puts a colour in
 * the now chunk, «if you double tap a color, that color is an open
 * block».
 *
 * The clock is FIXED rather than installed: every gesture here is a real
 * press over real milliseconds and the wait is a TIMER, which an installed
 * clock would freeze.
 *
 * ONE app for the file (sharedApp in support/app.mjs), each test in a room
 * of its own: a pan, a zoom and a locked pill are the widgets' own state,
 * so a test must not inherit the last one's.
 */
const shared = sharedApp({ clock: NOW, ...DESKTOP });

async function withMarks(fn) {
	await shared.fresh(async (app) => {
		for (const argv of [
			['switch', 'work', 'coding', '-t', '14:00'],
			['switch', 'mood', 'good', '-t', '09:00'],
			['switch', 'mood', 'great', '-t', '11:00']
		])
			{
				const ran = await app.run(argv);
				assert.equal(ran.code, 0, `${argv.join(' ')}: ${ran.stderr}`);
			}
		await app.redraw();
		await app.page.locator(`${NOWVIEW} svg.svg`).waitFor();
		await app.page.locator(`${TODAY} svg.svg`).waitFor();
		await app.idle();
		await fn(app);
	});
}

/**
 * The biggest mark in `scope` that NAMES an entry, and its box on screen.
 *
 * By `[data-id]` and by area: what can be inspected is exactly what names
 * an entry (a dot with no single entry behind it carries no id at all),
 * and the biggest of them is the one a finger can be aimed at without the
 * test knowing anything about how the picture laid itself out.
 */
const aMark = (page, scope, selector = '[data-id]') =>
	page.locator(scope).evaluate((el, sel) => {
		// Clipped to the picture's box: a nowview is drawn past it either
		// side (`--surround`, da-61nm8), and what is out there is not on screen.
		const clip = (n) => {
			const r = n.getBoundingClientRect();
			const f = n.closest('svg.svg').parentElement.getBoundingClientRect();
			const [left, right] = [Math.max(r.left, f.left), Math.min(r.right, f.right)];
			return { left, right, width: Math.max(0, right - left), top: r.top, height: r.height };
		};
		const seen = [...el.querySelectorAll(`svg.svg ${sel}`)]
			.map((n) => ({ n, r: clip(n) }))
			.filter(({ r }) => r.width >= 2 && r.height >= 2)
			// In whole square pixels, so two lanes of one height tie and the
			// first drawn wins: a band scaled to the box (da-m6i2a) puts a
			// float's noise between them.
			.sort((a, b) => Math.round(b.r.width * b.r.height) - Math.round(a.r.width * a.r.height));
		const hit = seen[0];
		if (!hit) return null;
		const { n, r } = hit;
		return {
			id: n.getAttribute('data-id'),
			field: n.getAttribute('data-field') ?? n.closest('.field')?.getAttribute('data-field') ?? null,
			state: n.getAttribute('data-state'),
			left: r.left,
			right: r.right,
			width: r.width,
			x: Math.round(r.left + r.width / 2),
			y: Math.round(r.top + r.height / 2)
		};
	}, selector);

/**
 * How wide today's picture is on screen, found and measured in ONE task
 * (da-h39mp). A locator's `evaluate` finds the element first and measures
 * it on a second round trip, and a picture that lands between the two —
 * a zoom's own redraw, late under load — replaces the svg, so the one
 * measured is detached and reads 0 wide whatever the window is.
 */
const todayWidth = (page) =>
	page.evaluate((sel) => document.querySelector(sel).getBoundingClientRect().width, `${TODAY} svg.svg`);

/** A point a fifth of the way along a mark: on it, and well clear of the now region. */
const early = (mark) => ({ x: Math.round(mark.left + mark.width * 0.2), y: mark.y });

/** The pills as `kind:text` — what each PUTS on the command line, which is what it means. */
const pillsOf = (page) => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/** Wait until the pills say exactly `want`, and report what they said if they never do. */
const saysPills = (page, want) =>
	until(
		() => pillsOf(page),
		(seen) => seen.join('|') === want.join('|')
	);

/** The entries brought out in `scope`'s picture: the marks wearing the halo. */
const litUp = (page, scope) =>
	page.locator(scope).evaluate((el) => [
		...new Set([...el.querySelectorAll('svg.svg [data-id][data-selected]')].map((m) => m.getAttribute('data-id')))
	]);

/**
 * The outlines the nowview draws around the selected entry's marks (da-67mwa),
 * each beside the box its mark is SEEN in: cut at the now-line where it flows
 * on past it, as `shownBox` (#lib/hover.ts) cuts it.
 */
const outlined = (page) =>
	page.locator(BAND).evaluate((band) => {
		const seen = (m) => {
			const r = m.getBoundingClientRect();
			const line = m.closest('.flow') && m.closest('svg').querySelector('.current-time-line')?.getBoundingClientRect();
			return line ? { left: r.left, right: Math.min(r.right, line.left), top: r.top, bottom: r.bottom } : r;
		};
		const marks = [...band.querySelectorAll('svg.svg [data-selected]')]
			.map(seen)
			.filter((r) => r.right > r.left);
		return [...band.querySelectorAll('.outline')].map((o) => {
			const r = o.getBoundingClientRect();
			const css = getComputedStyle(o);
			return {
				box: { left: r.left, right: r.right, top: r.top, bottom: r.bottom },
				border: [css.borderLeftWidth, css.borderTopWidth],
				// The mark it stands around: a pixel inside it on every side.
				on: marks.some(
					(m) =>
						Math.abs(m.left - 1 - r.left) < 0.6 &&
						Math.abs(m.right + 1 - r.right) < 0.6 &&
						Math.abs(m.top - 1 - r.top) < 0.6 &&
						Math.abs(m.bottom + 1 - r.bottom) < 0.6
				)
			};
		});
	});

/** Whether any label is up over the nowview: its own (gone, da-67mwa) or the picture's. */
const labeled = (page) =>
	page
		.locator(NOWVIEW)
		.evaluate((p) => !!p.querySelector('.nowview-tip') || [...p.querySelectorAll('.tip')].some((t) => !t.hidden));

/** A press that stays where it fell past the long press, then lifts. */
async function hold(page, at) {
	await page.mouse.move(at.x, at.y);
	await page.mouse.down();
	await longPressed(page);
	await page.mouse.up();
}

/** A press, a wait, a drag and a lift — the RANGE gesture. */
async function holdDrag(page, from, to) {
	await page.mouse.move(from.x, from.y);
	await page.mouse.down();
	await longPressed(page);
	await page.mouse.move(to.x, to.y, { steps: 8 });
	await page.mouse.up();
}

/** Every nowview argv the seam has been handed since the spy was armed. */
const windows = (page) => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview'));

// ── the nowview ────────────────────────────────────────────────────────

describe('a tap in the nowview selects the entry behind the mark', () => {
	test('a tap on a mark makes an hid pill — and NOT a time pill', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			assert.ok(mark?.id, 'the nowview drew no mark naming an entry');

			await page.mouse.click(early(mark).x, early(mark).y);

			// One pill, and it is the entry's. vividn: «creates an hid pill
			// rather than a time pill».
			await saysPills(page, [`hid:${mark.id}`]);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null, 'a time cursor outlived the inspect');
			assert.deepEqual(await litUp(page, NOWVIEW), [mark.id], 'the mark is not brought out');
		});
	});

	test('a STILL long press on a mark is a TIME; one that TRAVELS is the range', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);

			// Still: the time where it fell, on a mark or not — «long tap /
			// right click → select a time».
			await hold(page, from);
			const pills = await until(
				() => pillsOf(page),
				(seen) => seen.length > 0
			);
			assert.equal(pills.length, 1, `a long tap made ${JSON.stringify(pills)}`);
			assert.match(pills[0], /^time:/, 'a long tap on a mark did not select time');
			assert.deepEqual(await litUp(page, NOWVIEW), [], 'a long tap brought an entry out');
			await page.evaluate(() => window.datum.cursor.set(null));
			await saysPills(page, []);

			// Travelling: the range, untouched by any of this — an instant and
			// a length, which is the gesture the 400ms used to commit to
			// outright, and no entry selected.
			await holdDrag(page, from, { x: from.x + 120, y: from.y });
			await until(
				() => pillsOf(page),
				(seen) => seen.length === 2 && seen.some((p) => p.startsWith('dur:dur='))
			);
			assert.equal(
				(await pillsOf(page)).some((p) => p.startsWith('hid:')),
				false,
				'a long-press DRAG selected an entry'
			);
		});
	});

	test('the wait has a mark of its own, and it is gone the moment the gesture is decided', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);
			const held = page.locator(`${NOWVIEW} .hold-mark`);

			await page.mouse.move(from.x, from.y);
			await page.mouse.down();
			// The ring is drawn the instant the finger lands — that is the
			// part of the wait a thumb can watch — and the band says `hold`
			// once the wait is up. The bar dropping at 400ms used to be this
			// signal, and it was owed back (da-pzzoz).
			await held.waitFor({ state: 'attached' });
			assert.equal(await page.locator(BAND).getAttribute('data-mode'), 'press', 'held before the wait was up');
			await until(
				() => page.locator(BAND).getAttribute('data-mode'),
				(mode) => mode === 'hold'
			);
			assert.equal(await held.evaluate((el) => el.classList.contains('held')), true);

			await page.mouse.up();
			await held.waitFor({ state: 'detached' });
		});
	});

	// canary minimax, PR #71: `advance` leaves a HELD press in `hold` when
	// the finger then travels mostly DOWNWARD — the browser's axis — and
	// asked whether the lift would then inspect. It does not: `finish`
	// inspects only a press that is STILL, and a finger that has traveled
	// is not.
	test('a held press that travels off its mark selects nothing at all', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);
			await page.mouse.move(from.x, from.y);
			await page.mouse.down();
			await longPressed(page);
			await page.mouse.move(from.x, from.y + 60, { steps: 6 });
			await page.mouse.up();
			await page.waitForTimeout(200);
			assert.deepEqual(await pillsOf(page), [], 'a held press that wandered off selected something');
			assert.deepEqual(await litUp(page, NOWVIEW), []);
		});
	});

	// «For quickly selecting a time, a user can touch in the time axis, so
	// that won't slow them down.»
	test('a tap on the time axis selects TIME, and a plain drag pans', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const mark = await aMark(page, BAND);
			const from = early(mark);
			const axis = await page.locator(`${NOWVIEW} svg.svg path.axis`).boundingBox();

			// The axis between two labels: each label is a handle (da-57vj4),
			// and the axis scrubs in the gaps between them.
			const gap = await page.locator(NOWVIEW).evaluate((p) => {
				const hs = [...p.querySelectorAll('.nowview-handles .handle')].map((h) => h.getBoundingClientRect());
				return (hs[0].right + hs[1].left) / 2;
			});
			const on = { x: gap, y: axis.y + axis.height / 2 };
			await page.mouse.click(on.x, on.y);
			const pills = await pillsOf(page);
			assert.equal(pills.length, 1, `a tap made ${JSON.stringify(pills)}`);
			assert.match(pills[0], /^time:/, 'a tap on the axis did not select time');
			assert.deepEqual(await litUp(page, NOWVIEW), [], 'a tap on the axis brought an entry out');
			// The window stays on now: asked for again only to draw the
			// section at the pill (da-zccjs), centred no instant of its own.
			for (const argv of await windows(page)) {
				const center = argv[argv.indexOf('--center') + 1];
				assert.ok(!argv.includes('--center') || /^-/.test(center), `the tap moved the window: ${argv.join(' ')}`);
			}

			// And a drag with no wait is the pan it has always been: the
			// window off now — slid inside what is drawn, so asked of nobody
			// (da-61nm8) — and no entry selected.
			await page.mouse.move(from.x, from.y);
			await page.mouse.down();
			await page.mouse.move(from.x + 90, from.y, { steps: 8 });
			await page.mouse.up();
			await page.locator(`${NOWVIEW} .nowview-now`).waitFor();
			assert.equal(
				(await pillsOf(page)).some((p) => p.startsWith('hid:')),
				false,
				'a plain drag selected an entry'
			);
		});
	});

	test('the desk’s three: hover says what the mark is, shift+click takes it, and the right button is the time', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);

			// «hover will get some info on desktop» — the picture's own label
			// (da-s5da), which the band takes no part in: `data-points="own"`
			// takes the band's CLICKS and never its pointer.
			await page.mouse.move(from.x, from.y);
			const label = page.locator(`${NOWVIEW} .tip`);
			await until(
				() => label.evaluate((el) => (el.hidden ? '' : el.textContent ?? '')),
				(text) => /work/.test(text)
			);
			assert.deepEqual(await pillsOf(page), [], 'a hover selected something');
			// And it goes when the pointer leaves the mark: the label says
			// what is UNDER the pointer, so a stale one is a lie.
			await page.mouse.move(from.x, from.y - 300);
			await until(
				() => label.evaluate((el) => el.hidden),
				(hidden) => hidden === true
			);

			await page.keyboard.down('Shift');
			await page.mouse.click(from.x, from.y);
			await page.keyboard.up('Shift');
			await saysPills(page, [`hid:${mark.id}`]);

			// The right button is a TIME, as a long tap is (da-xg1vi), and
			// takes the place of the entry.
			await page.mouse.click(from.x, from.y, { button: 'right' });
			await until(
				() => pillsOf(page),
				(seen) => seen.some((p) => p.startsWith('time:'))
			);
		});
	});

	test('the now chunk selects the entry that OPENED the open block, and every mark of it is outlined', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const chunk = await aMark(page, BAND, '.now-chunk');
			assert.ok(chunk?.id, 'the now chunk named no entry');

			await page.mouse.click(chunk.x, chunk.y);
			await saysPills(page, [`hid:${chunk.id}`]);

			// The chunk carries an id and NOTHING else (da-6hg6v): the open
			// band beside it names the same entry, and both are outlined.
			const lines = await until(
				() => outlined(page),
				(o) => o.length >= 2
			);
			assert.ok(lines.every((o) => o.on), `an outline is off its mark: ${JSON.stringify(lines)}`);
		});
	});

	// canary inkling, PR #71, of the tooltip this outline replaced: read on a
	// selection change alone, it «could float over a mark that has moved or
	// disappeared after a resize/redraw». The outline is read in `place()`
	// too, which the widget's own MutationObserver and ResizeObserver both
	// drive — so this is that claim, run.
	test('the outline follows a redraw and a resize, and goes when the selection does', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			await page.mouse.click(early(mark).x, early(mark).y);
			await saysPills(page, [`hid:${mark.id}`]);
			// Read in ONE poll with the marks it stands around: a redraw landing
			// between two reads replaces them (da-alyue).
			const onIts = (o) => o.length > 0 && o.every((l) => l.on);
			const [first] = await until(() => outlined(page), onIts);

			// A REDRAW: every pane re-runs its command on a new epoch, which
			// replaces the picture and every mark in it.
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			await app.idle();
			await until(() => outlined(page), onIts);
			assert.deepEqual(await litUp(page, NOWVIEW), [mark.id], 'the redraw lost the selection');

			// A RESIZE: the box changes, the picture is re-asked at the new
			// width, and the outline lands on the mark's new place.
			await page.setViewportSize({ width: 900, height: 900 });
			await app.idle();
			await until(
				() => outlined(page),
				(o) => onIts(o) && o[0].box.right !== first.box.right
			);

			// And the selection gone, the outline goes with it.
			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await saysPills(page, []);
			await until(
				() => outlined(page),
				(o) => o.length === 0
			);
		});
	});

	test('a LOCKED hid pill accumulates a second entry instead of replacing the first', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const work = await aMark(page, BAND);
			await page.mouse.click(early(work).x, early(work).y);
			await saysPills(page, [`hid:${work.id}`]);

			// vividn: «locking the hid pill lets you select other pieces of
			// data, so you can manipulate them together».
			await page.locator('.prompt-bar .pill.hid button.lock').click();
			await until(
				() => page.evaluate(() => window.datum.bar.pills().find((p) => p.kind === 'hid')?.locked ?? false),
				(locked) => locked === true
			);

			// Its lane on the timeline, or its state in the strip: both name it.
			const mood = await aMark(page, NOWVIEW, '.field[data-field="mood"] [data-id]');
			assert.ok(mood?.id && mood.id !== work.id, 'no second entry to gather');
			await page.mouse.click(early(mood).x, early(mood).y);
			await saysPills(page, [`hid:${work.id},${mood.id}`]);
			assert.deepEqual((await litUp(page, NOWVIEW)).sort(), [work.id, mood.id].sort());
		});
	});
});

// ── the day strip ──────────────────────────────────────────────────────

describe('the same gestures on a band of the day strip', () => {
	// A tap on a mark is the inspect, and one on the ground is the time — and
	// neither does anything to the window. (A double tap on the ground gave
	// the whole day back until da-8rqb9, and the `⊖` in the strip's corner
	// does now: `daystrip.test.mjs`.)
	test('a tap inspects a mark; one on the ground is the time, and leaves the window alone', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, TODAY);
			assert.ok(mark?.id, 'today’s band drew no mark naming an entry');

			await page.mouse.click(early(mark).x, early(mark).y);
			await saysPills(page, [`hid:${mark.id}`]);
			assert.deepEqual(await litUp(page, TODAY), [mark.id]);

			// Zoom in, so there is a window a stray gesture could throw away.
			const band = await page.locator(TODAY).boundingBox();
			await page.mouse.move(band.x + band.width / 2, band.y + band.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -300);
			await page.keyboard.up('Control');
			const zoomed = await until(
				() => todayWidth(page),
				(w) => w > band.width * 1.5
			);

			// GROUND: over no mark, in the band's middle (da-r0ydu: a day is as
			// tall as its widget, so its top few pixels are the divider's row).
			// The assertion below fails loudly if a lane ever grows under it.
			const ground = await page.locator(TODAY).evaluate((el) => {
				const box = el.getBoundingClientRect();
				return { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2) };
			});
			assert.equal(
				await page.evaluate(
					({ x, y }) => !!document.elementFromPoint(x, y)?.closest('[data-id]'),
					ground
				),
				false,
				'the point chosen for “ground” is a mark'
			);
			await page.mouse.click(ground.x, ground.y);
			await until(
				() => pillsOf(page),
				(seen) => seen.length === 2 && seen[1].startsWith('time:')
			);
			assert.equal(await todayWidth(page), zoomed, 'a tap on the ground gave the whole day back');
		});
	});

	test('a still long press on a mark is the time, and leaves no hid pill behind it', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, TODAY);
			await hold(page, early(mark));
			// The strip's press ends in the CLICK the `picture` action takes,
			// whose answer over a mark is the entry — so a long tap that did
			// not SPEND the press would leave both pills.
			const pills = await until(
				() => pillsOf(page),
				(seen) => seen.length > 0
			);
			assert.equal(pills.length, 1, `a long tap made ${JSON.stringify(pills)}`);
			assert.match(pills[0], /^time:/);
			assert.deepEqual(await litUp(page, TODAY), []);
		});
	});
});

// ── how a selection looks in the nowview (da-67mwa) ───────────────────
//
// vividn, 2026-10-28: «when you select data, two separate labels show up,
// often very long and hard to read and overlapping each other. also the
// outline is huge and gets huger when you zoom in. Let's change the
// highlight to be like what happens when a field pill is selected. Subtle
// outline, we can also slightly dim everything else (but just a little bit
// to make the selected entry/field pop) / let's try getting rid of the
// selection labels on nowview».

describe('a selection in the nowview: no label, a pixel’s outline at any zoom, the rest dimmed a little', () => {
	/** The opacity of the selected entry's marks and of every other mark. */
	const opacities = (page) =>
		page.locator(BAND).evaluate((band) => {
			const marks = [...band.querySelectorAll('svg.svg :is([data-state], [data-id])')];
			const of = (m) => Number(getComputedStyle(m).opacity);
			return {
				chosen: marks.filter((m) => m.hasAttribute('data-selected')).map(of),
				rest: marks.filter((m) => !m.hasAttribute('data-selected')).map(of),
				halo: [...band.querySelectorAll('svg.svg [data-selected]')].map((m) => getComputedStyle(m).filter)
			};
		});

	test('a click: no label once the pointer leaves, the halo gone, the rest of the marks dimmed slightly', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			await page.mouse.click(early(mark).x, early(mark).y);
			await saysPills(page, [`hid:${mark.id}`]);
			// The hover label is the pointer's, and goes with it; the selection says none.
			await page.mouse.move(early(mark).x, early(mark).y - 300);
			await until(
				() => labeled(page),
				(up) => up === false
			);
			const seen = await until(
				() => opacities(page),
				(o) => o.chosen.length > 0
			);
			assert.ok(seen.chosen.every((o) => o === 1), `the selection is dimmed: ${seen.chosen}`);
			assert.ok(seen.rest.length > 0, 'no other mark to dim');
			assert.ok(seen.rest.every((o) => o < 1 && o >= 0.5), `the rest is not dimmed a little: ${seen.rest}`);
			assert.ok(seen.halo.every((f) => f === 'none'), `the halo is still drawn: ${seen.halo}`);
			const lines = await outlined(page);
			assert.ok(lines.length > 0 && lines.every((l) => l.on), `no outline on the mark: ${JSON.stringify(lines)}`);
			assert.ok(lines.every((l) => l.border.every((w) => w === '1px')), `the outline is not a pixel: ${JSON.stringify(lines)}`);

			// And nothing dimmed with nothing selected.
			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await until(
				() => opacities(page),
				(o) => o.chosen.length === 0 && o.rest.every((v) => v === 1)
			);
		});
	});

	test('a finger’s tap says no label, and a ×3 pinch leaves the outline a pixel, around its mark', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const argv of [
					['switch', 'work', 'coding', '-t', '14:00'],
					['switch', 'mood', 'good', '-t', '09:00'],
					['switch', 'mood', 'great', '-t', '11:00']
				])
					assert.equal((await app.run(argv)).code, 0);
				await app.redraw();
				await page.locator(`${NOWVIEW} svg.svg`).waitFor();
				await app.idle();
				const mark = await aMark(page, BAND);
				await page.touchscreen.tap(early(mark).x, early(mark).y);
				await saysPills(page, [`hid:${mark.id}`]);
				const rest = await until(
					() => outlined(page),
					(o) => o.length > 0 && o.every((l) => l.on)
				);
				// The picture's label waits for a finger's lift (#lib/hover.ts): a beat for it.
				await page.waitForTimeout(150);
				assert.equal(await labeled(page), false, 'a tap on the nowview put a label up');

				const band = await page.locator(BAND).boundingBox();
				const box = { x: early(mark).x - 100, y: band.y, width: 200, height: band.height };
				let during;
				await pinch(page, box, 60, 180, async () => {
					during = {
						zoom: Number(await page.locator(BAND).evaluate((b) => getComputedStyle(b).getPropertyValue('--zoom'))),
						lines: await outlined(page)
					};
				});
				// The control: the band is stretched three times along time.
				assert.ok(Math.abs(during.zoom - 3) < 0.1, `the pinch stretched by ${during.zoom}`);
				assert.ok(during.lines.length > 0, 'the outline went under the pinch');
				assert.ok(during.lines.every((l) => l.on), `the outline left its mark: ${JSON.stringify(during.lines)}`);
				assert.deepEqual(
					during.lines.map((l) => l.border),
					rest.map((l) => l.border).slice(0, during.lines.length),
					'the outline changed width under the pinch'
				);
				assert.ok(during.lines.every((l) => l.border.every((w) => w === '1px')));
				assert.deepEqual(app.errors, []);
			},
			{ clock: NOW, ...DESKTOP, device: { hasTouch: true } }
		);
	});
});

// ── tap again, tap twice ───────────────────────────────────────────────

/** Past a double tap's window, `DOUBLE_MS` in #lib/hover.ts: the next tap is a tap of its own. */
const apart = (page) => page.waitForTimeout(450);

/** A point of `scope`'s picture that is ground: in its svg, on no mark and off the time axis. */
const groundIn = (page, scope) =>
	page.locator(scope).evaluate((el) => {
		const svg = el.querySelector('svg.svg');
		const r = svg.parentElement.getBoundingClientRect();
		for (let j = 1; j < 20; j++)
			for (let i = 1; i < 20; i++) {
				const [x, y] = [Math.round(r.left + (r.width * i) / 20), Math.round(r.top + (r.height * j) / 20)];
				const at = document.elementFromPoint(x, y);
				if (at && svg.contains(at) && !at.closest('[data-id], [data-axis="time"], .axis, .time-axis')) return { x, y };
			}
		return null;
	});

// vividn, 2026-10-28 (da-fzdkc): «double tapping chooses a time, but it
// should also leave what was selected before the double tap selected
// (including nothing if nothing was selected beforehand). Tapping (not
// double tapping) on an entry that is already selected should unselect it».
describe('a tap on the entry selected unselects it; a double tap is the time and keeps the selection', () => {
	for (const [view, scope] of [
		['nowview', BAND],
		['dayview', TODAY]
	]) {
		test(`the ${view}`, async () => {
			await withMarks(async (app) => {
				const { page } = app;
				const mark = await aMark(page, scope);
				const on = early(mark);
				const ground = await groundIn(page, scope);
				assert.ok(mark?.id && ground, `the ${view} drew no mark, or no ground`);
				const time = (want) =>
					until(
						() => pillsOf(page),
						(seen) => seen.length === want.length + 1 && seen.slice(0, -1).join('|') === want.join('|') && seen.at(-1).startsWith('time:')
					);
				const clear = () => page.evaluate(() => window.datum.bar.setPills([]));

				await page.mouse.click(on.x, on.y);
				await saysPills(page, [`hid:${mark.id}`]);
				await apart(page);
				await page.mouse.click(on.x, on.y);
				await saysPills(page, []);

				// With nothing selected, nothing becomes selected.
				for (const at of [ground, on]) {
					await apart(page);
					await clear();
					await page.mouse.dblclick(at.x, at.y);
					await time([]);
					await apart(page);
				}
				// With the entry selected, it stays.
				for (const at of [ground, on]) {
					await clear();
					await page.mouse.click(on.x, on.y);
					await saysPills(page, [`hid:${mark.id}`]);
					await apart(page);
					await page.mouse.dblclick(at.x, at.y);
					await time([`hid:${mark.id}`]);
					await apart(page);
				}
				assert.deepEqual(app.errors, []);
			});
		});
	}
});

// ── one selection, every surface ───────────────────────────────────────

describe('a selection made anywhere is brought out everywhere', () => {
	test('a row tapped in the tail lights its mark in the dayview AND in the nowview', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			// The entry the nowview draws as an open block — `work` — is the
			// one that is in both pictures.
			const mark = await aMark(page, BAND);
			const row = page.locator(`${TAIL} tbody tr[data-hid^="${mark.id.slice(0, 5)}"]`).first();
			await row.waitFor();
			await row.click();

			await saysPills(page, [`hid:${await row.getAttribute('data-hid')}`]);
			await until(
				() => litUp(page, NOWVIEW),
				(lit) => lit.join() === mark.id
			);
			await until(
				() => litUp(page, TODAY),
				(lit) => lit.join() === mark.id
			);
			await until(
				() => outlined(page),
				(o) => o.length > 0 && o.every((l) => l.on)
			);
		});
	});
});
