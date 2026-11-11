// A switch field's states, opened in the collect widget (da-vg8vr,
// da-w6437, da-r48y9): in collect's rows, the first row back and the field,
// then the spec's states column by column, panned sideways as the fields at
// rest are. What the first row holds and the order the states come in is
// `collect-open.test.mjs`; the view as a whole is `collect.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { FIELD_FORM, leaveField, openField } from './support/field.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';
const VIEW = `${BAR} .collect.open`;
const ROOM = `${VIEW} .room`;
const STATES = ['a', 'b', 'c', 'd'];
/** Every field a switch of the same states, so each opens the same picture: one to tap wherever a field is drawn. */
const FIELDS = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra mood'.split(' ');
const SEED = FIELDS.map((f) => `setup ${f} --verb switch --states false,${STATES.join(',')}`);

/** What the open view draws: the room, the box round all it holds, and each state's box, by name. */
const seen = (page) =>
	page.locator(ROOM).evaluate((room) => {
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, x: r.left + r.width / 2, y: r.top + r.height / 2 };
		};
		const spots = [...room.querySelectorAll('.spot.open')].map(box);
		const ring = { left: Math.min(...spots.map((s) => s.left)), right: Math.max(...spots.map((s) => s.right)), top: Math.min(...spots.map((s) => s.top)), bottom: Math.max(...spots.map((s) => s.bottom)) };
		return {
			pane: box(room),
			ring,
			buttons: [...room.querySelectorAll('[data-state]')].map((b) => ({ name: b.dataset.state, row: Number(b.closest('.spot').dataset.row), ...box(b) }))
		};
	});

/** Read the states once placed: the room takes its size as the view's rows
 * mount, and its observer places the states again, so wait out two frames. */
async function placed(page) {
	await page.locator(`${ROOM} [data-state="a"]`).waitFor();
	await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
	return seen(page);
}

/**
 * Wait until any glide of the bar has landed after leaving the setup place: until
 * then the room the states stand in changes every frame, and a read made then
 * sees that frame's rows (da-jsrh8).
 */
const rested = (page) =>
	page.evaluate(async () => {
		// Two frames that start at or past the glide's scheduled end: the first runs its last step, the second sees it drawn.
		for (let past = 0; past < 2; ) {
			const t = await new Promise(requestAnimationFrame);
			past = t >= Number(document.querySelector('.shell').dataset.glideEnds) ? past + 1 : 0;
		}
	});

/** Open `field` by a press where it is drawn, read the states, close them with Escape. */
async function open(page, field) {
	await page.locator(`${BAR} [data-field="${field}"]`).dispatchEvent('click');
	const t = await placed(page);
	await page.keyboard.press('Escape');
	await page.locator(VIEW).waitFor({ state: 'detached' });
	return { ...t, find: (name) => t.buttons.find((b) => b.name === name) };
}

/** Drag from the room's middle by dx, dy, in steps, and read what is drawn. */
async function dragBy(page, dx, dy) {
	const from = await page.locator(ROOM).boundingBox();
	const [x, y] = [from.x + from.width / 2, from.y + from.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	for (let i = 1; i <= 5; i++) await page.mouse.move(x + (dx * i) / 5, y + (dy * i) / 5);
	await page.mouse.up();
	return seen(page);
}

/** Each state's center from the room's, to the pixel. */
const offsets = (t) => Object.fromEntries(t.buttons.map((b) => [b.name, [Math.round(b.x - t.pane.x), Math.round(b.y - t.pane.y)]]));

/** The pills on the input bar, by id. */
const pills = (page) => page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => p.dataset.pill));

const options = (width = 390) => ({ viewport: { width, height: 640 }, clock: todayAt(15), prepare: (context) => context.addInitScript(draft, PAGE) });

// da-r48y9: a field opened in collect stands its states in the room between
// its time and its keys, in the rows the fields stand in at rest — vividn,
// 2026-11-07: «then top to bottom, left to right list the states in order
// defined by the spec».
describe('a switch field’s states in collect, in the rows', () => {
	before(startApp);
	after(stopApp);

	for (const width of [390, 1280])
		test(`${width}px: the same picture whichever field is pressed and wherever it stands, the spec's states down the rows under the first`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				const fields = await page.locator(BAR).evaluate((bar) => [...bar.querySelectorAll('[data-field]')].map((b) => b.dataset.field));
				const picked = [fields[0], fields[Math.floor(fields.length / 2)], fields.at(-1)];
				const first = await open(page, picked[0]);
				assert.deepEqual(first.buttons.map((b) => b.name), STATES, 'off is the first row’s, not a state');
				// The rows hold them all in one column, a row each under the first row, at the room's left.
				assert.deepEqual(first.buttons.map((b) => b.row), [1, 2, 3, 4]);
				const a = first.find('a');
				assert.ok(first.buttons.every((b) => Math.abs(b.left - a.left) < 0.5 && b.left - first.pane.left < 20), JSON.stringify(first.buttons));
				assert.ok(first.buttons.every((b, i) => i === 0 || b.top > first.buttons[i - 1].bottom), 'not top to bottom');
				for (const field of picked.slice(1)) assert.deepEqual(offsets(await open(page, field)), offsets(first), `${field}: not ${picked[0]}'s picture`);
				// A key's press opens the same.
				await page.locator(`${BAR} [data-field="mood"]`).focus();
				await page.keyboard.press('Enter');
				assert.deepEqual(offsets(await placed(page)), offsets(first), 'by key: not the same picture');
			}, options(width));
		});

	test('22 states at 390px: a drag pans them and presses nothing, and a tap on a state logs it', async () => {
		// Long names, so they run past a phone's width as well as the room's height.
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await field.click();
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).waitFor();
			const was = await seen(page);
			assert.ok(was.ring.right > was.pane.right, 'the states fit across: the case is not tested');
			const before = (await app.echoes()).length;
			// Started ON the middle state, well past the slop: to the left.
			const moved = await dragBy(page, -30, 0);
			const got = moved.ring.left - was.ring.left;
			assert.ok(Math.abs(got + Math.min(30, was.ring.right - was.pane.right)) <= 1, `the states moved ${got}`);
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'a drag pressed the state it started on');
			assert.deepEqual(await pills(page), ['field'], 'a drag chose a state');

			// Past their extent either way, their far edge stops a gap inside the
			// room's, as the fields' do at rest, and the near one back where it
			// began, like a scroll; down they fit, so a drag there moves nothing.
			const strokes = async (dx, dy) => {
				const n = Math.max(1, Math.ceil(Math.max(Math.abs(dx), Math.abs(dy)) / 150));
				let t;
				for (let i = 0; i < n; i++) t = await dragBy(page, dx / n, dy / n);
				return t;
			};
			const span = was.ring.right - was.ring.left;
			const left = await strokes(-span, 0);
			assert.ok(Math.abs(left.ring.right + 6 - left.pane.right) <= 1, `panned past the states: their right ${left.ring.right}, the room's ${left.pane.right}`);
			const right = await strokes(2 * span, 0);
			assert.ok(Math.abs(right.ring.left - was.ring.left) <= 1, `panned past the states the other way: their left ${right.ring.left}, where they began ${was.ring.left}`);
			assert.ok(right.ring.bottom - right.ring.top <= right.pane.bottom - right.pane.top, 'the states do not fit down: the case below is not tested');
			const down = await strokes(0, 80);
			assert.equal(down.ring.top, right.ring.top, 'states that fit down were panned down');
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'a pan pressed something');

			// A press that does not travel is a press: a state logs at once
			// (vividn, 2026-11-03 16:07), and collect is back at rest.
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).click();
			await page.locator(VIEW).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).length, before + 1, 'the state did not run');
			assert.equal(JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows[0].state, many[0]);
		}, options());
	});

	// da-71p3u: the ring overruns its room, which clips it; a reveal of a
	// state it clips — a focus, a click's scroll into view — is a pan, so
	// nothing around the widget is left scrolled, and the room by its lead
	// alone, how far the ring overruns its left and top (da-u846n).
	test('22 states at 390px: focusing a state the room clips pans it toward sight, and the room does not scroll', async () => {
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await field.click();
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).waitFor();
			const outside = (b, pane) =>
				Math.max(0, pane.left - b.left) + Math.max(0, b.right - pane.right) + Math.max(0, pane.top - b.top) + Math.max(0, b.bottom - pane.bottom);
			const t = await seen(page);
			// Wholly past the room's right edge: a focus reveals only what is
			// out of sight, and a reveal scrolls a box toward its end — nothing
			// stands before its start for one to scroll back to.
			const clipped = t.buttons.find((b) => b.left >= t.pane.right && b.top >= t.pane.top && b.bottom <= t.pane.bottom);
			assert.ok(clipped, 'no state the room clips: the case is not tested');
			await page.evaluate(() => document.activeElement?.blur());
			await page.locator(`${ROOM} [data-state="${clipped.name}"]`).focus();
			await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
			const scrolled = await page.locator(ROOM).evaluate((el) => {
				const out = [];
				const lead = el.querySelector('.lead');
				for (let e = el; e; e = e.parentElement) {
					const [x, y] = e === el ? [lead.offsetLeft, lead.offsetTop] : [0, 0];
					if (e.scrollLeft !== x || e.scrollTop !== y) out.push(`${e.className}: ${e.scrollLeft},${e.scrollTop}`);
				}
				return out;
			});
			assert.deepEqual(scrolled, [], 'focusing scrolled');
			const now = await seen(page);
			const after = outside(now.buttons.find((b) => b.name === clipped.name), now.pane);
			assert.ok(after < outside(clipped, t.pane) - 1, `${clipped.name} was not panned toward sight`);
		}, options());
	});

	test('Escape and back put the fields back as they were, logging nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			const before = (await app.echoes()).length;
			for (const how of ['Escape', 'back']) {
				await field.dispatchEvent('click');
				await page.locator(`${ROOM} [data-state="a"]`).waitFor();
				assert.equal(await field.count(), 0, `${how}: the fields are still drawn under the states`);
				if (how === 'Escape') await page.keyboard.press('Escape');
				else await page.locator(`${VIEW} [data-back]`).click();
				await page.locator(VIEW).waitFor({ state: 'detached' });
				await app.idle();
				assert.equal((await app.echoes()).length, before, `${how} logged something`);
				assert.ok(await field.isVisible(), `${how}: the fields are not back`);
				assert.deepEqual(await pills(page), [], `${how}: a pill stayed`);
			}
		}, options());
	});

	test('reordered by the setup widget alone, and a state learned while they are open placed where a fresh open puts it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			// Logging does not reorder. Timed apart: the app's clock is held,
			// and two switches of one field at one minute collide.
			const first = await open(page, 'mood');
			for (const [s, t] of [['d', '1400'], ['d', '1410'], ['c', '1420'], ['d', '1430']]) await app.submit(`switch mood ${s} -t ${t}`);
			await app.idle();
			const logged = await open(page, 'mood');
			const rows = (t) => Object.fromEntries(t.buttons.map((b) => [b.name, b.row]));
			assert.deepEqual(rows(logged), rows(first), 'logging moved a state');

			const { saved } = await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} [aria-label="move d: drag, or the arrow keys"]`).focus();
			for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowUp');
			await page.locator(`${FIELD_FORM} .commit`).click();
			await saved;
			assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).allowed_states, ['false', 'd', 'a', 'b', 'c']);
			await leaveField(page);
			await field.waitFor();
			await app.idle();
			await rested(page);
			// d took the row a had, and so on round: rows, not pixels, as mood
			// is in d and its outline and `!` widen it.
			const [now, was] = [rows(await open(page, 'mood')), rows(logged)];
			assert.deepEqual(['d', 'a', 'b', 'c'].map((s) => now[s]), STATES.map((s) => was[s]));

			// Read once still: every run answered, and two reads a frame apart agreeing. A field opened again draws its kept
			// listing in its first frame (da-2ylmj), and the room's observer may place the states again after it.
			const offset = async () => {
				await app.idle();
				return page.locator(ROOM).evaluate(async (room) => {
					const read = () => {
						const at = (sel) => room.querySelector(sel);
						const [e, d] = [at('[data-state="e"]'), at('[data-state="d"]')];
						const [eb, db] = [e.getBoundingClientRect(), d.getBoundingClientRect()];
						return [e.closest('.spot').dataset.row, Math.round(eb.x - db.x), Math.round(eb.y - db.y)];
					};
					for (let was = read(), f = 0; ; f++) {
						await new Promise((r) => requestAnimationFrame(r));
						const now = read();
						if (now.every((v, i) => v === was[i]) || f > 600) return now;
						was = now;
					}
				});
			};
			await field.dispatchEvent('click');
			await page.locator(`${ROOM} [data-state="d"]`).waitFor();
			await app.submit('setup mood --states false,d,a,b,c,e');
			await page.locator(`${ROOM} [data-state="e"]`).waitFor();
			const live = await offset();
			await page.keyboard.press('Escape');
			await page.locator(VIEW).waitFor({ state: 'detached' });
			await field.dispatchEvent('click');
			await page.locator(`${ROOM} [data-state="e"]`).waitFor();
			assert.deepEqual(live, await offset(), 'the learned state was not placed');
		}, options());
	});
});
