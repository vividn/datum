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
import { opened, listedStates } from '../src/lib/fields.ts';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';
import { along, pickOption, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

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

// vividn, 2026-11-11: «in the state view, sort more recently occuring multi states before the less recently
// occuring single states. right now all the multi states happen at the end» (da-dz32d).
describe('listedStates', () => {
	const use = (state, last_used, count = 1) => ({ state, declared: false, count, last_used });
	test('the spec’s first, then singles and multi-states together, the last used first, then multi-states never used', () => {
		const rows = [use('good', '2026-11-10T09:00:00Z', 2), use('ox', '2026-11-10T10:00:00Z'), use(['good', 'bad'], '2026-11-10T11:00:00Z'), use(['bad', 'meh'], '2026-11-10T08:00:00Z'), use(false, '2026-11-10T12:00:00Z')];
		assert.deepEqual(listedStates(['good', 'bad'], rows, ['good,bad', 'bad,meh', 'good,meh']), ['good', 'bad', 'good,bad', 'ox', 'bad,meh', 'good,meh']);
	});
	test('an array no multi-state of the field is left out, as a state it cannot tap', () => {
		assert.deepEqual(listedStates([], [use(['solo'], '2026-11-10T11:00:00Z'), use('ox', '2026-11-10T10:00:00Z')], []), ['ox']);
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
			const state = b?.dataset.state ?? (spot.querySelector('[data-off]') ? 'off' : undefined);
			return { role: spot.dataset.role, row: Number(spot.dataset.row), state, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
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

/** The states listed, off as `off`, once the read of `datum states` has put `last` among them. */
async function listed(page, last) {
	await page.locator(`${OPEN} [data-state="${last}"]`).waitFor();
	return (await items(page)).filter((i) => i.role === 'state').map((i) => i.state);
}

/**
 * Starts recording every place each element `selector` names is drawn at while
 * visible, a frame at a time, keyed by its `data-<key>`; the function returned
 * waits 300 ms more, stops, and gives each key its places.
 */
async function placesShown(page, selector, key) {
	await page.evaluate(([selector, key]) => {
		const seen = (window.__places = {});
		const look = () => {
			for (const el of document.querySelectorAll(selector)) {
				if (getComputedStyle(el).visibility !== 'visible') continue;
				const r = el.getBoundingClientRect();
				(seen[el.dataset[key]] ??= new Set()).add(`${Math.round(r.left)},${Math.round(r.top)}`);
			}
			if (!window.__stopPlaces) requestAnimationFrame(look);
		};
		window.__stopPlaces = false;
		requestAnimationFrame(look);
	}, [selector, key]);
	return async () => {
		await page.evaluate(() => new Promise((r) => setTimeout(r, 300)));
		return page.evaluate(() => ((window.__stopPlaces = true), Object.fromEntries(Object.entries(window.__places).map(([k, v]) => [k, [...v]]))));
	};
}

/**
 * Every run whose command `which` names (`*`, every one) held from now until
 * the function returned is called, in the order asked; `fail` names those
 * that then answer exit 1 and run nothing. `window.__held` is what waits.
 */
async function hold(page, which, fail = []) {
	await page.evaluate(
		([which, fail]) => {
			window.__held = [];
			const previous = window.datum.setRunner((argv, ...rest) => {
				if (which !== '*' && !which.includes(argv[0])) return previous(argv, ...rest);
				const go = () => (fail.includes(argv[0]) && !argv.includes('--dry-run') ? { argv, code: 1, stdout: '', stderr: 'refused' } : previous(argv, ...rest));
				return new Promise((done) => window.__held.push({ argv, go: () => done(go()) }));
			});
			window.__release = () => {
				window.datum.setRunner(previous);
				for (const h of window.__held.splice(0)) h.go();
			};
		},
		[which, fail]
	);
	return () => page.evaluate(() => window.__release());
}

/** The commands held so far, each its words. */
const held = (page) => page.evaluate(() => window.__held.map((h) => h.argv));

describe('a field opened in collect', () => {
	before(startApp);
	after(stopApp);

	test("its first row in a dotted outline: the field's pill, which is back (da-dswuc), and the state it is in as the CLI paints it; off is the first state (da-qryu8)", async () => {
		await opening(['setup mood --verb switch --states false,good,bad,meh', 'switch mood bad -t 13:00'], 'mood', async (app, button) => {
			const { page } = app;
			await button.click();
			await page.locator(HEAD).waitFor();
			const parts = await page.locator(HEAD).evaluate((head) =>
				[...head.children].map((el) => ({
					what: ['back', 'current', 'off'].find((k) => el.hasAttribute(`data-${k}`)),
					field: el.dataset.openField,
					text: el.textContent.trim(),
					left: el.getBoundingClientRect().left,
					background: getComputedStyle(el.querySelector('.pill') ?? el).backgroundColor
				}))
			);
			assert.deepEqual(parts.map((p) => p.what), ['back', 'current']);
			assert.ok(parts.every((p, i) => i === 0 || p.left > parts[i - 1].left), 'not left to right');
			assert.equal(parts[0].field, 'mood', 'back is not the field');
			assert.equal(parts[0].text, 'mood×');
			assert.equal(parts[1].text, 'bad');
			// The colours `fields --json` resolves: the field's behind its pill, the state's behind the state.
			const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'mood');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			assert.equal(parts[0].background, rgb(row.color));
			assert.equal(parts[1].background, rgb(row.state_colors.bad));
			assert.equal(await page.locator(`${HEAD} [data-current]`).evaluate((el) => el.tagName), 'SPAN', 'the state it is in is a button');
			assert.ok(await outlined(page, HEAD), 'the first row is not in the dotted outline');
			// Its rows below: the spec's states, the one it is in outlined with a ! in front, the others neither.
			assert.deepEqual(await listed(page, 'meh'), ['off', 'good', 'bad', 'meh']);
			assert.equal(await page.locator(`${OPEN} [data-off]`).textContent(), 'off', 'off says ! while the field is on');
			assert.deepEqual(await page.locator(`${OPEN} [data-state]:has([data-repeat])`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['bad']);
			assert.equal(await page.locator(`${OPEN} [data-state="bad"]`).textContent(), '!bad', 'the ! is not in front');
			assert.equal(await page.locator(`${OPEN} [data-state="bad"] [data-repeat]`).evaluate((el) => getComputedStyle(el).color), 'rgb(255, 209, 102)', 'the ! is not the warning yellow');
			assert.ok(await outlined(page, `${OPEN} [data-state="bad"]`), 'the state it is in is not outlined');
			assert.equal(await outlined(page, `${OPEN} [data-state="good"]`), false, 'a state it is not in is outlined');
		});
	});

	// vividn, 2026-11-10 (da-a2cxa): «let's fully color all the state buttons, not just the active one. Which one is
	// active is now well represented with the exclamation point and surrounding dotted line», «the "off" button should
	// not be colored rather empty», and a multi-state colored «just like it is in the tail, so commas … the dividing line
	// between the colors should be tilted to the right / to match the diagonal lines in a multi state render», fitted to
	// the text: each state's color behind its own words.
	for (const screen of [{ width: 390, height: 844 }, { width: 1280, height: 720 }]) {
		test(`every state filled in its color, off empty, a multi-state in commas with a 45° boundary under each (${screen.width}x${screen.height})`, async () => {
			const lines = ['setup mood --verb switch --states false,good,bad,meh', 'switch mood good,bad -t 10:00', 'switch mood good,bad,meh -t 11:00', 'switch mood bad -t 13:00'];
			await opening(lines, 'mood', async (app, button) => {
				const { page } = app;
				await page.setViewportSize(screen);
				await button.click();
				const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'mood');
				const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
				assert.deepEqual(await listed(page, 'good,bad,meh'), ['off', 'good', 'bad', 'meh', 'good,bad,meh', 'good,bad']);
				for (const state of ['good', 'bad', 'meh']) {
					const fill = await page.locator(`${OPEN} [data-state="${state}"]`).evaluate((el) => getComputedStyle(el).backgroundColor);
					assert.equal(fill, rgb(row.state_colors[state]), `${state} is not filled in its color`);
				}
				assert.equal(await page.locator(`${OPEN} [data-off]`).evaluate((el) => getComputedStyle(el).backgroundColor), 'rgba(0, 0, 0, 0)', 'off is filled');
				// The active one alone carries the ! and the dotted outline.
				assert.deepEqual(await page.locator(`${OPEN} [data-state]:has([data-repeat])`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['bad']);
				assert.ok(await outlined(page, `${OPEN} [data-state="bad"]`), 'the state it is in is not outlined');
				assert.equal(await outlined(page, `${OPEN} [data-state="good"]`), false, 'a state it is not in is outlined');
				for (const set of [['good', 'bad'], ['good', 'bad', 'meh']]) {
					const label = set.join(',');
					const drawn = await page.locator(`${OPEN} [data-state="${label}"]`).evaluate((b) => {
						const commas = [...b.querySelectorAll('.comma')].map((c) => {
							const under = getComputedStyle(c, '::before');
							const r = c.getBoundingClientRect();
							return { text: c.textContent, half: r.width / 2, left: parseFloat(under.left), top: under.top, bottom: under.bottom, fill: under.backgroundColor, skew: under.transform };
						});
						const words = [...b.querySelectorAll('[data-part]')].map((w) => w.textContent);
						return { text: b.textContent, fill: getComputedStyle(b).backgroundColor, clips: getComputedStyle(b).overflow, commas, words };
					});
					assert.equal(drawn.text, label, 'not labelled with commas');
					assert.deepEqual(drawn.words, set);
					assert.equal(drawn.fill, rgb(row.state_colors[set[0]]), 'the first state is not behind its words');
					assert.equal(drawn.clips, 'hidden', 'the colors spill past the button');
					assert.equal(drawn.commas.length, set.length - 1);
					drawn.commas.forEach((c, i) => {
						assert.equal(c.text, ',');
						assert.equal(c.fill, rgb(row.state_colors[set[i + 1]]), `${set[i + 1]} is not behind its words`);
						assert.ok(Math.abs(c.left - c.half) < 0.5, `the boundary is not under comma ${i + 1}: ${c.left} vs ${c.half}`);
						assert.equal(c.top, c.bottom, 'the boundary is not centered on the comma');
						// skewX(-45deg): leaning right, as dayview's stripes under rotate(45).
						assert.equal(c.skew, 'matrix(1, 0, -1, 1, 0, 0)');
					});
				}
			});
		});
	}

	test('a field in a multi-state marks that button active: the ! and the dotted outline, and no other, and its head chip says it', async () => {
		const lines = ['setup mood --verb switch --states false,good,bad', 'switch mood bad -t 10:00', 'switch mood good,bad -t 13:00'];
		await opening(lines, 'mood', async (app, button) => {
			const { page } = app;
			await button.click();
			await page.locator(`${OPEN} [data-state="good,bad"]`).waitFor();
			assert.deepEqual(await page.locator(`${OPEN} [data-state]:has([data-repeat])`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['good,bad']);
			assert.equal(await page.locator(`${OPEN} [data-state="good,bad"]`).textContent(), '!good,bad');
			assert.ok(await outlined(page, `${OPEN} [data-state="good,bad"]`), 'the multi-state it is in is not outlined');
			assert.equal(await outlined(page, `${OPEN} [data-state="bad"]`), false, 'a state it is not in is outlined');
			// The head row's chip says it too (da-o5jfh), painted as the button: each state's color behind its words, a 45° boundary under the comma.
			const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'mood');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const chip = await page.locator(`${HEAD} [data-current]`).evaluate((el) => {
				const comma = el.querySelector('.comma');
				const under = getComputedStyle(comma, '::before');
				return { text: el.textContent, fill: getComputedStyle(el).backgroundColor, clips: getComputedStyle(el).overflow, next: under.backgroundColor, skew: under.transform };
			});
			assert.deepEqual(chip, { text: 'good,bad', fill: rgb(row.state_colors.good), clips: 'hidden', next: rgb(row.state_colors.bad), skew: 'matrix(1, 0, -1, 1, 0, 0)' });
		});
	});

	// vividn, 2026-11-11 (da-n2oa7): «it happens for states that have only ever appeared in a multistate before»: a
	// state no spec declares and never logged alone still takes its own color, not the field's.
	test('a state seen only inside a multi-state is painted in its own color, not the field\'s', async () => {
		await opening(['switch mood good,bad -t 10:00', 'switch mood calm -t 13:00'], 'mood', async (app, button) => {
			const { page } = app;
			await button.click();
			await page.locator(`${OPEN} [data-state="good,bad"]`).waitFor();
			const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'mood');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const drawn = await page.locator(`${OPEN} [data-state="good,bad"]`).evaluate((b) => ({
				fill: getComputedStyle(b).backgroundColor,
				next: getComputedStyle(b.querySelector('.comma'), '::before').backgroundColor
			}));
			assert.deepEqual(drawn, { fill: rgb(row.state_colors.good), next: rgb(row.state_colors.bad) });
			assert.notEqual(drawn.fill, rgb(row.color), 'good takes the field\'s color');
			assert.notEqual(drawn.next, rgb(row.color), 'bad takes the field\'s color');
		});
	});

	test('off is absent for a field whose spec forbids it, and wears a yellow ! while the field is off', async () => {
		await opening(['setup posture --verb switch --states sit,stand --never-false', 'setup mood --verb switch --states false,good,bad', 'switch mood good -t 12:00', 'off mood -t 13:00'], 'posture', async ({ page }, button) => {
			await button.click();
			await page.locator(`${OPEN} [data-state="stand"]`).waitFor();
			assert.equal(await page.locator(`${OPEN} [data-off]`).count(), 0, 'a never-off field has off');
			assert.deepEqual(await listed(page, 'stand'), ['sit', 'stand']);
			await page.keyboard.press('Escape');
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${OPEN} [data-off]`).waitFor();
			// Off is the first state, not in the first row (da-qryu8).
			assert.deepEqual(await listed(page, 'bad'), ['off', 'good', 'bad']);
			assert.equal(await page.locator(`${HEAD} [data-off]`).count(), 0, 'off in the first row');
			assert.ok(await outlined(page, `${OPEN} [data-off]`), 'off, the state it is in, is not outlined');
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
			assert.deepEqual((await items(page)).map((i) => [i.role, i.state]), [['head', undefined], ['state', 'off']], 'an on/off field’s one state is not off');
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

	// vividn, 2026-11-11: «when on is then enabled, it appears of course then as the second state in the collect widgets
	// state view» (da-qryu8): setup's on lists `true`, drawn «on» in the field's color, after off.
	test('a field that takes true lists on second, after off, in the field’s color, and logs it', async () => {
		await opening(['setup lamp --verb switch --states false,dim,true', 'switch lamp dim -t 12:00'], 'lamp', async (app, button) => {
			const { page } = app;
			await button.click();
			assert.deepEqual(await listed(page, 'true'), ['off', 'true', 'dim']);
			assert.equal(await page.locator(`${OPEN} [data-state="true"]`).textContent(), 'on');
			const row = JSON.parse((await app.run(['fields', '--json'])).stdout).find((r) => r.field === 'lamp');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			assert.equal(await page.locator(`${OPEN} [data-state="true"]`).evaluate((el) => getComputedStyle(el).backgroundColor), rgb(row.color), 'on is not the field’s color');
			await page.locator(`${OPEN} [data-state="true"]`).click();
			await page.locator(OPEN).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal(JSON.parse((await app.run(['tail', 'lamp', '-n', '1', '--json'])).stdout).rows[0].state, true);
			await button.click();
			await page.locator(`${OPEN} [data-state="true"] [data-repeat]`).waitFor();
			assert.equal(await page.locator(`${HEAD} [data-current]`).textContent(), 'on');
		});
	});

	// vividn, 2026-11-13: «The on button in the collect widget state view often appears a second after the other ones,
	// causing lower states to shift … Also it seems to appear even when on is not enabled for a field?» (da-i6kpg)
	test('a `true` only used, not in the spec, offers no on, before `states` answers or after', async () => {
		await opening(['setup spot --verb switch --states false,a,b', 'switch spot true -t 09:00', 'switch spot zed -t 10:00'], 'spot', async ({ page }, button) => {
			const release = await hold(page, ['states']);
			try {
				await button.click();
				assert.deepEqual(await listed(page, 'b'), ['off', 'a', 'b']);
			} finally {
				await release();
			}
			assert.deepEqual(await listed(page, 'zed'), ['off', 'a', 'b', 'zed']);
		});
	});

	test('a spec listing `true` draws on in the first frame, second after off, and nothing moves when `states` answers', async () => {
		await opening(['setup spot --verb switch --states false,a,b,true', 'switch spot true -t 09:00', 'switch spot zed -t 10:00'], 'spot', async ({ page }, button) => {
			const seen = await placesShown(page, '.collect.open [data-state]', 'state');
			const release = await hold(page, ['states']);
			try {
				await button.click();
				assert.deepEqual(await listed(page, 'b'), ['off', 'true', 'a', 'b']);
				assert.ok((await held(page)).some((a) => a[0] === 'states'), 'on waited for the listing');
			} finally {
				await release();
			}
			assert.deepEqual(await listed(page, 'zed'), ['off', 'true', 'a', 'b', 'zed']);
			assert.deepEqual(Object.entries(await seen()).filter(([, places]) => places.length > 1), [], 'a state shown moved');
		});
	});

	test('the spec’s states in its order, then every other used, the last used first, none twice', async () => {
		// `head`, which once shared the first row's key.
		const lines = ['setup mood --verb switch --states false,good,bad', 'switch mood head -t 08:00', 'switch mood zed -t 09:00', 'switch mood yak -t 10:00', 'switch mood good -t 11:00', 'switch mood ox -t 12:00', 'off mood -t 13:00'];
		await opening(lines, 'mood', async ({ page }, button) => {
			await button.click();
			assert.deepEqual(await listed(page, 'head'), ['off', 'good', 'bad', 'ox', 'yak', 'zed', 'head']);
		});
	});

	test('a multi-state among the used singles by its last use, not after them (da-dz32d)', async () => {
		const lines = ['setup mood --verb switch --states false,good,bad,meh', 'switch mood zed -t 09:00', 'switch mood bad,meh -t 10:00', 'switch mood ox -t 11:00', 'switch mood good,bad -t 12:00', 'off mood -t 13:00'];
		await opening(lines, 'mood', async ({ page }, button) => {
			await button.click();
			// `zed` shows once `states` answers: the multi-states are known before it.
			assert.deepEqual(await listed(page, 'zed'), ['off', 'good', 'bad', 'meh', 'good,bad', 'ox', 'bad,meh', 'zed']);
		});
	});

	// A tap the moment a state shows lands where it stays: the states are hidden
	// until each is measured, not drawn where a width of 0 put them, and once
	// shown no state moves. That tap had scrolled the page under the tutorial.
	test('a state shown stands where it stays', async () => {
		const states = ['a_rather_long_state', 'b', 'another_long_one', 'c', 'd', 'e'];
		await opening([`setup mood --verb switch --states false,${states.join(',')}`], 'mood', async ({ page }, button) => {
			const seen = await placesShown(page, '.collect.open [data-state]', 'state');
			await button.click();
			await page.locator(`${OPEN} [data-state="e"]`).waitFor();
			const at = await seen();
			assert.deepEqual(Object.keys(at).sort(), [...states].sort(), 'a state was never seen');
			assert.deepEqual(Object.entries(at).filter(([, places]) => places.length > 1), [], 'a state shown moved');
		});
	});

	// On the right the first row is placed by its own width, 0 until it is
	// measured, so it too is hidden until then (da-tbtt7).
	test('the first row shown on the right stands where it stays', async () => {
		await opening(['setup mood --verb switch --states false,good,bad', 'switch mood good -t 13:00'], 'mood', async ({ page }, button) => {
			await pickOption(page, 'bar', 'side', 'right');
			const seen = await placesShown(page, '.collect.open .spot.open[data-role="head"]', 'role');
			await button.click();
			await page.locator(HEAD).waitFor();
			const { head } = await seen();
			assert.equal(head?.length, 1, `the first row shown at ${JSON.stringify(head)}`);
		});
	});

	test('the states fill the rows column by column, a row fallen behind taking two before the long one its second, at the fields’ own height', async () => {
		const short = Array.from({ length: 30 }, (_, i) => `s${String(i).padStart(2, '0')}`);
		const states = ['a_state_with_a_rather_long_name', ...short];
		// Never off: off, the state a field never logged is in, is outlined, and stands in from its column.
		await opening([`setup mood --verb switch --states ${states.join(',')} --never-false`, 'setup coffee --verb occur'], 'mood', async ({ page }, button) => {
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
	// vividn, 2026-11-09: «clicking on a CHANGE data, feels very laggy … Prioritize
	// drawing the already known states from the spec. But this should probably all
	// be cached anyway» and «it really needs to be tap tap done» (da-2ylmj).
	test('a change field draws its spec’s states before `states` answers, the used ones after them, and the spec’s stay put', async () => {
		await opening(['setup tempo --verb switch --states false,adagio,presto,largo', 'switch tempo zed -t 10:00', 'switch tempo yak -t 11:00'], 'tempo', async (app, button) => {
			const { page } = app;
			const release = await hold(page, ['states']);
			let spec;
			try {
				await button.click();
				await page.locator(`${OPEN} [data-state="largo"]`).waitFor();
				assert.ok((await held(page)).some((a) => a[0] === 'states'), 'the spec’s states waited for the listing');
				spec = (await items(page)).filter((i) => i.role === 'state');
				assert.deepEqual(spec.map((i) => i.state), ['off', 'adagio', 'presto', 'largo']);
			} finally {
				await release();
			}
			await listed(page, 'zed');
			const all = (await items(page)).filter((i) => i.role === 'state');
			assert.deepEqual(all.map((i) => i.state), ['off', 'adagio', 'presto', 'largo', 'yak', 'zed']);
			assert.deepEqual(all.slice(0, 4), spec, 'a spec state moved when the used ones came');
		});
	});

	test('back, and a state logged, draw the fields with no run between; the write lands after, and one refused opens the field again', async () => {
		await opening(['setup pace --verb switch --states false,slow,fast'], 'pace', async (app, button) => {
			const { page } = app;
			const openPace = async () => {
				await button.click();
				await page.locator(`${OPEN} [data-state="fast"]`).waitFor();
				await app.idle();
			};
			await openPace();
			let release = await hold(page, '*');
			try {
				await page.locator(`${OPEN} [data-back]`).click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				await button.waitFor();
			} finally {
				await release();
			}
			await app.idle();
			await openPace();
			release = await hold(page, '*');
			try {
				await page.locator(`${OPEN} [data-state="fast"]`).click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				await button.waitFor();
				assert.ok((await held(page)).some((a) => a[0] === 'change' && a.includes('fast') && !a.includes('--dry-run')), 'the fields waited for the write');
				// Its pill, still on the bar while the write waits, opens nothing.
				await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
				assert.equal(await page.locator(OPEN).count(), 0, 'the field opened again before its write ran');
			} finally {
				await release();
			}
			await app.idle();
			assert.equal(await page.locator(OPEN).count(), 0, 'the field opened again after its write');
			const [row] = JSON.parse((await app.run(['tail', 'pace', '-n', '1', '--json'])).stdout).rows;
			assert.equal(row.state, 'fast');
			// Refused, its pills stay, and the field is open again for another try.
			await openPace();
			release = await hold(page, ['change'], ['change']);
			try {
				await page.locator(`${OPEN} [data-state="slow"]`).click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
			} finally {
				await release();
			}
			await page.locator(`${OPEN} [data-state="slow"]`).waitFor();
		});
	});

	// The ghost's second press (ghost.test.mjs) found it on a loaded box (da-3ikhj): the field tapped again while its
	// state's write still ran shared that write's pill, which the write then took, closing the field under the next tap.
	test('a field tapped again while its state’s write waits stays open once it lands, and its own state’s write keeps it closed', async () => {
		await opening(['setup pace --verb switch --states false,slow,fast'], 'pace', async (app, button) => {
			const { page } = app;
			const STATE = (s) => page.locator(`${OPEN} [data-state="${s}"]`);
			const settle = () => page.evaluate(() => new Promise((r) => setTimeout(() => requestAnimationFrame(() => requestAnimationFrame(r)), 100)));
			const landOne = async () => (await page.evaluate(() => window.__held.shift().go()), settle());
			const writes = async () => (await held(page)).filter((a) => a[0] === 'change' && !a.includes('--dry-run')).length;
			await button.click();
			await STATE('fast').waitFor();
			await app.idle();
			let release = await hold(page, ['change']);
			try {
				// The field again while the write waits, and the write lands: the field stays open.
				await STATE('fast').click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				await button.click();
				await STATE('slow').waitFor();
				while (await page.evaluate(() => window.__held.length)) await landOne();
				assert.equal(await STATE('slow').count(), 1, 'the write that landed closed the field opened after it');
			} finally {
				await release();
			}
			// The field still open from above. A state, the field again, another state: two writes waiting, the first lands.
			release = await hold(page, ['change']);
			try {
				await STATE('slow').click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				await button.click();
				await STATE('fast').click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				await until(writes, (n) => n === 2);
				await landOne();
				assert.equal(await page.locator(OPEN).count(), 0, 'the field opened again while its second write waited');
			} finally {
				await release();
			}
			await app.idle();
			assert.equal(await page.locator(OPEN).count(), 0, 'the field opened again after its writes');
			// The last state tapped is the one pace is in (a held write reads the clock as it runs, so the two at once collide).
			assert.equal((await app.run(['get', 'pace::state'])).stdout.trim(), 'fast');
		});
	});

	test('a field opened again draws its listing at once and reads nothing, until a write to it, whose states come after the spec’s', async () => {
		await opening(['setup tone --verb switch --states false,low', 'switch tone mid -t 10:00', 'setup cup --verb occur'], 'tone', async (app, button) => {
			const { page } = app;
			const back = async () => {
				await page.locator(`${OPEN} [data-back]`).click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
			};
			await button.click();
			await page.locator(`${OPEN} [data-state="mid"]`).waitFor();
			await app.idle();
			await back();
			// Nothing written to it, another field only: drawn from what was kept, and not read again.
			await app.submit('moment cup -t 12:00');
			await app.idle();
			let release = await hold(page, ['states']);
			try {
				await button.click();
				await page.locator(`${OPEN} [data-state="mid"]`).waitFor();
				await app.idle();
				assert.deepEqual(await held(page), [], 'read again with nothing written to it');
				await back();
			} finally {
				await release();
			}
			// Written to: its spec’s at once, what it has used once read again.
			await app.submit('switch tone high -t 12:00');
			await app.idle();
			release = await hold(page, ['states']);
			try {
				await button.click();
				await page.locator(`${OPEN} [data-state="low"]`).waitFor();
				await page.waitForFunction(() => window.__held.some((h) => h.argv[0] === 'states'));
				assert.equal(await page.locator(`${OPEN} [data-state="mid"]`).count(), 0, 'a used state drawn from before the write');
			} finally {
				await release();
			}
			assert.deepEqual(await listed(page, 'mid'), ['off', 'low', 'high', 'mid']);
		});
	});

	// vividn, 2026-11-09: «pressing and holding on a state in the state view adds a pill for that state, which is good.
	// It should also enter into multi select mode, so if you tap on another state afterwords it starts forming an array
	// state» (da-laibn). A hold is a right click.
	describe('a state held begins multi-select', () => {
		const MOOD = ['setup mood --verb switch --states false,good,bad,meh'];
		const STATE = (page, s) => page.locator(`${OPEN} [data-state="${s}"]`);
		/** The bar's state pill, as it will be logged, or null. */
		const statePill = (page) => page.evaluate(() => window.datum.bar.pills().find((p) => p.kind === 'state')?.text ?? null);
		/** The states drawn ringed as chosen. */
		const ringed = (page) => page.locator(`${OPEN} [data-state].chosen`).evaluateAll((bs) => bs.map((b) => b.dataset.state));
		/** The writes run, not their dry runs. */
		const writes = (seen) => seen().then((all) => all.filter((a) => a[0] === 'change' && !a.includes('--dry-run')));
		const lastState = async (app) => JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows[0]?.state;

		test('a hold then two taps logs the three-state array from the bar, and nothing before', async () => {
			await opening(MOOD, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				await app.idle();
				const seen = await app.spyOnRunner();
				await STATE(page, 'good').click({ button: 'right' });
				await STATE(page, 'bad').click();
				await STATE(page, 'meh').click();
				await until(() => statePill(page), (t) => t === 'good,bad,meh');
				assert.deepEqual(await ringed(page), ['good', 'bad', 'meh']);
				assert.equal(await STATE(page, 'bad').getAttribute('data-act'), 'arm', 'a tap while gathering still says it runs');
				await app.idle();
				assert.deepEqual(await writes(seen), [], 'a tap while gathering logged');
				const toggle = page.locator('.prompt-bar button.prompt');
				if ((await toggle.getAttribute('aria-expanded')) !== 'true') await toggle.click();
				const lead = page.locator('.prompt-bar #bar-commands button.lead');
				await until(() => lead.getAttribute('data-ready'), (r) => r === 'true');
				await lead.click();
				await until(() => lastState(app), (s) => JSON.stringify(s) === '["good","bad","meh"]');
				assert.deepEqual((await writes(seen)).length, 1);
				await page.locator(OPEN).waitFor({ state: 'detached' });
			});
		});

		test('a tap on a chosen state takes it out, and the last one out ends it', async () => {
			await opening(MOOD, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				const seen = await app.spyOnRunner();
				await STATE(page, 'good').click({ button: 'right' });
				await STATE(page, 'bad').click();
				await until(() => statePill(page), (t) => t === 'good,bad');
				await STATE(page, 'good').click();
				await until(() => statePill(page), (t) => t === 'bad');
				assert.deepEqual(await ringed(page), ['bad']);
				await STATE(page, 'bad').click();
				await until(() => statePill(page), (t) => t === null);
				assert.deepEqual(await ringed(page), []);
				// Out of multi-select: a tap logs at once.
				await STATE(page, 'meh').click();
				await until(() => lastState(app), (s) => s === 'meh');
				assert.deepEqual((await writes(seen)).map((a) => a.includes('meh')), [true]);
			});
		});

		// vividn, 2026-11-10: «If you long press a state, then replace the end button with a button that shows that new
		// state and the transition arrow between them. Clicking this, then activates enters the data. / if there is an
		// after state (like when there is a duration), then display the after state with and arrow afterwards» (da-rou2i).
		const SEEDED = [...MOOD, 'switch mood bad -t 13:00'];
		const TRANSITION = (page) => page.locator(`${HEAD} [data-transition]`);
		/** The transition's legs, each its states, `off` for off. */
		const legs = (page) => TRANSITION(page).locator('[data-leg]').evaluateAll((ls) => ls.map((l) => l.dataset.leg));

		test('a hold on a state shows the state now → it in the state now’s place, which is drawn once (da-qryu8), its pill on the bar; a tap on it logs the change', async () => {
			await opening(SEEDED, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				assert.equal(await TRANSITION(page).count(), 0, 'a transition before any hold');
				assert.equal(await page.locator(`${HEAD} [data-current]`).textContent(), 'bad');
				await STATE(page, 'good').click({ button: 'right' });
				await TRANSITION(page).waitFor();
				assert.deepEqual(await legs(page), ['bad', 'good']);
				assert.equal(await TRANSITION(page).textContent(), 'bad→good');
				assert.equal(await page.locator(`${HEAD} [data-current]`).count(), 0, 'the state now beside the transition, which starts with it');
				assert.equal(await page.locator(HEAD).innerText().then((t) => t.split('bad').length - 1), 1, 'the state now drawn twice');
				assert.equal(await page.locator(`${HEAD} [data-off]`).count(), 0, 'off in the first row');
				assert.equal(await statePill(page), 'good');
				await TRANSITION(page).click();
				await until(() => lastState(app), (s) => s === 'good');
				await page.locator(OPEN).waitFor({ state: 'detached' });
			});
		});

		test('gathering a second state still works: the transition takes both, and logs the array', async () => {
			await opening(SEEDED, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				await STATE(page, 'good').click({ button: 'right' });
				await STATE(page, 'meh').click();
				await until(() => statePill(page), (t) => t === 'good,meh');
				await until(() => legs(page), (l) => JSON.stringify(l) === '["bad","good,meh"]');
				await TRANSITION(page).click();
				await until(() => lastState(app), (s) => JSON.stringify(s) === '["good","meh"]');
			});
		});

		test('the last state out of the hold brings the state now back in the transition’s place', async () => {
			await opening(SEEDED, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				await STATE(page, 'good').click({ button: 'right' });
				await TRANSITION(page).waitFor();
				await STATE(page, 'good').click();
				await until(() => statePill(page), (t) => t === null);
				await page.locator(`${HEAD} [data-current]`).waitFor();
				assert.equal(await TRANSITION(page).count(), 0);
			});
		});

		test('with a length on the bar, the transition goes on to the state it returns to: now → it → back', async () => {
			await opening(SEEDED, 'mood', async (app, button) => {
				const { page } = app;
				await page.evaluate(() => window.datum.bar.setPills([{ id: 'len', kind: 'dur', text: '30m' }]));
				await button.click();
				await STATE(page, 'meh').waitFor();
				await STATE(page, 'good').click({ button: 'right' });
				await until(() => legs(page), (l) => JSON.stringify(l) === '["bad","good","bad"]');
				assert.equal(await TRANSITION(page).textContent(), 'bad→good→bad');
				await TRANSITION(page).click();
				// What was logged is what the transition drew: the tail's three states of the row.
				const row = async () => JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows[0];
				await until(row, (r) => r.state === 'good');
				const { previous_state, state, following_state, dur_seconds } = await row();
				assert.deepEqual([previous_state, state, following_state, dur_seconds], ['bad', 'good', 'bad', 1800]);
			});
		});

		test('back, its pill, leaves no selection behind, and a plain tap after logs that one state at once', async () => {
			await opening(MOOD, 'mood', async (app, button) => {
				const { page } = app;
				await button.click();
				await STATE(page, 'meh').waitFor();
				await STATE(page, 'good').click({ button: 'right' });
				await STATE(page, 'bad').click();
				await until(() => statePill(page), (t) => t === 'good,bad');
				await page.locator(`${OPEN} [data-back]`).click();
				await page.locator(OPEN).waitFor({ state: 'detached' });
				assert.equal(await statePill(page), null, 'the array stayed on the bar');
				await button.click();
				await STATE(page, 'meh').waitFor();
				assert.deepEqual(await ringed(page), []);
				assert.equal(await STATE(page, 'meh').getAttribute('data-act'), 'run');
				await STATE(page, 'meh').click();
				await until(() => lastState(app), (s) => s === 'meh');
			});
		});
	});
});
