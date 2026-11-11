// Collect's pinch (da-l1z6h). vividn, 2026-11-04: «let's add pinch to zoom
// which changes font size and height of the rows to allow you to fit more or
// less rows in the collect widget». And in modify mode alone, with the now
// section's width, flip, and + and − rows (da-xi4up), vividn, 2026-11-10: «in
// the collect widget for example some things that you could change in the
// modify mode would be the now section width, have a button to flip left and
// right, but a + button at the top of the line separating the fields section
// from the time/now section and a - button on the bottom which changes the
// number of rows in the widget», «let's move just into modify mode. We can
// allow pinch zooming when in modify mode though in addition to the +/-».
//
// The rows are `atRest`'s (fields-rest-pure.test.mjs), counted from the
// buttons' height as drawn; this is the built app, read off what it draws.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { DESKTOP, PHONE, choose, leaveWidget, modifyWidget, pane, rested, pickOption, rootMenuButton } from './support/layout.mjs';
import { pinch } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const COLLECT = 'section.pane[data-title="collect"] .bar .collect';
const ROOM = `${COLLECT} .room`;
/** A tall phone on collect, so its rows are many to begin with. */
const AT_REST = { ...PHONE, viewport: { width: 390, height: 844 }, home: ON_COLLECT, clock: todayAt(15) };

/** Thirty fields down the pictures' height, so every row the room holds has one. */
const SEED = Array.from({ length: 30 }, (_, i) => {
	const field = `f${String(i).padStart(2, '0')}`;
	return `setup ${field} --from-json '${JSON.stringify({ field, verb: 'occur', y: (i + 0.5) / 30, height: 0.01 })}'`;
});

/** Thirty fields with long names, so the rows run past the room's width and a finger pans them. */
const WIDE = Array.from({ length: 30 }, (_, i) => {
	const field = `a_rather_long_field_${String(i).padStart(2, '0')}`;
	return `setup ${field} --from-json '${JSON.stringify({ field, verb: 'occur', y: (i + 0.5) / 30, height: 0.01 })}'`;
});

/** How many rows stand, and a button's height and font size, once nothing glides. */
async function rows(page) {
	const read = () =>
		page.locator(`${ROOM} .spot.shelf`).evaluateAll((spots) => {
			const button = spots[0].querySelector('button');
			return JSON.stringify({
				rows: new Set(spots.map((s) => s.dataset.row)).size,
				height: Math.round(button.getBoundingClientRect().height),
				font: Math.round(parseFloat(getComputedStyle(button).fontSize) * Number(getComputedStyle(button).zoom || 1))
			});
		});
	let last = null;
	await until(read, (now) => {
		const same = now === last;
		last = now;
		return same;
	});
	return JSON.parse(last);
}

/**
 * Two fingers on two field buttons, lifted one and then the other, as a hand
 * does (da-gh1ep): the first still, the second drawn `by` px away from it
 * over 700 ms, past a hold's 450. Then, with `tap`, a third finger tapped on
 * the first's spot at once, while the rows would still be gliding. Through
 * CDP, as `pinch` is, and in one session, so nothing waits between them.
 */
async function pinchOver(page, [a, b], by, tap = false) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	const spread = (k) => [
		{ id: 1, x: a.x, y: a.y },
		{ id: 2, x: b.x + k * by, y: b.y + k * by }
	];
	try {
		await at('touchStart', [spread(0)[0]]);
		await at('touchStart', spread(0));
		// The draw's own length, past HOLD_MS (#lib/hold.ts, 450 ms), so a hold
		// the second finger failed to call off would fire before the lift.
		for (let i = 1; i <= 7; i++) {
			await page.waitForTimeout(100);
			await at('touchMove', spread(i / 7));
		}
		await at('touchEnd', [spread(1)[0]]);
		// The gap between a hand's two lifts: the gesture's shape, not a wait for
		// the page. `untapped` holds until every finger is up, whatever the gap.
		await page.waitForTimeout(80);
		await at('touchEnd', []);
		if (tap) {
			await at('touchStart', [{ id: 3, x: a.x, y: a.y }]);
			await at('touchEnd', []);
		}
	} finally {
		await cdp.detach();
	}
}

/** The middle of a field's button as drawn. */
const middle = async (page, field) => {
	const box = await page.locator(`${ROOM} [data-field="${field}"]`).boundingBox();
	return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
};

/**
 * A finger's touches through CDP, each `[type, points, ms to wait after]`: the
 * gesture's own timing (a swipe's speed, a hold's length), never a wait for
 * the page, which the caller's `rested` is.
 */
async function touches(page, steps) {
	const cdp = await page.context().newCDPSession(page);
	try {
		for (const [type, touchPoints, wait] of steps) {
			await cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
			if (wait) await page.waitForTimeout(wait);
		}
	} finally {
		await cdp.detach();
	}
}

/** The rows' zoom as drawn. */
const scaleOf = (page) => page.locator(ROOM).evaluate((el) => Number(getComputedStyle(el).getPropertyValue('--zoom')));
/** Where the first row's first field stands across the room. */
const firstX = (page) => page.locator(`${ROOM} .spot.shelf`).first().evaluate((el) => el.getBoundingClientRect().x);

/**
 * The room's part the rows stand in: past the time and duration section beside
 * the strip (da-us1n1), and a finger's width clear of it, which a touch landing
 * nearer is snapped onto (the browser's touch adjustment).
 */
async function rowsBox(page) {
	const room = await page.locator(ROOM).boundingBox();
	const when = await page.locator('section.pane[data-title="collect"] .bar [data-when]').boundingBox();
	const x = when.x + when.width + 16;
	return { ...room, x, width: room.x + room.width - x };
}

/** One finger drawn `by` px across the rows' middle, then the rows let settle: how far the first field moved. */
async function swipe(page, by) {
	const room = await rowsBox(page);
	const [x, y] = [room.x + room.width / 2 - by / 2, room.y + room.height / 2];
	const at = (k) => [{ id: 9, x: x + k * by, y }];
	const was = await firstX(page);
	// 30 ms a step is the swipe's speed, which the rows' momentum is read from (`speedOver`, #lib/grip.ts).
	await touches(page, [['touchStart', at(0), 30], ...[1, 2, 3, 4, 5].map((i) => ['touchMove', at(i / 5), 30]), ['touchEnd', []]]);
	// The glide the swipe let go of (`glideOn`, decaying by TAU) has stopped.
	await rested(page, page.locator(`${ROOM} .spot.shelf`).first());
	return (await firstX(page)) - was;
}

/** The collect pane's id, which modify mode is entered on. */
const COLLECT_PANE = 'section.pane[data-title="collect"]';
const idOf = (page) => page.locator(COLLECT_PANE).getAttribute('data-pane');
/** What the LAYOUT holds for the widget in pane `id`: its settings in the tree, no session over them. */
const inTree = (page, id) =>
	page.evaluate((id) => {
		const find = (p) => {
			if (p.id === id) return p.tabs[p.active].content.settings;
			for (const t of p.tabs) if (!('kind' in t.content)) for (const c of t.content.children) { const got = find(c); if (got) return got; }
			return null;
		};
		return find(window.datum.layout.current().root);
	}, id);
/** What this session holds over the collect widgets: each key's settings. */
const heldOver = (page) => page.evaluate(() => Object.values(window.datum.layout.session()));
/** The cover's controls and what it says: the rows the room holds, the now section's share. */
const cover = (page) => page.locator(`${COLLECT_PANE} [data-modify]`);
const stripBox = (page) => page.locator(`${COLLECT_PANE} [data-strip]`).boundingBox();

async function seeded(app, lines = SEED) {
	for (const line of lines) assert.equal((await app.runLine(line)).code, 0, line);
	await app.reload();
	await app.page.locator(`${ROOM} .spot.shelf`).first().waitFor();
	await app.idle();
}

/** A drag of the now width's handle `by` px across, with a mouse or a finger. */
async function dragGrip(page, by, finger) {
	const box = await cover(page).locator('.grip').boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	if (!finger) {
		await page.mouse.move(x, y);
		await page.mouse.down();
		for (let i = 1; i <= 5; i++) await page.mouse.move(x + (by * i) / 5, y);
		await page.mouse.up();
	} else {
		await touches(page, [['touchStart', [{ id: 4, x, y }], 30], ...[1, 2, 3, 4, 5].map((i) => ['touchMove', [{ id: 4, x: x + (by * i) / 5, y }], 30]), ['touchEnd', []]]);
	}
	await rested(page, page.locator(`${COLLECT_PANE} [data-strip]`));
}

describe('collect out of modify mode', () => {
	test('a pinch and ctrl+wheel zoom nothing, and none of modify mode’s controls is drawn', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			assert.equal(await cover(page).count(), 0, 'a control drawn out of modify mode');
			const [start, scale] = [await rows(page), await scaleOf(page)];
			await pinch(page, await rowsBox(page), 60, 180);
			await rested(page, page.locator(ROOM));
			const room = await page.locator(ROOM).boundingBox();
			await page.mouse.move(room.x + room.width / 2, room.y + room.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -100);
			await page.keyboard.up('Control');
			await rested(page, page.locator(ROOM));
			assert.equal(await scaleOf(page), scale, 'a pinch or ctrl+wheel zoomed the rows');
			assert.deepEqual(await rows(page), start);
			assert.deepEqual(await heldOver(page), [], 'something held in the session');
		}, AT_REST);
	});

	test('two fingers on two fields, lifted one by one, log and open nothing, nor does a tap as they lift; a tap then logs', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const [a, b] = await page.locator(`${ROOM} .spot.shelf button[data-field]`).evaluateAll((all) => all.slice(0, 2).map((el) => el.dataset.field));
			const scale = await scaleOf(page);
			await pinchOver(page, [await middle(page, a), await middle(page, b)], 60, true);
			// Past HOLD_MS (#lib/hold.ts, 450 ms) from the last finger down, by when a hold the gesture failed to call off has opened its field.
			await page.waitForTimeout(600);
			await app.idle();
			assert.deepEqual(await app.entries(), [], 'the fingers logged');
			assert.equal(await page.locator('.collect [data-back]').count(), 0, 'the fingers opened a field');
			assert.equal(await scaleOf(page), scale, 'the fingers zoomed');
			const tapped = await middle(page, a);
			await page.touchscreen.tap(tapped.x, tapped.y);
			await until(() => app.entries(), (rows) => rows.length === 1);
			assert.deepEqual((await app.entries()).map((row) => row.field), [a], 'a tap afterwards logs');
		}, AT_REST);
	});

	// vividn, 2026-11-06: «it doesnt seem like i can scroll or zoom on the
	// collect widget anymore even after [resetting] the widget». A hold opens
	// its field under the finger, so the finger lifts on the field's view and
	// not on the rows (da-yn6my).
	test('a hold that opens a field, then Back: a finger pans the rows, and so it does after «Reset widget»; in modify mode a pinch zooms them', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app, WIDE);
			assert.equal(await page.locator(ROOM).getAttribute('data-pans'), 'x', 'the rows run past the room, so a finger pans them');
			const holdOpen = async () => {
				const at = await middle(page, await page.locator(`${ROOM} .spot.shelf button[data-field]`).first().getAttribute('data-field'));
				// 800 ms down is a hold, past HOLD_MS (#lib/hold.ts, 450 ms).
				await touches(page, [['touchStart', [{ id: 8, ...at }], 800], ['touchEnd', []]]);
				await rested(page, page.locator(COLLECT));
				assert.equal(await page.locator('.collect [data-back]').count(), 1, 'the hold opened its field');
				await page.keyboard.press('Escape');
				await rested(page, page.locator(`${ROOM} .spot.shelf`).first());
			};
			const pans = async (when) => {
				const scale = await scaleOf(page);
				const moved = [await swipe(page, -150), await swipe(page, 150)];
				assert.ok(moved[0] <= 0 && moved[1] >= 0 && moved[1] - moved[0] > 100, `${when}: a finger's swipes moved the rows ${moved.map(Math.round)} px`);
				assert.equal(await scaleOf(page), scale, `${when}: a finger's swipes left the zoom alone`);
			};
			await holdOpen();
			await pans('after a hold');
			const id = await idOf(page);
			await holdOpen();
			await choose(page, id, 'Reset widget');
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await pans('after a hold and «Reset widget»');
			const scale = await scaleOf(page);
			await modifyWidget(page, id);
			await pinch(page, await rowsBox(page), 60, 180);
			await rested(page, page.locator(ROOM));
			assert.ok((await scaleOf(page)) > scale * 1.5, `in modify mode a pinch out took the zoom from ${scale} to ${await scaleOf(page)}`);
		}, AT_REST);
	});
});

describe('collect in modify mode', () => {
	test('a pinch out fits fewer, larger rows and a pinch in more, smaller ones; ✓ keeps it in the layout, over a reload, and ↺ takes it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const id = await idOf(page);
			const start = await rows(page);
			assert.ok(start.rows > 4, `only ${start.rows} rows to begin with`);
			await modifyWidget(page, id);
			await pinch(page, await rowsBox(page), 60, 180);
			const large = await rows(page);
			assert.ok(large.height > start.height * 1.5, `a button ${start.height}px tall became ${large.height}px`);
			assert.ok(large.font > start.font * 1.5, `a font of ${start.font}px became ${large.font}px`);
			assert.ok(large.rows < start.rows, `${start.rows} rows became ${large.rows}`);
			await pinch(page, await rowsBox(page), 160, 32);
			const small = await rows(page);
			assert.ok(small.height < start.height, `a button ${start.height}px tall became ${small.height}px`);
			assert.ok(small.font >= 8, `a font of ${small.font}px is past legible`);
			assert.ok(small.rows > start.rows, `${start.rows} rows became ${small.rows}`);
			assert.equal((await inTree(page, id)).zoom, 0.65, 'the smallest zoom, in the layout');
			assert.deepEqual(await heldOver(page), [], 'the zoom held in the session');
			await leaveWidget(page, id, 'apply');
			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			assert.deepEqual(await rows(page), small, 'a reload keeps the zoom ✓ kept');

			await modifyWidget(page, id);
			await pinch(page, await rowsBox(page), 60, 180);
			assert.ok((await rows(page)).rows < small.rows);
			await leaveWidget(page, id, 'reset');
			assert.deepEqual(await rows(page), small, '↺ took the pinch back');
			assert.equal((await inTree(page, id)).zoom, 0.65);
		}, AT_REST);
	});

	test('ctrl+wheel, a trackpad pinch, zooms the rows too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const start = await rows(page);
			await modifyWidget(page, await idOf(page));
			const room = await page.locator(ROOM).boundingBox();
			await page.mouse.move(room.x + room.width * 0.75, room.y + room.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -100);
			await page.keyboard.up('Control');
			const large = await rows(page);
			assert.ok(large.height > start.height, `a button ${start.height}px tall became ${large.height}px`);
			assert.ok(large.rows < start.rows, `${start.rows} rows became ${large.rows}`);
		}, AT_REST);
	});

	// canary-deepseek-v4-pro on #1938: the wheel's write waits for it to go
	// quiet, so a ↺ inside that wait left it to land after the mode, in the session.
	test('ctrl+wheel and ↺ at once: the zoom is neither in the layout nor in the session', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const start = await rows(page);
			const id = await idOf(page);
			const before = (await inTree(page, id)).zoom;
			await modifyWidget(page, id);
			const room = await page.locator(ROOM).boundingBox();
			await page.mouse.move(room.x + room.width * 0.75, room.y + room.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -100);
			await page.keyboard.up('Control');
			await page.keyboard.press('Escape');
			await page.locator('.frame[data-layout-mode]').waitFor({ state: 'detached' });
			await page.evaluate(() => new Promise((done) => setTimeout(done, 400)));
			assert.deepEqual(await heldOver(page), [], 'the zoom landed in the session after ↺');
			assert.equal((await inTree(page, id)).zoom, before, 'the zoom landed in the layout after ↺');
			assert.deepEqual(await rows(page), start, 'the rows are not back as they were');
		}, AT_REST);
	});

	for (const [name, device, finger] of [
		['desktop', { ...DESKTOP, home: ON_COLLECT, clock: todayAt(15) }, false],
		['phone', AT_REST, true]
	]) {
		test(`${name}: + and − change the rows by one, the zoom fitting them; ✓ keeps the rows in the layout`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seeded(app);
				const id = await idOf(page);
				await modifyWidget(page, id);
				const n = (await rows(page)).rows;
				assert.equal(Number(await cover(page).getAttribute('data-rows')), n, 'the cover counts the rows drawn');
				const tap = (label) => (finger ? cover(page).getByRole('button', { name: label }).tap() : cover(page).getByRole('button', { name: label }).click());
				await tap('more rows');
				await until(async () => (await rows(page)).rows, (m) => m === n + 1);
				// A row more until the zoom would pass its least (`ZOOM`), where + is disabled.
				for (let more = n + 2; await cover(page).getByRole('button', { name: 'more rows' }).isEnabled(); more++) {
					await tap('more rows');
					await until(async () => (await rows(page)).rows, (m) => m === more);
				}
				const most = (await rows(page)).rows;
				await tap('fewer rows');
				await until(async () => (await rows(page)).rows, (m) => m === most - 1);
				assert.equal(Number(await cover(page).getAttribute('data-rows')), most - 1);
				await leaveWidget(page, id, 'apply');
				const kept = await inTree(page, id);
				assert.equal(kept.rows, most - 1, 'the rows, in the layout');
				assert.equal(kept.zoom, undefined, 'a zoom left over the rows');
				assert.equal(await cover(page).count(), 0, 'a control drawn out of modify mode');
				assert.equal((await rows(page)).rows, most - 1, 'out of the mode the rows stay');
			}, device);
		});

		test(`${name}: the handle on the now section's line changes its width, ⇄ puts it on the other side, and ↺ takes both back`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seeded(app);
				const id = await idOf(page);
				const pane = await page.locator(COLLECT_PANE).boundingBox();
				const was = await stripBox(page);
				await modifyWidget(page, id);
				await dragGrip(page, 80, finger);
				const wide = await stripBox(page);
				assert.ok(wide.width > was.width + 60, `a drag of 80px took the now section from ${was.width}px to ${wide.width}px`);
				assert.ok(Math.abs(wide.x - was.x) < 2, 'the now section left its edge');
				const share = (await inTree(page, id)).nowWidth;
				assert.ok(share > 0.2, `the layout holds a share of ${share}`);
				const flip = cover(page).getByRole('button', { name: 'flip' });
				await (finger ? flip.tap() : flip.click());
				await until(async () => (await stripBox(page)).x, (x) => x > pane.x + pane.width / 2);
				const right = await stripBox(page);
				assert.ok(right.x + right.width > pane.x + pane.width - 30, 'the now section is not at the right edge');
				assert.ok(Math.abs(right.width - wide.width) < 2, `the width ${wide.width}px became ${right.width}px on the right`);
				assert.equal((await inTree(page, id)).side, 'right');
				await leaveWidget(page, id, 'reset');
				await until(async () => JSON.stringify(await stripBox(page)), (box) => box === JSON.stringify(was));
				const back = await inTree(page, id);
				assert.equal(back.nowWidth, undefined);
				assert.notEqual(back.side, 'right');
			}, device);
		});
	}

	// Chrome turns a drag let go at speed into a fling, `touch-action: none` or
	// not, and a tap within ~150 ms only cancels the fling: no click. Staging's
	// fast runner dragged at 530 px/s and tapped ⇄ 113 ms after (da-rw31h); the
	// drag's touches carry their own times, so every box drags that fast.
	test('phone: ⇄ tapped as a fast drag of the handle lets go still flips the now section', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const id = await idOf(page);
			const pane = await page.locator(COLLECT_PANE).boundingBox();
			await modifyWidget(page, id);
			const grip = await cover(page).locator('.grip').boundingBox();
			const [x, y] = [grip.x + grip.width / 2, grip.y + grip.height / 2];
			const cdp = await page.context().newCDPSession(page);
			const t0 = Date.now() / 1000 - 0.2;
			for (const [i, type] of ['touchStart', 'touchMove', 'touchMove', 'touchMove', 'touchMove', 'touchMove', 'touchEnd'].entries())
				await cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ id: 4, x: x + 16 * Math.min(i, 5), y }], timestamp: t0 + 0.03 * i });
			await cdp.detach();
			// Forced: an unforced `tap()` waits out the flip's widening first, past the fling.
			await cover(page).getByRole('button', { name: 'flip' }).tap({ force: true });
			await until(async () => (await stripBox(page)).x, (x) => x > pane.x + pane.width / 2);
		}, AT_REST);
	});

	test('in another widget’s modify mode collect draws no control, and is still and dimmed', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'bar').locator('.bar').first().waitFor();
			await modifyWidget(page, 'tail');
			assert.equal(await pane(page, 'bar').locator('[data-modify]').count(), 0, 'collect drew its controls in the tail’s modify mode');
			assert.equal(
				await pane(page, 'bar').locator('> .content > .tab-body').first().evaluate((el) => el.inert && Number(getComputedStyle(el).opacity) < 1),
				true,
				'collect is live or bright in the tail’s modify mode'
			);
			await leaveWidget(page, 'tail', 'reset');
			await modifyWidget(page, 'bar');
			assert.equal(await pane(page, 'bar').locator('[data-modify]').count(), 1, 'collect drew no controls in its own modify mode');
		}, DESKTOP);
	});

	// vremya, 2026-11-10: the session lays its settings over the layout's, so a
	// side chosen in the menu, held in the session, would mask a flip written
	// to the layout. A modify-mode change drops it there, and ↺ holds it again.
	test('a side the session holds is dropped by ⇄ and held again by ↺; ✓ and the tab’s Save write the layout entry', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seeded(app);
			const id = await idOf(page);
			const pane = await page.locator(COLLECT_PANE).boundingBox();
			const onRight = async () => {
				const box = await stripBox(page);
				return box.x > pane.x + pane.width / 2;
			};
			await pickOption(page, id, 'side', 'right');
			await until(onRight, Boolean);
			assert.deepEqual((await heldOver(page)).map((s) => s.side), ['right'], 'the menu’s side, held in the session');
			await modifyWidget(page, id);
			await cover(page).getByRole('button', { name: 'flip' }).click();
			await until(onRight, (r) => !r);
			assert.deepEqual(await heldOver(page), [], 'the session still holds the side');
			assert.equal((await inTree(page, id)).side, 'left');
			await leaveWidget(page, id, 'reset');
			await until(onRight, Boolean);
			assert.deepEqual((await heldOver(page)).map((s) => s.side), ['right'], '↺ did not hold the side again');

			await modifyWidget(page, id);
			await cover(page).getByRole('button', { name: 'more rows' }).click();
			const rowsNow = (await inTree(page, id)).rows;
			assert.ok(rowsNow > 1);
			await leaveWidget(page, id, 'apply');
			await rootMenuButton(page).click();
			await page.getByRole('menuitem', { name: 'Save', exact: true }).click();
			// The layout entry the Save wrote, whatever the tab is named.
			const saved = await until(
				() =>
					page.evaluate(async () => {
						const mine = (await window.datum.layouts.listLayouts()).filter((l) => l.source === 'user');
						return Promise.all(mine.map((l) => window.datum.layouts.loadLayout(l.name).then((t) => JSON.stringify(t))));
					}),
				(all) => all.length > 0
			);
			assert.ok(saved.some((l) => l.includes(`"rows":${rowsNow}`)), `the layout entry lacks the rows: ${saved}`);
		}, { ...DESKTOP, home: ON_COLLECT, clock: todayAt(15) });
	});
});
