// A field opened in collect looks and behaves like the fields at rest
// (da-r48y9). vividn, 2026-11-07: «let's change the state view in the collect
// widget as well so that it looks and behaves much more like the field view.
// So same zoom level. On the first row surrounded by a dotted line … (< back)
// current state … (off) [only show the off button if the field allows the off
// state. If the state is currently off, put a yellow ! before the off]. then
// top to bottom, left to right list the states in order defined by the spec
// … following the ordered states, comes all the states that have been used
// for this field sorted by recency».
//
// The placement is `opened` in fields.ts, tested alone here; the rest is the
// view as drawn. Panning and focus are `fields-cluster.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { opened } from '../src/lib/fields.ts';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';
import { along, pickOption, widgetPane } from './support/layout.mjs';

const GAP = 6;
const O = { height: 4 * (30 + GAP), rowH: 30, gap: GAP };

describe('opened', () => {
	test('the first item takes the first row, the rest fill the rows column by column', () => {
		const { stands } = opened([{ w: 100, outlined: true }, ...[40, 40, 40, 40, 40].map((w) => ({ w }))], O);
		assert.deepEqual(stands.map((s) => s.row), [0, 1, 2, 3, 1, 2]);
		assert.deepEqual(stands.slice(1, 4).map((s) => s.x), [GAP, GAP, GAP]);
		assert.equal(stands[4].x, GAP + 40 + GAP);
		// The pitch is atRest's: the rows share the height.
		assert.deepEqual(stands.slice(0, 4).map((s) => s.top), [3, 39, 75, 111]);
	});

	test('a row fallen behind takes two narrow items before a long one gets its second', () => {
		const { stands } = opened([{ w: 60, outlined: true }, { w: 200 }, { w: 30 }, { w: 30 }, { w: 30 }, { w: 30 }, { w: 30 }], O);
		assert.deepEqual(stands.map((s) => s.row), [0, 1, 2, 3, 2, 3, 2]);
	});

	test('an outlined item keeps a gap inside its outline, and the next in its row a gap past it', () => {
		const { stands, end } = opened([{ w: 50, outlined: true }, { w: 20 }, { w: 20 }, { w: 20 }, { w: 20, outlined: true }, { w: 20 }], O);
		const [head] = stands;
		assert.equal(head.x, 2 * GAP);
		assert.equal(head.outline, `M${GAP} 0H${GAP + 50 + 2 * GAP}V36H${GAP}Z`);
		assert.equal(stands[1].outline, null);
		// The fifth, outlined, in row 1 after the second: its outline at the second's end and a gap.
		assert.equal(stands[4].row, 1);
		assert.equal(stands[4].x, GAP + 20 + GAP + GAP);
		assert.equal(end, GAP + 50 + 2 * GAP);
	});

	test('a room shorter than two rows is one row', () => {
		assert.deepEqual(opened([{ w: 10 }, { w: 10 }, { w: 10 }], { ...O, height: 40 }).stands.map((s) => s.row), [0, 0, 0]);
	});
});

/** The CLI over collect, so collect is tall enough for several rows. */
const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const VIEW = 'section[data-pane="bar"] .bar .collect';
const OPEN = `${VIEW}.open`;
const HEAD = `${OPEN} .spot.open[data-role="head"]`;

const shared = sharedApp({ clock: todayAt(15) });

/** The app seeded with `lines`, the fields drawn, `field` tapped open. */
const opening = (lines, field, fn) =>
	shared.solo(async (app) => {
		await app.page.evaluate(() => window.datum.layout.forgetSession());
		for (const line of lines) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		await app.page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		const button = app.page.locator(`${VIEW} [data-field="${field}"]`);
		await button.waitFor();
		await fn(app, button);
	}, PAGE);

/** The open view as drawn: each item's role, row, box and words, in the DOM's order. */
const items = (page) =>
	page.locator(OPEN).evaluate((view) =>
		[...view.querySelectorAll('.spot.open')].map((spot) => {
			const r = spot.getBoundingClientRect();
			const b = spot.querySelector('[data-state]');
			return { role: spot.dataset.role, row: Number(spot.dataset.row), state: b?.dataset.state, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		})
	);

/** Whether the dotted outline goes round the middle of what `selector` names. */
const outlined = (page, selector) =>
	page.locator(selector).evaluate((el) => {
		const path = el.closest('.collect').querySelector('svg.dotted path');
		const r = el.getBoundingClientRect();
		const at = new DOMPoint(r.left + r.width / 2, r.top + r.height / 2).matrixTransform(path.getScreenCTM().inverse());
		return path.isPointInFill(at);
	});

/** The states listed, once the read of `datum states` has put `last` among them. */
async function listed(page, last) {
	await page.locator(`${OPEN} [data-state="${last}"]`).waitFor();
	return (await items(page)).filter((i) => i.role === 'state').map((i) => i.state);
}

describe('a field opened in collect', () => {
	before(startApp);
	after(stopApp);

	test('its first row in a dotted outline: back, the field in its colour, the state it is in as the CLI paints it, then off', async () => {
		await opening(['setup mood --verb switch --states false,good,bad,meh', 'switch mood bad -t 13:00'], 'mood', async (app, button) => {
			const { page } = app;
			await button.click();
			await page.locator(HEAD).waitFor();
			const parts = await page.locator(HEAD).evaluate((head) =>
				[...head.children].map((el) => ({
					what: ['back', 'open-field', 'current', 'off'].find((k) => el.hasAttribute(`data-${k}`)),
					text: el.textContent.trim(),
					left: el.getBoundingClientRect().left,
					color: getComputedStyle(el).color,
					background: getComputedStyle(el).backgroundColor
				}))
			);
			assert.deepEqual(parts.map((p) => p.what), ['back', 'open-field', 'current', 'off']);
			assert.ok(parts.every((p, i) => i === 0 || p.left > parts[i - 1].left), 'not left to right');
			assert.equal(parts[2].text, 'bad');
			assert.equal(parts[3].text, 'off', 'off says ! while the field is on');
			// The colours `fields --json` resolves: the field's for its name, the state's behind the state.
			const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'mood');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			assert.equal(parts[1].color, rgb(row.color));
			assert.equal(parts[2].background, rgb(row.state_colors.bad));
			assert.equal(await page.locator(`${HEAD} [data-current]`).evaluate((el) => el.tagName), 'SPAN', 'the state it is in is a button');
			assert.ok(await outlined(page, HEAD), 'the first row is not in the dotted outline');
			// Its rows below: the spec's states, the one it is in outlined with a ! in front, the others neither.
			assert.deepEqual(await listed(page, 'meh'), ['good', 'bad', 'meh']);
			assert.deepEqual(await page.locator(`${OPEN} [data-state]:has([data-repeat])`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['bad']);
			assert.equal(await page.locator(`${OPEN} [data-state="bad"]`).textContent(), '!bad', 'the ! is not in front');
			assert.equal(await page.locator(`${OPEN} [data-state="bad"] [data-repeat]`).evaluate((el) => getComputedStyle(el).color), 'rgb(255, 209, 102)', 'the ! is not the warning yellow');
			assert.ok(await outlined(page, `${OPEN} [data-state="bad"]`), 'the state it is in is not outlined');
			assert.equal(await outlined(page, `${OPEN} [data-state="good"]`), false, 'a state it is not in is outlined');
		});
	});

	test('off is absent for a field whose spec forbids it, and wears a yellow ! while the field is off', async () => {
		await opening(['setup posture --verb switch --states sit,stand --never-false', 'setup mood --verb switch --states false,good,bad', 'switch mood good -t 12:00', 'off mood -t 13:00'], 'posture', async ({ page }, button) => {
			await button.click();
			await page.locator(`${OPEN} [data-state="stand"]`).waitFor();
			assert.equal(await page.locator(`${OPEN} [data-off]`).count(), 0, 'a never-off field has off');
			await page.keyboard.press('Escape');
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${OPEN} [data-off]`).waitFor();
			assert.equal(await page.locator(`${OPEN} [data-off] [data-repeat]`).textContent(), '!');
			assert.equal(await page.locator(`${HEAD} [data-current]`).count(), 0, 'an off field shows a state it is in');
			assert.equal(await page.locator(`${OPEN} [data-state] [data-repeat]`).count(), 0, 'a state is marked as the one it is in');
		});
	});

	// In 2-click mode an on/off field opens too: its off runs with the bar's
	// pills, the field's among them, as a state's change does.
	test('off on an on/off field opened ends that field', async () => {
		await opening(['setup work --verb start', 'start work -t 14:00'], 'work', async (app, button) => {
			const { page } = app;
			await pickOption(page, 'bar', 'a tap', 'opens and chooses, 2-click');
			await button.click();
			await page.locator(`${OPEN} [data-off]`).waitFor();
			assert.equal(await page.locator(`${OPEN} [data-off] [data-repeat]`).count(), 0, 'work is on: off says !');
			const seen = await app.spyOnRunner();
			await page.locator(`${OPEN} [data-off]`).click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'off'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'off' && !a.includes('--dry-run')), ['off', 'work']);
			await app.idle();
			const [row] = JSON.parse((await app.run(['tail', 'work', '-n', '1', '--json'])).stdout).rows;
			assert.equal(row.state, false);
		});
	});

	// A hold does the other mode's tap (da-u017i); an on/off field's off runs in both.
	test('a hold on an on/off field’s off ends that field', async () => {
		await opening(['setup work --verb start', 'start work -t 14:00'], 'work', async (app, button) => {
			const { page } = app;
			await pickOption(page, 'bar', 'a tap', 'opens and chooses, 2-click');
			await button.click();
			await page.locator(`${OPEN} [data-off]`).waitFor();
			const seen = await app.spyOnRunner();
			await page.locator(`${OPEN} [data-off]`).click({ button: 'right' });
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'off'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'off' && !a.includes('--dry-run')), ['off', 'work']);
			await app.idle();
			const [row] = JSON.parse((await app.run(['tail', 'work', '-n', '1', '--json'])).stdout).rows;
			assert.equal(row.state, false);
		});
	});

	test('the spec’s states in its order, then every other used, the last used first, none twice', async () => {
		// `head`, which once shared the first row's key.
		const lines = ['setup mood --verb switch --states false,good,bad', 'switch mood head -t 08:00', 'switch mood zed -t 09:00', 'switch mood yak -t 10:00', 'switch mood good -t 11:00', 'switch mood ox -t 12:00', 'off mood -t 13:00'];
		await opening(lines, 'mood', async ({ page }, button) => {
			await button.click();
			assert.deepEqual(await listed(page, 'head'), ['good', 'bad', 'ox', 'yak', 'zed', 'head']);
		});
	});

	// A tap the moment a state shows lands where it stays: the states are hidden
	// until each is measured, not drawn where a width of 0 put them, and once
	// shown no state moves. That tap had scrolled the page under the tutorial.
	test('a state shown stands where it stays', async () => {
		const states = ['a_rather_long_state', 'b', 'another_long_one', 'c', 'd', 'e'];
		await opening([`setup mood --verb switch --states false,${states.join(',')}`], 'mood', async ({ page }, button) => {
			await page.evaluate(() => {
				const seen = (window.__seenStates = {});
				const look = () => {
					for (const b of document.querySelectorAll('.collect.open [data-state]')) {
						if (getComputedStyle(b).visibility !== 'visible') continue;
						const r = b.getBoundingClientRect();
						(seen[b.dataset.state] ??= new Set()).add(`${Math.round(r.left)},${Math.round(r.top)}`);
					}
					if (!window.__stopStates) requestAnimationFrame(look);
				};
				requestAnimationFrame(look);
			});
			await button.click();
			await page.locator(`${OPEN} [data-state="e"]`).waitFor();
			await page.evaluate(() => new Promise((r) => setTimeout(r, 300)));
			const seen = await page.evaluate(() => ((window.__stopStates = true), Object.fromEntries(Object.entries(window.__seenStates).map(([k, v]) => [k, [...v]]))));
			assert.deepEqual(Object.keys(seen).sort(), [...states].sort(), 'a state was never seen');
			assert.deepEqual(Object.entries(seen).filter(([, at]) => at.length > 1), [], 'a state shown moved');
		});
	});

	test('the states fill the rows column by column, a row fallen behind taking two before the long one its second, at the fields’ own height', async () => {
		const short = Array.from({ length: 30 }, (_, i) => `s${String(i).padStart(2, '0')}`);
		const states = ['a_state_with_a_rather_long_name', ...short];
		await opening([`setup mood --verb switch --states false,${states.join(',')}`, 'setup coffee --verb occur'], 'mood', async ({ page }, button) => {
			const rest = await page.locator(`${VIEW} .spot.shelf [data-field="coffee"]`).boundingBox();
			await button.click();
			const all = await listed(page, short.at(-1));
			assert.deepEqual(all, states);
			const drawn = (await items(page)).filter((i) => i.role === 'state');
			const rows = Math.max(...drawn.map((i) => i.row)) + 1;
			assert.ok(rows >= 3, `${rows} rows: the case is not tested`);
			// The first column: one state a row under the first row, in order, all at one x.
			const first = drawn.slice(0, rows - 1);
			assert.deepEqual(first.map((i) => i.row), Array.from({ length: rows - 1 }, (_, r) => r + 1));
			assert.ok(first.every((i) => Math.abs(i.left - first[0].left) < 0.5), JSON.stringify(first.map((i) => i.left)));
			// The long one's row fell ahead: another row holds more states than it does.
			const count = (r) => drawn.filter((i) => i.row === r).length;
			assert.ok(Array.from({ length: rows - 2 }, (_, r) => count(r + 2)).some((n) => n > count(1)), JSON.stringify(drawn.map((i) => i.row)));
			// Each state took the row whose end was furthest left at its turn.
			const ends = new Map();
			// The first row's outline keeps a gap past its last button.
			ends.set(0, (await items(page))[0].right + GAP);
			for (const i of drawn) {
				const end = ends.get(i.row) ?? -Infinity;
				for (let r = 0; r < rows; r++) assert.ok((ends.get(r) ?? -Infinity) >= end - 0.5 || r === i.row, `${i.state} took row ${i.row}, row ${r} ended further left`);
				ends.set(i.row, i.right);
			}
			// The same button height as the fields at rest.
			const state = await page.locator(`${OPEN} [data-state="s00"]`).boundingBox();
			assert.ok(Math.abs(state.height - rest.height) < 0.5, `a state ${state.height}px tall, a field ${rest.height}px`);
		});
	});
});
