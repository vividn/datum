// The nowview widget's − / + (da-pcis): they step how far back the picture
// reaches, down to 0 — the snapshot — and the step is the widget's setting,
// kept in the draft and in a saved layout.
//
// Delta-blind on purpose: each step is held to the label the widget shows
// and to the `--history` the seam was handed, never to "the picture changed".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const PANE = 'section.pane[data-title="nowview"]';
const label = (page) => page.locator(`${PANE} .nowview-steps span`).innerText();
const shorter = (page) => page.locator(`${PANE} button[aria-label="shorter window"]`);
const longer = (page) => page.locator(`${PANE} button[aria-label="longer window"]`);

/** Wait for the widget to say `text`. */
const says = (page, text) =>
	page.waitForFunction(
		([sel, text]) => document.querySelector(sel)?.textContent === text,
		[`${PANE} .nowview-steps span`, text]
	);

/** The `--history` of the last nowview the seam was handed. */
function lastHistory(seen) {
	const argv = seen.filter((a) => a[0] === 'nowview').at(-1);
	return argv?.[argv.indexOf('--history') + 1];
}

/** Every nowview widget's settings in a layout tree. */
function nowviewSettings(node, out = []) {
	if (node?.kind === 'nowview') out.push(node.settings);
	for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) {
		nowviewSettings(child.content ?? child, out);
	}
	return out;
}

describe('the nowview widget', () => {
	test('steps its window with − and +, through the seam, down to the snapshot and up to 12h', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal(await label(page), '30m');
			const seen = await app.spyOnRunner();

			await longer(page).click();
			await says(page, '45m');
			await app.idle();
			assert.equal(lastHistory(await seen()), '45m');

			// 45m → 30m, 20m, 15m, 10m, 5m, 0.
			for (let i = 0; i < 6; i++) await shorter(page).click();
			await says(page, 'now');
			await app.idle();
			assert.equal(lastHistory(await seen()), '0');
			assert.ok(await shorter(page).isDisabled(), '− is live at 0');
			const snapshot = await app.paneSvg('nowview');
			assert.ok(!snapshot.includes('tick-label') && !snapshot.includes('class="axis"'), snapshot);

			for (let i = 0; i < 12; i++) await longer(page).click();
			await says(page, '12h');
			await app.idle();
			assert.equal(lastHistory(await seen()), '12h');
			assert.ok(await longer(page).isDisabled(), '+ is live at 12h');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the step is kept in the draft and in a saved layout', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await longer(page).click();
			await says(page, '45m');

			await app.reload();
			assert.equal(await label(page), '45m', 'the step did not survive a reload');

			await page.locator('.layout-controls .save:not([disabled])').click();
			await page.waitForFunction(() => !document.querySelector('.layout-controls.modified'));
			const other = await app.newTab();
			const saved = await other.page.evaluate(() => window.datum.layouts.loadLayout('default'));
			assert.deepEqual(
				nowviewSettings(saved.root).map((s) => s.history),
				['45m']
			);
		});
	});

	test('at 360px the steps do not widen the pane', async () => {
		await withApp(
			async ({ page }) => {
				const tab = page.locator('[role="tab"]', { hasText: 'nowview' });
				if (await tab.count()) await tab.click();
				await page.locator(`${PANE} img.svg`).waitFor();
				const box = await page.locator(`${PANE} .body`).evaluate((b) => ({
					scroll: b.scrollWidth,
					client: b.clientWidth
				}));
				assert.ok(box.scroll <= box.client, `nowview scrolls sideways: ${JSON.stringify(box)}`);
				assert.ok(await longer(page).isVisible());
			},
			{ viewport: { width: 360, height: 640 } }
		);
	});
});
