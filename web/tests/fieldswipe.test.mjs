// A swipe across the setup widget is the super tabs' (da-4t0yn). vividn,
// 2026-10-26: «Let's completely remove the tabs on the setup widget, they are
// just annoying and unintutive and prevent swiping between super tabs
// easily». The form had three tabs of its own a sideways swipe moved between
// (da-kmr29, da-nmhf2); with none, a horizontal swipe on its body goes to the
// pane's frame (PaneFrame.svelte), which on a phone is the root's: the place
// opens as the root's last tab, `main` before it. A drag on the placement
// (which drags itself) or a state's handle (which reorders) is still theirs,
// and a vertical drag scrolls the form. Every gesture is a real touch through
// CDP (`swipe`, support/layout.mjs) on the iPhone 12 profile.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, swipe, swipeOver } from './support/layout.mjs';
import { FIELD_FORM, openField, openSection } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The form's body, which scrolls under its head and buttons. */
const body = (page) => page.locator(`${FIELD_FORM} .body`);

/** A new occur field's form, its placement drawn, in its place: the root's tab `setup`. */
async function open(page) {
	await openField(page);
	await page.locator(`${FIELD_FORM} input.field`).fill('thing');
	await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
	await page.locator(`${FIELD_FORM} .placement .band svg`).waitFor();
	await until(() => rootShown(page), (t) => t === 'setup');
}

/**
 * A point on the body itself, its scroll at the top: the first height that
 * is in no box, button or placement, where the finger goes down at either
 * side, and 16px above and below it, since the browser moves a touch onto a
 * box that near.
 */
const bare = (page) =>
	body(page).evaluate((el) => {
		el.scrollTop = 0;
		const r = el.getBoundingClientRect();
		const clear = (x, y) => {
			const hit = document.elementFromPoint(x, y);
			return el.contains(hit) && !hit.closest('input, textarea, select, button, summary, dialog, .placement');
		};
		for (let y = r.top + 16; y < r.bottom - 16; y += 4) {
			if ([60, innerWidth - 60].every((x) => [-16, 0, 16].every((d) => clear(x, y + d)))) return { x: r.left + r.width / 2, y };
		}
		throw new Error('no bare body to swipe on');
	});

/** The root tab in front, less the dot a changed layout wears. */
const rootShown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

test('on a phone a swipe across the setup widget, with data and aliases shut or open, changes the super tab', async () => {
	await withApp(async (app) => {
		const { page } = app;
		for (const opened of [[], ['data', 'aliases']]) {
			await open(page);
			assert.equal(await page.locator(`${FIELD_FORM} [role="tab"]`).count(), 0, 'the form is tabbed');
			for (const name of opened) await openSection(page, name);
			const { y } = await bare(page);
			await swipe(page, 60, y, 200, 0);
			await until(() => rootShown(page), (t) => t === 'main').catch((e) => {
				throw new Error(`a swipe on the form (${opened.join(', ') || 'all shut'} open) left the super tab: ${e.message}`);
			});
		}
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('on a phone a drag on the placement or a state\'s handle is theirs, and a vertical drag scrolls the form', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		// On the placement: a sideways drag on a dot is the dot's (da-r6wwe), toward `main` as a swipe would go, and a tap
		// on it boxes it.
		const discs = `${FIELD_FORM} .placement .band g.field .point-bg`;
		await swipeOver(page, page.locator(discs).last(), 60, 0);
		assert.equal(await rootShown(page), 'setup');
		// Read in one go: the band is drawn again once the drag lets go, which replaces the dots.
		const at = await page.waitForFunction((sel) => {
			const r = [...document.querySelectorAll(sel)].at(-1)?.getBoundingClientRect();
			return r && [r.x + r.width / 2, r.y + r.height / 2];
		}, discs);
		await page.touchscreen.tap(...(await at.jsonValue()));
		// A drag right of the dot's right side widens the dot.
		const dims = page.locator(`${FIELD_FORM} .placement .dims`);
		const before = await dims.innerText();
		await swipeOver(page, page.locator(`${FIELD_FORM} .placement [data-handle="1,0"]`), 60, 0);
		await until(() => dims.innerText(), (t) => t !== before);
		assert.equal(await rootShown(page), 'setup');

		// A state's handle: its first 10px lean toward `main`, and it goes on down past the row under it.
		await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
		for (const name of ['calm', 'busy']) {
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).last().fill(name);
		}
		const handle = page.locator(`${FIELD_FORM} .state .handle`).first();
		await handle.evaluate((el) => el.scrollIntoView({ block: 'center' }));
		const box = await handle.boundingBox();
		await swipe(page, box.x + box.width / 2, box.y + box.height / 2, 120, 60);
		const names = await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value));
		assert.deepEqual(names, ['busy', 'calm']);
		assert.equal(await rootShown(page), 'setup');

		// Last, since its scroll coasts on after the finger: a vertical drag scrolls, and the super tab stays.
		for (const name of ['data', 'aliases']) await openSection(page, name);
		const [whole, room] = await body(page).evaluate((el) => [el.scrollHeight, el.clientHeight]);
		assert.ok(whole > room + 100, `the form does not scroll: ${whole} in ${room}`);
		const { x, y } = await bare(page);
		await swipe(page, x, y + 60, 15, -120);
		await until(() => body(page).evaluate((el) => el.scrollTop), (top) => top > 0);
		assert.equal(await rootShown(page), 'setup');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});
