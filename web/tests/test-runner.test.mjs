// How many files `pnpm test` runs at once — in node, with no browser.
//
// Seven cans each ran this suite at six files on one 24-core box and the merge
// queue starved for 70 minutes (da-x38v). The cap is still six; what shrinks it
// is the load average the box already carries, read once at the run's start.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { filesAtOnce } from '../scripts/concurrency.mjs';

describe('filesAtOnce', () => {
	test('an idle box gets the cap, or its cores if it has fewer', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(4, 0, 6), 3); // an idle CI runner is unchanged
		assert.equal(filesAtOnce(2, 0, 6), 1);
	});

	test('the load shrinks it, and never below one', () => {
		assert.equal(filesAtOnce(24, 88, 6), 1); // the night of da-x38v
		assert.equal(filesAtOnce(24, 71.3, 6), 1);
		assert.equal(filesAtOnce(24, 34, 6), 6);
		assert.equal(filesAtOnce(24, 40, 6), 3);
		assert.equal(filesAtOnce(4, 6, 6), 1);
	});

	test('the override is exact, whatever the load and the cores say', () => {
		assert.equal(filesAtOnce(24, 88, 6, 6), 6); // the queue's own run
		assert.equal(filesAtOnce(4, 0, 6, 3), 3);
		assert.equal(filesAtOnce(24, 0, 6, 12), 12); // above the cap, too
	});

	test('a platform without loadavg reports 0, which is idle', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(24, -1, 6), 6);
	});
});
