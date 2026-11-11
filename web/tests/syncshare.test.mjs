// The Sync widget's three ways in (da-48kc3): [Share write access], [Share
// read access] and [Enter code] above the grid, and the same three under
// «+ add device ▾» at its foot. vividn, 2026-11-10: «the share links should
// let you copy code, copy url, copy cli command, and show qr code». So a
// share's panel draws its QR and copies each of the three, in the role it
// shares; Enter code, given a link or a bare code, opens the join card.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { nativeCopy, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { DESKTOP, WIDGET } from './support/syncactions.mjs';
import { until } from './support/until.mjs';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

const SCREENS = [
	{ name: 'phone', options: { device: devices['iPhone 12'], viewport: { width: 390, height: 844 } } },
	{ name: 'desktop', options: { viewport: DESKTOP } }
];
const WAYS = ['Share write access', 'Share read access', 'Enter code'];
const COPIES = ['Copy code', 'Copy URL', 'Copy CLI command'];

/** The clipboard, kept in the page: every text a copy wrote, in order. */
const holdClipboard = (page) =>
	page.addInitScript(() => {
		window.__copied = [];
		Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: (t) => (window.__copied.push(t), Promise.resolve()) } });
	});

/** A share pressed in `where`: its panel, once its QR is drawn, and the join link it offers. */
async function shared(widget, where, name) {
	await where.getByRole('button', { name, exact: true }).click();
	const offer = widget.locator('.offer');
	await offer.locator('.qr svg').waitFor();
	const [, link] = /datum sync chain join '([^']+)'/.exec(await offer.locator('.ticket').textContent());
	return { offer, link };
}

describe('the sync widget’s shares and Enter code (da-48kc3)', () => {
	for (const screen of SCREENS) {
		test(`${screen.name}: each share opens its panel with the QR, Copy code, Copy URL and Copy CLI command, in its role`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await holdClipboard(page);
					await app.goto('/sync');
					const widget = page.locator(WIDGET);
					const ways = widget.locator('.ways[data-db="datum"]');
					await ways.getByRole('button', { name: WAYS[0] }).waitFor();
					assert.deepEqual(await ways.getByRole('button').allInnerTexts(), WAYS);
					for (const [name, role, words] of [
						['Share write access', 'member', /Share write access to datum: the other device reads and writes it/],
						['Share read access', 'reader', /Share read access to datum: the other device reads it, writing nothing/]
					]) {
						const { offer, link } = await shared(widget, ways, name);
						assert.equal(await offer.getAttribute('data-role'), role);
						assert.match(await offer.locator('.head').innerText(), words);
						assert.match(link, /\/sync\/join#[^\s#]+$/);
						const own = await offer.locator('.actions button').allInnerTexts();
						assert.deepEqual(
							own.filter((l) => l !== 'Share'),
							[...COPIES, 'Done']
						);
						for (const copy of COPIES) await offer.getByRole('button', { name: copy }).click();
						await until(() => page.evaluate(() => window.__copied.length), (n) => n === COPIES.length);
						// The code is the ticket alone, the URL the join link, the command the one a terminal runs.
						assert.deepEqual(await page.evaluate(() => window.__copied.splice(0)), [link.split('#').pop(), link, `datum sync chain join '${link}'`]);
						assert.equal(await offer.locator('.copied').innerText(), 'Copied the CLI command.');
						await offer.getByRole('button', { name: 'Done' }).click();
						await offer.waitFor({ state: 'detached' });
					}
				},
				{ prepare: relayed(relay), ...screen.options }
			);
		});

		test(`${screen.name}: «+ add device ▾» offers the three; Enter code with a link, and with a bare code, opens the join card`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await app.goto('/sync');
					const widget = page.locator(WIDGET);
					const { offer, link } = await shared(widget, widget.locator('.ways'), 'Share write access');
					await offer.getByRole('button', { name: 'Done' }).click();

					const add = widget.locator('.grid tfoot button.add');
					assert.equal(await add.innerText(), '+ add device ▾');
					await add.click();
					const foot = widget.locator('.grid tfoot .actions');
					assert.deepEqual(await foot.getByRole('button').allInnerTexts(), WAYS);

					for (const [where, typed] of [
						[foot, link],
						[widget.locator('.ways'), `  ${link.split('#').pop()}  `]
					]) {
						await where.getByRole('button', { name: 'Enter code' }).click();
						const enter = widget.locator('form.enter');
						// The field opens under the button pressed: at the grid's foot, or above the grid.
						assert.equal(await widget.locator('.grid tfoot form.enter').count(), where === foot ? 1 : 0);
						assert.equal(await enter.count(), 1);
						await enter.getByLabel('Code or link').fill(typed);
						await enter.getByRole('button', { name: 'Open' }).click();
						const card = page.locator('.card.join');
						await card.locator('.what li').first().waitFor();
						assert.match(await card.locator('.what').innerText(), /^Join the sync chain of /);
						assert.equal(await enter.count(), 0, 'the field goes once the card opens');
						assert.equal(await foot.count(), 0, 'the foot’s three fold away');
						await card.getByRole('button', { name: 'Dismiss' }).click();
						await card.waitFor({ state: 'detached' });
					}
				},
				{ prepare: relayed(relay), ...screen.options }
			);
		});
	}

	test('Share read access admits a device that only reads: its cell is a reader’s', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await app.goto('/sync');
					const widget = page.locator(WIDGET);
					const { offer, link } = await shared(widget, widget.locator('.ways'), 'Share read access');
					const joined = await native.run(['sync', 'chain', 'join', link]);
					assert.equal(joined.code, 0, joined.stderr);
					await offer.locator('.joined').waitFor();
					const cell = widget.locator('.grid tbody tr:not(.here) td[data-db="datum"]');
					await until(() => cell.getAttribute('data-state').catch(() => null), (s) => s === 'reader');
					// Another device, once this one is used, is shared in the same role.
					await offer.getByRole('button', { name: 'Add another device' }).click();
					await widget.locator('.offer[data-role="reader"] .qr svg').waitFor();
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});
});
