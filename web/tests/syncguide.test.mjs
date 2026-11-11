// The sync guide (da-oyvsz): how to use the Sync widget, in plain words, in
// the Help place. vividn, 2026-10-15: «a nice sync widget web help text should
// be made as well to describe how to use it».
//
// The widget's own `?` opens it; the Help button on the Sync widget tours the
// widget instead (da-7dflk), the tour ending in the guide. The welcome's «Syncing your
// devices» was a third until the welcome became short (da-uzyih.19). Then
// a walk through the widget as a person takes it — the gear's Sync, Turn on
// sync, a device connecting, its notice, More, and a join link opened in a
// browser — and every control the guide names (in bold) is one the walk
// saw, in those words. The guide is the sync kind's, in its registry entry
// (da-oefd0).

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
	await until(() => modal.locator('h3.asked').innerText().catch(() => ''), (t) => t.trim() === 'sync');
	const body = modal.locator('.guide[data-guide="sync"]');
	const words = await body.evaluate((el) => {
		const copy = el.cloneNode(true);
		copy.querySelector('details')?.remove();
		return copy.textContent;
	});
	const sections = await body.locator('h2').count();
	const widget = await body.locator('h2', { hasText: 'Reading the widget' }).count();
	const details = body.locator('details.reference');
	assert.equal(await details.getAttribute('open'), null, 'the reference is open');
	await details.locator('summary').click();
	const reference = await until(() => details.locator('pre').innerText(), (t) => t.includes('sync chain invite'));
	await page.keyboard.press('Escape');
	await modal.waitFor({ state: 'detached' });
	return { words, sections, widget, reference };
}

/** What every entrance must open: the guide's own words, how to read the widget among them, the subcommands only folded. */
function isTheGuide({ words, sections, widget, reference }) {
	assert.ok(sections > 1, `${sections} sections`);
	assert.equal(widget, 1, 'how to read the widget');
	assert.doesNotMatch(words, /datum sync|sync chain (invite|connect|list|leave)|sync (status|pause|resume|admin|diff|refold)\b/, 'a subcommand in the body');
	assert.match(reference, /^datum sync/m);
}

describe('the sync guide opens from', () => {
	// Since da-7dflk the Help button, then the widget, tours it, and the tour's «read more» is its guide in the docs.
	test('the Help button, then the Sync widget: its tour, and nothing in the widget runs', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator('.root-bar button.help[data-command="help"]').click();
			await page.locator(`${WIDGET} .ways`).getByRole('button', { name: 'Share write access' }).click();
			await page.locator('.tour .say').waitFor();
			assert.equal(await page.locator('.tour').getAttribute('data-do'), null, 'it opened on a step that invites');
			await page.keyboard.press('Escape');
			await page.locator('.tour').waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${WIDGET} .offer`).count(), 0, 'the share ran');
		}, PAGE);
	});

	test('the Sync widget’s own ?', async () => {
		await shared.solo(async ({ page }) => {
			await page.locator(`${WIDGET} header`).getByRole('button', { name: 'how to sync' }).click();
			isTheGuide(await guide(page));
		}, PAGE);
	});
});

// «Where it is» ends in Open Sync, a link to the place (da-7s7xc), which the
// guide lost when it became Markdown.
test('the guide’s Open Sync opens the sync place', async () => {
	await shared.solo(async ({ page }) => {
		await page.locator(`${WIDGET} header`).getByRole('button', { name: 'how to sync' }).click();
		const open = place(page).locator('.guide[data-guide="sync"] p').getByRole('button', { name: 'Open Sync', exact: true });
		assert.equal(await open.getAttribute('data-act'), 'open', 'Open Sync wears no ›');
		await open.click();
		await page.locator('dialog.sheet[open][data-place="sync"]').waitFor();
		await place(page).waitFor({ state: 'detached' });
	}, PAGE);
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
				named = await place(page).locator('.guide strong').allInnerTexts();
				await page.keyboard.press('Escape');
				await place(page).waitFor({ state: 'detached' });
				await page.locator('.root-bar button.gear').click();
				await sync.click();

				// Connect a second device: this one named under its ✎ (da-5wlnr), the QR, and the other device redeeming it.
				await widget.locator('header button.rename').click();
				const edit = widget.locator('header .edit');
				await edit.getByLabel('This device’s name').fill('desk');
				await look(edit);
				await edit.getByRole('button', { name: 'Save' }).click();
				await until(() => widget.locator('header .device').innerText(), (t) => t === 'desk');
				await widget.locator('.ways[data-db="datum"]').getByRole('button', { name: 'Share write access' }).click();
				const offer = widget.locator('.offer[data-db="datum"]');
				await offer.locator('.qr svg').waitFor();
				await offer.locator('summary', { hasText: 'join from a terminal' }).click();
				await look(offer);
				const ticket = /datum sync chain join '([^']+)'/.exec(await offer.locator('.ticket').innerText())[1];
				const joined = await native.run(['sync', 'chain', 'join', ticket]);
				assert.equal(joined.code, 0, joined.stderr);
				await until(() => widget.locator('.grid tbody tr').count(), (n) => n === 2);
				// The chain it started names this device as kept under its ✎.
				assert.equal(JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).this.label, 'desk');
				// It joined: the ⚠, and Sync's line with OK.
				await widget.locator('.notice').getByRole('button', { name: 'OK' }).waitFor();
				await page.locator('.root-bar').getByRole('button', { name: 'sync notices' }).waitFor();
				await look(widget);
				await offer.getByRole('button', { name: 'Done' }).click();
				await widget.locator('.actions.chain').waitFor();
				await look(widget);
				// The grid: datum's column first, a database in no chain folded behind «other dbs ›» (da-opbpa).
				await app.run(['--db', 'notes', 'occur', 'jot']);
				await app.goto('/sync');
				const fold = widget.locator('.grid button.fold');
				await look(fold);
				await fold.click();
				await until(() => fold.innerText(), (t) => t === '‹ fewer');
				await look(widget.locator('.grid'));
				// The other device's cell opens its acts, and the database's name its menu (da-dxng1).
				await widget.locator('.grid tbody tr:not(.here) td[data-db="datum"] button.cell').click();
				await look(widget.locator('.grid .pair'));
				await widget.locator('.grid thead th[data-db="datum"] button.name').click();
				await look(widget.locator('.grid .dbmenu'));
				// And the other device's name its menu: Rename, Remove, Sync all (da-jlyyr).
				await widget.locator('.grid tbody tr:not(.here) th button.name').click();
				await look(widget.locator('.grid .devmenu'));

				// The other side: a join link opened in a browser, on a database in no chain.
				const serving = native.spawn(['sync', 'chain', 'invite']);
				const [, link] = await until(() => /datum sync chain join '([^']+)'/.exec(serving.stdout), Boolean);
				await app.goto(`/sync/join?db=phone#${link.split('#').pop()}`);
				const card = page.locator('.card.join');
				await card.getByRole('button', { name: 'Join', exact: true }).waitFor();
				await look(card);
				await card.getByRole('button', { name: 'Join', exact: true }).click();
				await card.locator('.done').waitFor({ timeout: 60_000 });
				// Each device now lists the other.
				await until(() => page.locator(`${WIDGET} .grid tbody tr`).count(), (n) => n >= 2);
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
