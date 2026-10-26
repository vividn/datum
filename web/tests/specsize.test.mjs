// The field form is one size however much of it is open (da-laxpn). vividn,
// 2026-09-25: «The spec modal should stay the same size across different tabs».
//
// A centred `<dialog>` whose height followed its content re-centred on
// every tab press, so the tabs jumped. There are no tabs now (da-4t0yn):
// «▸ data» and «▸ aliases» open in the one page, and the box stays as it
// was. The field here has a spec AND states, so every part has rows.
//
// It is a place now (da-hbrfg): a tab on a phone, the place modal on a
// desktop (da-jozgv), and the box is the host's rather than the dialog's
// 38rem. A form taller than the box scrolls in its body, and the header
// and Done stay where they are, so Done in view is what is held.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, sharedApp, startApp, stopApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';
import { FIELD_FORM as MODAL, leaveField, openField, openSection } from './support/field.mjs';
/** Shut, then each section opened in turn beneath the placement. */
const STEPS = ['shut', 'data', 'aliases'];

describe('the field form', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();
	// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
	// and drawing it costs what a boot did (da-m1m9a).
	const onSeam = (fn) => shared.solo(fn, ONE_PANE);

	test('keeps one width and one height with its sections shut or open', async (t) => {
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
				await page.locator(`${MODAL} .placement`).waitFor();
				const sizes = {};
				for (const step of STEPS) {
					if (step !== 'shut') await openSection(page, step);
					sizes[step] = await page.locator(MODAL).evaluate((d) => {
						const box = d.getBoundingClientRect();
						return { width: box.width, height: box.height, done: d.querySelector('button.commit').getBoundingClientRect().bottom - box.bottom };
					});
				}
				// Done is in the box however much is open, and however much of the body scrolls.
				for (const step of STEPS) assert.ok(sizes[step].done <= 1, `${step}: Done is ${sizes[step].done}px below the box`);
				const [first, ...rest] = STEPS.map((step) => sizes[step]);
				for (const [i, size] of rest.entries()) {
					assert.ok(Math.abs(size.width - first.width) <= 1, `${STEPS[i + 1]} width ${size.width} vs ${first.width}`);
					assert.ok(Math.abs(size.height - first.height) <= 1, `${STEPS[i + 1]} height ${size.height} vs ${first.height}`);
				}
				await leaveField(page);
				assert.equal(await closed, null);
			});
		});
	});
});
