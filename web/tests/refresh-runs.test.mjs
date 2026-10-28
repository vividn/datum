// What one command costs the panes (da-t6k2a, da-fprk0): a write, then ONE
// refresh — each pane's argv once; a read, nothing past itself. The gate is
// the worker's change counters (`RunResult.changed`), so a read typed at the
// bar re-runs no pane and leaves the widget bar alone.
//
// Counted at the seam with argv whole. A counter that kept three words of
// each saw the strip's two `dayview`s — its geometry (`--show json`) and
// today's picture — as one run made twice, and a minute tick (the strip's
// `tock` and nowview's `$tenSeconds`, tests/tick.test.mjs) landing inside an
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

test('a write runs each pane once, and a read runs nothing past itself', async () => {
	await withApp(async (app) => {
		await app.idle();
		const write = await runsAfter(app, 'occur coffee');
		assert.match(write.typed, /^occur coffee/);
		// The nowview's is its day's data layer (da-0e06p): the overlay it lays it under holds no data.
		for (const pane of ['fields --json', 'dayview --width', 'dayview --data-layer', 'tail --json'])
			assert.ok(write.refresh.some((argv) => argv.startsWith(pane)), `${pane} refreshed: ${write.refresh.join(' | ')}`);
		assert.deepEqual(write.refresh.filter((argv, i) => write.refresh.indexOf(argv) !== i), [], write.refresh.join(' | '));

		for (const line of ['tail', 'stats']) {
			const read = await runsAfter(app, line);
			// Its echo asked for, or asked again without (`runEchoed`): itself either way.
			const itself = [line, `${line} --show json`];
			assert.ok(itself.includes(read.typed), read.typed);
			assert.deepEqual(read.refresh.filter((argv) => !itself.includes(argv)), [], `${line} re-ran panes`);
		}
	}, DESKTOP);
});

// A database switch is no command, so no `changed` reports it: it is a LOAD
// (Brand.svelte), which runs every pane on the database it lands on. The sync
// widget's «Other databases» are links, which SvelteKit would otherwise take
// as a client-side navigation: the panes stayed on the database left, and
// went on hearing its writes (da-tlqde).
test('the sync widget\'s link to another database loads the page on it', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await page.evaluate(() => window.datum.runQuiet(['--db', 'work', 'occur', 'tea']));
		await app.goto('/sync');
		const widget = page.locator('section.pane[data-title="sync"]');
		await widget.locator('.others li[data-db="work"] a').click();
		await page.waitForURL(/[?&]db=work/);
		// Loaded on `work`, the widget has `datum` among the others.
		await widget.locator('.others li[data-db="datum"]').waitFor();
	}, DESKTOP);
});
