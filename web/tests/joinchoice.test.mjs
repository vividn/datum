// A join link chooses the database it joins (da-bedyz). vividn, 2026-11-10:
// «when you open up a sync invitation link, it would be nice to be able to
// select the db, or create a new db». Opened on a database holding entries,
// the card offers a new one, named after the inviter, so accepting a
// friend's database never mixes it into your own; an empty one is joined
// as it is; a name that is taken, or a database in another chain, is
// refused before anything is dialed. Phone and desktop.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { until } from './support/until.mjs';

const SHAPES = { desktop: {}, phone: { device: devices['Pixel 7'], home: null } };

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

/** `native`'s join link, served until redeemed. */
async function linkOf(native) {
	const serving = native.spawn(['sync', 'chain', 'invite']);
	const [, link] = await until(() => /datum sync chain join '([^']+)'/.exec(serving.stdout), Boolean);
	return link;
}

/** Open `native`'s join link in `app`: the card, its choice, and its Join. */
async function opened(app, native) {
	const link = await linkOf(native);
	await app.goto(`/sync/join#${link.split('#').pop()}`);
	const card = app.page.locator('.card.join');
	const into = card.getByLabel('Join with');
	await into.waitFor();
	return { card, into, fresh: card.getByLabel('New database’s name'), join: card.getByRole('button', { name: 'Join', exact: true }) };
}

/** `field`'s count in `db` of `app`'s browser. */
const countIn = async (app, db, field) => Number((await app.run(['--db', db, 'count', field])).stdout);

for (const [shape, opts] of Object.entries(SHAPES)) {
	describe(`the join card chooses its database, on a ${shape}`, () => {
		test('a page holding entries offers a new database, named for the inviter, and joining opens it with the chain’s alone', async () => {
			const native = await nativeCopy(relay);
			try {
				await native.run(['occur', 'theirs', '-y', '-t', '08:00']);
				await withWallClock(
					async (app) => {
						await app.run(['occur', 'mine', '-y', '-t', '09:00']);
						const { card, into, fresh, join } = await opened(app, native);
						assert.equal(await into.inputValue(), '', 'a new database is not the one chosen');
						const name = await fresh.inputValue();
						assert.match(name, /^[a-z0-9_-]+$/);
						assert.notEqual(name, 'datum');
						assert.match(await card.locator('.what').innerText(), new RegExp(`a new database «${name}»`));
						await join.click();
						// The page opens on the database joined, and says it joined.
						await until(() => new URL(app.page.url()).searchParams.get('db'), (db) => db === name, 60_000);
						await app.page.locator('.card.join .done').waitFor({ timeout: 60_000 });
						// Shown once: what the join said is not kept for a later mount or load.
						assert.equal(await app.page.evaluate(() => sessionStorage.getItem('datum-sync-joined')), null);
						assert.equal(await countIn(app, name, 'theirs'), 1);
						assert.equal(await countIn(app, name, 'mine'), 0);
						// The page's own database keeps its own, and nothing of the chain's.
						assert.equal(await countIn(app, 'datum', 'mine'), 1);
						assert.equal(await countIn(app, 'datum', 'theirs'), 0);
					},
					{ prepare: relayed(relay), ...opts }
				);
			} finally {
				await native.close();
			}
		});

		test('an empty database chosen is joined; a taken name typed as new is refused, and a database in another chain offered a new one', async () => {
			const [native, other] = [await nativeCopy(relay), await nativeCopy(relay)];
			try {
				await native.run(['occur', 'theirs', '-y', '-t', '08:00']);
				await withWallClock(
					async (app) => {
						await app.run(['occur', 'mine', '-y', '-t', '09:00']);
						// «spare» exists and holds nothing; «work» is in another chain, with another device.
						await app.run(['--db', 'spare', 'tail']);
						const ticket = (await linkOf(other)).split('#').pop();
						await app.page.evaluate((t) => window.datum.sync.connect('work', t), ticket);
						const { card, into, fresh, join } = await opened(app, native);

						await fresh.fill('datum');
						assert.match(await card.locator('.failure').innerText(), /«datum» is here already/);
						assert.equal(await join.count(), 0);

						// A database in another chain is no refusal: the card offers a new database instead (da-48kc3).
						await into.selectOption('work');
						assert.equal(await card.locator('.failure').count(), 0);
						assert.match(await card.locator('.what').innerText(), /«work» is in another sync chain.*join with a new database instead/);
						assert.equal(await join.count(), 0);
						await card.getByRole('button', { name: 'Join as a new database' }).click();
						assert.equal(await into.inputValue(), '');
						await fresh.waitFor();

						await into.selectOption('spare');
						assert.match(await card.locator('.what').innerText(), /this browser’s database «spare»/);
						await join.click();
						await until(() => new URL(app.page.url()).searchParams.get('db'), (db) => db === 'spare', 60_000);
						await app.page.locator('.card.join .done').waitFor({ timeout: 60_000 });
						assert.equal(await countIn(app, 'spare', 'theirs'), 1);
						assert.equal(await countIn(app, 'datum', 'theirs'), 0);
					},
					{ prepare: relayed(relay), ...opts }
				);
			} finally {
				await native.close();
				await other.close();
			}
		});
	});
}
