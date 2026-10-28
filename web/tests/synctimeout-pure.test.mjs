// The page's sync waits production's 20 s for a departure to be delivered
// unless a test says less in `datum.syncTimeoutS`, as `withApp` does
// (da-eqjri). The wire's 60 s is `idle_is_a_minute_unless_a_test_says`.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { deliverMs } = await import('../src/lib/sync.ts');

const stored = (value) =>
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: () => value } });

test('a departure waits 20 s, and a test’s seconds where it names some', () => {
	stored(null);
	assert.equal(deliverMs(), 20_000);
	for (const nonsense of ['', '0', '-3', '0.5', 'soon']) {
		stored(nonsense);
		assert.equal(deliverMs(), 20_000, nonsense);
	}
	stored('3');
	assert.equal(deliverMs(), 3_000);
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, get: () => { throw new Error('blocked'); } });
	assert.equal(deliverMs(), 20_000);
});
