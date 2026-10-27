// The unjoin confirm’s joint (da-tyj2t): datum.day hands `sync chain unjoin
// <who> --json` straight to `unjoinWords`, and each side is tested alone —
// the CLI's keys in sync_chain.rs, the words on objects built by hand in
// purgewords-pure. Here the real CLI's output meets the real function, so a
// key renamed on one side fails here instead of falling back to «this only
// expels it» for a device whose definitions go.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';
import { nativeCopy, startRelay } from './support/sync.mjs';
import { until } from './support/until.mjs';

resolveLibAsSource(import.meta.url);
const { unjoinWords } = await import('../src/lib/syncView.ts');

let relay;
before(async () => (relay = await startRelay()));
after(() => relay?.stop());

/** What `a` says of purging `b` after `b` did `writes` and joined `a`'s chain. */
async function purgeOf(writes) {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		for (const argv of writes) assert.equal((await b.run(argv)).code, 0, argv.join(' '));
		const serving = a.spawn(['sync', 'chain', 'invite']);
		const [, ticket] = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
		const joined = await b.run(['sync', 'chain', 'connect', ticket]);
		assert.equal(joined.code, 0, joined.stderr);
		const { endpoint } = JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).this;
		const preview = await a.run(['sync', 'chain', 'unjoin', endpoint, '--json']);
		assert.equal(preview.code, 0, preview.stderr);
		return unjoinWords('phone', 'home', JSON.parse(preview.stdout));
	} finally {
		await Promise.all([a.close(), b.close()]);
	}
}

test('a device that signed only a definition is told it goes', async () => {
	assert.match(await purgeOf([['setup', 'coffee', '--verb', 'occur']]), /only 1 definition: this expels it and sets that aside\./);
});

test('a device that made an entry and a definition is told both go', async () => {
	assert.match(
		await purgeOf([
			['setup', 'coffee', '--verb', 'occur'],
			['occur', 'coffee']
		]),
		/: 1 entry it created and 1 definition it signed, /
	);
});
