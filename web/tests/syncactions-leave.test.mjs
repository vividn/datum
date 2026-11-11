// The Sync widget's Stop syncing, in a database's menu (da-j8c4l, da-2o19r,
// da-dxng1): asked first, a departure the page's own sync delivers, and «stop
// anyway» when no member is reached, syncing once more if one has come up
// (da-48g3o).
//
// Budget (da-lt0k): on node 22 a FILE was the test `run()` timed
// (da-l5k9e: CI now runs node 24), so 300 s was this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. `syncactions`, `-leave` and
// `-purge` were one file, its six leave and purge tests waiting out real
// 20 s budgets and idle timeouts: 189 s alone, 243 s in a whole run, and cut
// at 300 s on CI (da-xwjul), before `withApp` shortened them (da-eqjri). A
// cancelled file hides every other file's failures behind it. A group that
// costs a minute wants a file of its own, not another `describe` in one of
// these. What more than one of them needs is `./support/syncactions.mjs`.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { SYNC_TIMEOUT_S, startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { AT_DATUM_DAY, DESKTOP, WIDGET, cli, dbMenu, members, openChain } from './support/syncactions.mjs';
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

describe('the sync widget’s Stop syncing', () => {
	test('stop syncing asks first, saying what happens to whose data, then keeps every entry under a new key, and the database is on this device only again (da-j8c4l)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await openChain(app);
			await cli(app, ['--db', 'home', 'occur', 'tea']);
			const key = await cli(app, ['--db', 'home', 'sync', 'endpoint-key']);
			await app.goto('/sync?db=home');
			const leave = (await dbMenu(page, 'home')).getByRole('button', { name: 'Stop syncing' });

			await leave.click();
			const dialog = page.locator('dialog.confirm');
			// Whose data goes where: this device keeps every entry, the others keep theirs, nothing is deleted.
			for (const fact of [/^Stop syncing home on this device\?/, /keeps every entry/, /other devices keep their copies/, /nothing is deleted/])
				assert.match(await dialog.innerText(), fact);
			await dialog.getByRole('button', { name: 'cancel' }).click();
			assert.equal((await members(app)).members.length, 2);

			await leave.click();
			const pressed = Date.now();
			await dialog.getByRole('button', { name: 'stop syncing' }).click();
			// The phone never syncs, so no member holds the departure: it waits for a sync, then asks again, to leave telling no one.
			assert.notEqual(await page.locator(`${WIDGET} .waiting`).innerText(), '');
			await dialog.getByRole('button', { name: 'stop anyway' }).waitFor({ timeout: 30_000 });
			// The phone has an address, so it is dialled: only the budget (`SYNC_TIMEOUT_S`) ends the wait.
			assert.ok(Date.now() - pressed >= SYNC_TIMEOUT_S * 1000 - 1000, `asked again after ${Date.now() - pressed} ms`);
			assert.match(JSON.stringify(await members(app)), /"left":"/);
			await dialog.getByRole('button', { name: 'stop anyway' }).click();
			// In no chain: only on this device, with nothing to leave (da-5wlnr).
			await leave.waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${WIDGET} header .state`).innerText(), 'only on this device');
			assert.equal(JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).chain, null);
			assert.match(await cli(app, ['--db', 'home', 'tail']), /tea/);
			assert.notEqual(await cli(app, ['--db', 'home', 'sync', 'endpoint-key']), key);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, AT_DATUM_DAY);
	});

	// The refusal can go stale while its question is open: «stop anyway» syncs
	// and leaves again, forcing only if still unheard (da-48g3o).
	test('a member that comes up while «stop anyway» is asked gets the departure, and lists this copy LEFT', async () => {
		const native = await nativeCopy(relay);
		const left = async () => /"left":"/.test((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
		try {
			// A native datum from before the departure record (da-dsob1) never lists anyone LEFT.
			const help = await native.run(['sync', 'chain', 'leave', '--help']);
			assert.match(help.stdout + help.stderr, /--force/, 'the native datum predates `chain leave`’s departure: build this tree’s');
			await withWallClock(
				async (app) => {
					const { page } = app;
					const serve = async () => {
						const serving = native.spawn(['sync', 'chain', 'invite']);
						return [serving, (await until(() => /datum sync chain join '([^']+)'/.exec(serving.stdout), Boolean))[1]];
					};
					const [pairing, ticket] = await serve();
					await page.evaluate((t) => window.datum.sync.connect('datum', t), ticket);
					pairing.child.kill();
					await until(() => pairing.child.exitCode !== null || pairing.child.signalCode !== null, Boolean);
					await app.goto('/sync');

					// The only other member is down: no one holds the departure.
					const leave = (await dbMenu(page, 'datum')).getByRole('button', { name: 'Stop syncing' });
					await leave.click();
					const dialog = page.locator('dialog.confirm');
					await dialog.getByRole('button', { name: 'stop syncing' }).click();
					await dialog.getByRole('button', { name: 'stop anyway' }).waitFor({ timeout: 30_000 });

					// It comes up, serving and dialing no one, while the question is open.
					await serve();
					assert.equal(await left(), false);
					await dialog.getByRole('button', { name: 'stop anyway' }).click();
					// First the dial begun while it was down, which a dead peer holds for the wire's idle timeout.
					await leave.waitFor({ state: 'detached', timeout: 90_000 });
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
					assert.equal(await left(), true);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('one press leaves: the departure waits for this page’s sync to deliver it, then the leave completes (da-2o19r)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await pairWith(app, native);
					native.spawn(['sync', 'run']);
					const me = JSON.parse(await cli(app, ['sync', 'status', '--json'])).this.endpoint;
					await app.goto('/sync');
					const leave = (await dbMenu(page, 'datum')).getByRole('button', { name: 'Stop syncing' });
					await leave.click();
					const dialog = page.locator('dialog.confirm');
					await dialog.getByRole('button', { name: 'stop syncing' }).click();
					await leave.waitFor({ state: 'detached', timeout: 90_000 });
					assert.equal(await dialog.count(), 0);
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
					// The native member holds the departure: it lists this copy as left.
					const { members } = JSON.parse((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
					assert.ok(members.find((m) => m.endpoint === me).left);
				},
				// Production's 20 s to deliver the departure, not 5: past it the page asks «stop anyway»,
				// which a session slowed by CI's load reached (da-0wovz, as da-go39y for the purge). Nothing here waits a timeout out.
				{ prepare: relayed(relay), viewport: DESKTOP, syncTimeout: 20 }
			);
		} finally {
			await native.close();
		}
	});
});
