// `db purge --leave` typed in datum.day's bar (da-a7kf7): one run when the
// page's own sync delivers the departure, and «purge anyway» when no member
// is reached, syncing once more if one has come up (as da-48g3o).
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 300 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. `syncactions`, `-leave` and
// `-purge` were one file, its six leave and purge tests waiting out real
// 20 s budgets and idle timeouts: 189 s alone, 243 s in a whole run, and cut
// at 300 s on CI (da-xwjul). A cancelled file hides every other file's
// failures behind it. A group that costs a minute wants a file of its own,
// not another `describe` in one of these. What more than one of them needs
// is `./support/syncactions.mjs`.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { DESKTOP, cli, openChain, runIn } from './support/syncactions.mjs';
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

describe('`db purge --leave` in the bar', () => {
	test('db purge --leave with a member is one run: the departure waits for this page’s sync to deliver it, then the purge completes (da-a7kf7)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await pairWith(app, native);
					native.spawn(['sync', 'run']);
					const me = JSON.parse(await cli(app, ['sync', 'status', '--json'])).this.endpoint;
					const dialog = page.locator('dialog.confirm');
					const said = app.submit('db purge --leave');
					await dialog.locator('input').fill('datum');
					await dialog.locator('input').press('Enter');
					await said;
					assert.match((await app.outputs()).at(-1), /left sync chain \S+\n\s+its departure is with: /);
					assert.equal(await dialog.count(), 0);
					const { members } = JSON.parse((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
					assert.ok(members.find((m) => m.endpoint === me).left);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('db purge --leave with no member reachable waits, then offers to purge telling no one (da-a7kf7)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await openChain(app);
			await cli(app, ['--db', 'home', 'occur', 'tea']);
			await app.goto('/?db=home');
			const dialog = page.locator('dialog.confirm');
			void app.submit('db purge --leave').catch(() => {});
			await dialog.locator('input').fill('home');
			await dialog.locator('input').press('Enter');
			// The phone never syncs: the purge waits for a sync, then asks again.
			await page.locator('.repl .progress', { hasText: 'Telling the chain’s other devices that this copy is leaving…' }).waitFor();
			const anyway = dialog.getByRole('button', { name: 'purge anyway' });
			await anyway.waitFor({ timeout: 30_000 });
			assert.match(await dialog.innerText(), /No device of sync chain \S+ could be reached to receive this copy’s departure\. Purge home now/);
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '1');
			await anyway.click();
			await until(() => app.outputs(), (o) => /left sync chain/.test(o.at(-1) ?? ''), 90_000);
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '0');
		}, { viewport: DESKTOP });
	});

	test('a member that comes up while «purge anyway» is asked gets the departure, and lists this copy LEFT (da-a7kf7, as da-48g3o)', async () => {
		const native = await nativeCopy(relay);
		const left = async () => /"left":"/.test((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					const serve = async () => {
						const serving = native.spawn(['sync', 'chain', 'invite']);
						return [serving, (await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean))[1]];
					};
					const [pairing, ticket] = await serve();
					await page.evaluate((t) => window.datum.sync.connect('datum', t), ticket);
					pairing.child.kill();
					await until(() => pairing.child.exitCode !== null || pairing.child.signalCode !== null, Boolean);

					// The only other member is down: no one holds the departure.
					const dialog = page.locator('dialog.confirm');
					// Its echo can come later than `submit` waits: the output is waited on instead.
					void app.submit('db purge --leave').catch(() => {});
					await dialog.locator('input').fill('datum');
					await dialog.locator('input').press('Enter');
					const anyway = dialog.getByRole('button', { name: 'purge anyway' });
					await anyway.waitFor({ timeout: 30_000 });

					// It comes up, serving and dialing no one, while the question is open.
					await serve();
					assert.equal(await left(), false);
					await anyway.click();
					// First the dial begun while it was down, which a dead peer holds for the wire's 60 s idle timeout.
					const out = await until(() => app.outputs(), (o) => /left sync chain/.test(o.at(-1) ?? ''), 90_000);
					assert.match(out.at(-1), /left sync chain \S+\n\s+its departure is with: /);
					assert.equal(await left(), true);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});
});
