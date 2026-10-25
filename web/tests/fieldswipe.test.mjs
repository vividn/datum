// Swiping between the setup form's tabs (da-kmr29). vividn, 2026-10-19: «you
// should be able to switch between tabs in the setup widget by swiping left
// and right». A horizontal swipe on the form's body moves to the next or the
// previous tab; a vertical drag stays a scroll; a drag begun in a text box,
// a popup or the placement (which drags itself, vremya at slinging) is never
// the form's. Every gesture is a real touch through CDP (`swipe`,
// support/layout.mjs) on the iPhone 12 profile.
//
// vividn, 2026-10-22 (da-nmhf2): «actually animate the swiping between the
// tabs (with next tab already preloaded so that it swipes in nicely) … If you
// swipe on the tab bar itself, or in the margins, it should go to the
// surrounding container». So the swipe is the TABBED REGION's alone: the
// finger drags the tab in front with the next one in sight, and a release
// short of a swipe springs back; begun on the bar, the head or a margin it
// changes the pane's tab, from any of the form's.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, swipe, swipeOver } from './support/layout.mjs';
import { FIELD_FORM, PANEL, openField } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const panel = (page) => page.locator(PANEL);
const shown = (page) => panel(page).getAttribute('data-panel');

/** A new occur field's form, its placement drawn. */
async function open(page) {
	await openField(page);
	await page.locator(`${FIELD_FORM} input.field`).fill('thing');
	await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
	await page.locator(`${FIELD_FORM} .placement .band svg`).waitFor();
}

/**
 * A point on the panel's own body, its scroll at the top: the first height
 * that is in no box, popup or placement — what the swipe is about — where
 * the finger goes down at either side, and 16px above and below it, since
 * the browser moves a touch onto a box that near.
 */
async function body(page) {
	// Not while the last swipe's glide is still bringing the panel in.
	const tab = await panel(page).getAttribute('data-panel');
	await until(() => lefts(page), (at) => at[tab] === 0);
	return panel(page).evaluate((el) => {
		el.scrollTop = 0;
		const r = el.getBoundingClientRect();
		const clear = (x, y) => {
			const hit = document.elementFromPoint(x, y);
			return el.contains(hit) && !hit.closest('input, textarea, select, button, dialog, .placement');
		};
		for (let y = r.top + 16; y < r.bottom - 16; y += 4) {
			if ([60, innerWidth - 60].every((x) => [-16, 0, 16].every((d) => clear(x, y + d)))) return { x: r.left + r.width / 2, y };
		}
		throw new Error(`no body to swipe on in ${el.dataset.panel}`);
	});
}

/** Swipe across the body by `dx` (negative is left), from a side so the finger stays on the screen; `every` ms a step, or as fast as it goes. */
async function across(page, dx, every) {
	const { y } = await body(page);
	const width = page.viewportSize().width;
	await swipe(page, dx < 0 ? width - 60 : 60, y, dx, 0, 6, every);
}

/** The tabbed region, which the panels move in. */
const region = (page) => page.locator(`${FIELD_FORM} .tabbed`);
/** Where each tab's panel stands, left of the region's content box: 0 in front, ± its width beside it. */
const lefts = (page) =>
	region(page).evaluate((el) => {
		const home = el.getBoundingClientRect().left + el.clientLeft;
		return Object.fromEntries(
			[...el.querySelectorAll('[data-panel]')].map((p) => [p.dataset.panel, Math.round(p.getBoundingClientRect().left - home)])
		);
	});
/** Press `tab` in the bar: whether the track glided there, the transition read as it starts, not timed. */
async function press(page, tab) {
	await region(page).locator('.track').evaluate((el) => {
		window.__glided = false;
		el.addEventListener('transitionrun', (e) => e.target === el && e.propertyName === 'transform' && (window.__glided = true));
	});
	await page.locator(`${FIELD_FORM} [data-tab="${tab}"]`).click();
	await until(() => lefts(page), (at) => at[tab] === 0);
	return page.evaluate(() => window.__glided);
}

/** Whether `tab`'s panel is drawn. */
const drawn = (page, tab) => page.locator(`${FIELD_FORM} [data-panel="${tab}"]`).evaluate((el) => getComputedStyle(el).visibility === 'visible');

/**
 * Put a finger on the body and drag it `dx` along; what is returned lifts it
 * a second after it went down, by the events' own clock, so no release is a
 * flick. What the form does UNDER the finger is read in between.
 */
async function hold(page, dx) {
	const { y } = await body(page);
	const x = dx < 0 ? page.viewportSize().width - 60 : 60;
	const cdp = await page.context().newCDPSession(page);
	const t0 = Date.now() / 1000;
	const touch = (type, touchPoints, ms) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints, timestamp: t0 + ms / 1000 });
	await touch('touchStart', [{ x, y }], 0);
	for (let i = 1; i <= 6; i++) await touch('touchMove', [{ x: x + (dx * i) / 6, y }], i * 50);
	return async () => {
		await touch('touchEnd', [], 1000);
		await cdp.detach();
	};
}

/** The root tab in front, less the dot a changed layout wears. */
const rootShown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

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
		// Short of a swipe, and slow, it is a touch, and stays; as short and quick, a flick, it goes on; but a slip
		// shorter still, however quick, stays (19px: the browser sends no move under ~15, its touch slop).
		await across(page, -25, 60);
		assert.equal(await shown(page), 'basics');
		await across(page, -19, 2);
		assert.equal(await shown(page), 'basics');
		await across(page, -25, 4);
		await until(() => shown(page), (t) => t === 'data');
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

test('on a phone the finger drags the tab in front with the next in sight, and short of a swipe it springs back', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		const width = await region(page).evaluate((el) => el.clientWidth);
		assert.deepEqual(await lefts(page), { basics: 0, data: width, aliases: 2 * width }, 'the tabs side by side');
		assert.equal(await drawn(page, 'data'), false, 'a tab beside is drawn at rest');
		// The tabs beside are mounted and out of reach until they are in front.
		assert.equal(await page.locator(`${FIELD_FORM} [data-panel="data"]`).getAttribute('inert'), '');
		assert.equal(await page.locator(`${FIELD_FORM} [data-panel="basics"]`).getAttribute('inert'), null);

		// Under the finger: the tab in front moves with it, and Data comes in beside it, drawn.
		const lift = await hold(page, -30);
		const held = await lefts(page);
		assert.equal(held.basics, -30, `basics stands at ${held.basics} under a finger 30px left`);
		assert.equal(held.data, width - 30);
		assert.equal(await drawn(page, 'data'), true, 'the next tab is not drawn under the finger');
		await lift();
		// Let go short of a swipe, and slowly, it glides back, and Data is put away.
		await until(() => lefts(page), (at) => at.basics === 0);
		await until(() => drawn(page, 'data'), (d) => d === false);
		assert.equal(await shown(page), 'basics');

		// Past a swipe it goes on, and lands Data where Basics stood.
		const go = await hold(page, -120);
		await go();
		await until(() => lefts(page), (at) => at.data === 0);
		assert.equal(await shown(page), 'data');
		// Basics is put away as the track lands, the one 150ms beside the other: wait for it, not one frame.
		await until(() => drawn(page, 'basics'), (d) => d === false);
		// A tab pressed in the bar glides too.
		assert.equal(await press(page, 'basics'), true, 'a tab pressed did not glide');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('a tab pressed in the bar lands in its place though something in it is brought into view as it glides in', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
		// What a click on it does first, and what a box taking the focus does: the region must not scroll for it.
		await page.locator(`${FIELD_FORM} ul.data + button.add`).evaluate((b) => b.scrollIntoView({ block: 'nearest', inline: 'nearest' }));
		await until(() => region(page).evaluate((el) => el.querySelector('.track').getAnimations().length), (n) => n === 0);
		assert.equal(await region(page).evaluate((el) => el.scrollLeft), 0, 'the region scrolled');
		assert.equal((await lefts(page)).data, 0);
		assert.deepEqual(app.errors, []);
	});
});

test('on a phone a sideways drag on a state\'s handle reorders, and never changes the tab', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
		for (const name of ['calm', 'busy']) {
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).last().fill(name);
		}
		// Its first 10px lean sideways, and it goes on down past the row under it: the rows swap, the tab stays.
		const handle = page.locator(`${FIELD_FORM} .state .handle`).first();
		await handle.evaluate((el) => el.scrollIntoView({ block: 'center' }));
		const box = await handle.boundingBox();
		await swipe(page, box.x + box.width / 2, box.y + box.height / 2, -120, 60);
		assert.equal(await shown(page), 'basics');
		const names = await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value));
		assert.deepEqual(names, ['busy', 'calm']);
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('the placement draws nothing while Basics is behind, and once when it is back', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		// Each drawing is a `dayview --sample` run; the ones opening the form asked for are over first.
		await app.idle();
		const seen = await app.spyOnRunner();
		const samples = async () => (await seen()).filter((argv) => argv.includes('--sample')).length;
		// A key typed on Data changes the spec the placement draws.
		await press(page, 'data');
		await page.locator(`${PANEL} ul.data + button.add`).click();
		await page.locator(`${PANEL} ul.data li.key input.name`).last().pressSequentially('cups');
		await app.idle();
		assert.equal(await samples(), 0, 'the placement drew behind Data');
		await press(page, 'basics');
		await until(samples, (n) => n > 0);
		await app.idle();
		assert.equal(await samples(), 1, 'the placement drew more than once');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('on a phone under reduced motion nothing follows the finger, and a swipe changes the tab at once', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await open(page);
		const lift = await hold(page, -120);
		assert.equal((await lefts(page)).basics, 0, 'the tab moved under the finger');
		await lift();
		await until(() => shown(page), (t) => t === 'data');
		assert.equal((await lefts(page)).data, 0);
		assert.equal(await drawn(page, 'basics'), false);
		assert.equal(await press(page, 'aliases'), false, 'a tab pressed glided');
		assert.deepEqual(app.errors, []);
	}, { ...PHONE, reducedMotion: 'reduce' });
});

test('on a phone a swipe on the tab bar, the head or a margin changes the pane\'s tab, from a middle tab', async () => {
	await withApp(async (app) => {
		const { page } = app;
		/** The form open on Data, the middle tab, in the place a phone opens it in: the root's last tab, `main` before it. */
		const onData = async () => {
			await open(page);
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
			await until(() => shown(page), (t) => t === 'data');
			assert.equal(await rootShown(page), 'setup');
		};
		const box = (sel) => page.locator(`${FIELD_FORM} ${sel}`).boundingBox();
		// A swipe right from Data the form would take to Basics: begun on the bar, the head or the margin
		// left of the region, it is the pane's, which goes back to `main` (and closes the place).
		for (const [what, at] of [
			['the tab bar', async () => ({ ...(await box('.tabs')), x: 40 })],
			['the head', async () => ({ ...(await box('.name-line')), x: 40 })],
			['the margin', async () => {
				// The pane's own padding left of the region: in the widget, and in no part of the form.
				const tabbed = await box('.tabbed');
				const edge = await page.locator(`${FIELD_FORM} .tabbed`).evaluate((el) => el.closest('section.pane').getBoundingClientRect().left);
				assert.ok(tabbed.x - edge >= 4, `no margin left of the region: it starts at ${tabbed.x}, the pane at ${edge}`);
				return { ...tabbed, x: (edge + tabbed.x) / 2 };
			}]
		]) {
			await onData();
			const { x, y, height } = await at();
			await swipe(page, x, y + height / 2, 200, 0);
			await until(() => rootShown(page), (t) => t === 'main').catch((e) => {
				throw new Error(`a swipe on ${what} left the pane's tab: ${e.message}`);
			});
		}

		// In the region it is the form's, from the same middle tab, either way.
		await onData();
		await across(page, -200);
		await until(() => shown(page), (t) => t === 'aliases');
		await across(page, 200);
		await until(() => shown(page), (t) => t === 'data');
		assert.equal(await rootShown(page), 'setup');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('on a phone a swipe in the form just opened, either way, keeps the place: its slide in is no tab switch', async () => {
	// da-bgve9: the place sliding in opened the root's window after a switch
	// (da-d9p5m), so a swipe right in the form within the second went to the
	// root, which has `main` that way, and closed the place.
	await withApp(async (app) => {
		const { page } = app;
		await openField(page);
		await until(() => rootShown(page), (t) => t === 'setup');
		await until(() => shown(page), (t) => t === 'basics');
		await across(page, -200);
		await until(() => shown(page), (t) => t === 'data');
		await across(page, 200);
		await until(() => shown(page), (t) => t === 'basics');
		assert.equal(await rootShown(page), 'setup');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});
