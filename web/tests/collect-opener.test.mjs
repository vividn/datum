// Collect's views wear the pill that opened them (da-dswuc). vividn,
// 2026-11-10: «Show the pill that is creating that view right now (like field
// or hid), with an x on it. then in the state selection view, you don't need
// to display both back and a field indicator, just the field pill. 2 birds»,
// and «yes, tapping the pill anywhere closes it».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';

before(startApp);
after(stopApp);

const VIEW = 'section.pane[data-title="collect"] .bar .collect';
const HEAD = `${VIEW} [data-role="head"]`;
const ENTRY = `${VIEW}[data-entry]`;
const SEED = ['setup mood --verb switch --states good,bad', 'switch mood bad -y -t 13:00'];
const SCREENS = {
	desktop: { clock: todayAt(15) },
	phone: { ...PHONE, home: ON_COLLECT, clock: todayAt(15) }
};

async function seeded(app) {
	for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
	await app.reload();
	await app.page.locator(`${VIEW} [data-field="mood"]`).waitFor();
	await app.idle();
}

/** The pill at `where` as a reader sees it: its kind, its words, whether it has its × and a lock, and its fill. */
const look = (page, where) =>
	page.locator(where).evaluate((p) => ({
		kind: [...p.classList].find((c) => c !== 'pill'),
		words: [...p.querySelectorAll('.label')].map((l) => l.innerText).join(' '),
		x: !!p.querySelector('.x'),
		lock: !!p.querySelector('.lock'),
		fill: getComputedStyle(p).backgroundColor
	}));

for (const [screen, options] of Object.entries(SCREENS)) {
	describe(screen, () => {
		const press = (locator) => (screen === 'phone' ? locator.tap() : locator.click());

		test("a field's view stands under the field's pill with its ×, no back and no name beside it; a press on its word or its × closes it", async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seeded(app);
				for (const part of ['.label', '.x']) {
					// Pressed where it stands: the rows pan sideways.
					await page.locator(`${VIEW} [data-field="mood"]`).dispatchEvent('click');
					await page.locator(`${VIEW} [data-state="good"]`).waitFor();
					const pill = `${HEAD} .pill`;
					const [here, bar] = await Promise.all([look(page, pill), look(page, '.prompt-bar .pill.field')]);
					assert.deepEqual({ ...here, fill: undefined }, { kind: 'field', words: 'mood', x: true, lock: false, fill: undefined });
					assert.equal(here.fill, bar.fill, "drawn in the bar's field pill's color");
					// Back and the field's name are the pill now: the head's other buttons are off alone.
					const others = await page.locator(HEAD).evaluate((h) => [...h.querySelectorAll('button')].filter((b) => !b.closest('.pill') && !b.hasAttribute('data-off')).length);
					assert.equal(others, 0, 'a button beside the pill and off');
					assert.equal(await page.locator(`${HEAD} .name`).count(), 0, "the field's name is still drawn beside its pill");
					assert.ok(!(await page.locator(HEAD).innerText()).includes('back'), 'back is still drawn');

					await press(page.locator(`${pill} ${part}`));
					await page.locator(`${VIEW} [data-field="mood"]`).waitFor();
					assert.equal(await page.locator(`${VIEW} [data-state]`).count(), 0, `${part}: the states stayed`);
					assert.equal(await page.locator('.prompt-bar .pill.field').count(), 0, `${part}: the bar's field pill stayed`);
				}
				assert.deepEqual(app.errors, []);
			}, options);
		});

		test("an entry's view stands under its hid pill with its ×, no back; a press on its word or its × closes it", async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seeded(app);
				const [{ hid }] = JSON.parse((await app.runLine('tail mood -n 1 --json')).stdout).rows;
				for (const part of ['.label', '.x']) {
					await page.evaluate((h) => window.datum.bar.setPills([{ id: 'hid', kind: 'hid', text: h }]), hid);
					await page.locator(`${ENTRY} [data-card]`).waitFor();
					const pill = `${ENTRY} .head .pill`;
					const [here, bar] = await Promise.all([look(page, pill), look(page, '.prompt-bar .pill.hid')]);
					assert.deepEqual(here, { ...bar, lock: false }, "the bar's hid pill, without its lock");
					assert.equal(here.kind, 'hid');
					assert.ok(here.x, 'no ×');
					assert.equal(await page.locator(`${ENTRY} .head button:not(.pill button)`).count(), 0, 'a button beside the pill');
					assert.ok(!(await page.locator(`${ENTRY} .head`).innerText()).includes('back'), 'back is still drawn');

					await press(page.locator(`${pill} ${part}`));
					await page.locator(`${VIEW} [data-field="mood"]`).waitFor();
					assert.equal(await page.locator(ENTRY).count(), 0, `${part}: the entry stayed`);
					assert.equal(await page.locator('.prompt-bar .pill.hid').count(), 0, `${part}: the bar's hid pill stayed`);
				}
				assert.deepEqual(app.errors, []);
			}, options);
		});
	});
}
