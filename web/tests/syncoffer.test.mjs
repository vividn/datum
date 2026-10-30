// The Sync widget's invite panel once its invite admits no one (da-2bvxq):
// used, it names the device that joined; expired, it says so. Either way the
// QR and the link go, since they no longer work, and [Add another device]
// makes a new invite.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { relayed, startRelay, withWallClock } from './support/sync.mjs';
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

/** Turn on sync in `app`'s widget: its invite panel, and the join link it offers. */
async function invite(app) {
	await app.goto('/sync');
	await app.page.locator(`${WIDGET} .actions[data-db="datum"]`).getByRole('button', { name: 'Turn on sync' }).click();
	const offer = app.page.locator(`${WIDGET} .offer[data-db="datum"]`);
	await offer.locator('.qr svg').waitFor();
	const [, link] = /datum sync chain join '([^']+)'/.exec(await offer.locator('.ticket').textContent());
	return { offer, link };
}

/** What the panel offers: its QR, its link, and its buttons. */
async function offered(offer) {
	return {
		qr: await offer.locator('.qr').count(),
		ticket: await offer.locator('.ticket').count(),
		buttons: await offer.getByRole('button').allInnerTexts()
	};
}

const SPENT = { qr: 0, ticket: 0, buttons: ['Add another device', 'Done'] };

describe('the invite panel (da-2bvxq)', () => {
	test('once another browser joins through it, it names that device in place of the QR and link', async () => {
		await withWallClock(
			(a) =>
				withWallClock(
					async (b) => {
						const { offer, link } = await invite(a);
						await b.goto(link);
						const card = b.page.locator('.card.join');
						await card.getByRole('button', { name: 'Join', exact: true }).click();
						await card.locator('.done').waitFor();

						const joined = offer.locator('.joined');
						await joined.waitFor();
						const endpoint = JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).this.endpoint;
						const label = async () =>
							JSON.parse((await a.run(['sync', 'chain', 'list', '--json'])).stdout).members.find((m) => m.endpoint === endpoint)?.label;
						await until(
							async () => [await joined.innerText(), await label()],
							// Who joined, by the name it took, and when.
							([said, name]) => !!name && said.includes(name) && /\b\d\d:\d\d\b/.test(said)
						);
						assert.deepEqual(await offered(offer), SPENT);

						await offer.getByRole('button', { name: 'Add another device' }).click();
						await until(() => offer.locator('.ticket').textContent().catch(() => ''), (t) => t.includes('chain join') && !t.includes(link));
						assert.equal(await offer.locator('.joined').count(), 0);
					},
					{ prepare: relayed(relay), viewport: DESKTOP }
				),
			{ prepare: relayed(relay), viewport: DESKTOP }
		);
	});

	test('once its invite expires unused, it says so in place of the QR and link', async () => {
		await withApp(
			async (app) => {
				const { offer } = await invite(app);
				const until_ = /\b(\d\d:\d\d)\b/.exec(await offer.locator('.head').innerText())[1];
				assert.equal(await offer.locator('.expired').count(), 0);
				await app.clock.forward(11 * 60_000);
				await offer.locator('.expired').waitFor();
				assert.ok((await offer.locator('.expired').innerText()).includes(until_), 'when it expired');
				assert.deepEqual(await offered(offer), SPENT);
			},
			{ prepare: relayed(relay), viewport: DESKTOP }
		);
	});

	// An invite waits for this device's relay, up to 30 s, before its ticket
	// is made (da-kfhty): meanwhile the widget says so, and a ticket made
	// without the relay is offered as one no other device reaches (da-y9voq).
	test('while its invite waits for the relay it says so, and without the relay it offers no QR', async () => {
		await withApp(
			async (app) => {
				await app.goto('/sync');
				const widget = app.page.locator(WIDGET);
				const turnOn = widget.locator('.actions[data-db="datum"]').getByRole('button', { name: 'Turn on sync' });
				await turnOn.click();
				assert.notEqual(await widget.locator('.waiting').innerText(), '');
				assert.equal(await turnOn.isDisabled(), true);
				const offer = widget.locator('.offer[data-db="datum"]');
				// The endpoint's bind waits 10 s for the relay, and the ticket 30 s more.
				await offer.locator('.unreachable').waitFor({ timeout: 60000 });
				assert.deepEqual(await offered(offer), { qr: 0, ticket: 0, buttons: ['Try again', 'Done'] });
				assert.equal(await widget.locator('.waiting').count(), 0);
			},
			{ prepare: (context) => context.addInitScript(() => localStorage.setItem('datum.syncRelay', 'http://127.0.0.1:1')), viewport: DESKTOP }
		);
	});
});
