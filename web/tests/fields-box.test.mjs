// The pane the collect widget lives in, and the size it keeps.
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

	// What reaches a button past the pane is the cluster's drag (da-w6437),
	// `fields-center.test.mjs`.
});
