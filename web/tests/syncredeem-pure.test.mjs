// A person's join while the leader's background redeem of the kept join is
// already dialing (da-smeca): the join waits it out rather than send the one
// invite a second time, which read «invite already used», and a redeem that
// admitted this copy to the link's chain is the join's answer.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();

/** The ops the sync worker was sent, answered by `answer`. */
const sent = [];
let answer = () => null;
globalThis.Worker = class {
	postMessage(m) {
		sent.push(m.op);
		queueMicrotask(() => {
			let data;
			try {
				data = { id: m.id, ok: true, value: answer(m) };
			} catch (e) {
				data = { id: m.id, ok: false, error: e.message };
			}
			this.onmessage({ data });
		});
	}
};
const { Leader } = await import('../src/lib/sync.ts');

const admitted = { id: 'p', label: 'laptop', added: true, redeemed: true, sent: 0, got: 0, conflicts: 0, verify: 'verify ok', vector: {} };

/** A leader holding a join kept for chain `c0ffee`, its redeem held open until `release`. */
function leader() {
	const l = new Leader('datum');
	Object.assign(l, { read: true, endpoint: 'e', pendingJoin: { from: 'laptop', chain: 'c0ffee', since: '' } });
	l.start = async () => {};
	l.refresh = async () => {};
	let release, fail;
	l.redeeming = new Promise((r, f) => ((release = r), (fail = f))).finally(() => (l.redeeming = undefined));
	sent.length = 0;
	return { l, release, fail };
}

const ticks = () => new Promise((r) => setTimeout(r, 10));

test('a join waits out a redeem already dialing, then dials once it kept nothing', { timeout: 30_000 }, async () => {
	const { l, release } = leader();
	answer = (m) => (m.op === 'connect' ? { ...admitted, sent: 1 } : null);
	const join = l.call({ op: 'connect', ticket: 't' });
	await ticks();
	assert.deepEqual(sent, [], 'nothing sent while the redeem dials');
	assert.equal(l.connecting, false);
	release(null);
	assert.equal((await join).sent, 1);
	assert.deepEqual(sent, ['connect']);
});

test('a redeem that admitted this copy to the link’s chain is the join’s answer, the invite not sent again', { timeout: 30_000 }, async () => {
	const { l, release } = leader();
	answer = (m) => (m.op === 'about' ? { from: 'laptop', chain: 'c0ffee' } : assert.fail(`${m.op} sent`));
	const join = l.call({ op: 'connect', ticket: 't' });
	await ticks();
	release(admitted);
	assert.equal(await join, admitted);
	assert.deepEqual(sent, ['about']);
});

test('a redeem that admitted this copy to another chain leaves the link to dial', { timeout: 30_000 }, async () => {
	const { l, release } = leader();
	answer = (m) => (m.op === 'about' ? { from: 'desk', chain: 'beef' } : { ...admitted, id: 'q' });
	const join = l.call({ op: 'connect', ticket: 't' });
	release(admitted);
	assert.equal((await join).id, 'q');
	assert.deepEqual(sent, ['about', 'connect']);
});

test('a redeem that failed admitted nothing: the join dials, not fails with it', { timeout: 30_000 }, async () => {
	const { l, fail } = leader();
	answer = () => ({ ...admitted, id: 'q' });
	const join = l.call({ op: 'connect', ticket: 't' });
	fail(new Error('the refresh failed'));
	assert.equal((await join).id, 'q');
	assert.deepEqual(sent, ['connect']);
});
