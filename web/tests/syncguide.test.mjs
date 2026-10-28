// The sync guide (da-oyvsz): how to use the Sync widget, in plain words, in
// the Help place. vividn, 2026-10-15: «a nice sync widget web help text should
// be made as well to describe how to use it».
//
// Three entrances open it, a test each: the welcome's «Syncing your
// devices», the Help button on the Sync widget (its kind's help), and the
// widget's own `?`. Then a walk through the widget as a person takes it —
// the gear's Sync, Turn on sync, a device connecting, its notice, More, and
// a join link opened in a browser — and every control the guide names (a
// `b.ui`) is one the walk saw, in those words.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { nativeCopy, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { WIDGET } from './support/syncactions.mjs';
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

const shared = sharedApp({ ...DESKTOP });
const PAGE = JSON.stringify(along('column', widgetPane('welcome', 'welcome'), widgetPane('sync', 'sync')));

/** The Help place's modal, on a desktop. */
const place = (page) => page.locator('dialog.sheet[open][data-place="help"]');

/** The guide, once the Help place shows it: its words outside the reference, and the reference. */
async function guide(page) {
	const modal = place(page);
	await until(() => modal.locator('h3.asked').innerText().catch(() => ''), (t) => t.trim() === 'Syncing your devices');
	const body = modal.locator('.guide');
	const words = await body.evaluate((el) => {
		const copy = el.cloneNode(true);
		copy.querySelector('details')?.remove();
		return copy.textContent;
	});
	const sections = await body.locator('h3').count();
	const widget = await body.locator('p.widget').innerText();
	const details = body.locator('details.reference');
	assert.equal(await details.getAttribute('open'), null, 'the reference is open');
	await details.locator('summary').click();
	const reference = await until(() => details.locator('pre').innerText(), (t) => t.includes('sync chain invite'));
	await page.keyboard.press('Escape');
	await modal.waitFor({ state: 'detached' });
	return { words, sections, widget, reference };
}

/** What every entrance must open: the guide's own words, the widget's help in it, the subcommands only folded. */
function isTheGuide({ words, sections, widget, reference }) {
	assert.ok(sections > 1, `${sections} sections`);
	assert.notEqual(widget.trim(), '', 'the widget’s help');
	assert.doesNotMatch(words, /datum sync|sync chain (invite|connect|list|leave)|sync (status|pause|resume|admin|diff|refold)\b/, 'a subcommand in the body');
	assert.match(reference, /^datum sync/m);
}

describe('the sync guide opens from', () => {
	test('the welcome’s «Syncing your devices»', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator('.host.welcome button.link', { hasText: 'Syncing your devices' }).click();
			isTheGuide(await guide(page));
		}, PAGE);
	});

	test('the Help button, then the Sync widget; nothing in the widget runs', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator('.root-bar button.help[data-command="help"]').click();
			await page.locator(`${WIDGET} .actions`).getByRole('button', { name: 'Turn on sync' }).click();
			isTheGuide(await guide(page));
			assert.equal(await page.locator(`${WIDGET} .offer`).count(), 0, 'Turn on sync ran');
		}, PAGE);
	});

	test('the Sync widget’s own ?', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator(`${WIDGET} header`).getByRole('button', { name: 'how to sync' }).click();
			isTheGuide(await guide(page));
		}, PAGE);
	});
});

test('every control the guide names is one the widget shows, walked as a person walks it', async () => {
	const native = await nativeCopy(relay);
	const seen = [];
	let named = [];
	const look = async (locator) => seen.push(await locator.innerText());
	try {
		await withWallClock(
			async (app) => {
				const { page } = app;
				await app.goto('/');
				// Where it is: the gear, then Sync.
				await page.locator('.root-bar button.gear').click();
				const sync = page.getByRole('menuitem', { name: 'Sync', exact: true });
				await look(sync);
				await sync.click();
				const widget = page.locator(WIDGET);
				await look(widget);

				// The guide, from the widget it is about.
				await widget.getByRole('button', { name: 'how to sync' }).click();
				await place(page).locator('.guide').waitFor();
				named = await place(page).locator('.guide b.ui').allInnerTexts();
				await page.keyboard.press('Escape');
				await place(page).waitFor({ state: 'detached' });
				await page.locator('.root-bar button.gear').click();
				await sync.click();

				// Connect a second device: the QR, and the other device redeeming it.
				await widget.locator('.actions[data-db="datum"]').getByRole('button', { name: 'Turn on sync' }).click();
				const offer = widget.locator('.offer[data-db="datum"]');
				await offer.locator('.qr svg').waitFor();
				await offer.locator('summary', { hasText: 'connect from a terminal' }).click();
				await look(offer);
				const ticket = /datum sync chain connect '([^']+)'/.exec(await offer.locator('.ticket').innerText())[1];
				const joined = await native.run(['sync', 'chain', 'connect', ticket]);
				assert.equal(joined.code, 0, joined.stderr);
				await until(() => widget.locator('.devices li').count(), (n) => n === 2);
				// It joined: the ⚠, and Sync's line with OK.
				await widget.locator('.notice').getByRole('button', { name: 'OK' }).waitFor();
				await page.locator('.root-bar').getByRole('button', { name: 'sync notices' }).waitFor();
				await look(widget);
				await offer.getByRole('button', { name: 'Done' }).click();
				await widget.locator('.actions[data-db="datum"]').getByRole('button', { name: /^More/ }).click();
				await widget.locator('.more .device').waitFor();
				await look(widget);

				// The other side: a join link opened in a browser, on a database in no chain.
				const serving = native.spawn(['sync', 'chain', 'invite']);
				const [, link] = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
				await app.goto(`/sync/join?db=phone#${link.split('#').pop()}`);
				const card = page.locator('.card.join');
				await card.getByRole('button', { name: 'Join', exact: true }).waitFor();
				await look(card);
				await card.getByRole('button', { name: 'Join', exact: true }).click();
				await card.locator('.done').waitFor({ timeout: 60_000 });
				// Each device now lists the other.
				await until(() => page.locator(`${WIDGET} .devices li`).count(), (n) => n >= 2);
			},
			{ prepare: relayed(relay), viewport: DESKTOP.viewport }
		);
	} finally {
		await native.close();
	}
	const shown = seen.join('\n').replace(/\s+/g, ' ');
	assert.ok(named.length >= 10, `the guide names ${named.length} controls`);
	for (const word of named) assert.ok(shown.includes(word.replace(/\s+/g, ' ')), `the guide says «${word}», which the widget never showed:\n${shown}`);
});
