// The nowview widget's + / − (da-pcis): they step how far back the picture
// reaches, down to 0 — the snapshot — and the step is the widget's setting,
// kept in the draft and in a saved layout. They sit at the picture's
// top-left, + first, with no label: each one's title names where it goes
// (da-rex1).
//
// Delta-blind on purpose: each step is held to the `--history` the seam was
// handed and to the buttons' titles, never to "the picture changed".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const PANE = 'section.pane[data-title="nowview"]';
const shorter = (page) => page.locator(`${PANE} button[aria-label="shorter window"]`);
const longer = (page) => page.locator(`${PANE} button[aria-label="longer window"]`);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to be `--history h`. */
const asked = (page, h) =>
	page.waitForFunction((h) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		return argv?.[argv.indexOf('--history') + 1] === h;
	}, h);

/** The two buttons' titles, + then −. */
const titles = (page) =>
	Promise.all([longer(page).getAttribute('title'), shorter(page).getAttribute('title')]);

/** Every nowview widget's settings in a layout tree. */
function nowviewSettings(node, out = []) {
	if (node?.kind === 'nowview') out.push(node.settings);
	for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) {
		nowviewSettings(child.content ?? child, out);
	}
	return out;
}

describe('the nowview widget', () => {
	test('sit at the top-left, + then −, with no label', async () => {
		await withApp(async ({ page }) => {
			const steps = page.locator(`${PANE} .nowview-steps`);
			const [box, content] = await Promise.all([
				steps.boundingBox(),
				page.locator(`${PANE} .nowview`).boundingBox()
			]);
			assert.ok(box.x - content.x <= 8, `steps at ${box.x}, content at ${content.x}`);
			// The title lies over the content's top strip (da-13bc): the steps
			// sit below it, not under it.
			const title = await page.locator(`${PANE} h2`).boundingBox();
			assert.ok(box.y >= title.y + title.height, `steps at ${box.y}, the title ends at ${title.y + title.height}`);
			assert.equal(await steps.locator('span').count(), 0);
			assert.deepEqual(
				await steps.locator('button').evaluateAll((bs) => bs.map((b) => [b.textContent, b.ariaLabel])),
				[
					['+', 'longer window'],
					['−', 'shorter window']
				]
			);
		});
	});

	test('steps its window with + and −, through the seam, down to the snapshot and up to 12h', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await titles(page), ['longer window (45m)', 'shorter window (20m)']);
			await app.spyOnRunner();

			await longer(page).click();
			await asked(page, '45m');
			assert.deepEqual(await titles(page), ['longer window (60m)', 'shorter window (30m)']);

			// 45m → 30m, 20m, 15m, 10m, 5m, 0.
			for (let i = 0; i < 5; i++) await shorter(page).click();
			await asked(page, '5m');
			assert.deepEqual(await titles(page), ['longer window (10m)', 'shorter window (now)']);
			await shorter(page).click();
			await asked(page, '0');
			await app.idle();
			assert.ok(await shorter(page).isDisabled(), '− is live at 0');
			assert.deepEqual(await titles(page), ['longer window (5m)', 'shorter window']);
			const snapshot = await app.paneSvg('nowview');
			assert.ok(!snapshot.includes('tick-label') && !snapshot.includes('class="axis"'), snapshot);

			for (let i = 0; i < 12; i++) await longer(page).click();
			await asked(page, '12h');
			assert.ok(await longer(page).isDisabled(), '+ is live at 12h');
			assert.deepEqual(await titles(page), ['longer window', 'shorter window (6h)']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the step is kept in the draft and in a saved layout', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			await longer(page).click();
			await asked(page, '45m');

			await app.reload();
			assert.deepEqual(
				await titles(page),
				['longer window (60m)', 'shorter window (30m)'],
				'the step did not survive a reload'
			);

			// The root bar's save icon, on the `day` tab (da-0a5o).
			await page.locator('.root-bar .act.save:not([disabled])').click();
			await page.waitForFunction(() => !document.querySelector('.root-bar.modified'));
			const other = await app.newTab();
			const saved = await other.page.evaluate(() => window.datum.layouts.loadLayout('day'));
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
				await page.locator(`${PANE} svg.svg`).waitFor();
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
