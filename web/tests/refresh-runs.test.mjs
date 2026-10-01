// What one command costs the panes (da-t6k2a): the command, then ONE
// refresh — each pane's argv once, and the same refresh whether the command
// read or wrote.
//
// Counted at the seam with argv whole. A counter that kept three words of
// each saw the strip's two `dayview`s — its geometry (`--show json`) and
// today's picture — as one run made twice, and a minute tick (the strip's
// `tock` and nowview's `$minute`, tests/tick.test.mjs) landing inside an
// `occur`'s window as a second refresh after the write. The clock is held
// here, so no tick lands in the window.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The typed line's own run, and every run after it until the page is quiet. */
async function runsAfter(app, line) {
	const seen = await app.spyOnRunner();
	await app.submit(line);
	await app.idle();
	// A second round starts after the first settles, if one comes — an echo
	// of the write, or a pane hearing its own data — so wait, then settle again.
	await app.page.waitForTimeout(500);
	await app.idle();
	const [typed, ...refresh] = (await seen()).map((argv) => argv.join(' '));
	return { typed, refresh };
}

test('a command runs each pane once, and a write the same refresh as a read', async () => {
	await withApp(async (app) => {
		await app.idle();
		const read = await runsAfter(app, 'tail');
		const write = await runsAfter(app, 'occur coffee');
		assert.equal(read.typed, 'tail');
		assert.match(write.typed, /^occur coffee/);
		for (const pane of ['fields --json', 'dayview', 'nowview', 'tail --json'])
			assert.ok(read.refresh.some((argv) => argv.startsWith(pane)), `${pane} refreshed: ${read.refresh.join(' | ')}`);
		for (const { refresh } of [read, write])
			assert.deepEqual(refresh.filter((argv, i) => refresh.indexOf(argv) !== i), [], refresh.join(' | '));
		assert.deepEqual([...write.refresh].sort(), [...read.refresh].sort());
	}, DESKTOP);
});
