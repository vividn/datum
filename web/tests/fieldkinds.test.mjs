// The kind cards of the field form (da-e6eqv): each card's name stands over
// its line, the chosen card's too. The chosen card was `.on`, the «on» state
// row's class, and took that row's five-column grid (#2082), which held its
// name to a 28px column its text ran out of, over the line. So the name is
// read by its TEXT (a Range over it), never its box: the box keeps to the
// column and reads clear.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { FIELD_FORM, nameField, openField } from './support/field.mjs';

before(startApp);
after(stopApp);

/** Each card whose name's text overlaps its line, with the two rects. */
const overrun = (page) =>
	page.locator(`${FIELD_FORM} button.verb`).evaluateAll((cards) =>
		cards.flatMap((card) => {
			const range = document.createRange();
			range.selectNodeContents(card.querySelector('.name'));
			const n = range.getBoundingClientRect();
			const l = card.querySelector('.line').getBoundingClientRect();
			const hit = n.left < l.right && l.left < n.right && n.top < l.bottom && l.top < n.bottom;
			return hit ? [`${card.dataset.verb}: name ${Math.round(n.left)}–${Math.round(n.right)}, line from ${Math.round(l.left)}`] : [];
		})
	);

for (const [where, device] of [['a phone', PHONE], ['a desktop', DESKTOP]]) {
	test(`on ${where} no kind card's name runs into its line, the chosen one's included`, async () => {
		await withApp(async ({ page }) => {
			await openField(page);
			await nameField(page, 'thing');
			const kinds = await page.locator(`${FIELD_FORM} button.verb`).evaluateAll((all) => all.map((b) => b.dataset.verb));
			assert.ok(kinds.length > 1, `kinds: ${kinds}`);
			for (const kind of kinds) {
				await page.locator(`${FIELD_FORM} button[data-verb="${kind}"]`).click();
				await page.locator(`${FIELD_FORM} button[data-verb="${kind}"][aria-checked="true"]`).waitFor();
				assert.deepEqual(await overrun(page), [], `with ${kind} chosen`);
			}
		}, device);
	});
}
