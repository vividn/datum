// A browser reloaded mid-join resumes the join (da-idw54, da-38eye).
//
// Starting a chain comes after every op of the history it starts on, so a
// first sync carries the inviter's chain records — the joiner's own
// admission among them — last. A join cut short before them left the
// joiner holding part of the history and in no chain: nothing told its page
// to dial, and nothing told the inviter's page it had a member to push to.
// Measured before: 1857 of 3010, still 1857 fifty seconds later.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
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

/** Enough that the join takes seconds: it applies 64 ops a chunk. */
const N = 1000;

/** A `db restore` of `N` entries of `seed`, a minute apart. */
function backup() {
	const lines = [JSON.stringify({ type: 'daydumb_backup', schema_version: 54, backup_time: new Date().toISOString(), source_db: 'seed', entry_count: N })];
	const start = Date.parse('2026-01-01T00:00:00Z');
	for (let i = 0; i < N; i++) {
		const t = new Date(start + i * 60_000).toISOString().replace('.000Z', 'Z');
		lines.push(
			JSON.stringify({
				hid: `s${i.toString(36).padStart(11, '0')}`,
				slug: `seed:${t}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field: 'seed',
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: { i },
				occur_time: { utc: t, offset_minutes: 0, timezone: 'UTC' },
				create_time: t,
				modify_time: t,
				client_id: '01M4BNZ0H79A4P457779B5WTJ8',
				id_structure: '%occur_time%',
				parser_version: 2
			})
		);
	}
	return lines.join('\n');
}

const twoBrowsers = (body) =>
	withWallClock((a) => withWallClock((b) => body(a, b), { prepare: relayed(relay) }), { prepare: relayed(relay) });

test('a browser reloaded mid-join resumes it by itself, and the inviter pushes to it', async () => {
	await twoBrowsers(async (a, b) => {
		const restored = await a.run(['db', 'restore', '-', '--merge', '-y'], backup());
		assert.equal(restored.code, 0, restored.stderr);
		assert.equal(await countOf(a, 'seed'), N);
		const link = await a.page.evaluate(async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`);
		await b.goto(link);
		const join = b.page.locator('.card.join').getByRole('button', { name: 'Join', exact: true });
		await join.waitFor();
		// A name typed at the join outlives the reload (da-uyyyg).
		await b.page.locator('.card.join').getByLabel('This device’s name').fill('kitchen');
		await join.click();
		// Some of the history has arrived, and none of the chain.
		await until(() => countOf(b, 'seed'), (n) => n > 0);
		await b.reload();
		const partial = await countOf(b, 'seed');
		assert.ok(partial < N, `the join finished before the reload (${partial} of ${N}): nothing was cut short`);
		const { chain } = JSON.parse((await a.run(['sync', 'status', '--json'])).stdout);

		await until(() => countOf(b, 'seed'), (n) => n === N, 90_000);
		await until(async () => [await verifyOf(a), await verifyOf(b)], ([x, y]) => x === y);
		// In the chain now, and said what it is, as `join` does.
		const status = JSON.parse((await b.run(['sync', 'status', '--json'])).stdout);
		assert.equal(status.chain, chain);
		await until(
			async () => JSON.parse((await a.run(['sync', 'chain', 'list', '--json'])).stdout).members,
			(members) => members.find((m) => m.endpoint === status.this.endpoint)?.kind === 'web'
		);
		// The describe that told it so is the rename's, under the name typed (da-uyyyg).
		const seen = JSON.parse((await a.run(['sync', 'chain', 'list', '--json'])).stdout).members;
		assert.equal(seen.find((m) => m.endpoint === status.this.endpoint).label, 'kitchen');

		// The inviter's page knows it has a member, and pushes to it.
		await until(
			() => a.page.evaluate(() => Object.keys(window.datum.sync.states().datum?.peers ?? {})),
			(peers) => peers.includes(status.this.endpoint)
		);
		await a.run(['occur', 'later', '-t', '1000']);
		await until(() => countOf(b, 'later'), (n) => n === 1);
	});
});

// A reload after the handshake but before the first op applies (da-38eye):
// the inviter has used the invite, and the joiner holds none of its ops, so
// only its own side of the handshake can say where it resumes.
test('a browser reloaded after its handshake, holding nothing yet, resumes it by itself', async () => {
	await twoBrowsers(async (a, b) => {
		const restored = await a.run(['db', 'restore', '-', '--merge', '-y'], backup());
		assert.equal(restored.code, 0, restored.stderr);
		const link = await a.page.evaluate(async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`);
		const inviter = JSON.parse((await a.run(['sync', 'status', '--json'])).stdout);
		await b.goto(link);
		await b.page.evaluate(() => localStorage.setItem('datum.syncHoldApply', '1'));
		const join = b.page.locator('.card.join').getByRole('button', { name: 'Join', exact: true });
		await join.waitFor();
		await join.click();
		const list = async () => {
			const r = await b.run(['sync', 'chain', 'list', '--json']);
			return r.code === 0 ? JSON.parse(r.stdout) : null;
		};
		const joining = (await until(list, (l) => l?.joining?.length > 0)).joining;
		assert.equal(joining[0].endpoint, inviter.this.endpoint);
		assert.equal(await countOf(b, 'seed'), 0, 'an op applied before the reload: the cut is not before the first one');
		await b.page.evaluate(() => localStorage.removeItem('datum.syncHoldApply'));
		await b.reload();

		await until(() => countOf(b, 'seed'), (n) => n === N, 90_000);
		await until(async () => [await verifyOf(a), await verifyOf(b)], ([x, y]) => x === y);
		assert.equal(JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).chain, inviter.chain);
	});
});
