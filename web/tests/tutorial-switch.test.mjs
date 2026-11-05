// The tutorial's switch lesson (da-uzyih.4867): a house seen from above, a
// person, the dot, wandering between its rooms and the garden and about each
// room's furniture (da-uzyih.4880), and where they are one switch field, `room`, logged through the collect widget
// itself, filtered to it (da-2s9im): a tap on the field opens its states, a tap
// on a room logs the change. Played in the game box (da-uzyih.4864), and
// started, as its cue says, by changing `room` to the room the dot stands in,
// whatever logs it.
//
// The clock is held, so each tap is logged at the instant the test set, and
// the expectation is computed here from the plan, not by the page's own
// scoring.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { cuedRoom, isStatus, startSwitch, switchRoom } from './support/tutorial.mjs';
import { until } from './support/until.mjs';
import { finger } from './support/press.mjs';
import { DOORS, ROOMS, WALK_MS, WALLS, dotAt } from '../src/lib/house.ts';

before(startApp);
after(stopApp);

const ROUND_MS = 60_000;
const SEED = 3;
const PANE = '[data-pane="switch-lesson"]';
/** Its fields, a pane of their own (da-uzyih.4908). */
const FIELDS = '[data-pane="switch-fields"]';
const rows = (app, db) =>
	app.run(['tail', 'room', '--json', '-n', '50', ...(db ? ['--db', db] : [])]).then((r) => JSON.parse(r.stdout).rows);
/** The state a timeline in time order holds at `t`, null before its first. */
const at = (timeline, t) => timeline.filter((c) => c.at <= t).at(-1)?.state ?? null;

/** The switch tab, its walk seeded before the tab is first drawn, which draws the scene at rest. The walk. */
async function seeded(app) {
	const { page } = app;
	assert.equal(await page.locator(PANE).count(), 0, 'the switch tab not drawn yet');
	const walk = await page.evaluate((seed) => {
		window.datum.tutorial.house.round.seed = seed;
		return window.datum.tutorial.house.scene(seed);
	}, SEED);
	await page.locator('.root-bar [role="tab"]', { hasText: 'change' }).click();
	await page.locator(`${PANE} .game[aria-busy="false"]`).waitFor();
	return walk;
}

/** The clock `t` ms into the round, and a frame drawn there: the time bar has reached it. */
async function frameAt(app, start, t) {
	await app.clock.set(start + t);
	await app.page.waitForFunction(
		([pane, share]) => Math.abs(parseFloat(document.querySelector(`${pane} .game .go .spent`)?.style.inlineSize) - share) < 0.01,
		[PANE, (t / ROUND_MS) * 100]
	);
}

const dot = (page) =>
	page.locator(`${PANE} .game .house .dot`).evaluate((c) => [Number(c.getAttribute('cx')), Number(c.getAttribute('cy'))]);
/** The plan's place for `state`, as the page has it. */
const place = (page, state) => page.evaluate((s) => window.datum.tutorial.house.ROOMS.find((r) => r.state === s), state);
const inside = (r, [x, y]) => x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height;
const among = (spots, [x, y]) => spots.some((s) => s[0] === x && s[1] === y);

test('the switch game: started by switching room to where the person is, a move recorded, and «End now» shows the share of the round the record had right', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const walk = await seeded(app);
			assert.ok(walk.length >= 6 && walk.some((c) => c.state === 'garden'), `seed ${SEED} walks a while, and out: ${JSON.stringify(walk)}`);
			const house = page.locator(`${PANE} .game .house`);

			// At rest: the dot stands in the walk's first room, and the box names the switch that starts the game, in Start's place.
			assert.equal(await house.getAttribute('data-room'), walk[0].state, 'the dot at rest in its first room');
			assert.deepEqual(await page.locator(`${PANE} .game .cue b`).allInnerTexts(), ['room', walk[0].state], 'the cue names the field and the room');
			assert.equal(await page.locator(`${PANE} .game`).getByRole('button', { name: 'Start', exact: true }).count(), 0, 'no Start button');
			assert.deepEqual(
				await page.locator(`${FIELDS} .fields button[data-field]`).evaluateAll((bs) => bs.map((b) => b.dataset.field)),
				['room'],
				"the lesson's buttons are the fields widget's, for its one field"
			);

			// A switch to another room logs, and starts nothing.
			const elsewhere = walk.find((c) => c.state !== walk[0].state).state;
			await app.clock.forward(1000);
			const early = app.clock.at();
			await switchRoom(page, elsewhere);
			assert.equal(await page.locator(`${PANE} .game`).getAttribute('data-phase'), 'neutral', 'not started by another room');
			assert.equal(await house.getAttribute('data-room'), walk[0].state, 'the dot stays where it stands');

			// The room the dot is in starts it, at that tap.
			await app.clock.forward(1000);
			const start = app.clock.at();
			assert.equal(await startSwitch(page), walk[0].state);

			// While it stays, the person moves about the room (da-uzyih.4880): at its middle, then a step away.
			const { HOP_MS, STEP_MS } = await page.evaluate(() => window.datum.tutorial.house);
			const first = await place(page, walk[0].state);
			await frameAt(app, start, 500);
			assert.deepEqual(await dot(page), first.at, 'at the middle as it arrives');
			await frameAt(app, start, HOP_MS + STEP_MS);
			const moved = await dot(page);
			assert.notDeepEqual(moved, first.at, 'moved within the room');
			assert.ok(inside(first, moved) && among(first.spots, moved), `to another of its spots: ${moved}`);
			assert.equal(await house.getAttribute('data-room'), walk[0].state);

			// Mid-walk the dot is on its way through the doorway, between the two rooms' middles.
			await frameAt(app, start, walk[1].at - 500);
			const midway = await page.evaluate(([walk, t]) => window.datum.tutorial.house.dotAt(walk, t), [walk, walk[1].at - 500]);
			const [from, to] = await page.evaluate(
				(states) => states.map((s) => window.datum.tutorial.house.ROOMS.find((r) => r.state === s).at),
				[walk[0].state, walk[1].state]
			);
			assert.deepEqual(await dot(page), midway);
			assert.notDeepEqual(midway, from);
			assert.notDeepEqual(midway, to);
			assert.equal(await house.getAttribute('data-room'), walk[0].state, 'not through the door yet');

			// Each move tapped a little late, the third missed, and one wrong room 3 s after the second.
			const T = walk[5].at + 2500;
			assert.ok(T < walk[6].at, 'the round ends between moves');
			const lags = [400, 900, 1500];
			const taps = [];
			walk.slice(1, 6).forEach((c, i) => i !== 2 && taps.push({ state: c.state, at: c.at + lags[i % 3] }));
			const wrong = ['kitchen', 'living_room', 'bedroom', 'bathroom', 'garden'].find((s) => s !== walk[2].state && s !== walk[3].state);
			taps.push({ state: wrong, at: walk[2].at + 3000 });
			taps.sort((a, b) => a.at - b.at);
			for (const tap of taps) {
				await frameAt(app, start, tap.at);
				assert.equal(await house.getAttribute('data-room'), at(walk, tap.at), `at ${tap.at} the dot is in ${at(walk, tap.at)}`);
				await switchRoom(page, tap.state);
			}

			await frameAt(app, start, T);
			await page.getByRole('button', { name: 'End now', exact: true }).click();
			const plan = [{ state: walk[0].state, at: 0 }, ...taps];
			let same = 0;
			for (let t = 0; t < T; t++) if (at(plan, t) === at(walk, t)) same++;
			await page.locator(`${PANE} .game .result`).waitFor();
			// The game over screen is its result and Reset: no cue, which only rest has.
			assert.equal(await page.locator(`${PANE} .game .cue`).count(), 0, 'no cue on the game over screen');
			assert.deepEqual(await page.locator(`${PANE} .game .result dt, ${PANE} .game .result dd`).allTextContents(), [
				'accuracy',
				`${Math.round((same / T) * 100)}%`
			]);

			// The record is the database's: each tap a switch entry at its instant, in the tutorial alone.
			const entries = await rows(app);
			assert.deepEqual(
				entries.filter((r) => !isStatus(r)).map((r) => ({ state: r.state, at: Date.parse(r.time) - start })),
				[{ state: elsewhere, at: early - start }, ...plan],
				"room's entries are the taps"
			);
			assert.deepEqual(await rows(app, 'datum'), [], 'no room in datum');

			// Ended, room is IGNORED (da-uzyih.4882), which holds no room; Reset writes nothing, and deletes nothing.
			assert.equal(entries.at(-1).state, 'IGNORED', 'room IGNORED once the game ended');
			const { active } = JSON.parse((await app.run(['now', '--json'])).stdout);
			assert.ok(!active.some((a) => a.field === 'room'), 'room holds no room');
			await app.clock.forward(1000);
			await page.getByRole('button', { name: 'Reset', exact: true }).click();
			await page.locator(`${PANE} .game[aria-busy="false"]`).waitFor();
			assert.equal(await page.locator(`${PANE} .game`).getAttribute('data-phase'), 'neutral');
			assert.equal(await page.locator(`${PANE} .game .cue`).count(), 1, 'the cue back at rest');
			assert.equal(await page.locator(`${PANE} .game`).getByRole('button').count(), 0, 'and no button: no Reset, no Start');
			assert.deepEqual((await rows(app)).map((r) => r.hid), entries.map((r) => r.hid), 'nothing written or deleted');
			assert.equal(await house.getAttribute('data-room'), await cuedRoom(page), 'the dot at rest where the cue says');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-c196m, vividn: «flip position of the kitchen and the bathroom and add an
// exit between the kitchen and the outside», «we could call outside garden too».
const roomOf = (state) => ROOMS.find((r) => r.state === state);
const overlaps = (a0, a1, b0, b1) => Math.min(a1, b1) - Math.max(a0, b0);

test('the kitchen is under the living room and against the garden, the bathroom over the bedroom, and each place holds its own spots, furniture and flowers', () => {
	const [kitchen, bathroom, garden] = ['kitchen', 'bathroom', 'garden'].map(roomOf);
	assert.ok(kitchen.x >= bathroom.x + bathroom.width && kitchen.y >= bathroom.y + bathroom.height, 'the kitchen right of the bathroom and below it');
	assert.equal(kitchen.x + kitchen.width, garden.x, "the kitchen's right wall is the garden's edge");
	assert.ok(overlaps(kitchen.y, kitchen.y + kitchen.height, garden.y, garden.y + garden.height) === kitchen.height, 'along the whole of it');

	const bounds = (s) => (s.length === 3 ? [s[0] - s[2], s[1] - s[2], s[0] + s[2], s[1] + s[2]] : [s[0], s[1], s[0] + s[2], s[1] + s[3]]);
	for (const r of ROOMS) {
		for (const p of [r.at, ...r.spots]) assert.ok(inside(r, p), `${r.state}: a spot outside it at ${p}`);
		for (const { name, shapes } of r.things)
			for (const s of shapes) {
				const [x0, y0, x1, y1] = bounds(s);
				assert.ok(inside(r, [x0, y0]) && inside(r, [x1, y1]), `${r.state}: ${name} past its walls: ${s}`);
			}
	}

	// The flowers keep from where the dot, radius 6, stands, and have colors of their own.
	const flowers = garden.things.filter((t) => t.colors);
	assert.ok(flowers.length >= 1, 'flowers in the garden');
	for (const { name, shapes, colors } of flowers) {
		assert.equal(colors.length, shapes.length, `${name}: a color a shape`);
		for (const [cx, cy, r] of shapes)
			for (const [x, y] of [garden.at, ...garden.spots])
				assert.ok(Math.hypot(cx - x, cy - y) >= r + 6, `${name} at ${cx},${cy} is under the dot at ${x},${y}`);
	}
});

/** Each straight stretch of `WALLS`. */
const walls = [...WALLS.matchAll(/M(\d+) (\d+)((?: [HV]\d+)+)/g)].flatMap(([, x, y, moves]) => {
	let p = [Number(x), Number(y)];
	return moves.trim().split(' ').map((m) => {
		const from = p;
		p = m[0] === 'H' ? [Number(m.slice(1)), p[1]] : [p[0], Number(m.slice(1))];
		return [from, p];
	});
});
const side = (a, b, p) => Math.sign((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
/** Whether the step from `p` to `q` crosses a stretch of wall, or touches one. */
const hitsWall = (p, q) => walls.some(([a, b]) => side(a, b, p) !== side(a, b, q) && side(p, q, a) !== side(p, q, b));

test('every doorway is a gap in a wall the two places share, and a walk between any two of them keeps to floor and door', () => {
	assert.ok(hitsWall(roomOf('kitchen').at, roomOf('bathroom').at), 'a straight line between rooms with no door meets a wall: the check can fail');
	assert.ok(DOORS.some(({ between }) => between.includes('kitchen') && between.includes('garden')), 'a door between the kitchen and the garden');

	for (const { between, at } of DOORS) {
		for (const state of between) {
			const r = roomOf(state);
			assert.ok(inside(r, at) && (at[0] === r.x || at[0] === r.x + r.width || at[1] === r.y || at[1] === r.y + r.height), `${between}: on ${state}'s edge`);
		}
		const across = between.every((s) => at[0] === roomOf(s).x || at[0] === roomOf(s).x + roomOf(s).width);
		// As wide as the others: 8 either side of its middle is clear, and just past it is wall.
		const along = (d) => (across ? [at[0], at[1] + d] : [at[0] + d, at[1]]);
		const normal = (p) => (across ? [[p[0] - 3, p[1]], [p[0] + 3, p[1]]] : [[p[0], p[1] - 3], [p[0], p[1] + 3]]);
		for (const d of [-7, 0, 7]) assert.ok(!hitsWall(...normal(along(d))), `${between}: clear at ${d} from its middle`);
		for (const d of [-9, 9]) assert.ok(hitsWall(...normal(along(d))), `${between}: wall at ${d} from its middle`);
	}

	// From wherever the dot stands when it sets off, however long it stayed: a walk each way through each door.
	for (const { between } of DOORS)
		for (const [from, to] of [between, [...between].reverse()])
			for (let leave = 5000; leave <= 11_000; leave += 500) {
				const walk = [{ state: from, at: 0 }, { state: to, at: leave }];
				let last = dotAt(walk, leave - WALK_MS / 2);
				for (let t = leave - WALK_MS / 2 + 20; t <= leave + WALK_MS / 2; t += 20) {
					const here = dotAt(walk, t);
					assert.ok(!hitsWall(last, here), `${from} to ${to} leaving at ${leave}: through a wall at ${t}, ${last} to ${here}`);
					assert.ok(ROOMS.some((r) => inside(r, here)), `${from} to ${to}: off the plan at ${here}`);
					last = here;
				}
			}
});

// da-uzyih.4880, vividn: «add some furniture too to make the rooms more
// apparent what they are … The spec shouldn't allow end on this one either».
// `room` is never_false: an `end` is refused, and a game's Reset IGNOREs the
// room held instead of ending it.
test('each room has its furniture; room cannot be ended, and Reset leaves it IGNORED', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await seeded(app);
			const things = page.locator(`${PANE} .game .house .thing`);
			assert.deepEqual(await things.evaluateAll((gs) => gs.map((g) => `${g.dataset.room}:${g.dataset.thing}`)), [
				'kitchen:counter',
				'kitchen:stove',
				'living_room:sofa',
				'living_room:table',
				'bedroom:bed',
				'bedroom:wardrobe',
				'bathroom:bath',
				'bathroom:sink',
				'garden:tree',
				'garden:bench',
				'garden:cosmos',
				'garden:daisies'
			]);
			assert.ok((await things.evaluateAll((gs) => gs.map((g) => g.querySelectorAll('rect, circle').length))).every((n) => n > 0), 'each piece drawn');

			// Not the cued room, so the game stays at rest.
			const elsewhere = (await cuedRoom(page)) === 'kitchen' ? 'bedroom' : 'kitchen';
			await app.clock.forward(1000);
			assert.equal((await app.run(['switch', 'room', elsewhere])).code, 0);
			// Past the switch's instant, so a refusal is the spec's and not a collision.
			await app.clock.forward(1000);
			for (const argv of [['end', 'room'], ['switch', 'room', 'false']]) {
				const refused = await app.run(argv);
				assert.notEqual(refused.code, 0, `${argv.join(' ')} refused`);
				assert.match(refused.stderr, /never false/);
			}

			// A game played, and a room switched to after it: Reset IGNOREs it.
			await app.clock.forward(1000);
			await startSwitch(page);
			await app.clock.forward(1000);
			await page.getByRole('button', { name: 'End now', exact: true }).click();
			await page.locator(`${PANE} .game[data-phase="done"][aria-busy="false"]`).waitFor();
			await app.clock.forward(1000);
			assert.equal((await app.run(['switch', 'room', elsewhere])).code, 0);
			await app.clock.forward(1000);
			const before = (await rows(app)).map((r) => r.hid);
			await page.getByRole('button', { name: 'Reset', exact: true }).click();
			await page.locator(`${PANE} .game[aria-busy="false"]`).waitFor();
			const after = await rows(app);
			assert.deepEqual(after.slice(0, -1).map((r) => r.hid), before, 'nothing deleted');
			assert.equal(after.at(-1).state, 'IGNORED', 'room IGNORED by Reset');
			assert.ok(!after.some((r) => r.state === false), 'no end written');
			assert.equal(await page.locator(`${PANE} .game .failed`).count(), 0, 'nothing refused');
			assert.equal(await page.locator(`${PANE} .game`).getAttribute('data-phase'), 'neutral');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-2s9im: the game reads what is logged in its field, whatever logged it, not
// the collect widget's taps alone: a line typed in the bar starts it as a tap does.
test('a change of room typed in the bar starts the switch game, as a tap does', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const walk = await seeded(app);
			await app.clock.forward(1000);
			const input = page.locator('.prompt-bar input');
			await input.fill(`change room ${walk[0].state}`);
			await input.press('Enter');
			await page.locator(`${PANE} .game[data-phase="playing"]`).waitFor();
			assert.equal((await rows(app)).filter((r) => !isStatus(r)).at(-1).state, walk[0].state, 'the typed change logged');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4943: on a 280 px phone `room`'s states are dealt into one row
// wider than the room, which pans, and a cue naming any of them must be
// followable by a tap. The picker is collect's own now (da-2s9im), as in any
// layout, whose states a drag pans. Every state is tapped here. Todo until
// da-u846n: at 280 px «garden» is dealt off the page's left, where neither a
// reveal nor a finger's drag on the states reaches it.
test('on a 280 px phone every state of room is in reach of a tap', { todo: 'da-u846n: collect pans no state left of its room into reach' }, async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await seeded(app);
			for (const { state } of ROOMS) {
				await app.clock.forward(1000);
				const before = (await rows(app)).length;
				await page.locator(`${FIELDS} .fields button[data-field="room"]`).click();
				const states = await page.locator(`${FIELDS} .ring button[data-state]`).evaluateAll((bs) => bs.map((b) => b.dataset.state));
				assert.deepEqual(states.sort(), ROOMS.map((r) => r.state).sort());
				// One left of the room is panned in as a learner does, by a finger's drag (a reveal scrolls the
				// room only right and down), then tapped.
				const button = page.locator(`${FIELDS} .ring button[data-state="${state}"]`);
				const [room, at] = [await page.locator(`${FIELDS} .room`).boundingBox(), await button.boundingBox()];
				const dx = room.x - at.x + 8;
				const mid = { x: room.x + room.width / 2, y: room.y + room.height / 2 };
				if (dx > 0) await finger(page, mid, { x: mid.x + dx, y: mid.y });
				await button.click();
				await until(() => rows(app).then((r) => r.length), (n) => n === before + 1);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, viewport: { width: 280, height: 653 }, query: '?db=tutorial' }
	);
});

// Under prefers-reduced-motion the dot does not walk: it stands in its room
// and jumps at the doorway's moment.
test('where motion is unwelcome the dot jumps from room to room', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const walk = await seeded(app);
			const middle = (state) =>
				page.evaluate((s) => window.datum.tutorial.house.ROOMS.find((r) => r.state === s).at, state);
			const start = app.clock.at();
			await startSwitch(page);
			await frameAt(app, start, walk[1].at - 500);
			assert.deepEqual(await dot(page), await middle(walk[0].state), 'still in the room it leaves');
			// Half as many moves about a room, and each at once: at a spot the instant it goes.
			const { HOP_MS } = await page.evaluate(() => window.datum.tutorial.house);
			assert.ok(2 * HOP_MS > walk[1].at - 500 && 2 * HOP_MS < walk[1].at, `seed ${SEED} moves once in its first room`);
			await frameAt(app, start, 2 * HOP_MS);
			const there = await dot(page);
			assert.ok(among((await place(page, walk[0].state)).spots, there), `at a spot: ${there}`);
			await frameAt(app, start, walk[1].at);
			assert.deepEqual(await dot(page), await middle(walk[1].state), 'and at once in the next');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial', reducedMotion: 'reduce' }
	);
});
