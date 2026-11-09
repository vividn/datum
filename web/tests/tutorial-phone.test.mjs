// The tutorial on a phone: each lesson's start button, its game's picture and
// control, the order of its tab, and its fields' lanes in the nowview.
// One of 4 files split from tutorial.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/tutorial.mjs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, ready, startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { settled, startSwitch, lessonTab } from './support/tutorial.mjs';

before(startApp);
after(stopApp);

// The switch game starts itself, and its cue stands in Start's place (da-uzyih.4867).
test('at phone size each lesson’s start button is in sight without scrolling, under the nowview and not covered by it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const tab of ['moment', 'on-off', 'change']) {
				await lessonTab(page, tab).click();
				const name = tab === 'change' ? 'its cue' : 'Start';
				const button =
					tab === 'change' ? page.locator('[data-pane="change-lesson"] .game .cue') : page.getByRole('button', { name, exact: true });
				await button.waitFor();
				// What is drawn at each corner is the button itself: inside the viewport, and covered by no pane.
				const hits = await button.evaluate((el) => {
					const r = el.getBoundingClientRect();
					return [
						[r.left + 2, r.top + 2],
						[r.right - 2, r.top + 2],
						[r.left + 2, r.bottom - 2],
						[r.right - 2, r.bottom - 2]
					].map(([x, y]) => el.contains(document.elementFromPoint(x, y)));
				});
				assert.deepEqual(hits, [true, true, true, true], `«${name}» on ${tab} is wholly in sight`);
				// What is watched while tapping, right above what is tapped (vividn, 2026-10-22): the nowview over the lesson.
				const [now, lesson] = await Promise.all(
					['now', 'lesson'].map((id) => page.locator(`[data-pane="${tab}-${id}"]`).boundingBox())
				);
				assert.ok(now.y + now.height <= lesson.y + 1, `on ${tab} the nowview is above the lesson`);
			}
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

/** Where a picture is DRAWN, by its viewBox on screen: an svg's own box says nothing under `meet`. */
const drawn = (svg) =>
	svg.evaluate((s) => {
		const m = s.getScreenCTM();
		const { x, y, width, height } = s.viewBox.baseVal;
		return { left: m.e + x * m.a, top: m.f + y * m.d, right: m.e + (x + width) * m.a, height: height * m.d };
	});

// da-uzyih.4917: a game's picture was 20 px tall on a phone, the den's 1.9, under a row of controls; Playwright counts
// 1.9 px as visible. vividn, 2026-10-30: Start is smaller and part of the game, on its right, the scene on its left.
test('on a phone each game draws a picture a finger can read, the control beside it in every phase', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const [tab, picture] of [['moment', 'svg.cafe'], ['on-off', '.thing svg'], ['change', 'svg.house']]) {
				const pane = `[data-pane="${tab}-lesson"]`;
				await lessonTab(page, tab).click();
				await page.locator(`${pane} ${picture}`).first().waitFor();
				const pictures = await Promise.all((await page.locator(`${pane} ${picture}`).all()).map(drawn));
				const control = page.locator(`${pane} .game ${tab === 'change' ? '.cue' : '.go'}`);
				const at = await control.boundingBox();
				for (const p of pictures) {
					assert.ok(p.height >= 50, `${tab}: its picture is drawn ${p.height} px tall`);
					assert.ok(p.right <= at.x, `${tab}: the picture (right ${p.right}) is clear of the control (left ${at.x})`);
				}
				const game = await page.locator(`${pane} .game`).boundingBox();
				assert.ok(at.y >= game.y && at.y + at.height <= game.y + game.height, `${tab}: the control is inside the box`);
				// The other states of one control stand where Start stands: «End now» as the game runs, «Reset» once over. The
				// switch game has no Start, its cue standing there at rest, so they stand where each other does, clear of the house.
				const start = app.clock.at();
				if (tab === 'change') await startSwitch(page);
				else await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, pane);
				await app.clock.set(start + 1500);
				const ending = page.getByRole('button', { name: 'End now', exact: true });
				await page.waitForFunction((pane) => parseFloat(document.querySelector(`${pane} .game .go .spent`)?.style.inlineSize) > 0, pane);
				const place = await ending.boundingBox();
				if (tab !== 'change') assert.deepEqual(place, at, `${tab}: «End now» is where Start was`);
				for (const p of pictures) assert.ok(p.right <= place.x, `${tab}: the picture (right ${p.right}) is clear of «End now» (left ${place.x})`);
				await ending.click();
				const reset = page.getByRole('button', { name: 'Reset', exact: true });
				await reset.waitFor();
				assert.deepEqual(await reset.boundingBox(), place, `${tab}: «Reset» is where «End now» was`);
				const shown = await page.locator(`${pane} .game .result`).boundingBox();
				assert.ok(shown.x + shown.width <= place.x, `${tab}: the result is clear of it`);
				await reset.click();
				await settled(page, pane);
			}
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-qx1it, vividn, 2026-10-31: «on the tutorial move the placemat and everything back to the middle. There is enough
// room, only start pushing it to the left on extremely narrow phones where the start button would start overlapping».
// `iPhone 12` and a 280 px phone; the café fits centered on the first, the house and the den, at #1439's heights, do not.
for (const [screen, options, centered] of [
	['iPhone 12', PHONE, ['moment']],
	['280 px', { ...PHONE, viewport: { width: 280, height: 653 } }, []]
]) {
	test(`on a ${screen} screen each game's picture is in the middle of the stage, or as near it as its control leaves room for`, async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const [tab, picture] of [['moment', 'svg.cafe'], ['on-off', '.thing svg'], ['change', 'svg.house']]) {
					const pane = `[data-pane="${tab}-lesson"]`;
					await lessonTab(page, tab).click();
					await page.locator(`${pane} ${picture}`).first().waitFor();
					const stage = await page.locator(`${pane} .stage`).boundingBox();
					const middle = stage.x + stage.width / 2;
					// All a game's pictures, the den's three, as one.
					const scene = async () => {
						const all = await Promise.all((await page.locator(`${pane} ${picture}`).all()).map(drawn));
						const left = Math.min(...all.map((p) => p.left));
						const right = Math.max(...all.map((p) => p.right));
						return { left, right, center: (left + right) / 2 };
					};
					const rest = await scene();
					const control = await page.locator(`${pane} .game ${tab === 'change' ? '.cue' : '.go'}`).boundingBox();
					const column = await page.locator(`${pane} .controls`).boundingBox();
					assert.ok(rest.right <= control.x, `${tab}: the picture (right ${rest.right}) is clear of the control (left ${control.x})`);
					assert.ok(rest.right <= column.x, `${tab}: the picture (right ${rest.right}) is clear of the controls' column (left ${column.x})`);
					assert.ok(rest.left >= stage.x, `${tab}: the picture (left ${rest.left}) is inside the stage (left ${stage.x})`);
					// As near the middle as the column lets it: the center it would have against the column, or the stage's.
					const nearest = Math.min(middle, column.x - 4 - (rest.right - rest.left) / 2);
					assert.ok(Math.abs(rest.center - nearest) <= 2, `${tab}: the picture is centered at ${rest.center}, and could be at ${nearest}`);
					if (centered.includes(tab)) assert.ok(Math.abs(rest.center - middle) <= 2, `${tab}: the picture is centered at ${rest.center}, the stage at ${middle}`);
					if (screen === '280 px') assert.ok(rest.center < middle - 2, `${tab}: the picture (center ${rest.center}) has moved left of the middle (${middle})`);

					const start = app.clock.at();
					if (tab === 'change') await startSwitch(page);
					else await page.getByRole('button', { name: 'Start', exact: true }).click();
					await settled(page, pane);
					await app.clock.set(start + 1500);
					const ending = page.getByRole('button', { name: 'End now', exact: true });
					await ending.waitFor();
					const place = await ending.boundingBox();
					const playing = await scene();
					assert.ok(Math.abs(playing.center - rest.center) <= 1, `${tab}: the picture stays put as the game runs (${rest.center}, then ${playing.center})`);
					assert.ok(playing.right <= place.x, `${tab}: the picture (right ${playing.right}) is clear of «End now» (left ${place.x})`);
					await ending.click();
					const reset = page.getByRole('button', { name: 'Reset', exact: true });
					await reset.waitFor();
					const again = await reset.boundingBox();
					const shown = await page.locator(`${pane} .game .result`).boundingBox();
					assert.ok(shown.x + shown.width <= again.x, `${tab}: the result (right ${shown.x + shown.width}) is clear of «Reset» (left ${again.x})`);
					await reset.click();
					await settled(page, pane);
					await page.locator(`${pane} ${picture}`).first().waitFor();
				}
			},
			{ ...options, query: '?db=tutorial' }
		);
	});
}

// da-uzyih.18, vividn: «[nowview] [game] [fields widget] [explanation] that
// way, as you're tapping and paying attention to the game, you also have the
// nowview close by so that you can see what is happening there without the
// explanation in between». da-uzyih.4908, vividn, 2026-10-29: «1/3 for the
// description, 1/6 for nowview … 1/4 for game and 1/4 for field widget. Let's
// have the game and the fields widgets be separate widgets here». Each game,
// before, during and after a round: its title and what its shape is, then the
// nowview, then its game box, with nothing of the lesson above it, then its
// fields, then what just happened, its terms (da-zb1dr). That the nowview,
// the game and collect are in sight together past the intro is
// tutorial-scroll.test.mjs's.
test('each game reads title and shape, nowview, game, fields widget, discussion, in every phase', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			/** The tab's parts top to bottom, and whether any of the lesson is drawn above its game. */
			const parts = (tab) =>
				page.locator(`[data-pane="${tab}-lesson"] .game-widget`).evaluate((root, tab) => {
					const box = (el) => el && el.getBoundingClientRect();
					const pane = (part) => document.querySelector(`[data-pane="${tab}-${part}"]`);
					const game = root.querySelector('.game');
					const named = {
						title: box(pane('about').querySelector('h1')),
						shape: box(pane('about').querySelector('p')),
						now: box(pane('now')),
						game: box(game),
						act: box(pane('fields').querySelector('.fields')),
						discuss: box(pane('discuss').querySelector('h2'))
					};
					const above = [...root.querySelectorAll('*')]
						.filter((el) => !el.contains(game) && !game.contains(el) && box(el).height > 0 && box(el).top < box(game).top - 0.5)
						.map((el) => el.outerHTML.slice(0, 60));
					/** How far each pane under the about is scrolled. */
					const scrolled = ['lesson', 'fields'].map((part) => {
						let scroller = pane(part).querySelector('.game-widget, .fields');
						while (scroller && getComputedStyle(scroller).overflowY === 'visible') scroller = scroller.parentElement;
						return scroller.scrollTop;
					});
					return { ...named, above, scrolled };
				}, tab);
			const inOrder = (tab, phase, p) => {
				const order = ['title', 'shape', 'now', 'game', 'act', 'discuss'];
				for (const [a, b] of order.slice(1).map((b, i) => [order[i], b]))
					assert.ok(p[a].bottom <= p[b].top + 0.5, `${tab}, ${phase}: ${a} (bottom ${p[a].bottom}) above ${b} (top ${p[b].top})`);
				assert.deepEqual(p.above, [], `${tab}, ${phase}: nothing of the lesson above the game`);
			};
			// Each game's shape, by the name the setup widget gives it, and the term its discussion names.
			const words = { moment: ['moment', 'entry'], 'on-off': ['on/off', 'state'], change: ['change', 'state'] };

			for (const tab of ['moment', 'change', 'on-off']) {
				await lessonTab(page, tab).click();
				await page.locator(`[data-pane="${tab}-lesson"] .game`).waitFor();
				inOrder(tab, 'before', await parts(tab));
				assert.deepEqual(await page.locator(`[data-pane="${tab}-about"] p b`).allInnerTexts(), words[tab].slice(0, 1));
				assert.deepEqual(await page.locator(`[data-pane="${tab}-discuss"] p b`).allInnerTexts(), words[tab].slice(1));

				const start = app.clock.at();
				if (tab === 'change') await startSwitch(page);
				else await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, `[data-pane="${tab}-lesson"]`);
				await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field]`).first().waitFor();
				const during = await parts(tab);
				inOrder(tab, 'during', during);
				assert.deepEqual(during.scrolled, [0, 0], `${tab}: neither the game nor the fields is scrolled`);
				// Past the longest round: the café's, 3 s + 11 gaps of at most 6 s + MATCH_MS + 1 s.
				await app.clock.set(start + 75_000);
				await page.locator(`[data-pane="${tab}-lesson"] .game .result`).waitFor();
				inOrder(tab, 'after', await parts(tab));
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4894, vividn: «be a bit more intentional about the colors and y
// placement and heights of the bars … the bars overlap quite a bit … some
// variation in the size of the bars». On each lesson's tab, no two of the
// lesson's own lanes drawn in the nowview overlap, and they differ in height.
// Two lessons' lanes may share the band: one game is played at a time
// (vividn: «prioritize distinctness within a lesson rather than across lessons»).
const ago = (s) => ['-t', new Date(HELD_AT - s * 1000).toISOString()];

const SEED = [
	['occur', 'coffee', ...ago(100)],
	['occur', 'tea', ...ago(85)],
	['occur', 'croissant', ...ago(70)],
	['occur', 'cake', ...ago(55)],
	['occur', 'bagel', ...ago(40)],
	['start', 'reading', ...ago(110)],
	['start', 'tv', ...ago(90)],
	['start', 'phone', ...ago(70)],
	['switch', 'room', 'kitchen', ...ago(110)],
	['switch', 'room', 'garden', ...ago(50)]
];

test('on each lesson’s tab no two of its fields’ lanes in the nowview overlap, and they differ in height', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await lessonTab(page, 'moment').click();
			await page.locator('[data-pane$="-fields"] .fields button[data-field]').first().waitFor();
			for (const argv of SEED) assert.equal((await app.run(argv)).code, 0, argv.join(' '));
			// A quiet run tells no pane: each draws them afresh.
			await page.reload();
			await ready(page);
			const lessons = await page.evaluate(() => window.datum.tutorial.lessons.filter((l) => l.fields.length));
			assert.deepEqual([...new Set(SEED.map((argv) => argv[1]))].sort(), lessons.flatMap((l) => l.fields).sort(), 'every lesson field is seeded');
			for (const { name, fields } of lessons) {
				const tab = lessonTab(page, name);
				// The open tab's name opens its menu: click only another.
				if ((await tab.getAttribute('aria-selected')) !== 'true') await tab.click();
				const now = `[data-pane="${name}-now"]`;
				for (const field of fields) await page.locator(`${now} g.field[data-field="${field}"] :is(.block, .point-bg)`).first().waitFor({ state: 'attached' });
				// A lane's extent: the top of its highest mark to the bottom of its lowest, blocks and dots alike.
				const lanes = await page.locator(now).evaluate((pane, own) => {
					const out = {};
					for (const field of own) {
						const rects = [...pane.querySelectorAll(`g.field[data-field="${field}"] :is(.block, .point-bg)`)]
							.map((m) => m.getBoundingClientRect())
							.filter((r) => r.width && r.height);
						out[field] = { top: Math.min(...rects.map((r) => r.top)), bottom: Math.max(...rects.map((r) => r.bottom)) };
					}
					return out;
				}, fields);
				for (const [i, a] of fields.entries())
					for (const b of fields.slice(i + 1)) {
						const [p, q] = [lanes[a], lanes[b]];
						// The nearest lanes are 0.015 of the band apart, about 0.9px on a phone: a slack of float error alone.
						assert.ok(p.bottom <= q.top + 0.01 || q.bottom <= p.top + 0.01, `on ${name}: ${a} ${JSON.stringify(p)} and ${b} ${JSON.stringify(q)} overlap`);
					}
				if (fields.length > 1) {
					const heights = fields.map((f) => Math.round(lanes[f].bottom - lanes[f].top));
					assert.ok(new Set(heights).size > 1, `on ${name} its lanes differ in height: ${heights}`);
				}
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});
