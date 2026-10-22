// The pane the fields widget lives in, and the size it keeps.
//
// The five: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box) and `fields-states` (a
// state field's states). There was a sixth, `fields-bubbles`, over the
// hold panel's `-t` / `dur` bubbles; da-e78x6 removed the panel and the
// file with it. What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): on node 22 a FILE was the test `run()` timed
// (da-l5k9e: CI now runs node 24), so 120 s was this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, at concurrency 3 on the
// 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, box 36s (and bubbles, now gone, 35s) — the one
// file was 215s serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { type } from './support/fields.mjs';

// The pane the widget lives in keeps the height its layout gives it
// (da-mbz2). It used to size itself to its content — the only kind that
// did — so every field tracked pushed the rest of the page up a little,
// and da-d08c, which draws a button for EVERY field, would have made that
// a shove. Now the box is the split's to decide and the bar scrolls
// inside it.
describe("the fields pane's box", () => {
	before(startApp);
	after(stopApp);

	/** The pane's border box, as the reader sees it. */
	const boxOf = (page) =>
		page.locator('section[data-pane="bar"]').evaluate((el) => {
			const { width, height } = el.getBoundingClientRect();
			return { width: Math.round(width), height: Math.round(height) };
		});

	const TEN = ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel', 'india', 'juliett'];

	test('tracking ten more fields does not change it', async () => {
		await withApp(async ({ page }) => {
			await type(page, 'setup coffee --verb occur');
			await page.locator('.bar [data-field="coffee"]').waitFor();
			const before = await boxOf(page);

			for (const field of TEN) await type(page, `setup ${field} --verb occur`);
			await page.locator('.bar [data-field="juliett"]').waitFor();

			assert.deepEqual(await boxOf(page), before, 'the pane grew with its buttons');
		});
	});

	// A phone, where the pane is short enough that a handful of fields
	// overflows it — the shape da-d08c makes ordinary.
	//
	// The scroller is the BAR (da-ltux), and which way it goes is the
	// scroll rule's — sideways today, down once a pivot asks for it
	// (da-vu7pi's `axis`). The claim is the same either way — the last
	// button is reachable, not clipped away — so it is made along whichever
	// axis the bar chose.
	test('a button past the end of the bar is scrolled to and pressed', async () => {
		await withApp(async ({ page }) => {
			for (const field of TEN) await type(page, `setup ${field} --verb occur`);
			await page.locator('.bar [data-field="juliett"]').waitFor();

			const bar = page.locator('section[data-pane="bar"] .bar');
			const over = (el) =>
				el.scrollWidth > el.clientWidth + 1 ? 'x' : el.scrollHeight > el.clientHeight + 1 ? 'y' : null;
			const axis = await bar.evaluate(over);
			assert.ok(axis, 'the bar fits its pane — there is nothing this test can prove');

			const echoes = await page.locator('.repl .echo').count();
			// Playwright refuses to click what is out of view or covered, and
			// scrolls its scrollable ancestor to reach it: the press is the
			// assertion that the button is reachable, not clipped away.
			await page.locator('.bar button.widget').last().click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);
			assert.ok(
				await bar.evaluate((el, a) => (a === 'x' ? el.scrollLeft : el.scrollTop) > 0, axis),
				'the bar never scrolled to reach it'
			);
		}, { viewport: { width: 390, height: 640 } });
	});
});
