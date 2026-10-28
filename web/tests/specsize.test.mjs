// The field form is one size across its tabs (da-laxpn). vividn, 2026-09-25:
// «The spec modal should stay the same size across different tabs».
//
// A centred `<dialog>` whose height followed its content re-centred on
// every tab press, so the tabs jumped. The field here has a spec AND
// states, so no tab is disabled; each tab is measured once its own content
// is in the box, polled from node (da-1grm7).
//
// It is a place now (da-hbrfg): a tab on a phone, the place modal on a
// desktop (da-jozgv), and the box is the host's rather than the dialog's
// 38rem. A tab taller than the box scrolls with the shapes and name over
// it (da-uz2td), and the tabs and Done stay where they are, so Done in view
// is what is held.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, sharedApp, startApp, stopApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { FIELD_FORM as MODAL, leaveField, openField } from './support/field.mjs';
const TABS = ['basics', 'data', 'aliases'];

describe('the field form', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();
	// `main` as a draft of ONE CLI widget: nothing here reads the day frame,
	// and drawing it costs what a boot did (da-m1m9a).
	const onSeam = (fn) => shared.solo(fn, ONE_PANE);

	test('keeps one width and one height on every tab', async (t) => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			const spec = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			const key = { required: false, value_type: 'any', in_slug: false, default: null };
			const r = await app.run(['setup', 'mood', '--from-json', JSON.stringify({
				...spec,
				allowed_states: ['happy', 'sad', 'tired'],
				subfields: { where: key, who: key }
			})]);
			assert.equal(r.code, 0, r.stderr);

			await atEachScreen(t, page, [{ width: 390, height: 844 }, { width: 1280, height: 900 }], async () => {
				const { saved: closed } = await openField(page, 'mood');
				await page.locator(MODAL).waitFor();
				const sizes = {};
				for (const tab of TABS) {
					await page.locator(`${MODAL} [data-tab="${tab}"]`).click();
					sizes[tab] = await until(
						() => page.locator(MODAL).evaluate((d, tab) => {
							const panel = d.querySelector('[data-panel]');
							const box = d.getBoundingClientRect();
							return panel.dataset.panel === tab && panel.childElementCount > 0
								? { width: box.width, height: box.height, done: d.querySelector('button.commit').getBoundingClientRect().bottom - box.bottom }
								: null;
						}, tab),
						(seen) => seen !== null
					);
					// The panel is named by its tab, and the tab controls it (da-p2l88).
					assert.equal(await page.locator(MODAL).getByRole('tabpanel', { name: tab, exact: true }).count(), 1, `no tabpanel named ${tab}`);
					assert.equal(await page.locator(MODAL).getByRole('tab', { name: tab, exact: true }).getAttribute('aria-controls'), `spec-panel-${tab}`);
				}
				// Done is in the box on every tab, however much of the panel scrolls.
				for (const tab of TABS) assert.ok(sizes[tab].done <= 1, `${tab}: Done is ${sizes[tab].done}px below the box`);
				const [first, ...rest] = TABS.map((tab) => sizes[tab]);
				for (const [i, size] of rest.entries()) {
					assert.ok(Math.abs(size.width - first.width) <= 1, `${TABS[i + 1]} width ${size.width} vs ${first.width}`);
					assert.ok(Math.abs(size.height - first.height) <= 1, `${TABS[i + 1]} height ${size.height} vs ${first.height}`);
				}
				await leaveField(page);
				assert.equal(await closed, null);
			});
		});
	});
});
