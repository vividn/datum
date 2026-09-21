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
// ⛔ AND THE ONE HE CARES MOST ABOUT: "being able to select time (at least
// in nowview) is a much more important operation than selecting the data."
// So the controls here are not decoration — a plain tap and a plain drag
// still mean TIME, and they are asserted beside every gesture that does
// not.
//
// Delta-blind, as `nowpoint.test.mjs` is: the entry a gesture is expected
// to select is read off the mark's OWN `data-id` (da-6hg6v) and compared
// with the hid the bar's pill says, never with a row the test counted to.
// The state machine is `nowpoint-pure.test.mjs` and the double tap's own
// numbers are `inspect-pure.test.mjs`, both in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
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
/** Past `$lib/nowpoint.ts`'s 400ms wait, with room to spare. */
const HOLD = 650;

/**
 * The app with two entries the pictures both draw: a CLOSED block on
 * today, and a state that is still OPEN — which is what puts a colour in
 * the now chunk, «if you double tap a color, that color is an open
 * block».
 *
 * The clock is FIXED rather than installed: every gesture here is a real
 * press over real milliseconds and the wait is a TIMER, which an installed
 * clock would freeze.
 */
async function withMarks(fn, options = {}) {
	await withApp(async (app) => {
		for (const argv of [
			['switch', 'work', 'coding', '-t', '14:00'],
			['switch', 'mood', 'good', '-t', '09:00'],
			['switch', 'mood', 'great', '-t', '11:00']
		])
			{
				const ran = await app.run(argv);
				assert.equal(ran.code, 0, `${argv.join(' ')}: ${ran.stderr}`);
			}
		await app.reload();
		await app.page.locator(`${NOWVIEW} svg.svg`).waitFor();
		await app.page.locator(`${TODAY} svg.svg`).waitFor();
		await app.idle();
		await fn(app);
	}, { clock: NOW, ...DESKTOP, ...options });
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
		const seen = [...el.querySelectorAll(`svg.svg ${sel}`)]
			.map((n) => ({ n, r: n.getBoundingClientRect() }))
			.filter(({ r }) => r.width >= 2 && r.height >= 2)
			.sort((a, b) => b.r.width * b.r.height - a.r.width * a.r.height);
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

/** The nowview's floating tooltip, as a reader reads it. */
const tipText = async (page) => {
	const tip = page.locator(`${NOWVIEW} .nowview-tip`);
	return (await tip.count()) ? (await tip.innerText()).replace(/\s+/g, ' ').trim() : null;
};

/**
 * Wait until the next press in the same place is NOT the second half of a
 * double tap ($lib/inspect.ts's 350ms, with room over it).
 *
 * Needed wherever a test presses twice on one spot meaning two gestures:
 * a tap and then a drag from the same pixel inside a third of a second is
 * a double tap, and a double tap INSPECTS — correctly, and not what that
 * test is about.
 */
const apart = (page) => page.waitForTimeout(420);

/** A press that stays where it fell for `ms`, then lifts. */
async function hold(page, at, ms = HOLD) {
	await page.mouse.move(at.x, at.y);
	await page.mouse.down();
	await page.waitForTimeout(ms);
	await page.mouse.up();
}

/** A press, a wait, a drag and a lift — the RANGE gesture. */
async function holdDrag(page, from, to, ms = HOLD) {
	await page.mouse.move(from.x, from.y);
	await page.mouse.down();
	await page.waitForTimeout(ms);
	await page.mouse.move(to.x, to.y, { steps: 8 });
	await page.mouse.up();
}

/** Every nowview argv the seam has been handed since the spy was armed. */
const windows = (page) => page.evaluate(() => window.__datumSeen.filter((a) => a[0] === 'nowview'));

// ── the nowview ────────────────────────────────────────────────────────

describe('a deliberate gesture in the nowview selects the entry behind the mark', () => {
	test('a double tap makes an hid pill — and NOT a time pill, though the first tap made one', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			assert.ok(mark?.id, 'the nowview drew no mark naming an entry');

			await page.mouse.dblclick(early(mark).x, early(mark).y);

			// One pill, and it is the entry's. vividn: «creates an hid pill
			// rather than a time pill» — the single tap inside the double tap
			// makes a time pill on its way past, and it may not survive.
			await saysPills(page, [`hid:${mark.id}`]);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null, 'a time cursor outlived the inspect');
			assert.deepEqual(await litUp(page, NOWVIEW), [mark.id], 'the mark is not brought out');
		});
	});

	test('a STILL long press does the same; one that TRAVELS is still the range it always was', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);

			// Still: the inspect. «maybe long press could as well, as long as
			// it's not a long press drag».
			await hold(page, from);
			await saysPills(page, [`hid:${mark.id}`]);

			// Put the bar back as a reader would, so what the range makes is
			// all there is to read: the two selections are separate pills and
			// neither takes the other away.
			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await saysPills(page, []);

			// Travelling: the range, untouched by any of this — an instant and
			// a length, which is the gesture the 400ms used to commit to
			// outright, and no entry selected.
			await apart(page);
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
	// inspects only a press that is STILL, and a finger that has travelled
	// is not.
	test('a held press that travels off its mark selects nothing at all', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			const from = early(mark);
			await page.mouse.move(from.x, from.y);
			await page.mouse.down();
			await page.waitForTimeout(HOLD);
			await page.mouse.move(from.x, from.y + 60, { steps: 6 });
			await page.mouse.up();
			await page.waitForTimeout(200);
			assert.deepEqual(await pillsOf(page), [], 'a held press that wandered off selected something');
			assert.deepEqual(await litUp(page, NOWVIEW), []);
		});
	});

	// ⛔ THE CONTROL, and the thing vividn cares most about: "everywhere has
	// data under it", so a rule that made a plain press select data would
	// put time selection out of reach. A tap ON A MARK is still a time.
	test('a plain tap on a mark still selects TIME, and a plain drag still pans', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const mark = await aMark(page, BAND);
			const from = early(mark);

			await page.mouse.click(from.x, from.y);
			const pills = await pillsOf(page);
			assert.equal(pills.length, 1, `a tap made ${JSON.stringify(pills)}`);
			assert.match(pills[0], /^time:/, 'a tap on a mark stopped selecting time');
			assert.deepEqual(await litUp(page, NOWVIEW), [], 'a tap brought an entry out');
			assert.deepEqual(await windows(page), [], 'the tap moved the window');

			// And a drag with no wait is the pan it has always been: a window
			// asked for, and no entry selected.
			await apart(page);
			await page.mouse.move(from.x, from.y);
			await page.mouse.down();
			await page.mouse.move(from.x + 90, from.y, { steps: 8 });
			await page.mouse.up();
			await until(
				() => windows(page),
				(asked) => asked.length > 0
			);
			assert.equal(
				(await pillsOf(page)).some((p) => p.startsWith('hid:')),
				false,
				'a plain drag selected an entry'
			);
		});
	});

	test('the desk’s three: hover says what the mark is, and shift+click or the right button take it', async () => {
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

			// The same press again REPLACES a selection of one with nothing
			// ($lib/selection.ts), so clear it the way a reader would and
			// come at it with the other button.
			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await saysPills(page, []);
			await page.mouse.click(from.x, from.y, { button: 'right' });
			await saysPills(page, [`hid:${mark.id}`]);
		});
	});

	test('the now chunk selects the entry that OPENED the open block, and the tooltip names it', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const chunk = await aMark(page, BAND, '.now-chunk');
			assert.ok(chunk?.id, 'the now chunk named no entry');

			await page.mouse.dblclick(chunk.x, chunk.y);
			await saysPills(page, [`hid:${chunk.id}`]);

			// The chunk carries an id and NOTHING else (da-6hg6v), so the
			// tooltip's words are gathered from every mark that names the same
			// entry — the open band beside it included.
			const said = await until(() => tipText(page), (text) => !!text);
			assert.match(said, /work/, `the tooltip said ${JSON.stringify(said)}`);
			assert.match(said, /coding/, `the tooltip said ${JSON.stringify(said)}`);
			assert.match(said, /\d\d:\d\d/, `the tooltip said no time: ${JSON.stringify(said)}`);
		});
	});

	// canary inkling, PR #71 (REQUEST_CHANGES): «the reactive call
	// `$: readTip($selected)` does NOT trigger on resize — only on
	// selection change. The tooltip could float over a mark that has moved
	// or disappeared after a resize/redraw until the selection changes
	// again.» It is read in `place()` too, which the widget's own
	// MutationObserver and ResizeObserver both drive — so this is that
	// claim, run.
	test('the tooltip follows a redraw and a resize, and goes when the window leaves the entry', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, BAND);
			await page.mouse.dblclick(early(mark).x, early(mark).y);
			await saysPills(page, [`hid:${mark.id}`]);
			const where = async () => {
				const box = await page.locator(`${NOWVIEW} .nowview-tip`).boundingBox();
				return box && Math.round(box.x + box.width / 2);
			};
			const first = await where();
			assert.ok(first !== null, 'no tooltip to follow');

			// A REDRAW: every pane re-runs its command on a new epoch, which
			// replaces the picture and every mark in it.
			await page.evaluate(() => window.datum.layout.apply((l) => l));
			await app.idle();
			await until(where, (x) => x !== null);
			assert.deepEqual(await litUp(page, NOWVIEW), [mark.id], 'the redraw lost the halo');

			// A RESIZE: the box changes, the picture is re-asked at the new
			// width, and the tooltip has to land on the mark's new place.
			await page.setViewportSize({ width: 900, height: 900 });
			await app.idle();
			const moved = await until(where, (x) => x !== null && x !== first);
			const box = await page
				.locator(`${NOWVIEW} svg.svg [data-id="${mark.id}"]`)
				.first()
				.boundingBox();
			assert.ok(
				Math.abs(moved - (box.x + box.width / 2)) <= box.width / 2 + 8,
				`the tooltip at ${moved}, its mark at ${box.x}–${box.x + box.width}`
			);

			// And a window PANNED off the entry draws none of its marks, so
			// there is nothing left to say about it — «and nowview if that
			// particular piece of data is represented there».
			await page.evaluate(() => window.datum.cursor.point(null));
			await page.locator(`${NOWVIEW} .nowview-band`).evaluate((band) => {
				const box = band.getBoundingClientRect();
				band.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientX: box.left + box.width * 0.8, clientY: box.top + box.height / 2 }));
			});
			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await saysPills(page, []);
			await until(
				() => page.locator(`${NOWVIEW} .nowview-tip`).count(),
				(n) => n === 0
			);
		});
	});

	test('a LOCKED hid pill accumulates a second entry instead of replacing the first', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const work = await aMark(page, BAND);
			await page.mouse.dblclick(early(work).x, early(work).y);
			await saysPills(page, [`hid:${work.id}`]);

			// vividn: «locking the hid pill lets you select other pieces of
			// data, so you can manipulate them together».
			await page.locator('.prompt-bar .pill.hid button.lock').click();
			await until(
				() => page.evaluate(() => window.datum.bar.pills().find((p) => p.kind === 'hid')?.locked ?? false),
				(locked) => locked === true
			);

			const mood = await aMark(page, `${NOWVIEW} .field[data-field="mood"]`);
			assert.ok(mood?.id && mood.id !== work.id, 'no second entry to gather');
			await page.mouse.dblclick(early(mood).x, early(mood).y);
			await saysPills(page, [`hid:${work.id},${mood.id}`]);
			assert.deepEqual((await litUp(page, NOWVIEW)).sort(), [work.id, mood.id].sort());
		});
	});
});

// ── the day strip ──────────────────────────────────────────────────────

describe('the same gestures on a band of the day strip', () => {
	test('a double tap on a MARK inspects; on the picture’s GROUND it still gives the whole day back', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, TODAY);
			assert.ok(mark?.id, 'today’s band drew no mark naming an entry');

			await page.mouse.dblclick(early(mark).x, early(mark).y);
			await saysPills(page, [`hid:${mark.id}`]);
			assert.deepEqual(await litUp(page, TODAY), [mark.id]);

			// And the strip's own double tap — «the whole day back», the
			// gesture a map gives its world for — is untouched over ground
			// that names no entry. Zoom in first, so there is a window to
			// give back.
			const band = await page.locator(TODAY).boundingBox();
			await page.mouse.move(band.x + band.width / 2, band.y + band.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -300);
			await page.keyboard.up('Control');
			await until(
				() => page.locator(`${TODAY} svg.svg`).evaluate((svg) => svg.getBoundingClientRect().width),
				(w) => w > band.width * 1.5
			);

			const ground = await page.locator(TODAY).evaluate((el) => {
				const box = el.getBoundingClientRect();
				// The top hairline of a band: inside it, over no mark.
				return { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + 1) };
			});
			assert.equal(
				await page.evaluate(
					({ x, y }) => !!document.elementFromPoint(x, y)?.closest('[data-id]'),
					ground
				),
				false,
				'the point chosen for “ground” is a mark'
			);
			await page.mouse.dblclick(ground.x, ground.y);
			await until(
				() => page.locator(`${TODAY} svg.svg`).evaluate((svg) => svg.getBoundingClientRect().width),
				(w) => w <= band.width * 1.1
			);
		});
	});

	test('a still long press inspects, and leaves no time pill behind it', async () => {
		await withMarks(async (app) => {
			const { page } = app;
			const mark = await aMark(page, TODAY);
			await hold(page, early(mark));
			// The strip's press is the CLICK the `picture` action takes, and
			// that click has set the time cursor since da-wnt2 — so an
			// inspect that did not SPEND the press would leave both pills.
			await saysPills(page, [`hid:${mark.id}`]);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
		});
	});
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
			assert.match((await tipText(page)) ?? '', /work/, 'the nowview said nothing about the selection');
		});
	});
});
