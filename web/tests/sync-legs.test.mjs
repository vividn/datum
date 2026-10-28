// datum.day and a terminal on one machine (da-shfv4): the page invites, the
// CLI connects — vividn's direction, sync.test.mjs's `pairWith` being the
// reverse — and they sync both ways. Then each leg that can be down is taken
// down, and the terminal's line in the Sync widget reads offline (da-ueb4p),
// its tooltip saying which:
// the other copy not running (no `sync run`, no daemon), or this copy's own
// relay. «timed out» alone said neither.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, freePort, nativeCopy, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
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
/**
 * A dial to a copy that is not there fails in the wire's idle timeout
 * (`SYNC_TIMEOUT_S`), where production's waits iroh's 30 s or its own 60 s.
 */
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
				const line = app.page.locator(`${WIDGET} .devices li:not(.here)`);
				const reason = { innerText: () => line.getAttribute('title').then((t) => t ?? '') };

				// The terminal stops: the page's next push reaches the relay, and nobody there.
				running.child.kill();
				await app.run(['occur', 'unheard', '-t', '1200']);
				// Read with the mark: a dial again in flight reads «syncing now: could not reach …».
				const mark = () => line.getAttribute('data-mark').catch(() => '');
				const [down] = await until(
					async () => [await reason.innerText().catch(() => ''), await mark()],
					([t, m]) => t !== '' && m === 'offline',
					DIAL
				);
				// The other copy's side: it names what runs a terminal's sync.
				assert.match(down, /`datum sync run`/);
				assert.equal(await line.locator('.reason').count(), 0);

				// The relay stops: this copy is the one cut off, and it says so.
				await relay.stop();
				await app.run(['occur', 'offline', '-t', '1300']);
				const cut = await until(() => reason.innerText().catch(() => ''), (t) => /http:\/\/127\.0\.0\.1/.test(t), DIAL);
				// This copy's side: it names its relay, and not the other copy's `sync run`.
				assert.match(cut, /http:\/\/127\.0\.0\.1:\d+/);
				assert.doesNotMatch(cut, /`datum sync run`/);
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
				// The kind, used, and the step it leads to: back to datum.day, not `sync status`.
				assert.match(used.stderr, /refused: .*\bused\b/);
				assert.match(used.stderr, /datum\.day/);
				assert.doesNotMatch(used.stderr, /datum sync status/);
			},
			{ prepare: relayed(own) }
		);
	} finally {
		await Promise.all([first.close(), second.close()]);
		await own.stop();
	}
});

// A page's endpoint waits 10 s for its relay as it binds, and a loaded
// machine can take longer: the invite it then made named no relay, an
// address no terminal reaches («No addressing information available»,
// da-kfhty). The relay here comes up only after those 10 s, and the invite
// waits for it.
test('an invite made before the page reaches its relay names the relay once it is up', { timeout: 120_000 }, async () => {
	const port = await freePort();
	const url = `http://127.0.0.1:${port}`;
	const native = await nativeCopy({ url });
	let late;
	try {
		await withWallClock(
			async (app) => {
				const offered = app.page.evaluate(() => window.datum.sync.offer('datum'));
				await new Promise((resolve) => setTimeout(resolve, 12_000));
				late = await startRelay(port);
				const joined = await native.run(['sync', 'chain', 'connect', await offered]);
				assert.equal(joined.code, 0, joined.stderr);
			},
			{ prepare: relayed({ url }) }
		);
	} finally {
		await native.close();
		await late?.stop();
	}
});
