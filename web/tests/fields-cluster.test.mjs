// A switch field's states, put in the fields' place and arranged around
// `end` at the widget's centre (da-vg8vr). vividn: «break them out of rows
// entirely … packing things efficiently as close as possible»; then «keep
// the states in a consistent place inside of the fields widget. I still
// want them to be arranged radially, with end in the center, but back can
// move to the top left corner of the widget, add state can go in the top
// right», «allow panning in 2 dimensions if states run off the sides
// radially», «replace the widget's contents».
//
// The spec's order is a rank by distance (`ringOf` in fields.ts); the
// setup widget's ↑ / ↓ beside each state is what reorders them, and
// logging never does. The list's other behaviour — the colours, `+`, Tab —
// is `fields-states.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ringOf } from '../src/lib/fields.ts';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows } from './support/rows.mjs';
import { FIELD_FORM, leaveField, openField } from './support/field.mjs';

const H = 30;
const GAP = 6;

/** Every button of a ring as a box. */
function boxes(ring, w) {
	return [
		{ name: 'end', ...ring.end, w: w.end },
		...ring.states.map((s, i) => ({ name: i, ...s, w: w.states[i] }))
	];
}

/** Whether two boxes stand closer than `GAP`. */
const touch = (a, b) => Math.abs(a.x - b.x) < (a.w + b.w) / 2 + GAP - 1e-9 && Math.abs(a.y - b.y) < H + GAP - 1e-9;
/** The pairs that stand closer than `GAP`. */
const overlaps = (list) => list.flatMap((a, i) => list.slice(i + 1).filter((b) => touch(a, b)).map((b) => [a.name, b.name]));
/** How far the centre is from the nearest point of a box. */
const near = (p, wide) => Math.hypot(Math.max(0, Math.abs(p.x) - wide / 2), Math.max(0, Math.abs(p.y) - H / 2));

describe('ringOf', () => {
	const sizes = (n, end = 50, wide = (i) => 40 + 7 * (i % 3)) => ({
		end,
		states: Array.from({ length: n }, (_, i) => wide(i))
	});

	test('`end` the centre; the first state straight up, then down, right and left', () => {
		const ring = ringOf(sizes(4, 50, () => 40), H, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		const [up, down, right, left] = ring.states;
		assert.ok(up.x === 0 && up.y < 0, `the first is at ${up.x},${up.y}`);
		assert.ok(down.x === 0 && down.y > 0, `the second is at ${down.x},${down.y}`);
		assert.ok(right.y === 0 && right.x > 0, `the third is at ${right.x},${right.y}`);
		assert.ok(left.y === 0 && left.x < 0, `the fourth is at ${left.x},${left.y}`);
	});

	test('no two buttons touch, for none to forty states', () => {
		for (const end of [50, 30])
			for (let n = 0; n <= 40; n++) {
				const w = sizes(n, end);
				assert.deepEqual(overlaps(boxes(ringOf(w, H, GAP), w)), [], `${n} states, end ${end}`);
			}
	});

	// «a later-ranked state never nearer than an earlier one could have
	// stood»: no spot on the 2px grid that was free when a state was placed
	// is nearer the centre than the one it took.
	test('each state takes the nearest spot free when its turn came', () => {
		const w = sizes(16, 54.75, (i) => (i % 4 === 3 ? 90 : 46.6));
		const ring = ringOf(w, H, GAP);
		const all = boxes(ring, w);
		ring.states.forEach((s, i) => {
			const wide = w.states[i];
			const before = all.filter((b) => b.name === 'end' || (typeof b.name === 'number' && b.name < i));
			const took = near(s, wide);
			for (let x = -2 * Math.ceil(took + wide); x <= 2 * Math.ceil(took + wide); x += 2)
				for (let y = -2 * Math.ceil(took + H); y <= 2 * Math.ceil(took + H); y += 2) {
					const p = { x, y, w: wide };
					if (before.every((b) => !touch(b, p))) assert.ok(near(p, wide) >= took - 1e-9, `state ${i} took ${took}, ${x},${y} was free at ${near(p, wide)}`);
				}
			// And so no later state stands where it would have been nearer.
			for (const later of ring.states.slice(i + 1)) {
				const p = { ...later, w: wide };
				if (before.every((b) => !touch(b, p))) assert.ok(near(p, wide) >= took - 1e-9, `state ${i} at ${took}, a later one at ${near(p, wide)}`);
			}
		});
	});
});

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';
const PICKER = `${BAR} .picker`;
const STATES = ['a', 'b', 'c', 'd'];
/** Every field a switch of the same states, so each opens the same picture: one to tap wherever a field is drawn. */
const FIELDS = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra mood'.split(' ');
const SEED = FIELDS.map((f) => `setup ${f} --verb switch --states false,${STATES.join(',')}`);

/** What the open states draw: the pane, the box of states, and each button's box, by name. */
const seen = (page) =>
	page.locator(PICKER).evaluate((pane) => {
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, x: r.left + r.width / 2, y: r.top + r.height / 2 };
		};
		return {
			pane: box(pane),
			ring: box(pane.querySelector('.ring')),
			buttons: [...pane.querySelectorAll('button')].map((b) => ({ name: b.dataset.state ?? (b.dataset.back !== undefined ? 'back' : b.textContent.trim()), ...box(b) }))
		};
	});

/** Press `field`'s button where it is drawn, read the states, close them with Escape. */
async function open(page, field) {
	await page.locator(`${BAR} [data-field="${field}"]`).dispatchEvent('click');
	await page.locator(`${PICKER} [data-add-state]`).waitFor();
	const t = await seen(page);
	await page.keyboard.press('Escape');
	await page.locator(PICKER).waitFor({ state: 'detached' });
	return { ...t, find: (name) => t.buttons.find((b) => b.name === name) };
}

/** Drag from the pane's middle by dx, dy, in steps, and read what is drawn. */
async function dragBy(page, dx, dy) {
	const from = await page.locator(PICKER).boundingBox();
	const [x, y] = [from.x + from.width / 2, from.y + from.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	for (let i = 1; i <= 5; i++) await page.mouse.move(x + (dx * i) / 5, y + (dy * i) / 5);
	await page.mouse.up();
	return seen(page);
}

/** Drag by dx, dy as several strokes of at most 150px, so the pointer stays on the page. */
async function panBy(page, dx, dy) {
	const n = Math.max(1, Math.ceil(Math.max(Math.abs(dx), Math.abs(dy)) / 150));
	let t;
	for (let i = 0; i < n; i++) t = await dragBy(page, dx / n, dy / n);
	return t;
}

/** Each state's centre from the pane's, to the pixel. */
const offsets = (t) =>
	Object.fromEntries(t.buttons.filter((b) => b.name !== 'back' && b.name !== '+ Add state').map((b) => [b.name, [Math.round(b.x - t.pane.x), Math.round(b.y - t.pane.y)]]));

/** Where the two corner buttons are in the pane. */
const corners = (t) => {
	const [back, add] = [t.find('back'), t.find('+ Add state')];
	return { back: [back.left - t.pane.left, back.top - t.pane.top], add: [t.pane.right - add.right, add.top - t.pane.top] };
};

const options = (width = 390) => ({ viewport: { width, height: 640 }, clock: todayAt(15), prepare: (context) => context.addInitScript(draft, PAGE) });

describe('a switch field’s states in the fields’ place, `end` at the centre', () => {
	before(startApp);
	after(stopApp);

	for (const width of [390, 1280])
		test(`${width}px: the same picture whichever field is pressed and wherever it stands, back and add in the top corners`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				await atRows(page, 'bar', 3);
				// The first field, the last and one between, each where the bar's
				// own scroll leaves it.
				const fields = await page.locator(BAR).evaluate((bar) => [...bar.querySelectorAll('[data-field]')].map((b) => b.dataset.field));
				const picked = [fields[0], fields[Math.floor(fields.length / 2)], fields.at(-1)];
				const first = await open(page, picked[0]);
				assert.deepEqual(first.buttons.map((b) => b.name), ['back', 'false', ...STATES, '+ Add state']);
				const end = first.find('false');
				assert.ok(Math.abs(end.x - first.pane.x) <= 1 && Math.abs(end.y - first.pane.y) <= 1, `end at ${end.x},${end.y}, the widget's centre at ${first.pane.x},${first.pane.y}`);
				// The corners: back top left, add top right, out of the packing.
				const at = corners(first);
				for (const [name, [side, top]] of Object.entries(at))
					assert.ok(side >= 0 && side <= 8 && top >= 0 && top <= 8, `${name} is ${side}px from its side and ${top}px from the top`);
				// The rank: a no further from `end` than b, b than c, to the
				// nearest point of each.
				const reach = (b) => Math.hypot(Math.max(0, b.left - end.x, end.x - b.right), Math.max(0, b.top - end.y, end.y - b.bottom));
				const d = STATES.map((s) => reach(first.find(s)));
				assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1] - 1), `a higher rank stands further: ${d}`);
				for (const field of picked.slice(1)) {
					const t = await open(page, field);
					assert.deepEqual(offsets(t), offsets(first), `${field}: not ${picked[0]}'s picture`);
					assert.deepEqual(corners(t), at, `${field}: the corners moved`);
				}
				// The bar's own scroll moved: still the same picture.
				await page.locator(BAR).evaluate((bar) => (bar.scrollLeft = bar.scrollWidth));
				const scrolled = await open(page, fields.at(-1));
				assert.deepEqual(offsets(scrolled), offsets(first), 'scrolled: not the same picture');
				// A key's press opens the same.
				await page.locator(`${BAR} [data-field="mood"]`).focus();
				await page.keyboard.press('Enter');
				await page.locator(`${PICKER} [data-add-state]`).waitFor();
				const keyed = await seen(page);
				assert.deepEqual(offsets(keyed), offsets(first), 'by key: not the same picture');
			}, options(width));
		});

	test('22 states at 390px: a drag pans them both ways and presses nothing, the corners stay, a tap on a state logs it', async () => {
		// Long names, so they run past a phone's width as well as a short pane's height.
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await atRows(page, 'bar', 3);
			await field.click();
			await page.locator(`${PICKER} [data-add-state]`).waitFor();
			const was = await seen(page);
			assert.ok(was.ring.left < was.pane.left && was.ring.right > was.pane.right, 'the states fit across: the case is not tested');
			assert.ok(was.ring.bottom - was.ring.top > was.pane.bottom - was.pane.top, 'the states fit down: the case is not tested');
			const before = (await app.echoes()).length;
			const drag = (dx, dy) => dragBy(page, dx, dy);
			// Started ON a state — `end`, at the pane's middle — well past the
			// slop: up and to the left.
			const moved = await drag(-30, -20);
			// As far as the drag went, or as far as the states run past the pane.
			const want = [-Math.min(30, was.ring.right - was.pane.right), -Math.min(20, was.ring.bottom - was.pane.bottom)];
			const got = [moved.ring.left - was.ring.left, moved.ring.top - was.ring.top];
			assert.ok(got.every((g, i) => g < 0 && Math.abs(g - want[i]) <= 1), `the states moved ${got}, the drag ${want}`);
			assert.deepEqual(corners({ ...moved, find: (n) => moved.buttons.find((b) => b.name === n) }), corners({ ...was, find: (n) => was.buttons.find((b) => b.name === n) }), 'the corners moved');
			const strokes = (dx, dy) => panBy(page, dx, dy);
			// Further than their extent: their far edge stops at the pane's.
			const span = [was.ring.right - was.ring.left, was.ring.bottom - was.ring.top];
			const far = await strokes(-span[0], -span[1]);
			assert.ok(Math.abs(far.ring.right - far.pane.right) <= 1 && Math.abs(far.ring.bottom - far.pane.bottom) <= 1, `panned past the states: ${far.ring.right},${far.ring.bottom} in ${far.pane.right},${far.pane.bottom}`);
			const back = await strokes(span[0], span[1]);
			// The other way their top stops a gap below the corners, not at the pane's top.
			const lid = Math.max(...['back', '+ Add state'].map((n) => back.buttons.find((b) => b.name === n).bottom)) + GAP;
			assert.ok(Math.abs(back.ring.left - back.pane.left) <= 1 && Math.abs(back.ring.top - lid) <= 1, `panned past the states the other way: ${back.ring.left},${back.ring.top}, the corners end at ${lid - GAP}`);
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'a drag pressed the state it started on');

			// Back where they opened, as far as they went; then a press that
			// does not travel is a press, on the state right of `end`.
			const home = await strokes(was.ring.left - back.ring.left, was.ring.top - back.ring.top);
			assert.ok(Math.abs(home.ring.left - was.ring.left) <= 1 && Math.abs(home.ring.top - was.ring.top) <= 1, 'not back where they opened');
			await page.locator(`${PICKER} [data-state="a_rather_long_state_02"]`).click();
			await page.locator(PICKER).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).at(-1), 'switch mood a_rather_long_state_02');
			assert.ok(await field.isVisible(), 'the fields are not back');
		}, options());
	});

	// vremya, on 43ebbe8c: «a state can come to rest under a corner … a tap
	// on it presses the corner». The room a pan keeps starts below them.
	for (const rows of [3, 2])
		test(`22 states in a ${rows}-row pane at 390px: every state can be panned out from under the corners and pressed`, async () => {
			const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
			await withApp(async (app) => {
				const { page } = app;
				await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
				const field = page.locator(`${BAR} [data-field="mood"]`);
				await field.waitFor();
				await atRows(page, 'bar', rows);
				await field.click();
				await page.locator(`${PICKER} [data-add-state]`).waitFor();
				const was = await seen(page);
				assert.ok(was.ring.bottom - was.ring.top > was.pane.bottom - was.pane.top, 'the states fit down: the case is not tested');
				const before = (await app.echoes()).length;
				/** Every state drawn with its centre in the pane, and what a tap at that centre would press. */
				const hits = () =>
					page.locator(PICKER).evaluate((pane) => {
						const box = pane.getBoundingClientRect();
						return [...pane.querySelectorAll('.ring [data-state]')]
							.map((b) => [b, b.getBoundingClientRect()])
							.map(([b, r]) => ({ state: b.dataset.state, x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height }))
							.filter((s) => s.x > box.left && s.x < box.right && s.y > box.top && s.y < box.bottom)
							.map((s) => ({ ...s, pressed: document.elementFromPoint(s.x, s.y)?.closest('button')?.dataset.state ?? document.elementFromPoint(s.x, s.y)?.closest('button')?.textContent.trim() ?? null }));
					});
				const span = [was.ring.right - was.ring.left, was.ring.bottom - was.ring.top];
				// Panned fully down, the top row stands clear of the corners, either
				// way across. (Panned up, rows from the middle of a ring taller than
				// the room pass under them, and are pressed at another pan, below.)
				for (const [sx, sy] of [[1, 1], [-1, 1]]) {
					await panBy(page, sx * span[0], sy * span[1]);
					const under = (await hits()).filter((s) => s.pressed !== s.state);
					assert.deepEqual(under.map((s) => `${s.state} → ${s.pressed}`), [], `panned ${sx},${sy}`);
				}
				// …and every state can be brought into the room below them and pressed.
				const lid = await page.locator(PICKER).evaluate((pane) => Math.max(...[...pane.querySelectorAll(':scope > .corner')].map((c) => c.getBoundingClientRect().bottom)));
				for (const state of ['false', ...many]) {
					const [pane, b] = [await page.locator(PICKER).boundingBox(), await page.locator(`${PICKER} [data-state="${state}"]`).boundingBox()];
					const into = (lo, hi, at, size) => Math.max(lo - at, Math.min(0, hi - (at + size)));
					await panBy(page, into(pane.x + 2, pane.x + pane.width - 2, b.x, b.width), into(lid + 2, pane.y + pane.height - 2, b.y, b.height));
					const hit = (await hits()).find((s) => s.state === state);
					assert.ok(hit, `${state} could not be panned into the pane`);
					assert.equal(hit.pressed, state, `${state}: a tap at its centre presses ${hit.pressed}`);
				}
				await app.idle();
				assert.equal((await app.echoes()).length, before, 'a pan pressed something');
			}, options());
		});

	// da-amd70: at rest a state that would stand under a corner, where a tap
	// presses the corner, is moved down just clear of it, `end` as near the
	// center as that leaves it; and a tap on each state drawn presses it.
	for (const [count, rows] of [[22, 3], [22, 2], [8, 3], [8, 2]])
		test(`${count} states in a ${rows}-row pane at 390px: at rest none stands under a corner, and a tap on each state drawn logs it`, async () => {
			const many = Array.from({ length: count }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
			await withApp(async (app) => {
				const { page } = app;
				await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
				const field = page.locator(`${BAR} [data-field="mood"]`);
				await field.waitFor();
				await atRows(page, 'bar', rows);
				await field.click();
				await page.locator(`${PICKER} [data-add-state]`).waitFor();
				const t = await seen(page);
				const corner = ['back', '+ Add state'].map((n) => t.buttons.find((b) => b.name === n));
				const states = t.buttons.filter((b) => !corner.includes(b));
				const across = (b, c) => b.left < c.right + GAP && b.right + GAP > c.left;
				// A pixel's give: the widget places by whole-pixel offsets.
				const under = (b, dy = 0) => corner.filter((c) => across(b, c) && b.top - dy < c.bottom + GAP - 1 && b.bottom - dy + GAP > c.top);
				// How far down they rest: `end` from the center.
				const end = t.buttons.find((b) => b.name === 'false');
				const dy = end.y - t.pane.y;
				assert.ok(Math.abs(end.x - t.pane.x) <= 1, `end moved across, to ${end.x - t.pane.x}`);
				assert.ok(states.some((b) => under(b, dy).length), 'centered, no state stands under a corner: the case is not tested');
				assert.deepEqual(states.filter((b) => under(b).length).map((b) => b.name), [], 'at rest under a corner');
				// …and no further: one is just clear of the corner it would be under.
				assert.ok(states.some((b) => corner.some((c) => across(b, c) && Math.abs(b.top - c.bottom - GAP) <= 1.5)), `moved down ${dy}, further than clears the corners`);
				const drawn = states.filter((b) => b.x > t.pane.left && b.x < t.pane.right && b.y > t.pane.top && b.y < t.pane.bottom);
				for (const { name } of drawn) {
					// Read again: the state the field is in is drawn bold, so a tap can move the rest.
					const b = (await seen(page)).buttons.find((b) => b.name === name);
					await app.clock.forward(60_000);
					await page.mouse.click(b.x, b.y);
					await page.locator(PICKER).waitFor({ state: 'detached' });
					await app.idle();
					assert.equal((await app.echoes()).at(-1), name === 'false' ? 'end mood' : `switch mood ${name}`, `the tap on ${name}`);
					await field.click();
					await page.locator(`${PICKER} [data-add-state]`).waitFor();
				}
			}, options());
		});

	// da-71p3u: the ring overruns its room, which clips it; a reveal of a
	// state it clips — a focus, a click's scroll into view — is a pan, so
	// neither the room nor anything around the widget is left scrolled.
	test('22 states in a 3-row pane at 390px: focusing a state the room clips pans it toward sight, at rest and after a pan, and nothing scrolls', async () => {
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await atRows(page, 'bar', 3);
			await field.click();
			await page.locator(`${PICKER} [data-add-state]`).waitFor();
			/** Every box from the room up that is scrolled, and how far. */
			const scrolled = () =>
				page.locator(PICKER).evaluate((el) => {
					const out = [];
					for (let e = el; e; e = e.parentElement) if (e.scrollTop || e.scrollLeft) out.push(`${e.className}: ${e.scrollLeft},${e.scrollTop}`);
					return out;
				});
			/** How far a box stands outside the pane, summed over its sides. */
			const outside = (b, pane) =>
				Math.max(0, pane.left - b.left) + Math.max(0, b.right - pane.right) + Math.max(0, pane.top - b.top) + Math.max(0, b.bottom - pane.bottom);
			/** Focus a state the room clips, and read what is drawn once a frame has passed. */
			const reveal = async (when) => {
				const t = await seen(page);
				const clipped = t.buttons.find((b) => many.includes(b.name) && outside(b, t.pane) > 1 && b.bottom > t.pane.top && b.top < t.pane.bottom);
				assert.ok(clipped, `${when}: no state the room clips: the case is not tested`);
				await page.locator(`${PICKER} [data-state="${clipped.name}"]`).focus();
				await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
				assert.deepEqual(await scrolled(), [], `${when}: focusing ${clipped.name} scrolled`);
				// As far as the browser's reveal took it: drawn there by the pan.
				const now = await seen(page);
				const after = outside(now.buttons.find((b) => b.name === clipped.name), now.pane);
				assert.ok(after < outside(clipped, t.pane) - 1, `${when}: ${clipped.name} was not panned toward sight (${outside(clipped, t.pane)} → ${after}px out)`);
				return clipped.name;
			};
			assert.deepEqual(await scrolled(), [], 'at rest');
			await reveal('at rest');
			await dragBy(page, 40, 60);
			assert.deepEqual(await scrolled(), [], 'panned');
			const name = await reveal('after a pan');
			// The focused state presses as ever.
			await app.clock.forward(60_000);
			await page.keyboard.press('Enter');
			await page.locator(PICKER).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).at(-1), `switch mood ${name}`);
		}, options());
	});

	test('Escape and back put the fields back as they were, logging nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await atRows(page, 'bar', 3);
			const scroll = () => page.locator(BAR).evaluate((bar) => [bar.scrollLeft, bar.scrollTop]);
			await page.locator(BAR).evaluate((bar) => (bar.scrollLeft = bar.scrollWidth / 3));
			const scrolled = await scroll();
			assert.ok(scrolled[0] > 0, 'the bar does not scroll: the case is not tested');
			const before = (await app.echoes()).length;
			for (const how of ['Escape', 'back']) {
				await field.dispatchEvent('click');
				await page.locator(`${PICKER} [data-add-state]`).waitFor();
				assert.equal(await field.isVisible(), false, `${how}: the fields are still drawn over the states`);
				if (how === 'Escape') await page.keyboard.press('Escape');
				else await page.locator(`${PICKER} [data-back]`).click();
				await page.locator(PICKER).waitFor({ state: 'detached' });
				await app.idle();
				assert.equal((await app.echoes()).length, before, `${how} logged something`);
				assert.deepEqual(await scroll(), scrolled, `${how}: the bar moved`);
				assert.ok(await field.isVisible(), `${how}: the fields are not back`);
			}
		}, options());
	});

	test('reordered by the setup widget alone, and a state learned while they are open placed where a fresh open puts it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await atRows(page, 'bar', 3);
			// Logging does not reorder. Timed apart: the app's clock is held,
			// and two switches of one field at one minute collide.
			const first = await open(page, 'mood');
			for (const [s, t] of [['d', '1400'], ['d', '1410'], ['c', '1420'], ['d', '1430']]) await app.submit(`switch mood ${s} -t ${t}`);
			await app.idle();
			const logged = await open(page, 'mood');
			assert.deepEqual(logged.buttons.map((b) => b.name), first.buttons.map((b) => b.name), 'logging reordered the states');
			assert.deepEqual(offsets(logged), offsets(first), 'logging moved a state');

			const { saved } = await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} [aria-label="move d: drag, or the arrow keys"]`).focus();
			for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowUp');
			await page.locator(`${FIELD_FORM} .commit`).click();
			await saved;
			assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).allowed_states, ['false', 'd', 'a', 'b', 'c']);
			await leaveField(page);
			await field.waitFor();
			await app.idle();
			const reordered = await open(page, 'mood');
			// d took the place a had, and so on round; `end` stays at the centre.
			const [now, was] = [offsets(reordered), offsets(logged)];
			assert.deepEqual(['false', 'd', 'a', 'b', 'c'].map((s) => now[s]), ['false', ...STATES].map((s) => was[s]));

			const offset = () =>
				page.locator(`${PICKER} .ring`).evaluate((ring) => {
					const at = (sel) => ring.querySelector(sel).getBoundingClientRect();
					const [e, end] = [at('[data-state="e"]'), at('[data-state="false"]')];
					return [Math.round(e.x - end.x), Math.round(e.y - end.y)];
				});
			await field.dispatchEvent('click');
			await page.locator(`${PICKER} [data-add-state]`).waitFor();
			await app.submit('setup mood --states false,d,a,b,c,e');
			await page.locator(`${PICKER} [data-state="e"]`).waitFor();
			const live = await offset();
			await page.keyboard.press('Escape');
			await page.locator(PICKER).waitFor({ state: 'detached' });
			await field.dispatchEvent('click');
			await page.locator(`${PICKER} [data-state="e"]`).waitFor();
			assert.deepEqual(live, await offset(), 'the learned state was not placed');
		}, options());
	});
});
