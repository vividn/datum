// The `timezone` field on datum.day (da-yw6o2): opt in from the gear, a very
// thin band at the top of the day, and in the fields widget unless IGNORED.
// What it accepts is the CLI's (`crates/datum-cli/tests/timezone_field.rs`).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { type } from './support/fields.mjs';
import { until } from './support/until.mjs';
import { devices } from 'playwright';

before(startApp);
after(stopApp);

const BERLIN = { timezone: 'Europe/Berlin', viewport: { width: 1280, height: 900 } };
const PHONE = { timezone: 'Asia/Tokyo', clock: Date.parse('2026-06-01T03:00:00Z'), device: devices['iPhone 12'] };
const GEAR = '.root-bar button.gear';
const ITEM = '[role="menu"][aria-label="settings"] button';

const held = async (app) => JSON.parse((await app.run(['fields', 'timezone', '--json'])).stdout)[0].zone;

/** The gear's timezone item, read with the menu open and left open. */
async function item(page) {
	await page.locator(GEAR).click();
	const it = page.locator(ITEM).filter({ hasText: /tracking timezone|Track timezone/ });
	await it.waitFor();
	return it;
}

test("the gear's item logs the device's zone, and again IGNORED, and says which", async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal(await held(app), null, 'nothing logs a zone unless someone asks');
		const off = await item(page);
		assert.equal((await off.textContent()).trim(), 'Track timezone (Europe/Berlin)');
		await off.click();
		await until(() => held(app), (zone) => zone === 'Europe/Berlin');
		await app.clock.forward(60_000);

		const on = await item(page);
		await until(() => on.textContent(), (label) => label.trim() === 'Stop tracking timezone (Europe/Berlin)');
		await on.click();
		await until(() => held(app), (zone) => zone === null);
		await app.clock.forward(60_000);
		const state = JSON.parse((await app.run(['fields', 'timezone', '--json'])).stdout)[0];
		assert.equal(state.ignored, true);

		const back = await item(page);
		await until(() => back.textContent(), (label) => label.trim() === 'Track timezone (Europe/Berlin)');
		await page.keyboard.press('Escape');
	}, BERLIN);
});

test('from the bar: a zone logs, a name that is none is refused offering real ones', async () => {
	await withApp(async (app) => {
		await type(app.page, 'timezone Asia/Seoul');
		await until(() => held(app), (zone) => zone === 'Asia/Seoul');
		const refused = await app.run(['timezone', 'Asia/Tokio']);
		assert.notEqual(refused.code, 0);
		assert.match(refused.stderr, /Asia\/Tokyo/);
		assert.equal(await held(app), 'Asia/Seoul');
	}, BERLIN);
});

/** The band's rects, and every other lane's, in the day strip's today, in screen pixels. */
const lanes = (page) =>
	page.locator('.band[data-days-ago="0"]').evaluate((band) => {
		const of = (el) => {
			const r = el.getBoundingClientRect();
			return { top: r.top, bottom: r.bottom, height: r.height };
		};
		const area = band.querySelector('svg.svg .day-data');
		const marks = [...band.querySelectorAll('svg.svg .field')].map((g) => ({
			field: g.dataset.field,
			rects: [...g.querySelectorAll('rect, path')].map(of).filter((r) => r.height > 0)
		}));
		return { day: of(area), marks };
	});

for (const [name, screen] of [['desktop', BERLIN], ['phone', PHONE]]) {
	test(`a very thin band at the top of the day, over every lane, at ${name}`, async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['switch', 'work', 'desk', '-t', '-200'])).code, 0);
			assert.equal((await app.run(['timezone', screen.timezone, '-t', '-100'])).code, 0);
			await app.redraw();
			const mark = page.locator('.band[data-days-ago="0"] svg.svg .field[data-field="timezone"] [data-state]');
			await mark.first().waitFor({ state: 'attached' });
			await app.idle();

			const { marks } = await lanes(page);
			const band = marks.find((m) => m.field === 'timezone').rects[0];
			const work = marks.find((m) => m.field === 'work').rects[0];
			assert.ok(band.height <= work.height / 2, `thin: ${band.height} against ${work.height}`);
			assert.equal(marks.at(-1).field, 'timezone', 'drawn last, so over every lane');
			const box = await page.locator('.band[data-days-ago="0"] svg.svg').boundingBox();
			assert.ok(band.top - box.y <= 4, `at the top: ${band.top} in a box from ${box.y}`);
			assert.equal(await mark.first().getAttribute('data-state'), screen.timezone, 'the zone is the band\'s label');
			await page.screenshot({ path: process.env.DATUM_SHOTS ? `${process.env.DATUM_SHOTS}/timezone-${name}.png` : undefined });
		}, screen);
	});
}

test('in the fields widget unless IGNORED', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const chip = page.locator('.bar [data-field="timezone"]');
		assert.equal((await app.run(['timezone', 'Asia/Tokyo', '-t', '-100'])).code, 0);
		await app.redraw();
		await until(() => chip.count(), (n) => n > 0);
		assert.equal((await app.run(['timezone', 'IGNORED', '-t', '-50'])).code, 0);
		await app.redraw();
		await until(() => chip.count(), (n) => n === 0);
	}, BERLIN);
});

test('its picker offers no `end`, which the CLI refuses, and leaves the gap of a field never off', async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal((await app.run(['timezone', 'Asia/Tokyo'])).code, 0);
		await app.redraw();
		const chip = page.locator('.bar [data-field="timezone"]');
		await chip.waitFor();
		await chip.dispatchEvent('click');
		const picker = page.locator('.bar .picker');
		await picker.locator('[data-state="Asia/Tokyo"]').waitFor();
		assert.equal(await picker.locator('[data-role="end"].gap').count(), 1);
		assert.equal(await picker.locator('[data-role="end"] button:not([disabled])').count(), 0);
	}, BERLIN);
});
