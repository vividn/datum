// datum.day and a terminal on one machine (da-shfv4): the page invites, the
// CLI connects — vividn's direction, sync.test.mjs's `pairWith` being the
// reverse — and they sync both ways. Then each leg that can be down is taken
// down, and the Sync widget's reason under the terminal's line says which:
// the other copy not running (no `sync run`, no daemon), or this copy's own
// relay. «timed out» alone said neither.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, nativeCopy, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
import { until } from './support/until.mjs';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

const WIDGET = 'section.pane[data-title="sync"]';
/** A dial to a copy that is not there fails in iroh's 30 s, the relay's in the wire's 60 s. */
const DIAL = 90_000;

test('the page invites, the CLI connects, they sync both ways, and the widget names a leg that is down', { timeout: 280_000 }, async () => {
	const native = await nativeCopy(relay);
	try {
		await withWallClock(
			async (app) => {
				await app.run(['occur', 'left', '-t', '0800']);
				assert.equal((await native.run(['occur', 'right', '-t', '0900'])).code, 0);
				const link = await app.page.evaluate(async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`);
				const joined = await native.run(['sync', 'chain', 'connect', link]);
				assert.equal(joined.code, 0, joined.stderr);
				assert.match(joined.stdout, /verify ok/);
				assert.equal(await countOf(app, 'right'), 1);
				assert.equal(await native.count('left'), 1);

				// Both ways, each copy's write reaching the other by itself.
				const running = native.spawn(['sync', 'run']);
				await app.run(['occur', 'page', '-t', '1000']);
				await until(() => native.count('page'), (n) => n === 1);
				assert.equal((await native.run(['occur', 'terminal', '-t', '1100'])).code, 0);
				await until(() => countOf(app, 'terminal'), (n) => n === 1);
				await until(async () => [await native.verify(), await verifyOf(app)], ([a, b]) => a === b);

				await app.goto('/sync');
				const reason = app.page.locator(`${WIDGET} .devices li:not(.here) .reason`);

				// The terminal stops: the page's next push reaches the relay, and nobody there.
				running.child.kill();
				await app.run(['occur', 'unheard', '-t', '1200']);
				const down = await until(() => reason.innerText().catch(() => ''), (t) => t.includes('could not reach'), DIAL);
				assert.match(down, /is the other copy running\? A terminal syncs while `datum sync run` or its daemon runs/);

				// The relay stops: this copy is the one cut off, and it says so.
				await relay.stop();
				await app.run(['occur', 'offline', '-t', '1300']);
				const cut = await until(() => reason.innerText().catch(() => ''), (t) => t.includes('relay'), DIAL);
				assert.match(cut, /this copy is not connected to its relay \(http:\/\/127\.0\.0\.1:\d+\/?\); is it online\?/);
			},
			{ prepare: relayed(relay) }
		);
	} finally {
		await native.close();
	}
});

// The page's refusal reaches the terminal as a KIND beside its words (da-b6xh6):
// the page's store hears it from `sync handshake`'s stdout, and the terminal's
// next step follows it. Read as any other refusal, it would point at
// `datum sync status` instead.
test('a terminal redeeming a link already used is told by the page to ask for a fresh one', { timeout: 120_000 }, async () => {
	// Its own relay: the test above ends by stopping the file's.
	const own = await startRelay();
	const [first, second] = [await nativeCopy(own), await nativeCopy(own)];
	try {
		await withWallClock(
			async (app) => {
				const link = await app.page.evaluate(async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`);
				const joined = await first.run(['sync', 'chain', 'connect', link]);
				assert.equal(joined.code, 0, joined.stderr);
				const used = await second.run(['sync', 'chain', 'connect', link]);
				assert.notEqual(used.code, 0, used.stdout);
				// «refused: Error: …» is da-16d3s; what is asserted here is the kind.
				assert.match(used.stderr, /invite already used\. Next: ask datum\.day for a fresh invite/);
			},
			{ prepare: relayed(own) }
		);
	} finally {
		await Promise.all([first.close(), second.close()]);
		await own.stop();
	}
});
