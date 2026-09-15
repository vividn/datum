// The fields widget as it fits a screen, and as it reads on one (da-c7u8):
// four tests, each of which draws the whole widget at a size of its own.
//
// The six: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box), `fields-states` (a
// state field's states) and `fields-bubbles` (the long-press panel's
// bubbles against that list). What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, the six at concurrency 3 on
// the 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, bubbles 35s, box 36s — the one file was 215s
// serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { CLEAR, FIXTURE, type, untilFieldStyle } from './support/fields.mjs';

/**
 * Every label in the widget, with the contrast its ink actually has.
 *
 * Contrast, not colour: the bug this measures (da-qnvh) was a chip drawn
 * in two colours that were the same colour, and every assertion the suite
 * had said the right name for each of them. Only the ratio between them
 * said anything was wrong.
 *
 * `behind` walks UP for the first ancestor that paints. A chip's own
 * background is `transparent` whenever it is outlined, and reading the
 * element alone measures the label against nothing and calls it fine —
 * which is most of how this was missed.
 */
const contrasts = (page) =>
	page.locator('.bar').evaluate((bar) => {
		const parse = (color) => (color.match(/[\d.]+/g) ?? []).map(Number);
		const channel = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
		const luminance = ([r, g, b]) =>
			0.2126 * channel(r / 255) + 0.7152 * channel(g / 255) + 0.0722 * channel(b / 255);
		const behind = (el) => {
			for (let node = el; node; node = node.parentElement) {
				const bg = parse(getComputedStyle(node).backgroundColor);
				if (bg[3] === undefined || bg[3] > 0) return bg.slice(0, 3);
			}
			return [0, 0, 0];
		};
		const ratio = (fg, bg) => {
			const [light, dark] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
			return (light + 0.05) / (dark + 0.05);
		};
		return [...bar.querySelectorAll('button.track, [data-field]')].map((el) => {
			const fg = parse(getComputedStyle(el).color).slice(0, 3);
			const bg = behind(el);
			return {
				label: el.dataset.field ?? el.textContent.trim(),
				fg: `rgb(${fg})`,
				bg: `rgb(${bg})`,
				contrast: Math.round(ratio(fg, bg) * 100) / 100
			};
		});
	});

/** The measurements as the bead writes them, for a failure to print. */
const table = (rows) =>
	rows.map((r) => `  ${r.label.padEnd(10)} ${r.fg.padEnd(20)} ${r.bg.padEnd(20)} ${r.contrast}`).join('\n');

describe('the widget fits its screen, legibly', () => {
	before(startApp);
	after(stopApp);

	// The four kinds, with NO colours set except the one state colour that
	// tells a coloured state from an uncoloured one. The default case is
	// the broken one: a field tracked today has no `state_colors`, and a
	// switch in a state then filled itself with the field's colour and
	// wrote the label in that same colour (da-qnvh).
	const PLAIN = [
		'spec set pushups --default-verb occur',
		'spec set work --default-verb start',
		'spec set mood --default-verb switch --state good --state bad',
		'spec set energy --default-verb switch --state high --state low --state-color high=#008000',
		'start work',
		'switch mood good',
		'switch energy high',
		'occur untracked'
	];

	test('every field chip clears 4.5:1 against what is behind it, the uncoloured switch included', async () => {
		await withApp(async ({ page }) => {
			for (const line of PLAIN) await type(page, line);
			await page.locator('.bar [data-field="untracked"]').waitFor();
			// No spec colour on `work`: resolved server-side to md5("work"),
			// not the app's accent (da-yiyj).
			await untilFieldStyle(page, 'work', 'backgroundColor', 'rgb(103, 233, 44)');

			const on = await contrasts(page);
			assert.deepEqual(
				on.map((r) => r.label),
				['Track +', 'energy', 'mood', 'pushups', 'untracked', 'work'],
				'all four kinds have to be in the measurement'
			);
			for (const row of on) {
				assert.ok(row.contrast >= 4.5, `${row.label} is at ${row.contrast}:1\n${table(on)}`);
			}

			// And outlined: a block that is off is the one chip whose fill is
			// on nothing, so its label is measured against the pane.
			await type(page, 'end work');
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
			const off = await contrasts(page);
			for (const row of off.filter((r) => !r.dim)) {
				assert.ok(row.contrast >= 4.5, `${row.label} is at ${row.contrast}:1 with work off\n${table(off)}`);
			}
		});
	});

	for (const viewport of [
		{ width: 390, height: 844 },
		{ width: 320, height: 664 },
		{ width: 1280, height: 800 }
	]) {
		test(`at ${viewport.width}x${viewport.height} the widget wraps, every button ≥ 24px, Track + first in the Tab order`, async () => {
			await withApp(async ({ page }) => {
				for (const line of [...FIXTURE, 'occur tea', 'occur water', 'occur run']) await type(page, line);
				await page.locator('.bar [data-field="run"]').waitFor();
				const bar = await page.locator('.bar').evaluate((el) => ({
					width: el.scrollWidth <= el.clientWidth,
					rows: new Set([...el.children].map((c) => Math.round(c.getBoundingClientRect().top))).size,
					heights: [...el.querySelectorAll('button')].map((b) => b.getBoundingClientRect().height)
				}));
				assert.ok(bar.width, 'the widget scrolls sideways rather than wrapping');
				if (viewport.width < 500) assert.ok(bar.rows > 1, 'the widget did not wrap');
				for (const h of bar.heights) assert.ok(h >= 24, `a button is ${h}px tall`);

				const first = await page.locator('.bar').evaluate((el) => {
					const focusable = [...el.querySelectorAll('button, input, [tabindex]')].filter((b) => !b.disabled && b.tabIndex >= 0);
					return focusable[0]?.textContent.trim();
				});
				assert.equal(first, 'Track +');
			}, { viewport });
		});
	}
});
