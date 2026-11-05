// The `timezone` field on datum.day (da-yw6o2): opt in from the Settings place (da-99g5q), a very
// thin band at the top of the day, and in the collect widget unless IGNORED.
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
const PLACE = 'dialog.sheet[data-place="settings"]';

const held = async (app) => JSON.parse((await app.run(['fields', 'timezone', '--json'])).stdout)[0].zone;

/** The settings place's timezone button: the gear's «Settings» opens it, a modal on a desktop (da-99g5q). */
async function item(page) {
	await page.locator(GEAR).click();
	await page.locator(ITEM).first().waitFor();
	const setting = (await page.locator(ITEM).allInnerTexts()).find((t) => /timezone|Wrap tabs/.test(t));
	assert.equal(setting, undefined, 'the gear still holds a setting');
	await page.locator(ITEM).filter({ hasText: /^Settings$/ }).click();
	const it = page.locator(`${PLACE} button[data-setting="timezone"]`);
	await it.waitFor();
	return it;
}

test("Settings' timezone button logs the device's zone, and again IGNORED, and says which", async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal(await held(app), null, 'nothing logs a zone unless someone asks');
		const it = await item(page);
		assert.equal((await it.textContent()).trim(), 'Track timezone (Europe/Berlin)');
		await it.click();
		await until(() => held(app), (zone) => zone === 'Europe/Berlin');
		await app.clock.forward(60_000);
		await until(() => it.textContent(), (label) => label.trim() === 'Stop tracking timezone (Europe/Berlin)');

		await it.click();
		await until(() => held(app), (zone) => zone === null);
		await app.clock.forward(60_000);
		const state = JSON.parse((await app.run(['fields', 'timezone', '--json'])).stdout)[0];
		assert.equal(state.ignored, true);
		await until(() => it.textContent(), (label) => label.trim() === 'Track timezone (Europe/Berlin)');
		await page.keyboard.press('Escape');
		await page.locator(PLACE).waitFor({ state: 'detached' });
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

// vividn, 2026-11-05: «Let's not show the timezone field in the collect
// widget, it is kind of a special field and wouldn't be manually changed»
// (da-cyq92): no chip whatever it holds, a zone or IGNORED, while a field
// beside it has one. It is still logged from the bar and Settings (above).
test('no chip in collect, holding a zone or IGNORED', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const chip = page.locator('.bar [data-field="timezone"]');
		for (const zone of ['Asia/Tokyo', 'IGNORED']) {
			assert.equal((await app.run(['timezone', zone, '-t', zone === 'IGNORED' ? '-50' : '-100'])).code, 0);
			assert.equal((await app.run(['occur', 'coffee', '-t', zone === 'IGNORED' ? '-40' : '-90'])).code, 0);
			await app.redraw();
			await page.locator('.bar [data-field="coffee"]').waitFor();
			assert.equal(await chip.count(), 0, zone);
		}
	}, BERLIN);
});

// A device in another zone than the field (da-mvt5i), which the CLI asks and
// the page shows: on load and on a log, each answer once.
/** The zones the field has held, newest first. */
const stored = async (app) => (await app.entries()).filter((e) => e.field === 'timezone').map((e) => e.state);
const dialogOf = (page) => page.locator('dialog.confirm');
const YES = { name: 'switch to Europe/Berlin', exact: true };

test('a load in another zone asks once: no keeps the stored zone and is not asked again', async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal((await app.run(['timezone', 'Asia/Tokyo', '-t', '-100'])).code, 0);
		await page.reload();
		const dialog = dialogOf(page);
		await dialog.waitFor();
		await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
		await dialog.waitFor({ state: 'detached' });
		await until(() => app.run(['follow-zone']).then((r) => r.code), (code) => code === 0);
		assert.deepEqual(await stored(app), ['Asia/Tokyo']);

		const input = page.locator('.prompt-bar input');
		await input.fill('occur coffee');
		await input.press('Enter');
		const coffee = await until(() => app.entries(), (rows) => rows.some((e) => e.field === 'coffee'));
		assert.equal(coffee.find((e) => e.field === 'coffee').at.timezone, 'Asia/Tokyo');
		assert.equal(await dialog.count(), 0, 'not asked again');
		assert.deepEqual(app.errors, []);
	}, BERLIN);
});

test('yes on a load logs the device zone, and a log asked then follows it', async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal((await app.run(['timezone', 'Asia/Tokyo', '-t', '-100'])).code, 0);
		await page.reload();
		const dialog = dialogOf(page);
		await dialog.getByRole('button', YES).click();
		await until(() => stored(app), (zones) => zones[0] === 'Europe/Berlin');

		// The field moves away with the page open, and a log is asked.
		await app.clock.forward(60_000);
		assert.equal((await app.run(['timezone', 'Asia/Tokyo'])).code, 0);
		await app.clock.forward(60_000);
		const input = page.locator('.prompt-bar input');
		await input.fill('occur coffee');
		await input.press('Enter');
		await dialog.getByRole('button', YES).click();
		const rows = await until(() => app.entries(), (es) => es.some((e) => e.field === 'coffee'));
		assert.equal(rows.find((e) => e.field === 'coffee').at.timezone, 'Europe/Berlin');
		assert.equal((await stored(app))[0], 'Europe/Berlin');
		assert.deepEqual(app.errors, []);
	}, BERLIN);
});

test('with timezone_follow auto a load logs the switch with no question, and says so', async () => {
	await withApp(async (app) => {
		const { page } = app;
		assert.equal((await app.run(['timezone', 'Asia/Tokyo', '-t', '-100'])).code, 0);
		assert.equal((await app.run(['db', 'config', 'timezone_follow', 'auto'])).code, 0);
		await page.reload();
		await page.waitForFunction(() => window.datum);
		await until(() => stored(app), (zones) => zones[0] === 'Europe/Berlin');
		await page.locator('.prompt-bar').getByText('Europe/Berlin').first().waitFor();
		assert.equal(await dialogOf(page).count(), 0);
		assert.deepEqual(app.errors, []);
	}, BERLIN);
});
