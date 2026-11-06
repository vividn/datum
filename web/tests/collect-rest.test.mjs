// Collect at rest (da-lw7ue). vividn, 2026-11-04: «on the far left (maybe
// 20%) of the collect widget, let's draw the current state (so like the now
// section of the nowview), then have this active section with a dotted line.
// Freeze this section. So then you pan to the right to see more and more
// distantly used fields, but can always see the active ones», «no vertical
// panning. Everything should be locked to it's proper y positioning», and of
// a field ending: «Since it is the most recent it should always get the first
// slot». At 16:05: «scrolling should have some momentum», and 16:11: «clicking
// on a piece of data in the now section of the collect widget should select
// its field, not the entry».
//
// The arrangement is pure (`atRest` in fields-rest-pure.test.mjs); this is
// the built app, read off where each field stands.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE, pickOption, swipe } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const ROOM = 'section.pane[data-title="collect"] .bar .collect .room';

/** A phone on collect, its main tab, tall enough for rows. */
const AT_REST = { ...PHONE, home: ON_COLLECT, clock: todayAt(15) };

/** A field with its band set, the way a spec sets it. */
const at = (field, verb, y, more = {}) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y, height: 0.05, ...more })}'`;

/**
 * Run `lines`, a minute apart on the held clock, so each is used after the
 * one before it; then load, which sorts by recency.
 */
async function seeded(app, lines) {
	for (const line of lines) {
		assert.equal((await app.runLine(line)).code, 0, line);
		await app.clock.forward(60_000);
	}
	await app.reload();
	await app.page.locator(`${ROOM} .spot.shelf`).first().waitFor();
	await app.idle();
}

/** Log `line` as another tab would, and let the page hear it. */
async function logs(app, line) {
	assert.equal((await app.runLine(line)).code, 0, line);
	await app.redraw();
}

/**
 * What the now strip draws of `field`: each mark's kind, fill, and where it
 * stands inside the strip, cut to it (`left`, `right`, `mid`, from the strip's
 * left), and the strip's width (`of`). A mark wholly outside it is left out;
 * one that only touches its edge, as a tap's block does at once, is not.
 * `strip` is the one at rest unless named.
 */
const drawn = (page, field, strip = `${ROOM} svg.now-strip`) =>
	page.locator(strip).evaluate((svg, field) => {
		const box = svg.getBoundingClientRect();
		return [...svg.querySelectorAll(`[data-mark="${field}"]`)]
			.map((el) => {
				const r = el.getBoundingClientRect();
				const [left, right] = [Math.max(r.left, box.left) - box.left, Math.min(r.right, box.right) - box.left];
				return { kind: el.localName, pending: el.classList.contains('pending'), fill: getComputedStyle(el).fill, left, right, mid: (r.left + r.right) / 2 - box.left, of: box.width };
			})
			.filter((m) => m.right >= m.left);
	}, field);

/** Every field at rest: its row, its section, and where it is drawn. */
const shelf = (page) =>
	page.locator(`${ROOM} .spot.shelf`).evaluateAll((spots) =>
		spots.map((s) => {
			const r = s.getBoundingClientRect();
			return { field: s.querySelector('[data-field]').dataset.field, row: Number(s.dataset.row), section: s.dataset.section, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		})
	);

/** Each row's fields of `section`, left to right. */
const rowsOf = (spots, section) => {
	const rows = new Map();
	for (const s of [...spots].filter((s) => s.section === section).sort((a, b) => a.left - b.left))
		rows.set(s.row, [...(rows.get(s.row) ?? []), s.field]);
	return Object.fromEntries(rows);
};

/** Wait until the fields stand still: no glide in flight. */
const settled = async (page) => {
	let last = null;
	await until(
		async () => JSON.stringify(await shelf(page)),
		(now) => {
			const same = now === last;
			last = now;
			return same;
		}
	);
	return JSON.parse(last);
};

// High: `mood` (a switch) and `tea`, `coffee`, used in that order; low:
// `work` (a block) and `walk`, `read`. So coffee and read are the most recent
// of their rows, and mood and work, once on, are active.
const SEED = [
	at('mood', 'switch', 0.05, { allowed_states: ['good', 'bad'] }),
	at('tea', 'occur', 0.05),
	at('coffee', 'occur', 0.05),
	at('work', 'start', 0.9),
	at('walk', 'occur', 0.9),
	at('read', 'occur', 0.9),
	'occur tea',
	'occur coffee',
	'occur walk',
	'occur read'
];

describe('collect at rest', () => {
	test('nothing active: no outline, no strip, each row by recency and locked to y', async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			const spots = await settled(page);
			assert.equal(await page.locator(`${ROOM} .dotted`).count(), 0, 'an outline with nothing active');
			assert.equal(await page.locator(`${ROOM} [data-now-field]`).count(), 0);
			const rows = rowsOf(spots, 'recent');
			const keys = Object.keys(rows).map(Number).sort((a, b) => a - b);
			assert.equal(keys.length, 2, `two rows, by y: ${JSON.stringify(rows)}`);
			// Most recent first; never used last, then by name.
			assert.deepEqual(rows[keys[0]], ['coffee', 'tea', 'mood']);
			assert.deepEqual(rows[keys[1]], ['read', 'walk', 'work']);
			// The rows start past the strip, a fifth of the room.
			const room = await page.locator(ROOM).boundingBox();
			assert.ok(Math.min(...spots.map((s) => s.left)) >= room.x + room.width / 5, 'a field over the strip');
		}, AT_REST);
	});

	test('active fields stand in the outlined section past the strip, ahead of every other, and their states are on the strip', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'switch mood good', 'start work']);
			const { page } = app;
			const spots = await settled(page);
			assert.deepEqual(rowsOf(spots, 'active'), { [spots.find((s) => s.field === 'mood').row]: ['mood'], [spots.find((s) => s.field === 'work').row]: ['work'] });
			for (const s of spots.filter((s) => s.section === 'recent')) {
				const section = Math.max(...spots.filter((a) => a.section === 'active' && a.row === s.row).map((a) => a.right));
				assert.ok(s.left > section, `${s.field} stands in the active section`);
			}
			await page.locator(`${ROOM} .dotted path`).waitFor();
			const strip = await page.locator(`${ROOM} [data-now-field]`).evaluateAll((bs) => bs.map((b) => b.dataset.nowField));
			assert.deepEqual(strip.sort(), ['mood', 'work']);
			// Each drawn across the whole strip, in the colour its button is filled with.
			for (const field of strip) {
				const fill = await page.locator(`${ROOM} [data-field="${field}"]`).evaluate((b) => getComputedStyle(b).backgroundColor);
				await until(() => drawn(page, field), (marks) => marks.some((m) => m.fill === fill && m.left <= 0.5 && m.right >= m.of - 0.5));
			}
		}, AT_REST);
	});

	test('a field that ends takes the first slot of its row; one that starts goes into the section', async () => {
		await withApp(async (app) => {
			// work started before walk and read were used: by the last sort it is the oldest of its row.
			await seeded(app, [...SEED.slice(0, 6), 'start work', ...SEED.slice(6)]);
			const { page } = app;
			assert.equal((await settled(page)).find((s) => s.field === 'work').section, 'active');
			await logs(app, 'end work');
			await until(async () => (await shelf(page)).find((s) => s.field === 'work').section, (s) => s === 'recent');
			let spots = await settled(page);
			const row = spots.find((s) => s.field === 'work').row;
			assert.deepEqual(rowsOf(spots, 'recent')[row], ['work', 'read', 'walk']);
			assert.equal(await page.locator(`${ROOM} .dotted`).count(), 0);

			await logs(app, 'switch mood bad');
			await until(async () => (await shelf(page)).find((s) => s.field === 'mood').section, (s) => s === 'active');
			spots = await settled(page);
			const mood = spots.find((s) => s.field === 'mood');
			assert.ok(mood.right < Math.min(...spots.filter((s) => s.section === 'recent' && s.row === mood.row).map((s) => s.left)));
		}, AT_REST);
	});

	// vividn, 2026-11-04: «nestle the inactive states right up next to the
	// active zone, right now there is a big gap if one row has a lot of
	// active states in it» (da-il6ni).
	for (const side of ['left', 'right'])
		test(`side: ${side}: each row's other fields stand right against its own active fields, not the widest row's; none is under the backing at rest, and panned, a row goes under its own part of the section and no further`, async () => {
			await withApp(async (app) => {
				// More in work's row than the room holds, so the rows pan.
				const more = Array.from({ length: 8 }, (_, i) => at(`low_${i}`, 'occur', 0.9));
				await seeded(app, [at('x', 'start', 0.05), at('z', 'start', 0.05), ...more, ...SEED, 'start x', 'start z', 'start work']);
				const { page } = app;
				if (side === 'right') await pickOption(page, 'bar', 'side', 'right');
				const near = (s) => (side === 'left' ? s.left : -s.right);
				const far = (s) => (side === 'left' ? s.right : -s.left);
				const room = await page.locator(ROOM).boundingBox();
				// On its side: past the time and duration section beside the strip (da-us1n1).
				const by = await page.locator('section.pane[data-title="collect"] .bar [data-when]').boundingBox();
				const spots = await until(
					() => settled(page),
					(spots) => {
						const work = spots.find((s) => s.field === 'work');
						return spots.filter((s) => s.section === 'active').length === 3 && (side === 'left' ? work.left >= by.x + by.width - 1 : work.right <= by.x + 1);
					}
				);
				const work = spots.find((s) => s.field === 'work');
				const wide = Math.max(...spots.filter((s) => s.section === 'active' && s.row !== work.row).map(far));
				const next = spots.filter((s) => s.section === 'recent' && s.row === work.row).sort((a, b) => near(a) - near(b))[0];
				assert.ok(far(work) < wide, `work's row is the narrower: ${far(work)} against ${wide}`);
				// Narrow enough to stay while the rows pan under it, so the backing is the section's and not the strip's.
				// The time and duration section beside the strip (da-us1n1) is not counted against the half.
				const when = (await page.locator('section.pane[data-title="collect"] .bar [data-when]').boundingBox()).width;
				assert.ok(wide - far(side === 'left' ? { right: room.x } : { left: room.x + room.width }) - when <= room.width / 2, `the section is not frozen: ${wide}`);
				assert.ok(near(next) - far(work) <= 2 * 6 + 1, `${next.field} ${near(next) - far(work)}px past work, the widest row ending at ${wide}`);
				// Drawn, not under the backing the panned rows go under: each in the room, read among them (walk
				// past the room's edge since the time and duration section, da-us1n1, stands beside the strip).
				const shown = spots.filter((s) => s.section === 'recent' && s.left >= room.x && s.right <= room.x + room.width);
				assert.ok(shown.some((s) => s.field === 'read'), JSON.stringify(shown));
				for (const s of shown) {
					// Its edge toward the section, where a backing too wide would lie over it.
					const hit = await page.locator(`${ROOM} [data-field="${s.field}"]`).evaluate((b, side) => {
						const r = b.getBoundingClientRect();
						return b.contains(document.elementFromPoint(side === 'left' ? r.left + 2 : r.right - 2, (r.top + r.bottom) / 2));
					}, side);
					assert.ok(hit, `${s.field} is covered`);
				}

				// Panned toward the section, a slow swipe the finger's length: work's row goes under
				// its own part of the section, and shows past it where the widest row's part is wider.
				// The strip's edge and past it the time and duration section's (da-us1n1), which no panned row goes under.
				const strip = await page.locator(`section.pane[data-title="collect"] .bar [data-when]`).boundingBox();
				const [edge, wideEdge, stripEdge] = [far(work) + 6, wide + 6, far(side === 'left' ? { right: strip.x + strip.width } : { left: strip.x })];
				const y = (work.top + work.bottom) / 2;
				await swipe(page, room.x + room.width * (side === 'left' ? 0.85 : 0.15), y, side === 'left' ? -80 : 80, 0, 8, 60, 200);
				const moved = await until(
					() => settled(page),
					(now) => now.find((s) => s.field === 'read').left !== next.left || now.find((s) => s.field === 'read').right !== next.right
				);
				// Points along the row's panned fields, away from work's own button and from the edge itself.
				const points = moved
					.filter((s) => s.section === 'recent' && s.row === work.row)
					.flatMap((s) => Array.from({ length: Math.floor((far(s) - near(s)) / 2) }, (_, i) => near(s) + 1 + 2 * i))
					.filter((u) => u > stripEdge + 1 && (u < near(work) - 1 || u > far(work) + 1) && Math.abs(u - edge) > 1);
				const hits = await page.evaluate(
					({ xs, y }) => xs.map((x) => {
						const el = document.elementFromPoint(x, y);
						return el?.closest('svg.backing') ? 'backing' : (el?.closest('[data-field]')?.dataset.field ?? String(el?.className));
					}),
					{ xs: points.map((u) => (side === 'left' ? u : -u)), y }
				);
				const wrong = points.flatMap((u, i) => ((u < edge) === (hits[i] === 'backing') ? [] : [`${hits[i]} at ${u}`]));
				assert.deepEqual(wrong, [], `work's part of the section ends at ${edge}`);
				assert.ok(points.some((u) => u < edge), 'nothing panned under the section');
				assert.ok(points.some((u) => u > edge && u < wideEdge - 1), `nothing panned between ${edge} and the widest row's ${wideEdge}`);
			}, AT_REST);
		});

	// vividn, 16:41: «a "side: left | right" row in the widget menu».
	test('side: right is the mirror: the strip and the section at the right, each row last used first leftward', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'start work']);
			const { page } = app;
			await pickOption(page, 'bar', 'side', 'right');
			const room = await page.locator(ROOM).boundingBox();
			const strip = await page.locator(`${ROOM} [data-now-field="work"]`).boundingBox();
			assert.ok(strip.x >= room.x + room.width * 0.75, `the strip at ${strip.x} of ${room.x}–${room.x + room.width}`);
			await until(async () => {
				const spots = await shelf(page);
				const work = spots.find((s) => s.field === 'work');
				const row = spots.filter((s) => s.row === work.row && s.section === 'recent').sort((a, b) => b.right - a.right);
				return work.right <= strip.x && row.map((s) => s.field).join() === 'read,walk' && row[0].right <= work.left;
			}, Boolean);
			// The dotted outline mirrored with it: around work, at the strip's edge.
			const [line, work] = await Promise.all([page.locator(`${ROOM} .dotted path`).boundingBox(), page.locator(`${ROOM} [data-field="work"]`).boundingBox()]);
			assert.ok(line.x <= work.x && line.x + line.width >= work.x + work.width && line.x + line.width <= strip.x + 1, `the outline at ${line.x}–${line.x + line.width}, work at ${work.x}, the strip at ${strip.x}`);
		}, AT_REST);
	});

	test('a tap on the strip puts its field on the bar, not an entry', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'start work']);
			const { page } = app;
			await page.locator(`${ROOM} [data-now-field="work"]`).click();
			await until(
				() => page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => [[...p.classList].filter((c) => c !== 'pill').join(' '), p.innerText.replace(/[×\s]+$/, '').trim()])),
				(pills) => pills.length === 1 && pills[0][1] === 'work' && /field/.test(pills[0][0])
			);
		}, AT_REST);
	});
});

// da-uuzu6, vividn, 2026-11-04: «I want this now section to show the last
// second essentially so immediately after you tap and state change, the new
// change sweeps in from the right for a second. Likewise occur dots start on
// the right hand side and sweep over over a second», and 2026-11-05 (da-qtaxt):
// «maybe half a second instead of a full second».
describe('the now strip is the last half second', () => {
	/** `field`'s button's fill, as the strip paints it. */
	const fillOf = (page, field) => page.locator(`${ROOM} [data-field="${field}"]`).evaluate((b) => getComputedStyle(b).backgroundColor);
	/** `field`'s colour as `fields --json` resolves it, as a computed `fill` reads. */
	const colorOf = async (app, field) => {
		const hex = JSON.parse((await app.runLine('fields --json')).stdout).find((f) => f.field === field).color;
		return `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
	};
	/** How much of the strip `field` holds in `fill`, from its right edge, once it is `want` px within `slack`. */
	const sweep = async (page, field, fill, want, slack = 3) =>
		until(
			async () => (await drawn(page, field)).filter((m) => m.kind === 'rect' && m.fill === fill),
			(marks) => marks.some((m) => Math.abs(m.right - m.of) <= 0.5 && Math.abs(m.right - m.left - want) <= slack)
		);

	test('a tap that starts a block sweeps its colour in from the right edge over half a second, then the row is solid', async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			await page.locator(`${ROOM} [data-field="work"]`).click();
			const fill = await colorOf(app, 'work');
			const of = (await page.locator(`${ROOM} svg.now-strip`).boundingBox()).width;
			// At once: at the right edge, and no wider than the moments since the tap.
			await sweep(page, 'work', fill, 0);
			await app.clock.forward(250);
			await sweep(page, 'work', fill, of / 2);
			await app.clock.forward(250);
			await sweep(page, 'work', fill, of, 0.5);
			await app.clock.forward(5000);
			await sweep(page, 'work', fill, of, 0.5);
		}, AT_REST);
	});

	test("a moment's dot enters at the right edge and crosses the strip in half a second", async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			await page.locator(`${ROOM} [data-field="coffee"]`).click();
			const dot = () => drawn(page, 'coffee').then((marks) => marks.find((m) => m.kind === 'circle'));
			await until(dot, (d) => d && Math.abs(d.mid - d.of) <= 3);
			await app.clock.forward(250);
			await until(dot, (d) => d && Math.abs(d.mid - d.of / 2) <= 3);
			await app.clock.forward(300);
			await until(dot, (d) => d === undefined);
		}, AT_REST);
	});

	/** `field`'s dots on the strip, the tap's and the layer's, each with where it stands from the strip's left and how tall it is. */
	const dotsOf = (page, field) =>
		page.locator(`${ROOM} svg.now-strip`).evaluate((svg, field) => {
			const box = svg.getBoundingClientRect();
			const at = (el) => {
				const r = el.getBoundingClientRect();
				return { left: r.left - box.left, right: r.right - box.left, mid: (r.left + r.right) / 2 - box.left, h: r.height, of: box.width, tall: box.height };
			};
			return {
				taps: [...svg.querySelectorAll(`circle.pending[data-mark="${field}"]`)].map(at),
				layer: [...svg.querySelectorAll(`g[data-mark="${field}"] circle.strip-point-fg`)].map(at)
			};
		}, field);

	// vividn, 2026-11-05 (da-qtaxt): «moment dots that have a different size configured start out the wrong size and then change size a half second later».
	test("a tap's dot is drawn at its field's size from the first frame, and keeps it when the layer's takes over", async () => {
		await withApp(async (app) => {
			await seeded(app, [at('big', 'occur', 0.5, { symbol_scale: 2 }), ...SEED]);
			const { page } = app;
			await holdFiles(page);
			await page.locator(`${ROOM} [data-field="big"]`).click();
			const atTap = await dotsOf(page, 'big');
			await release(page);
			assert.equal(atTap.layer.length, 0, 'the layer had the tap before the database answered');
			assert.equal(atTap.taps.length, 1, 'no dot at the tap');
			// The CLI's standard dot, 0.08 of the day, twice over.
			const [tap] = atTap.taps;
			assert.ok(Math.abs(tap.h - 0.16 * tap.tall) < 0.5, `the tap's dot is ${tap.h}px of ${tap.tall}px`);
			const landed = await until(
				() => dotsOf(page, 'big'),
				(d) => d.taps.length === 0 && d.layer.length === 1
			);
			assert.ok(Math.abs(landed.layer[0].h - tap.h) < 0.5, `the dot went from ${tap.h}px to ${landed.layer[0].h}px`);
		}, AT_REST);
	});

	// vividn, 2026-11-05 (da-qtaxt): «they should go completely off the left side and then be destroyed. Right now with a big occur dot … it just hangs there indefinitely».
	// The page's timers held (`holdTimers`), so a forward fires the frame that came due and nothing else:
	// each place is read at once, never waited for, so a strip that stopped sliding is read standing (vremya on #1723).
	test('a big dot slides wholly past the left edge, and is then gone', async () => {
		await withApp(
			async (app) => {
				await seeded(app, [at('huge', 'occur', 0.5, { symbol_scale: 6 }), ...SEED]);
				const { page } = app;
				await page.locator(`${ROOM} [data-field="huge"]`).click();
				const [dot] = (await until(() => dotsOf(page, 'huge'), (d) => d.taps.length === 0 && d.layer.length === 1)).layer;
				assert.ok(Math.abs(dot.mid - dot.of) <= 3, `the dot lands at ${dot.mid}px, not the right edge`);
				const width = dot.right - dot.left;
				// Its half width in time: 500 ms is the strip's width.
				const out = Math.round((width / 2) * (500 / dot.of));
				assert.ok(out > 100, `the dot is too small to show it: ${width}px of ${dot.of}px`);
				// Its centre at the left edge, its right half still in the strip.
				await app.clock.forward(500);
				const [left] = (await dotsOf(page, 'huge')).layer;
				assert.ok(left && Math.abs(left.mid) <= 3, `at its centre's exit it stands at ${left?.mid}px`);
				assert.ok(left.right > 1, `the dot is out of view already: right at ${left.right}px`);
				await app.clock.forward(out / 2);
				const [half] = (await dotsOf(page, 'huge')).layer;
				assert.ok(half && Math.abs(half.mid + width / 4) <= 3, `halfway out it stands at ${half?.mid}px, not ${-width / 4}px`);
				await app.clock.forward(out / 2 + 50);
				assert.deepEqual((await dotsOf(page, 'huge')).layer, [], 'the dot is still there once wholly past the left edge');
				assert.equal(await page.locator(`${ROOM} svg.now-strip [data-mark="huge"] :is(circle, text, use, line)`).count(), 0, 'a mark of the dot is left');
			},
			{ ...AT_REST, holdTimers: true }
		);
	});

	/** Hold the database's files, so a command waits until `release`. */
	const holdFiles = (page) =>
		page.evaluate(() => new Promise((granted) => navigator.locks.request('datum-cli-files', () => (granted(), new Promise((release) => (window.release = release))))));
	const release = (page) => page.evaluate(() => window.release());
	/** The strip's tap marks and the layer's dots of `field`. */
	const dots = (page, field) =>
		page.locator(`${ROOM} svg.now-strip`).evaluate((svg, field) => ({
			taps: svg.querySelectorAll(`.pending[data-mark="${field}"]`).length,
			layer: svg.querySelectorAll(`.strip-point-bg[data-mark="${field}"]`).length
		}), field);

	// vremya on #1694: drawn exactly over the layer's own, a mark never dropped shows nowhere.
	test("a written tap's mark goes once the layer holding the write is drawn: one dot, not two", async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			// The seed's coffee is laid too, out of the second's view: the tap's is one more.
			const before = await dots(page, 'coffee');
			await holdFiles(page);
			await page.locator(`${ROOM} [data-field="coffee"]`).click();
			const atTap = await dots(page, 'coffee');
			await release(page);
			assert.deepEqual(atTap, { taps: 1, layer: before.layer }, 'the tap drew its dot before the database answered');
			await until(() => dots(page, 'coffee'), (d) => d.layer === before.layer + 1 && d.taps === 0);
		}, AT_REST);
	});

	// vividn, 2026-11-04 22:40: «it should literally just draw the same now as nowview does … right now hidden fields are shown and multi state is not rendered».
	test('the strip draws as the nowview does: a field in two states in its stripes, a field hidden from the pictures not at all', async () => {
		await withApp(async (app) => {
			await seeded(app, [at('secret', 'start', 0.5, { render: false }), at('cat', 'occur', 0.5, { symbol: '★' }), ...SEED, 'switch mood good,bad', 'start secret']);
			const { page } = app;
			await page.locator(`${ROOM} [data-now-field="secret"]`).waitFor({ state: 'attached' });
			const strip = () =>
				page.locator(`${ROOM} svg.now-strip`).evaluate((svg) => ({
					secret: svg.querySelectorAll('[data-mark="secret"]').length,
					mood: [...svg.querySelectorAll('rect.strip-block[data-mark="mood"]')].map((b) => {
						const id = /^url\(#(.+)\)$/.exec(b.getAttribute('fill') ?? '')?.[1];
						return !!id && svg.querySelector(`defs > pattern[id="${id}"]`) !== null;
					})
				}));
			await until(strip, (s) => s.mood.length > 0 && s.mood.every(Boolean));
			assert.equal((await strip()).secret, 0, 'a field hidden from the pictures is on the strip');
			// A moment's symbol is its mark, as the nowview draws it, and it enters at the right edge.
			await page.locator(`${ROOM} [data-field="cat"]`).click();
			await until(
				() =>
					page.locator(`${ROOM} svg.now-strip`).evaluate((svg) => {
						const box = svg.getBoundingClientRect();
						return [...svg.querySelectorAll('g[data-mark="cat"] text')].map((t) => [t.textContent, Math.abs(t.getBoundingClientRect().left + t.getBoundingClientRect().width / 2 - box.right) <= 3]);
					}),
				(marks) => marks.some(([text, atEdge]) => text === '★' && atEdge)
			);
		}, AT_REST);
	});

	// vividn, 2026-11-05 (da-u7tij): «in the secondary view of the collect widget, still show the now section so you can see the animation right after changing states».
	for (const side of ['left', 'right'])
		test(`side: ${side}: a field's view keeps the now strip at the widget's edge beside it, and a state tapped there sweeps in at now's edge while the view is up`, async () => {
			await withApp(async (app) => {
				await seeded(app, [...SEED, 'switch mood good']);
				const { page } = app;
				if (side === 'right') await pickOption(page, 'bar', 'side', 'right');
				const BAR = 'section.pane[data-title="collect"] .bar';
				await page.locator(`${ROOM} [data-field="mood"]`).click();
				const bad = page.locator(`${BAR} .collect.open [data-state="bad"]`);
				await bad.waitFor();
				const strip = await page.locator(`${BAR} > [data-strip]`).boundingBox();
				const view = await page.locator(`${BAR} .collect.open`).boundingBox();
				const widget = await page.locator(`${BAR}`).boundingBox();
				assert.ok(strip && strip.width > 0, 'no now strip in the view');
				const edge = side === 'left' ? strip.x - widget.x : widget.x + widget.width - (strip.x + strip.width);
				assert.ok(Math.abs(edge) <= 1 && Math.abs(strip.height - widget.height) <= 1, `the strip is ${edge}px off the widget's ${side} edge, or short of its height`);
				const clear = side === 'left' ? view.x - (strip.x + strip.width) : strip.x - (view.x + view.width);
				assert.ok(clear >= -0.5, `the view overlaps the strip by ${-clear}px`);
				// Now is the strip's inner edge: its right on the left, its left mirrored.
				const atNow = (m) => (side === 'left' ? Math.abs(m.right - m.of) : Math.abs(m.left)) <= 0.5;
				const VIEW_STRIP = `${BAR} > [data-strip] svg.now-strip`;
				const swept = (want, slack = 3) =>
					until(
						async () => (await drawn(page, 'mood', VIEW_STRIP)).filter((m) => m.kind === 'rect' && m.pending),
						(marks) => marks.some((m) => atNow(m) && Math.abs(m.right - m.left - want) <= slack)
					);
				// The files held, the change waits and the view stays: its sweep is drawn on the view's strip at the tap.
				await holdFiles(page);
				await bad.click();
				await swept(0);
				await app.clock.forward(250);
				await swept(strip.width / 2);
				assert.equal(await bad.count(), 1, 'the view closed before the change was answered');
				await release(page);
				await until(async () => (await app.runLine('get mood::state')).stdout.trim(), (s) => s === 'bad');
			}, AT_REST);
		});

	test('a refused press takes its sweep back: the row is as it was', async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			// work on since three seconds ago, and a moment of it now, which an end now collides with:
			// refused, as work's spec says (da-wudrc).
			assert.equal((await app.runLine('setup work --conflict refuse')).code, 0);
			const now = app.clock.at();
			await app.clock.set(now - 3000);
			await logs(app, 'start work');
			await app.clock.set(now);
			await logs(app, 'moment work');
			const fill = await fillOf(page, 'work');
			const whole = (marks) => marks.some((m) => m.fill === fill && m.left <= 0.5 && m.right >= m.of - 0.5);
			await until(() => drawn(page, 'work'), whole);
			// The database's files held, so the end waits: its sweep is drawn at the tap, before any answer.
			await holdFiles(page);
			await page.locator(`${ROOM} [data-field="work"]`).click();
			// An end's sweep is its lane clipped to before the tap (NowStrip.svelte, da-kqloo).
			const wiped = () => page.locator(`${ROOM} svg.now-strip`).evaluate((svg) => svg.querySelector('g[data-mark="work"] > g[clip-path]') !== null);
			const atTap = await wiped();
			await release(page);
			assert.ok(atTap, 'no sweep at the tap');
			await until(async () => !(await wiped()) && whole(await drawn(page, 'work')), Boolean);
			assert.equal((await app.runLine('get work::state')).stdout.trim(), 'true', 'the end was refused');
		}, AT_REST);
	});

	// vividn, 2026-11-05 22:36 (da-efw3a): «the wipe animation for a changing state will draw on top of other fields that are normally on top of it, please keep z ordering during this animation».
	test("a tap's sweep is drawn at its field's place in the stacking: under the field the layer draws over it", async () => {
		await withApp(async (app) => {
			// `under`'s last entry is out of the strip's reach, so its lane holds nothing the strip draws.
			await seeded(app, [
				at('over', 'start', 0.4, { height: 0.2, z_index: 2 }),
				at('under', 'start', 0.45, { height: 0.1, z_index: 1 }),
				'start under -t -30',
				'stop under -t -25',
				'start over'
			]);
			const { page } = app;
			const fill = await colorOf(app, 'over');
			await until(() => drawn(page, 'over'), (marks) => marks.some((m) => m.fill === fill && m.left <= 0.5 && m.right >= m.of - 0.5));
			const stacking = () =>
				page.locator(`${ROOM} svg.now-strip`).evaluate((svg) => {
					const tap = svg.querySelector('.pending[data-mark="under"]');
					const over = [...svg.querySelectorAll('g[data-mark="over"] rect.strip-block')];
					const [t, o] = [tap?.getBoundingClientRect(), over.map((b) => b.getBoundingClientRect())];
					return {
						tap: !!tap,
						lane: tap?.parentElement?.getAttribute('data-mark') ?? null,
						overlaps: !!t && o.some((r) => r.left < t.right && t.left < r.right && r.top < t.bottom && t.top < r.bottom),
						under: !!tap && over.some((b) => tap.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING)
					};
				});
			await holdFiles(page);
			await page.locator(`${ROOM} [data-field="under"]`).click();
			await app.clock.forward(250);
			const mid = await stacking();
			await release(page);
			assert.deepEqual(mid, { tap: true, lane: 'under', overlaps: true, under: true }, "the sweep is drawn over the field above it");
		}, AT_REST);
	});
});

describe('collect at rest on a phone', () => {
	/** Twenty fields in one row, more than the room holds, and one active. */
	const MANY = Array.from({ length: 20 }, (_, i) => `field_${String(i).padStart(2, '0')}`);
	const CROWD = [at('mood', 'switch', 0.5, { allowed_states: ['good'] }), ...MANY.map((f) => at(f, 'occur', 0.5)), 'switch mood good'];

	/** How far the rows are panned: a recent field's shift. */
	const panned = (page) =>
		page.locator(`${ROOM} .spot.shelf[data-section="recent"]`).first().evaluate((s) => -new DOMMatrix(getComputedStyle(s).transform).m41);
	/** Wait until the pan stands still, and read it. */
	const rests = async (page) => {
		let last = null;
		await until(
			() => panned(page),
			(now) => {
				const same = now === last;
				last = now;
				return same;
			}
		);
		return last;
	};

	test('a fast swipe pans on past the finger and a slow one does not; the strip and the section stay', async () => {
		await withApp(async (app) => {
			await seeded(app, CROWD);
			const { page } = app;
			assert.equal(await page.locator(ROOM).getAttribute('data-pans'), 'x', 'the rows fit: nothing to pan');
			const mood = () => page.locator(`${ROOM} [data-field="mood"]`).boundingBox();
			const before = await mood();
			const room = await page.locator(ROOM).boundingBox();
			const [x, y] = [room.x + room.width * 0.85, room.y + room.height / 2];

			// Slow, and held still before it lifts: no speed, so the pan is the finger's.
			await swipe(page, x, y, -80, 0, 8, 60, 200);
			const slow = await rests(page);
			assert.ok(Math.abs(slow - 80) <= 2, `a slow swipe of 80px panned ${slow}px`);

			// Fast: 80px in under 50ms carries on well past the finger.
			await swipe(page, x, y, -80, 0, 6, 8);
			const fast = (await rests(page)) - slow;
			assert.ok(fast > 160, `a fast swipe of 80px panned ${fast}px`);

			assert.deepEqual(await mood(), before, 'the active section moved');

			// The mirror pans the other way: rows run leftward, so a finger moving right brings older fields in.
			await pickOption(page, 'bar', 'side', 'right');
			await until(() => panned(page), (p) => p === 0);
			const [mx, my] = [room.x + room.width * 0.15, y];
			await swipe(page, mx, my, 80, 0, 8, 60, 200);
			const mirrored = await rests(page);
			assert.ok(Math.abs(mirrored + 80) <= 2, `a slow swipe right of 80px on the mirror panned ${-mirrored}px`);
		}, AT_REST);
	});
});
