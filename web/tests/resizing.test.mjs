// `settle` (#lib/layout/resizing.ts) in node, on a mock clock: what a drag
// ASKS a pane to draw. The browser tests cannot count that — they count the
// CLI's runs, and a pane keeps one run out (CliPane's `refresh`), so on a
// loaded box a second's drag is one run however often it asked (da-i1ok).

import test from 'node:test';
import assert from 'node:assert/strict';
import { resizing, settle, THROTTLE_MS } from '../src/lib/layout/resizing.ts';

test('a drag at pointer rate applies the newest box once a throttle period, and the drop the last one at once', (t) => {
	t.mock.timers.enable({ apis: ['setTimeout'] });
	const applied = [];
	const settled = settle((box) => applied.push(box));
	settled.push(0);
	assert.deepEqual(applied, [0], 'the first box waits for nothing');

	resizing.set(true);
	let at = 0;
	for (; at < 1000; at += 16) {
		settled.push(at);
		t.mock.timers.tick(16);
	}
	// A period starts at the first move after the last tick, so it is the
	// throttle's plus up to one pointer frame.
	const during = applied.length - 1;
	assert.equal(during, Math.floor(1000 / (THROTTLE_MS + 16)), `${during} redraws in a second's drag`);
	assert.ok(applied.slice(1).every((box, i, all) => i === 0 || box > all[i - 1]), `not the newest each time: ${applied}`);

	resizing.set(false);
	assert.equal(applied.at(-1), at - 16, 'the drop did not apply the last box at once');
	const dropped = applied.length;
	t.mock.timers.tick(THROTTLE_MS);
	assert.equal(applied.length, dropped, 'a tick still came after the drop');
	settled.stop();
});
