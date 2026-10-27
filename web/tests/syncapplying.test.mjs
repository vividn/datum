// A sync's apply outside the join card (da-a7oa4): a page holding a chain
// pulls a few thousand ops from a native copy, and while the apply runs the
// root bar says how far it is — each step holds the database, so a command
// typed meanwhile waits, and nothing else on the page would say why.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { backup, countOf, nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';

const ENTRIES = 3000;

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

test(`a page pulling ${ENTRIES} ops shows how far the apply is in its root bar`, async () => {
	const native = await nativeCopy(relay);
	try {
		await withWallClock(
			async (app) => {
				assert.equal((await pairWith(app, native)).verified, true);
				const restored = await native.run(['db', 'restore', '-', '--merge', '--yes'], backup(ENTRIES));
				assert.equal(restored.code, 0, restored.stderr);
				const status = app.page.locator('nav.root-bar .syncing');
				assert.equal(await status.count(), 0, 'no apply, nothing shown');

				const said = new Set();
				let pushed;
				// The page pushes once more after it joins, to the invite that has
				// ended, and holds that dial until it fails: it runs one session at
				// a time, so a sync meanwhile is refused as busy, and is run again
				// as a person would (da-td7qj).
				do {
					const pushing = native.run(['sync']);
					for (let over = false; !over; ) {
						over = (await Promise.race([pushing.then(() => true), new Promise((r) => setTimeout(() => r(false), 50))])) === true;
						const s = await status.textContent({ timeout: 50 }).catch(() => null);
						if (s) said.add(s);
					}
					pushed = await pushing;
				} while (pushed.stdout.includes('refused: is busy'));
				assert.equal(pushed.code, 0, pushed.stdout);

				const counts = [...said].map((s) => {
					const m = /^syncing ([\d,]+) of ([\d,]+)$/.exec(s);
					assert.ok(m, s);
					return [m[1], m[2]].map((n) => Number(n.replaceAll(',', '')));
				});
				assert.ok(counts.length > 0, 'no progress was shown');
				for (const [done, of] of counts) assert.ok(done < of && of >= ENTRIES, `${done} of ${of}`);
				await status.waitFor({ state: 'detached' });
				assert.equal(await countOf(app, 'tea'), ENTRIES / 3);
			},
			{ prepare: relayed(relay) }
		);
	} finally {
		await native.close();
	}
});
