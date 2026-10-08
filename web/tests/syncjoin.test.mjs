// A browser's first join of a chain holding years of history (da-89ujs).
//
// The native copy holds `DATUM_JOIN_ENTRIES` generated entries (a `db
// restore` of them), and the page joins its chain through the join link and
// the card's Join, as a person does. The card says how far the join is while
// it runs, and after it the two copies HOLD the same: one verify hash, and
// one `tail --json` byte for byte. How long it took is printed: the recorded
// runs are 20,000 entries (`DATUM_JOIN_ENTRIES=20000`).

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { backup, nativeCopy, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
import { until } from './support/until.mjs';

const ENTRIES = Number(process.env.DATUM_JOIN_ENTRIES ?? 1500);

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

test(`a page joins a chain holding ${ENTRIES} entries, says how far it is, and holds what the chain holds`, async () => {
	const native = await nativeCopy(relay);
	try {
		const restored = await native.run(['db', 'restore', '-', '--yes'], backup(ENTRIES));
		assert.equal(restored.code, 0, restored.stderr);
		await withWallClock(
			async (app) => {
				const serving = native.spawn(['sync', 'chain', 'invite']);
				const [, link] = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
				await app.goto(`/sync/join#${link.split('#').pop()}`);
				const card = app.page.locator('.card.join');
				const join = card.getByRole('button', { name: 'Join', exact: true });
				await join.waitFor();
				const began = Date.now();
				await join.click();
				// Every word the card says until it has joined, and the longest
				// it said one: a chunk that outlasts the native copy's 20-s
				// timeout is carried by the keepalives sent while it runs.
				const said = new Set();
				let [last, since, longest] = ['', Date.now(), 0];
				for (;;) {
					const status = await card.locator('.joining').textContent({ timeout: 100 }).catch(() => null);
					if (status !== last) [last, since] = [status, Date.now()];
					longest = Math.max(longest, Date.now() - since);
					if (status) said.add(status);
					else if (await card.locator('.done').count()) break;
					await new Promise((r) => setTimeout(r, 50));
				}
				const took = Date.now() - began;
				const done = await card.locator('.done').textContent();
				console.log(`joined ${ENTRIES} entries in ${(took / 1000).toFixed(1)} s, one status shown ${(longest / 1000).toFixed(1)} s at most: ${done}`);

				const counts = [...said].flatMap((s) => {
					const m = /^Joining: ([\d,]+) of ([\d,]+) ops applied… A command typed meanwhile waits for the step under way\.$/.exec(s);
					return m ? [[m[1], m[2]].map((n) => Number(n.replaceAll(',', '')))] : [];
				});
				assert.ok(counts.length > 0, `no progress was shown: ${[...said]}`);
				for (const [done, of] of counts) assert.ok(done < of && of >= ENTRIES, `${done} of ${of}`);

				assert.equal(await verifyOf(app), await native.verify());
				const tail = ['tail', '--json', '-n', String(ENTRIES + 10)];
				const mine = (await app.run(tail)).stdout;
				assert.equal(JSON.parse(mine).rows.length, ENTRIES);
				// In the page's colours: it runs the CLI with FORCE_COLOR=1, which the
				// native copy's NO_COLOR=1 would outrank, so that is emptied.
				assert.equal(mine, (await native.run(tail, '', { NO_COLOR: '', FORCE_COLOR: '1' })).stdout);
			},
			{ prepare: relayed(relay) }
		);
	} finally {
		await native.close();
	}
});
