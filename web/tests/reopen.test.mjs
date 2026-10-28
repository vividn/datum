// A reload always reaches the panes, or says why not (da-89kom).
//
// vividn, 2026-10-07: «after connecting a mobile web browser to a sync chain
// generated on desktop, and refreshing, the mobile view gets hung up on
// "opening database..." I'm completely unable to proceed from here».
//
// One wait on the way to the panes has no deadline and no word: the files'
// Web Lock, which another tab of the site may hold for as long as it likes — a
// tab the phone froze in the background mid-burst would hold it for good
// (suspected; headless Chromium never froze one). The page cannot take the
// files from it, so it names the wait and the way out.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';
import { countOf, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';

const PHONE = { device: devices['Pixel 7'], home: null };

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

test('a phone joins a chain a desktop made, reloads, and reaches its panes with its data', async () => {
	await withWallClock(
		async (desk) => {
			await withWallClock(
				async (phone) => {
					await desk.run(['occur', 'left', '-t', '0800']);
					await phone.run(['occur', 'right', '-t', '0900']);
					const link = await desk.page.evaluate(
						async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`
					);
					await phone.goto(link);
					const card = phone.page.locator('.card.join');
					await card.getByRole('button', { name: 'Join', exact: true }).click();
					await card.locator('.done').waitFor();

					await phone.reload();
					assert.equal(await countOf(phone, 'left'), 1);
					assert.equal(await countOf(phone, 'right'), 1);
					assert.equal(await verifyOf(phone), await verifyOf(desk));
				},
				{ prepare: relayed(relay), ...PHONE }
			);
		},
		{ prepare: relayed(relay) }
	);
});

/** `tab` takes the files' lock and keeps it, as a frozen tab does, until `window.letGo()`. */
const holdFiles = (tab) =>
	tab.page.evaluate(
		() =>
			new Promise((taken) =>
				navigator.locks.request('datum-cli-files', () => {
					taken();
					return new Promise((resolve) => (window.letGo = resolve));
				})
			)
	);

// The line names the other tab, and counts the seconds.
const BLOCKED = /\btab\b.* \d+ s$/;

test('another tab holding the files: the line says so, and the page opens once it lets go', async () => {
	await withApp(async (other) => {
		await other.run(['occur', 'kept']);
		const phone = await other.newTab();
		// The waiting line, frame by frame, from the reload on.
		await phone.page.context().addInitScript(() => {
			const said = (window.__said = []);
			const tick = () => {
				said.push(document.querySelector('.waiting')?.textContent.replace(/\s+/g, ' ').trim() ?? null);
				if (!document.querySelector('.prompt-bar input:not([disabled])')) requestAnimationFrame(tick);
			};
			tick();
		});
		await holdFiles(other);
		await phone.page.reload();
		const line = phone.page.locator('.waiting');
		await line.filter({ hasText: BLOCKED }).waitFor();
		assert.equal(await phone.page.locator('.prompt-bar input').isDisabled(), true);

		await other.page.evaluate(() => window.letGo());
		await ready(phone.page);
		assert.equal(await line.count(), 0);
		assert.equal(await countOf(phone, 'kept'), 1);
		// Up from its first frame to the answer, with no blink as the words
		// change from «Opening your data…» (kimi-k3 on #493).
		const said = await phone.page.evaluate(() => window.__said);
		const shown = said.slice(said.findIndex(Boolean), said.findLastIndex(Boolean) + 1);
		assert.ok(shown.some((l) => BLOCKED.test(l)), JSON.stringify([...new Set(said)]));
		assert.equal(shown.indexOf(null), -1, `the line went and came back: ${JSON.stringify([...new Set(said)])}`);

		// And on a page already open, a command waits, and the line says that.
		await holdFiles(other);
		const counted = phone.run(['count', 'kept']);
		await line.filter({ hasText: BLOCKED }).waitFor();
		await other.page.evaluate(() => window.letGo());
		assert.equal((await counted).stdout, '1');
		await line.waitFor({ state: 'detached' });
	}, PHONE);
});

// A boot that never becomes ready fails naming what it waited on (da-hqpe7):
// on CI one sat 30 s short of an enabled prompt, and its bare TimeoutError
// said nothing more. Here the database worker's script is never answered.
test('a boot that never becomes ready says what it was waiting on', async () => {
	const prepare = async (context) => {
		// The hang-stop shortened for this test alone: the wait is the subject.
		context.setDefaultTimeout(3000);
		await context.route('**/db.worker-*', () => {});
	};
	await assert.rejects(withApp(() => {}, { prepare }), (e) => {
		assert.match(e.message, /Timeout/);
		assert.match(e.message, /the boot: shows "[^"]+"/);
		assert.match(e.message, /unanswered \S+\/db\.worker-/);
		return true;
	});
});
