// How many files `pnpm test` runs at once — in node, with no browser.
//
// Seven cans each ran this suite at six files on one 24-core box and the merge
// queue starved for 70 minutes (da-x38v). The cap is still six; what shrinks it
// is the load average the box already carries, read once at the run's start.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { filesAtOnce } from '../scripts/concurrency.mjs';
import { parseShard, shardFiles } from '../scripts/shard.mjs';

describe('filesAtOnce', () => {
	test('an idle box gets the cap, or its cores if it has fewer', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(4, 0, 6), 2); // an idle CI runner
		assert.equal(filesAtOnce(2, 0, 6), 1);
	});

	// The defect lived only between the cap (24 cores) and the load term (a
	// loaded 3-core box): one core charged a file where the load term charges
	// two, so an idle 6-core box got five files — ten cores of work (da-j2okg).
	test('an idle box is committed to no more cores of work than it has', () => {
		const coresPerFile = 2; // a Chromium and the node driving it
		for (const [cores, files] of [
			[4, 2],
			[6, 3],
			[8, 4]
		]) {
			const got = filesAtOnce(cores, 0, 6);
			assert.equal(got, files, `${cores} cores`);
			assert.ok(got * coresPerFile <= cores, `${got} files is ${got * coresPerFile} cores of work on ${cores}`);
		}
	});

	test('the load shrinks it, and never below one', () => {
		assert.equal(filesAtOnce(24, 88, 6), 1); // the night of da-x38v
		assert.equal(filesAtOnce(24, 71.3, 6), 1);
		assert.equal(filesAtOnce(24, 34, 6), 6);
		assert.equal(filesAtOnce(24, 40, 6), 3);
		assert.equal(filesAtOnce(4, 6, 6), 1);
	});

	test('the override ignores the load', () => {
		assert.equal(filesAtOnce(24, 88, 6, 6), 6); // the queue's own run
		assert.equal(filesAtOnce(12, 8.5, 6, 3), 3); // CI's 3 on the 12-core box
	});

	// CI's 3 on the 3-core forge lane was six cores of work on three (da-dwq7g).
	test('the override is capped by what the cores allow an idle box', () => {
		assert.equal(filesAtOnce(3, 1.9, 6, 3), 1); // the forge lane
		assert.equal(filesAtOnce(4, 0, 6, 3), 2);
		assert.equal(filesAtOnce(12, 0, 6, 8), 6);
		assert.equal(filesAtOnce(24, 0, 6, 12), 6);
	});

	test('a platform without loadavg reports 0, which is idle', () => {
		assert.equal(filesAtOnce(24, 0, 6), 6);
		assert.equal(filesAtOnce(24, -1, 6), 6);
	});
});

// Which files a run takes — the other half of `pnpm test`'s arithmetic. The
// invariant the CI matrix rests on: the M shards, concatenated, are the suite
// exactly once (da-buqa).

describe('shardFiles', () => {
	const files = Array.from({ length: 60 }, (_, i) => `tests/f${i}.test.mjs`);

	test('no shard is every file', () => {
		assert.deepEqual(shardFiles(files, undefined), files);
	});

	test('shard N/M takes index % M === N - 1', () => {
		assert.deepEqual(shardFiles(files, { n: 1, total: 4 }).slice(0, 3), [
			'tests/f0.test.mjs',
			'tests/f4.test.mjs',
			'tests/f8.test.mjs'
		]);
		assert.deepEqual(shardFiles(files, { n: 4, total: 4 })[0], 'tests/f3.test.mjs');
	});

	test('the four shards are the suite, once, and are within one of each other', () => {
		const shards = [1, 2, 3, 4].map((n) => shardFiles(files, { n, total: 4 }));
		assert.deepEqual(shards.flat().sort(), [...files].sort());
		assert.deepEqual(
			shards.map((s) => s.length),
			[15, 15, 15, 15]
		);
	});

	test('more shards than files leaves the tail empty, and loses nothing', () => {
		const three = ['a', 'b', 'c'];
		const shards = [1, 2, 3, 4].map((n) => shardFiles(three, { n, total: 4 }));
		assert.deepEqual(shards.flat().sort(), three);
		assert.deepEqual(shards[3], []);
	});
});

describe('parseShard', () => {
	test('N/M, and nothing at all', () => {
		assert.deepEqual(parseShard('2/4'), { n: 2, total: 4 });
		assert.deepEqual(parseShard(' 1/1 '), { n: 1, total: 1 });
		assert.equal(parseShard(undefined), undefined);
		assert.equal(parseShard(''), undefined); // an unset env var in a workflow
	});

	test('nonsense is loud, never a silently whole suite', () => {
		for (const bad of ['0/4', '5/4', '4', 'a/b', '1/0', '-1/4', '1/4/4']) {
			assert.throws(() => parseShard(bad), /--shard wants/, bad);
		}
	});
});
