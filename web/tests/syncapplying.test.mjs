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
				// The page renames itself once it joins, and pushes nothing on it:
				// its dial to the invite that had ended held its one session, and a
				// sync meanwhile was refused as busy (da-qlffb).
				const first = await native.run(['sync']);
				assert.equal(first.code, 0, first.stdout);
				const restored = await native.run(['db', 'restore', '-', '--merge', '--yes'], backup(ENTRIES));
				assert.equal(restored.code, 0, restored.stderr);
				const status = app.page.locator('nav.root-bar .syncing');
				assert.equal(await status.count(), 0, 'no apply, nothing shown');

				const said = new Set();
				const pushing = native.run(['sync']);
				for (let over = false; !over; ) {
					over = (await Promise.race([pushing.then(() => true), new Promise((r) => setTimeout(() => r(false), 50))])) === true;
					const s = await status.textContent({ timeout: 50 }).catch(() => null);
					if (s) said.add(s);
				}
				const pushed = await pushing;
				assert.equal(pushed.code, 0, pushed.stdout);

				const counts = [...said].map((s) => {
					// Its two numbers: how many applied, of how many.
					const m = s.match(/\d[\d,]*/g) ?? [];
					assert.equal(m.length, 2, s);
					return m.map((n) => Number(n.replaceAll(',', '')));
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
