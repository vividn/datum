// A sync command typed in the bar whose answer is a secret (da-odkpz): its
// output is shown as it runs and kept nowhere on disk, neither in the
// scrollback's store nor in the copy of the CLI widget kept for the next
// visit. Its line stays, and the store says what took the output's place.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** What the page left in `localStorage` once it was hidden, every key's value. */
const stored = (app) =>
	app.page.evaluate(() => {
		dispatchEvent(new Event('pagehide'));
		return Object.fromEntries(Object.keys(localStorage).map((k) => [k, localStorage.getItem(k)]));
	});

/** The JSON the last command printed, on its last line. */
const answer = async (app) => JSON.parse((await app.stdouts()).at(-1).trim().split('\n').at(-1));

test('an agent code, an invite, the pending join and the account are shown once, and not stored (da-odkpz)', async () => {
	await withApp(async (app) => {
		await app.run(['occur', 'coffee', '-y', '-t', '08:00']);
		// In no chain it starts one first, and says so on the line before.
		await app.submit('sync new-invite');
		const { secret } = await answer(app);
		assert.ok(secret, 'the invite printed no secret: the test tests nothing');
		// In the chain the invite started, as the Sync widget's «Add agent» runs it.
		await app.submit('sync chain agent --level light --label t --json');
		const agent = await answer(app);
		assert.ok(agent.secret, 'the agent printed no secret: the test tests nothing');
		// Held as the page holds one, by `run`, which keeps nothing; typed bare, it prints the ticket.
		assert.equal((await app.run(['sync', 'pending-join', '--hold', 'TICKETSECRET'])).code, 0);
		await app.submit('sync pending-join');
		assert.match((await app.stdouts()).at(-1), /TICKETSECRET/);
		// Signed in, as a test signs in (syncaccount.test.mjs): `sync account` prints the kept key.
		await app.page.evaluate((s) => window.datum.sync.signIn(undefined, s), '07'.repeat(32));
		await app.submit('sync account');
		const { kept: account } = await answer(app);
		assert.ok(account, 'sync account printed no kept key: the test tests nothing');
		// A command that answers no secret is kept whole.
		await app.submit('tail coffee');

		const disk = await stored(app);
		assert.match(disk['datum.stale.v1'] ?? '', /tail coffee/, 'no copy of the CLI widget was kept: its half is untested');
		const all = JSON.stringify(disk);
		for (const s of [agent.secret, secret, 'TICKETSECRET', account]) assert.ok(!all.includes(s), `${s} is on disk`);
		const kept = JSON.parse(disk['datum.repl.v1']);
		assert.deepEqual(
			kept.scrollback.map((c) => [c.line, c.result.stdout === 'output not kept: it held a secret']),
			[
				['sync new-invite', true],
				['sync chain agent --level light --label t --json', true],
				['sync pending-join', true],
				['sync account', true],
				['tail coffee', false]
			]
		);
		assert.match(all, /coffee/, 'the tail was not kept: the store held nothing to look in');

		await app.reload();
		await app.page.waitForFunction(() => document.querySelectorAll('.cli .echo').length === 5);
		assert.ok(!(await app.outputs()).join('\n').includes(agent.secret));
		assert.deepEqual(app.errors, []);
	});
});
