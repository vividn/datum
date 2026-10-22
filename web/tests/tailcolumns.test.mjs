// The tail pane's columns in a narrow pane (da-kk0d).
//
// vividn, 2026-09-17: «Sacrifice the _n and hid columns in the tail view at
// smaller widths» — `hid` first, `_n` only if the table still does not fit
// without it. On a phone the two took two of five columns and broke
// `field` and `state` mid-word.
//
// Too narrow is measured: the pane's hidden width probe (TailTable.svelte)
// draws every column at its narrowest with words whole, and the widths a
// test sizes the pane by are read off it, never written down. A broken word
// is a row taller than the one-line probe's row.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, sharedApp } from './support/app.mjs';
import { widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const SEED = ['switch activity working', 'start music', 'occur coffee'];

/** The `main` tab as one tail pane, so the pane's width is the viewport's less its frame. */
const ONLY_TAIL = JSON.stringify({ version: 1, name: 'main', root: widgetPane('only', 'tail') });

/** Store `lines` through the seam's quiet door, then redraw: these layouts have no REPL to type into. */
async function seedLines(app, lines) {
	for (const line of lines) await app.runLine(line);
	await app.redraw();
}

const TAIL = 'section.pane[data-title="tail"]';

/** The tail as drawn: its box, header keys, rows, whether any row is taller than a line, and the probe's widths. */
const table = (page) =>
	page.evaluate((sel) => {
		const pane = document.querySelector(sel);
		const body = pane.querySelector('.body');
		const style = getComputedStyle(body);
		const line = pane.querySelector('table.probe:not(.wide) tfoot:last-child tr').getBoundingClientRect().height;
		const trs = [...pane.querySelectorAll('tbody tr')];
		// What each set of columns needs, as the component reckons it: the
		// cells' widths, less the right padding the last one drawn drops.
		const cells = [...pane.querySelectorAll('div.probe.wide > .tr:first-child > .cell')];
		const needs = (drop) => {
			const set = cells.filter((c) => !drop.includes(c.dataset.key));
			const sum = set.reduce((s, c) => s + c.getBoundingClientRect().width, 0);
			return sum - parseFloat(getComputedStyle(set.at(-1)).paddingRight);
		};
		return {
			width: body.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
			keys: [...pane.querySelectorAll('thead th')].map((th) => th.dataset.key),
			hids: trs.map((tr) => tr.dataset.hid),
			tall: trs.filter((tr) => tr.getBoundingClientRect().height > line + 0.5).map((tr) => tr.textContent),
			sideways: body.scrollWidth > body.clientWidth + 1,
			needs: { all: needs([]), noHid: needs(['hid']), neither: needs(['hid', '_n']) }
		};
	}, TAIL);

/** The tail as drawn, once its width probe has measured every column. */
async function measured(page) {
	await page.waitForFunction(
		(sel) =>
			window.__datumInflight === 0 &&
			[...document.querySelectorAll(`${sel} div.probe.wide > .tr:first-child > .cell`)].every(
				(cell, _, all) => all.length > 0 && cell.getBoundingClientRect().width > 0
			),
		TAIL
	);
	return table(page);
}

/** Wait until the tail, at rest, draws the columns `keys`; say what it drew if it does not. */
const drawsKeys = (page, keys) =>
	page
		.waitForFunction(
			([sel, keys]) =>
				window.__datumInflight === 0 &&
				JSON.stringify([...document.querySelectorAll(`${sel} thead th`)].map((th) => th.dataset.key)) === JSON.stringify(keys),
			[TAIL, keys]
		)
		.catch(async (e) => {
			throw new Error(`not ${keys}: ${JSON.stringify(await table(page))}`, { cause: e });
		});

/** Resize the viewport so the tail's content box is `width` wide. */
async function paneWidth(page, width) {
	const { width: now } = await table(page);
	const viewport = page.viewportSize();
	await page.setViewportSize({ width: viewport.width + Math.round(width - now), height: viewport.height });
	await page.waitForFunction(
		([sel, width]) => {
			const body = document.querySelector(`${sel} .body`);
			const style = getComputedStyle(body);
			return Math.abs(body.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - width) < 1;
		},
		[TAIL, width]
	);
}

const ALL = ['_n', 'time', 'field', 'state', 'hid'];
const NO_HID = ['_n', 'time', 'field', 'state'];
const THREE = ['time', 'field', 'state'];

/** The columns a box `width` wide draws, by the probe's `needs`: the widest set that fits. */
const drops = (needs, width) => (needs.all <= width ? ALL : needs.noHid <= width ? NO_HID : THREE);

// ONE app for the tests on the tail alone (sharedApp in support/app.mjs),
// `main` opened as a draft of that one pane: each seeds and then REDRAWS
// through the change channel (da-s4i98), so a test is a redraw and not a
// boot. A test that wants another screen resizes to it; `home` puts it back.
// The phones open apps of their own, FRESH: a touch device is the context's.
const shared = sharedApp({ viewport: { width: 400, height: 700 } });
const onTail = (fn) => shared.solo(fn, ONLY_TAIL);
describe('a narrow tail gives up hid, then _n', () => {
	test('at 250px: hid goes, no word broken, every row keeps its hid', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await paneWidth(page, 250);
			const { needs } = await measured(page);
			assert.ok(needs.all > 250, `all five fit 250px (${needs.all}px), which tests nothing`);
			await drawsKeys(page, drops(needs, 250));
			const t = await table(page);
			assert.ok(!t.keys.includes('hid'), `hid is drawn: ${t.keys}`);
			assert.ok(t.hids.length >= SEED.length, `rows: ${t.hids}`);
			assert.ok(t.hids.every((hid) => hid), 'a row without its data-hid');
			assert.deepEqual(t.tall, [], 'a row taller than one line');
			assert.ok(!t.sideways, 'the pane scrolls sideways');
			assert.deepEqual(app.errors, []);
		});
	});

	test('narrower than _n needs: time, field, state', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const { needs } = await measured(page);
			await paneWidth(page, Math.ceil(needs.neither) + 1);
			await drawsKeys(page, THREE);
			const t = await table(page);
			assert.ok(t.hids.every((hid) => hid), 'a row without its data-hid');
			assert.deepEqual(t.tall, [], 'a row taller than one line');
			assert.ok(!t.sideways, 'the pane scrolls sideways');
		});
	});

	test('at 420px: all five columns', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await paneWidth(page, 420);
			await drawsKeys(page, ALL);
			const t = await table(page);
			assert.deepEqual(t.tall, [], 'a row taller than one line');
		});
	});

	test('between the two: _n stays, hid goes', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await page.setViewportSize({ width: 600, height: 700 });
			await seedLines(app, SEED);
			const { needs } = await measured(page);
			assert.ok(needs.all - needs.noHid > 4, `no room between the two widths: ${JSON.stringify(needs)}`);
			await paneWidth(page, Math.ceil(needs.noHid) + 1);
			await drawsKeys(page, NO_HID);
			assert.deepEqual((await table(page)).tall, [], 'a row taller than one line');
		});
	});

	test('across the boundary both ways: the columns go and come back, the reader keeps their place', async () => {
		await onTail(async (app) => {
			const { page } = app;
			await page.setViewportSize({ width: 600, height: 400 });
			await seedLines(app, Array.from({ length: 60 }, (_, i) => `occur coffee -t 05${String(i).padStart(2, '0')}`));
			const { needs } = await measured(page);
			const wide = Math.ceil(needs.all) + 20;
			const narrow = Math.ceil(needs.neither) + 2;
			await paneWidth(page, wide);
			await drawsKeys(page, ALL);

			// The whole log is held from the first listing, the page above
			// the screen with it (da-gtk48), and is three screens and more:
			// scroll to the middle, scrolled up, so a resize keeps the
			// place, not the live end.
			const held = await until(
				() =>
					page.evaluate((sel) => {
						const body = document.querySelector(`${sel} .body`);
						return { rows: body.querySelectorAll('tbody tr').length, scrollHeight: body.scrollHeight, clientHeight: body.clientHeight };
					}, TAIL),
				(seen) => seen.rows === 60
			);
			assert.ok(held.scrollHeight >= 3 * held.clientHeight, `the log is not three screens: ${JSON.stringify(held)}`);
			// The scroll event lands on a frame: let it, before the resize.
			await page.evaluate(async (sel) => {
				document.querySelector(`${sel} .body`).scrollTop = 200;
				for (let i = 0; i < 2; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
			}, TAIL);
			/** The reader's row: the first whole one under the header, and where it is. */
			const at = await page.evaluate((sel) => {
				const body = document.querySelector(`${sel} .body`);
				const top = body.getBoundingClientRect().top;
				const tr = [...body.querySelectorAll('tbody tr')].find((tr) => tr.getBoundingClientRect().top > top + 40);
				return { hid: tr.dataset.hid, y: tr.getBoundingClientRect().top - top };
			}, TAIL);
			const y = () =>
				page.evaluate(
					([sel, hid]) => {
						const body = document.querySelector(`${sel} .body`);
						const tr = body.querySelector(`tbody tr[data-hid="${hid}"]`);
						return tr ? tr.getBoundingClientRect().top - body.getBoundingClientRect().top : null;
					},
					[TAIL, at.hid]
				);
			// Put back once the box lands and the columns change: polled.
			const kept = (seen) => seen !== null && Math.abs(seen - at.y) < 2;

			await paneWidth(page, narrow);
			await drawsKeys(page, THREE);
			await until(y, kept);

			await paneWidth(page, wide);
			await drawsKeys(page, ALL);
			await until(y, kept);
		});
	});

	for (const [width, height] of [
		[390, 664],
		[360, 640]
	]) {
		// What the rule is, at a phone's width: the widest set of columns
		// whose widths the box holds. Which set that IS depends on the box
		// AND on the type, and da-hkjp made `mobile`'s tail type smaller —
		// so at 390×664 all five now fit where they did not, and at 360×640
		// `hid` still goes. Both are the same rule, so the test asserts the
		// rule and lets the MEASUREMENT say which side of it this size is on
		// rather than writing the answer down per size.
		test(`the mobile layout at ${width}x${height}: no hid, no word broken`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seedLines(app, SEED);
					const { needs, width: box } = await measured(page);
					await drawsKeys(page, drops(needs, box));
					const t = await table(page);
					assert.ok(t.hids.length > 0, 'no rows');
					assert.deepEqual(t.tall, [], 'a row taller than one line');
					assert.ok(!t.sideways, 'the pane scrolls sideways');
				},
				{ viewport: { width, height }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT }
			);
		});
	}
});

// The header keeps its height (da-1o33i). vividn, 2026-10-06: «The header row
// changes its height quite a bit as you are scrolling when it should remain
// fixed (I notice "field" sometimes wraps to "fiel\nd")». One row with a long
// state let the table squeeze `field` to a character a line, and the header
// with it, so the header grew and shrank as such rows paged in and out.
describe('the tail header keeps its height', () => {
	/** The header's height, how many lines each `th` draws, and whether the pane scrolls sideways. */
	const header = (page) =>
		page.evaluate((sel) => {
			const pane = document.querySelector(sel);
			const body = pane.querySelector('.body');
			const lines = (th) => {
				const range = document.createRange();
				range.selectNodeContents(th);
				return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
			};
			return {
				height: pane.querySelector('thead').getBoundingClientRect().height,
				lines: Object.fromEntries([...pane.querySelectorAll('thead th')].map((th) => [th.dataset.key, lines(th)])),
				sideways: body.scrollWidth > body.clientWidth + 1
			};
		}, TAIL);

	// Each a minute of its own: the app's clock is held, and a second
	// `switch work` at the same minute is refused.
	const LONG = [
		'switch work "deep in the long refactoring of the parser input module, then reviewing the rest" -t -3',
		'occur medication_evening_dose -t -2',
		`switch work ${'x'.repeat(90)} -t -1`
	];

	/** Store `lines`, each refused loudly, then redraw. */
	async function store(app, lines) {
		for (const line of lines) assert.equal((await app.runLine(line)).code, 0, `\`${line}\` refused`);
		await app.redraw();
	}

	for (const [name, options] of [
		['desktop 1280', { viewport: { width: 1280, height: 720 } }],
		['phone 390', { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true } }]
	]) {
		test(`${name}: a long state, a long field name, an unbroken state`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await store(app, ['switch work focus -t -5', 'occur coffee -t -4']);
					await measured(page);
					const short = await header(page);
					for (const line of LONG) {
						await store(app, [line]);
						await measured(page);
						const h = await header(page);
						assert.ok(Object.values(h.lines).every((n) => n === 1), `after ${line}: a header wraps: ${JSON.stringify(h.lines)}`);
						assert.ok(Math.abs(h.height - short.height) < 0.5, `after ${line}: header ${h.height}px, ${short.height}px with short rows`);
						assert.ok(!h.sideways, `after ${line}: the pane scrolls sideways`);
					}
					assert.deepEqual(app.errors, []);
				},
				{ ...options, home: ON_COLLECT, desk: null }
			);
		});
	}
});
