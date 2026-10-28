// «Backups» to an S3-compatible bucket (da-pstzw, #lib/s3.ts), against a
// fake S3 on another origin (support/s3.mjs) that does real CORS and checks
// every signature. SigV4 itself is held to AWS's vectors in s3-pure.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { KEY, startS3 } from './support/s3.mjs';

const HOUR = 3_600_000;
const PREFIX = 'datum-backups/';

let s3;
before(async () => {
	await startApp();
	s3 = await startS3(['probe', 'plan', 'reread', 'shared', 'older', 'tabs']);
});
after(async () => {
	await s3?.close();
	await stopApp();
});

async function openSheet(page) {
	await page.locator('.root-bar button.gear').click();
	await page.getByRole('menuitem', { name: 'Backups', exact: true }).click();
	const sheet = page.locator('section.pane[data-title="backups"]');
	await sheet.locator('.backups').waitFor();
	return sheet;
}

/** This device's folder in the bucket, under the prefix (da-help3). */
const folderOf = async (page) => `${PREFIX}client-${await page.evaluate(() => localStorage.getItem('datum-client-id'))}/`;

/** Every file `folder`'s index in `bucket` names is there, and nothing else is in it: its slots. */
function whole(bucket, folder) {
	const objects = s3.objects(bucket);
	const backups = JSON.parse(objects.get(`${folder}index.json`)).dbs.datum.backups;
	const there = [...objects.keys()].filter((k) => k.startsWith(folder)).sort();
	assert.deepEqual(there, [...backups.map((x) => folder + x.file), `${folder}index.json`].sort());
	return backups.map((x) => x.slot);
}

/** Fill the new plan's S3 settings: the fake's, with `fields` over them. */
async function fillS3(sheet, fields = {}) {
	const values = { endpoint: s3.endpoint, region: 'us-east-1', key: KEY.key, secret: KEY.secret, ...fields };
	for (const [field, value] of Object.entries(values)) {
		const input = sheet.locator(`input[data-plan-field="${field}"]`);
		await input.fill(value);
		await input.dispatchEvent('change');
	}
}

/** Press «Add plan» for a plan named `name` on `bucket`. */
async function addPlan(sheet, name, bucket) {
	await fillS3(sheet, { bucket });
	await sheet.locator('input[data-plan-field="name"]').fill(name);
	await sheet.locator('button[data-backup="add"]').click();
	return sheet.locator(`section.plan[data-plan="${name}"]`);
}

describe('an S3-compatible destination', () => {
	test('«Test connection» writes, lists, reads and deletes a probe, and names the step that fails and why', async () => {
		await withApp(async ({ page }) => {
			const sheet = await openSheet(page);
			const press = async () => {
				await sheet.locator('button[data-backup="test"]').click();
				await sheet.locator('[data-probe]').waitFor();
				return sheet.locator('[data-probe] li').allInnerTexts();
			};
			await fillS3(sheet, { bucket: 'probe' });
			assert.deepEqual(await press(), ['✓ write', '✓ list', '✓ read', '✓ delete']);
			assert.deepEqual([...s3.objects('probe').keys()], [], 'the probe was left behind');

			// One line, the step that failed, naming what to fix and the bucket's own code.
			const failed = async (...facts) => {
				const lines = await press();
				assert.equal(lines.length, 1, lines.join(' | '));
				assert.match(lines[0], /^✗ write\b/);
				for (const fact of facts) assert.ok(lines[0].includes(fact), `${lines[0]} does not name ${fact}`);
			};
			await fillS3(sheet, { bucket: 'probe', secret: 'wrong' });
			await failed('credentials', 'SignatureDoesNotMatch');
			await fillS3(sheet, { bucket: 'probe', key: 'AKIDNOBODY' });
			await failed('credentials', 'InvalidAccessKeyId');
			await fillS3(sheet, { bucket: 'nope' });
			await failed('bucket', '«nope»');
			s3.cors = false;
			try {
				await fillS3(sheet, { bucket: 'probe' });
				await failed('CORS');
			} finally {
				s3.cors = true;
			}
		});
	});

	// Each field re-asked the reach and the size, four runs a field, so filling
	// a plan in queued some thirty ahead of its first backup, which a loaded
	// lane took past 30 s to reach (da-a0rcg).
	test('a destination’s fields ask the CLI nothing; the frequency asks the reach again', async () => {
		await withApp(async (app) => {
			const sheet = await openSheet(app.page);
			await sheet.locator('.add .explain').waitFor();
			const seen = await app.spyOnRunner();
			await fillS3(sheet, { bucket: 'plan' });
			await sheet.locator('input[data-plan-field="name"]').fill('cloud');
			await app.idle();
			assert.deepEqual(await seen(), []);

			await sheet.getByLabel('Frequency').selectOption({ label: 'daily' });
			await app.idle();
			assert.deepEqual(
				(await seen()).map((argv) => argv.slice(0, 3).join(' ')),
				['db backup --plan', 'db backup --plan', 'db backup --plan']
			);
		});
	});

	test('off the DEBUG database a plan backs up to the bucket: the slot file and the index, in this device’s folder under the prefix, found again', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			// The destinations on offer; the tab's memory is DEBUG's alone.
			assert.deepEqual(await sheet.locator('select[data-plan-field="kind"] option').evaluateAll((os) => os.map((o) => o.value)), [
				's3',
				'webdav'
			]);
			assert.equal(await sheet.locator('input[data-plan-field="prefix"]').inputValue(), PREFIX);
			assert.equal(await sheet.locator('input[data-plan-field="secret"]').getAttribute('type'), 'password');
			// The secret says how it is kept, under it.
			assert.match(await sheet.locator('label:has(input[data-plan-field="secret"]) + p.note-field').innerText(), /unencrypted|not encrypted/, 'the note under the secret says it is kept unencrypted');
			const origin = await page.evaluate(() => location.origin);
			assert.deepEqual(JSON.parse(await sheet.locator('[data-cors]').textContent())[0].AllowedOrigins, [origin]);

			const plan = await addPlan(sheet, 'cloud', 'plan');
			await plan.locator('li[data-slot="0"]').waitFor();
			const keys = [...s3.objects('plan').keys()].sort();
			const folder = await folderOf(page);
			assert.match(folder, /^datum-backups\/client-web-\w+\/$/);
			assert.equal(keys.length, 2, keys.join(' '));
			assert.ok(keys[0].startsWith(folder) && /\/datum\.slot0\.\d{8}T\d{6}\.\d{3}Z\.jsonl\.gz$/.test(keys[0]), keys[0]);
			assert.equal(keys[1], `${folder}index.json`);
			// The index names its device, for the others to list it by.
			assert.match(JSON.parse(s3.objects('plan').get(keys[1])).label, /^Chrome on \w+$/);

			// Another plan on the same bucket and prefix finds it.
			await fillS3(sheet, { bucket: 'plan' });
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			assert.match(await sheet.locator('[data-found]').innerText(), /\b1\b.*\bdatum\b.*\b0 s\b/);
			assert.deepEqual(app.errors, []);
		});
	});

	// The bucket says when the index was last modified and nothing of how long
	// to keep it, so a browser's cache would answer every read here but the
	// first (da-8g7zo) — the run's own among them, which would rotate an
	// index that is not the bucket's.
	test('a run reads the index from the bucket each time, never from the browser’s cache', async () => {
		await withApp(async (app) => {
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const plan = await addPlan(await openSheet(app.page), 'cloud', 'reread');
			await plan.locator('li[data-slot="0"]').waitFor();

			const before = s3.requests.length;
			await app.clock.forward(HOUR);
			await plan.locator('button[data-backup="now"]').click();
			await plan.locator('li[data-slot="1"]').waitFor();
			const at = await folderOf(app.page);
			const listing = [`GET ${at}index.json`, `GET ${PREFIX}index.json`, 'GET '];
			assert.deepEqual(
				s3.requests.slice(before).map((r) => r.replace(/\.slot1\..*/, '.slot1')),
				// The listing (this device's, the top's, the devices' folders), the CLI's index, its slot and its length, the put, the listing.
				[...listing, `GET ${at}index.json`, `PUT ${at}datum.slot1`, `HEAD ${at}datum.slot1`, `PUT ${at}index.json`, ...listing]
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// index.json is read, rotated and written back, so two devices sharing one
	// lost a slot whenever one put its index between the other's read and put.
	// Each writes its own folder now (da-help3): B is held at its index's put,
	// A takes a whole backup meanwhile, and neither loses anything.
	test('two devices backing up one database to one bucket each keep their own folder, interleaved, and each lists the other’s', async () => {
		await withApp(async (a) => {
			await withApp(async (b) => {
				for (const app of [a, b]) assert.equal((await app.run(['occur', 'coffee'])).code, 0);
				const sheetA = await openSheet(a.page);
				const planA = await addPlan(sheetA, 'a', 'shared');
				await planA.locator('li[data-slot="0"]').waitFor();
				const [atA, atB] = [await folderOf(a.page), await folderOf(b.page)];
				assert.notEqual(atA, atB);

				// A is an hour on (slot 1). B finds A's backups, and keeps its own beside them.
				await a.clock.forward(HOUR);
				await b.clock.forward(2 * HOUR);
				const held = s3.hold((method, key) => method === 'PUT' && key === `${atB}index.json`);
				const sheetB = await openSheet(b.page);
				const planB = await addPlan(sheetB, 'b', 'shared');
				await sheetB.locator('[data-found]').waitFor();
				// A's one, under this device's label and so perhaps this device, earlier; 2 h old.
				assert.match(await sheetB.locator('[data-found]').innerText(), /\b1\b.*\bdatum\b.*\bChrome on \w+.*\bearlier\b.*\b2 h\b/);
				await sheetB.locator('button[data-backup="add"]').click();
				await held.reached;

				await planA.locator('button[data-backup="now"]').click();
				await planA.locator('li[data-slot="1"]:not([data-from])').waitFor();
				held.release();
				await planB.locator('li[data-slot="0"]:not([data-from])').waitFor();

				assert.deepEqual(whole('shared', atA), [0, 1]);
				assert.deepEqual(whole('shared', atB), [0]);
				assert.deepEqual([...s3.objects('shared').keys()].filter((k) => !k.startsWith(atA) && !k.startsWith(atB)), []);
				// B lists A's two as A's, and its own; and B's says the last backup is its own.
				const fromA = planB.locator('li[data-from^="Chrome on "]');
				assert.deepEqual(await fromA.evaluateAll((li) => li.map((l) => l.dataset.slot)), ['1', '0']);
				assert.match(await planB.locator('[data-last]').innerText(), /\b0 s\b/);
				assert.deepEqual([...a.errors, ...b.errors], []);
			});
		});
	});

	// Backups went to the top of the prefix before each device had a folder:
	// listed still, under their own name, and restored from.
	test('backups at the top from before device folders are listed beside this device’s own, and restore', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'cloud', 'older');
			await plan.locator('li[data-slot="0"]').waitFor();
			const backedUp = (await app.allEntries(500)).map((e) => e.hid).sort();
			// Moved to the top: the bucket as a device left it before da-help3.
			const objects = s3.objects('older');
			const at = await folderOf(page);
			for (const [key, bytes] of [...objects]) objects.delete(key), objects.set(key.replace(at, PREFIX), bytes);

			await app.clock.forward(1000);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			await plan.locator('button[data-backup="now"]').click();
			const older = plan.locator('li[data-slot="0"][data-from="before devices had folders"]');
			await older.waitFor();
			await plan.locator('li[data-slot="0"]:not([data-from])').waitFor();
			assert.deepEqual(whole('older', at), [0]);

			await older.locator('button', { hasText: 'Restore' }).click();
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			// It names the backup it restores, by whose it is, and what it takes away.
			const asked = await confirm.innerText();
			assert.ok(asked.includes(await older.getAttribute('data-from')), asked);
			assert.match(asked, /\b1 deleted\b/);
			await confirm.getByRole('button', { name: 'restore', exact: true }).click();
			await sheet.locator('[role="status"]', { hasText: 'datum restored' }).waitFor();
			assert.deepEqual((await app.allEntries(500)).map((e) => e.hid).sort(), backedUp);
			assert.deepEqual(app.errors, []);
		});
	});

	// Two tabs of one device write its one folder: tab A is held at its index
	// put, and tab B is asked for a backup. Run then, B's slot would be in an
	// index A's put replaces, its file named by nothing (da-u050c). One run a device.
	//
	// Past the HTTP cache, because Chromium keeps a tab's request for a URL
	// behind another's unanswered PUT of it for 20 s (measured: B's four on
	// the index arrived 20 s apart), so a B that did run would only be seen
	// to once this test had timed out. Past it, B is done in 200 ms.
	test('two tabs of one device take a plan’s backups one at a time: the second says the first is at it, and the index names every file', async () => {
		await withApp(async (a) => {
			assert.equal((await a.run(['occur', 'coffee'])).code, 0);
			const planA = await addPlan(await openSheet(a.page), 'tabs', 'tabs');
			await planA.locator('li[data-slot="0"]').waitFor();
			const b = await a.newTab();
			const planB = (await openSheet(b.page)).locator('section.plan[data-plan="tabs"]');
			await planB.locator('li[data-slot="0"]').waitFor();

			const at = await folderOf(a.page);

			// A is an hour on (slot 1), B two (slot 2).
			await a.clock.forward(HOUR);
			const held = s3.hold((method, key) => method === 'PUT' && key.endsWith('index.json'));
			await planA.locator('button[data-backup="now"]').click();
			await held.reached;
			await a.clock.forward(HOUR);
			await b.page.waitForFunction((at) => window.__datumHeldAt === at, a.clock.at());
			await planB.locator('button[data-backup="now"]').click();
			// B's answer, whichever it is: that A is at it, or a backup taken.
			const answer = planB.locator('.error, li[data-slot="2"]');
			await answer.first().waitFor();
			held.release();
			await planA.locator('li[data-slot="1"]').waitFor();
			assert.deepEqual(whole('tabs', at), [0, 1]);
			// The refusal, naming the other tab; no slot 2.
			const said = await answer.allInnerTexts();
			assert.equal(said.length, 1, said.join(' | '));
			assert.equal(await planB.locator('li[data-slot="2"]').count(), 0);
			assert.match(said[0], /\btab\b/);

			// And B's turn comes: asked again, it rotates the index as A left it.
			await planB.locator('button[data-backup="now"]').click();
			await planB.locator('li[data-slot="2"]').waitFor();
			assert.deepEqual(whole('tabs', at), [0, 1, 2]);
			assert.equal(await planB.locator('.error').count(), 0);
			assert.deepEqual([...a.errors, ...b.errors], []);
		}, { prepare: (context) => context.addInitScript(() => {
			const real = window.fetch;
			window.fetch = (url, init) => real(url, { ...init, cache: 'no-store' });
		}) });
	});
});
