// The first field's tour (da-ii8lo). vividn, 2026-10-15: «after saving their
// first field, let's automatically change tabs to the collect tab and pulse
// the field button that was created. Then pulse the nowview … Finally pulse
// the dayview».
//
// So, on a phone and on a desktop, from a first visit's tabs with setup in
// front: the first field's Save brings collect to the front and rings its
// button, the nowview and the dayview in that order, each with its sentence;
// a second field's Save shows none of it. Where a fields widget is on the
// page already, the tour stays there.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { FIELD_FORM } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const ON_SETUP = { names: ['welcome', 'setup', 'collect'], active: 1 };
const FRESH = { home: ON_SETUP, desk: null, tour: true };
const PHONE = { ...FRESH, viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };
const DESKTOP = { ...FRESH, viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' };

const NOWVIEW = 'Your recent history appears here. Tap a time to enter something that happened then but you forgot to record.';
const DAYVIEW = 'Longer history lives here. Scroll up to see earlier days. You can select times here too.';

/** The root tab in front, by the layout it holds. */
const front = (page) =>
	page.evaluate(() => {
		const { tabs, active } = window.datum.layout.current().root;
		return tabs[active].layout;
	});

/** The sentence on screen, and whether the ring stands round the drawn element `selector` names. */
const stop = (page, selector) =>
	page.evaluate((selector) => {
		const ring = document.querySelector('.tour .ring')?.getBoundingClientRect();
		const el = [...document.querySelectorAll(selector)].map((e) => e.getBoundingClientRect()).find((r) => r.width > 0);
		const round = !!ring && !!el && [ring.left - el.left, ring.top - el.top, el.right - ring.right, el.bottom - ring.bottom].every((d) => Math.abs(d + 4) < 1);
		return { say: document.querySelector('.tour .say p')?.textContent ?? null, round };
	}, selector);

/** Make a new field in the field widget: its name, its shape, Save. */
async function make(page, field, verb) {
	await page.locator(`${FIELD_FORM} input.field`).fill(field);
	await page.locator(`${FIELD_FORM} .verb[data-verb="${verb}"]`).click();
	await page.locator(`${FIELD_FORM} button.commit`).click();
}

describe('the first field\'s tour', () => {
	for (const [what, options, field, verb, tap, pulse] of [
		['a phone', PHONE, 'coffee', 'occur', 'Tap to log coffee.', true],
		['a desktop', DESKTOP, 'sleep', 'start', 'Tap to turn sleep on, and again to turn it off.', false]
	])
		test(`${what}: the first field's Save tours its button on collect, the nowview and the dayview; the second's shows nothing`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await page.locator(FIELD_FORM).waitFor();
				await make(page, field, verb);

				// 1. Collect in front, the new button ringed.
				const button = `[data-widget="widgetbar"] button[data-field="${field}"]`;
				await until(() => stop(page, button), (s) => s.round && s.say === tap);
				assert.equal(await front(page), 'collect');
				// A pulse, or a still ring where motion is reduced.
				const animation = await page.locator('.tour .ring').evaluate((el) => getComputedStyle(el, '::after').animationName);
				assert.equal(animation.includes('pulse'), pulse, animation);

				// 2. A tap anywhere: the nowview.
				await page.mouse.click(8, 400);
				await until(() => stop(page, '[data-widget="nowview"] > section'), (s) => s.round && s.say === NOWVIEW);

				// 3. «next»: the dayview, and «done» or Escape ends it.
				await page.locator('.tour button.next').click();
				await until(() => stop(page, '[data-widget="daystrip"] > section'), (s) => s.round && s.say === DAYVIEW);
				assert.equal((await page.locator('.tour button.next').innerText()).trim(), 'done');
				if (pulse) await page.locator('.tour button.next').click();
				else await page.keyboard.press('Escape');
				await page.locator('.tour').waitFor({ state: 'detached' });
				assert.equal(await front(page), 'collect', 'Escape did more than end the tour');

				// A second field: saved, and no tour.
				await page.evaluate(() => window.datum.shell.activate(1));
				await page.locator('[data-widget="setup"] .chips button.new').click();
				await make(page, 'tea', 'occur');
				await page.locator('[data-widget="setup"] .chips [data-chip="tea"][aria-current="true"]').waitFor();
				await app.idle();
				assert.equal(await page.locator('.tour').count(), 0);
				assert.equal(await front(page), 'setup');
				assert.deepEqual(app.errors, []);
			}, options);
		});

	test('a fields widget on the page already: the place shuts and the tour stays on that page', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator('.bar button[data-track]').click();
			await page.locator(FIELD_FORM).waitFor();
			await make(page, 'coffee', 'occur');
			await until(() => stop(page, '[data-widget="widgetbar"] button[data-field="coffee"]'), (s) => s.round && s.say === 'Tap to log coffee.');
			assert.equal(await front(page), 'main');
			assert.equal(await page.locator('dialog[data-place="setup"]').count(), 0);
			await page.keyboard.press('Escape');
			await page.locator('.tour').waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		}, { viewport: { width: 1280, height: 900 }, tour: true });
	});
});
