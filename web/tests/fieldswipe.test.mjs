// Swiping between the setup form's tabs (da-kmr29). vividn, 2026-10-19: «you
// should be able to switch between tabs in the setup widget by swiping left
// and right». A horizontal swipe on the form's body moves to the next or the
// previous tab; a vertical drag stays a scroll; a drag begun in a text box,
// a popup or the placement (which drags itself, vremya at slinging) is never
// the form's. Every gesture is a real touch through CDP (`swipe`,
// support/layout.mjs) on the iPhone 12 profile.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, swipe, swipeOver } from './support/layout.mjs';
import { FIELD_FORM, openField } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const panel = (page) => page.locator(`${FIELD_FORM} [data-panel]`);
const shown = (page) => panel(page).getAttribute('data-panel');

/** A new occur field's form, its placement drawn. */
async function open(page) {
	await openField(page);
	await page.locator(`${FIELD_FORM} input.field`).fill('thing');
	await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
	await page.locator(`${FIELD_FORM} .placement .band svg`).waitFor();
}

/**
 * A point on the panel's own body, its scroll at the top: the first one down
 * its middle that is in no box, popup or placement — what the swipe is about.
 */
const body = (page) =>
	panel(page).evaluate((el) => {
		el.scrollTop = 0;
		const r = el.getBoundingClientRect();
		for (let y = r.top + 4; y < r.bottom; y += 4) {
			const hit = document.elementFromPoint(r.left + r.width / 2, y);
			if (el.contains(hit) && !hit.closest('input, textarea, select, button, dialog, .placement')) return { x: r.left + r.width / 2, y };
		}
		throw new Error(`no body to swipe on in ${el.dataset.panel}`);
	});

/** Swipe across the body by `dx` (negative is left), from a side so the finger stays on the screen. */
async function across(page, dx) {
	const { y } = await body(page);
	const width = page.viewportSize().width;
	await swipe(page, dx < 0 ? width - 60 : 60, y, dx, 0);
}

test('on a phone a swipe left goes to the next tab and a swipe right to the one before', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		assert.equal(await shown(page), 'basics');
		for (const next of ['data', 'aliases']) {
			await across(page, -200);
			await until(() => shown(page), (t) => t === next);
		}
		for (const back of ['data', 'basics']) {
			await across(page, 200);
			await until(() => shown(page), (t) => t === back);
		}
		// Short of a swipe it is a touch, and stays.
		await across(page, -25);
		assert.equal(await shown(page), 'basics');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('on a phone a drag in a text box, a popup or the placement keeps the tab, and a vertical drag scrolls the form', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		// In the description box the finger is the box's.
		await swipeOver(page, page.locator(`${FIELD_FORM} input.description`), -200, 0, { x: 0.9, y: 0.5 });
		assert.equal(await shown(page), 'basics');

		// In the colour's popup.
		await page.locator(`${FIELD_FORM} button.swatch[aria-label^="the field's color"]`).click();
		const set = page.locator(`${FIELD_FORM} dialog [role="listbox"]`);
		await set.waitFor();
		await swipeOver(page, set, -150, 0, { x: 0.8, y: 0.5 });
		await page.keyboard.press('Escape');
		await set.waitFor({ state: 'detached' });
		assert.equal(await shown(page), 'basics');

		// On the placement: a drag left of the dot's left side widens the dot (left, as a swipe to Data would be).
		const dims = page.locator(`${FIELD_FORM} .placement .dims`);
		const before = await dims.innerText();
		await swipeOver(page, page.locator(`${FIELD_FORM} .placement [data-handle="-1,0"]`), -60, 0);
		await until(() => dims.innerText(), (t) => t !== before);
		assert.equal(await shown(page), 'basics');

		// Last, since its scroll coasts on after the finger: a vertical drag scrolls, and the tab stays.
		const [whole, box] = await panel(page).evaluate((el) => [el.scrollHeight, el.clientHeight]);
		assert.ok(whole > box + 100, `basics does not scroll: ${whole} in ${box}`);
		const { x, y } = await body(page);
		await swipe(page, x, y + 60, 15, -120);
		await until(() => panel(page).evaluate((el) => el.scrollTop), (top) => top > 0);
		assert.equal(await shown(page), 'basics');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});
