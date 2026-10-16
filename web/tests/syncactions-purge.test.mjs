// `db purge --leave` typed in datum.day's bar (da-a7kf7): one run when the
// page's own sync delivers the departure, and «purge anyway» when no member
// is reached, syncing once more if one has come up (as da-48g3o).
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 300 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. `syncactions`, `-leave` and
// `-purge` were one file, its six leave and purge tests waiting out real
// 20 s budgets and idle timeouts: 189 s alone, 243 s in a whole run, and cut
// at 300 s on CI (da-xwjul), before `withApp` shortened them (da-eqjri). A
// cancelled file hides every other file's failures behind it. A group that
// costs a minute wants a file of its own, not another `describe` in one of
// these. What more than one of them needs is `./support/syncactions.mjs`.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { AT_DATUM_DAY, DESKTOP, addAdmin, authenticator, cli, openChain, runIn } from './support/syncactions.mjs';
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
					// First the dial begun while it was down, which a dead peer holds for the wire's idle timeout.
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

// da-x5lw4: a locked chain's purge takes an admin's signature, which the page
// asks a passkey for and a key file can give only the CLI. The dialog says
// so before the name, which was otherwise typed for a refusal.
describe('the purge dialog in a locked chain', () => {
	const ask = async (app) => {
		const dialog = app.page.locator('dialog.confirm');
		await cli(app, ['--db', 'home', 'occur', 'tea']);
		await app.goto('/repl?db=home');
		const said = app.submit('db purge');
		// Handled, so a failed assertion is the failure reported, not its closing the page under this.
		said.catch(() => {});
		await dialog.waitFor();
		const button = (label) => dialog.getByRole('button', { name: label, exact: true });
		return { said, dialog, button, text: await dialog.locator('p').innerText() };
	};

	test('locked by a key file: said before any name, and the whole chain is not offered', async () => {
		await withWallClock(async (app) => {
			await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']);
			await cli(app, ['--db', 'home', 'sync', 'admin', 'add', '/data/admin.key']);
			const { said, dialog, button, text } = await ask(app);
			assert.match(text, /; the chain is locked by an admin key file, which only the CLI reads: `datum db purge --chain --admin-key <FILE>` wipes every copy in it\.\nLeave or rejoin\?$/);
			assert.equal(await button('chain').count(), 0);
			assert.equal(await dialog.locator('input').count(), 0);
			await button('cancel').click();
			await said;
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '1');
		}, AT_DATUM_DAY);
	});

	test('locked by a passkey: said before the name, and the passkey approves the purge of every copy', async () => {
		await withWallClock(async (app) => {
			await authenticator(app.page);
			await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']);
			await app.goto('/sync?db=home');
			await addAdmin(app.page);
			const { said, dialog, button, text } = await ask(app);
			assert.match(text, /; the chain is locked, so an admin’s passkey approves it once the name is typed\.\nLeave, rejoin, or purge the whole chain\?$/);
			await button('chain').click();
			await dialog.locator('input').fill('home');
			await dialog.locator('input').press('Enter');
			await said;
			// The preview's lines, then the approval's, each on its own.
			const [, backup] = /beside the database\napproved: wipe; backup: (\S+)$/.exec((await app.outputs()).at(-1)) ?? [];
			assert.ok(backup, (await app.outputs()).at(-1));
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '0');
			// The backup the approval wrote is kept, and restores what went (da-459uh).
			await cli(app, ['--db', 'other', 'db', 'restore', backup, '--yes']);
			assert.equal((await runIn(app, ['--db', 'other', 'count', 'tea'])).stdout.trim(), '1');
		}, AT_DATUM_DAY);
	});
});

// da-art6t: a hard restore of every copy in a locked chain takes an admin's
// signature, which the CLI reads on stdin, and here stdin is the backup. An
// approve link answered alone would wipe every copy and restore none, so the
// page says it cannot before the name, and nothing is wiped.
describe('the hard restore dialog in a locked chain', () => {
	const restore = async (app) => {
		const { page } = app;
		await cli(app, ['--db', 'home', 'occur', 'tea']);
		const backup = await cli(app, ['--db', 'home', 'db', 'backup', '-']);
		await app.goto('/repl?db=home');
		const chooser = page.waitForEvent('filechooser');
		// Its echo lands once the file is chosen: handled, so a failed assertion is the failure reported.
		const ran = app.submit('db restore --hard --chain');
		ran.catch(() => {});
		await (await chooser).setFiles({ name: 'home.jsonl', mimeType: 'application/json', buffer: Buffer.from(backup) });
		const dialog = page.locator('dialog.confirm[open]');
		const [asked, said] = await until(
			async () => [await dialog.count(), (await app.outputs()).at(-1) ?? ''],
			([asked, said]) => asked > 0 || /locked/.test(said)
		);
		assert.equal(asked, 0, `asked before saying it cannot: ${asked && (await dialog.innerText())}`);
		assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '1');
		return said;
	};

	test('locked by a key file: said before any name, and nothing restored', async () => {
		await withWallClock(async (app) => {
			await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']);
			await cli(app, ['--db', 'home', 'sync', 'admin', 'add', '/data/admin.key']);
			assert.match(await restore(app), /--admin-key <FILE>/);
		}, AT_DATUM_DAY);
	});

	test('locked by a passkey: said before any name, and nothing restored', async () => {
		await withWallClock(async (app) => {
			await authenticator(app.page);
			await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']);
			await app.goto('/sync?db=home');
			await addAdmin(app.page);
			assert.match(await restore(app), /the link it prints for a passkey/);
		}, AT_DATUM_DAY);
	});
});
