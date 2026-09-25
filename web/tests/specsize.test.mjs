// The spec modal is one size across its tabs (da-laxpn). vividn, 2026-09-25:
// «The spec modal should stay the same size across different tabs».
//
// A centred `<dialog>` whose height followed its content re-centred on
// every tab press, so the tabs jumped. The field here has a spec AND
// states, so no tab is disabled; each tab is measured once its own content
// is in the box, polled from node (da-1grm7).

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, sharedApp, startApp, stopApp } from './support/app.mjs';
import { until } from './support/until.mjs';

const MODAL = 'dialog.spec';
const TABS = ['basics', 'states', 'keys', 'aliases', 'placement'];

describe('the spec modal', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();

	test('keeps one width and one height on every tab', async (t) => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			const spec = JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout);
			const key = { required: false, value_type: 'any', in_slug: false, default: null, unit: null };
			const r = await app.run(['spec', 'set', 'mood', '--from-json', JSON.stringify({
				...spec,
				allowed_states: ['happy', 'sad', 'tired'],
				subfields: { where: key, who: key }
			})]);
			assert.equal(r.code, 0, r.stderr);

			await atEachScreen(t, page, [{ width: 390, height: 844 }, { width: 1280, height: 900 }], async () => {
				const closed = page.evaluate(() => window.datum.spec('mood'));
				await page.locator(MODAL).waitFor();
				const sizes = {};
				for (const tab of TABS) {
					await page.locator(`${MODAL} [data-tab="${tab}"]`).click();
					sizes[tab] = await until(
						() => page.locator(MODAL).evaluate((d, tab) => {
							const panel = d.querySelector('.panel');
							const box = d.getBoundingClientRect();
							return panel.dataset.panel === tab && panel.childElementCount > 0
								? { width: box.width, height: box.height, overflow: panel.scrollHeight - panel.clientHeight }
								: null;
						}, tab),
						(seen) => seen !== null
					);
					// The panel is named by its tab, and the tab controls it (da-p2l88).
					assert.equal(await page.locator(MODAL).getByRole('tabpanel', { name: tab, exact: true }).count(), 1, `no tabpanel named ${tab}`);
					assert.equal(await page.locator(MODAL).getByRole('tab', { name: tab, exact: true }).getAttribute('aria-controls'), 'spec-panel');
				}
				// The height is the tallest tab at rest (Basics, 506px at 390px
				// wide): if it now scrolls, measure again and raise the 32rem.
				assert.ok(sizes.basics.overflow <= 1, `basics overflows its panel by ${sizes.basics.overflow}px`);
				const [first, ...rest] = TABS.map((tab) => sizes[tab]);
				for (const [i, size] of rest.entries()) {
					assert.ok(Math.abs(size.width - first.width) <= 1, `${TABS[i + 1]} width ${size.width} vs ${first.width}`);
					assert.ok(Math.abs(size.height - first.height) <= 1, `${TABS[i + 1]} height ${size.height} vs ${first.height}`);
				}
				await page.keyboard.press('Escape');
				assert.equal(await closed, null);
			});
		});
	});
});
