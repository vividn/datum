// «Backups» to an S3-compatible bucket (da-pstzw, $lib/s3.ts), against a
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
	s3 = await startS3(['probe', 'plan', 'reread', 'shared', 'tabs']);
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

			await fillS3(sheet, { bucket: 'probe', secret: 'wrong' });
			assert.deepEqual(await press(), ['✗ write failed — credentials: the access key id or secret is wrong (SignatureDoesNotMatch)']);
			await fillS3(sheet, { bucket: 'probe', key: 'AKIDNOBODY' });
			assert.deepEqual(await press(), ['✗ write failed — credentials: the access key id or secret is wrong (InvalidAccessKeyId)']);
			await fillS3(sheet, { bucket: 'nope' });
			assert.deepEqual(await press(), ['✗ write failed — bucket name: there is no bucket «nope» at this endpoint']);
			s3.cors = false;
			try {
				await fillS3(sheet, { bucket: 'probe' });
				assert.match((await press()).join('\n'), /^✗ write failed — CORS: the bucket does not allow this page/);
			} finally {
				s3.cors = true;
			}
		});
	});

	test('off the DEBUG database a plan backs up to the bucket: the slot file and the index, under the prefix, found again', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			// The destinations on offer; the tab's memory is DEBUG's alone.
			assert.deepEqual(await sheet.locator('select[data-plan-field="kind"] option').allInnerTexts(), [
				'S3-compatible bucket (AWS, R2, B2, Wasabi, MinIO)',
				'WebDAV folder (Nextcloud, ownCloud, many NAS boxes)'
			]);
			assert.equal(await sheet.locator('input[data-plan-field="prefix"]').inputValue(), PREFIX);
			assert.equal(await sheet.locator('input[data-plan-field="secret"]').getAttribute('type'), 'password');
			assert.match(await sheet.innerText(), /Kept in this browser, unencrypted\. Use a key scoped to this one bucket/);
			const origin = await page.evaluate(() => location.origin);
			assert.deepEqual(JSON.parse(await sheet.locator('[data-cors]').textContent())[0].AllowedOrigins, [origin]);

			const plan = await addPlan(sheet, 'cloud', 'plan');
			await plan.locator('li[data-slot="0"]').waitFor();
			const keys = [...s3.objects('plan').keys()].sort();
			assert.equal(keys.length, 2, keys.join(' '));
			assert.match(keys[0], /^datum-backups\/datum\.slot0\.\d{8}T\d{6}\.\d{3}Z\.jsonl\.gz$/);
			assert.equal(keys[1], `${PREFIX}index.json`);

			// Another plan on the same bucket and prefix finds it.
			await fillS3(sheet, { bucket: 'plan' });
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			assert.equal(await sheet.locator('[data-found]').innerText(), '1 backup of datum found, newest 0 s ago Add plan continues that rotation.');
			assert.deepEqual(app.errors, []);
		});
	});

	// The bucket says when the index was last modified and nothing of how long
	// to keep it, so a browser's cache would answer every read here but the
	// last (da-8g7zo) — the re-read before the put among them, which is what
	// keeps a second device's slot in the test below.
	test('a run reads the index from the bucket each time, never from the browser’s cache', async () => {
		await withApp(async (app) => {
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const plan = await addPlan(await openSheet(app.page), 'cloud', 'reread');
			await plan.locator('li[data-slot="0"]').waitFor();

			const before = s3.requests.length;
			await app.clock.forward(HOUR);
			await plan.locator('button[data-backup="now"]').click();
			await plan.locator('li[data-slot="1"]').waitFor();
			const index = `${PREFIX}index.json`;
			assert.deepEqual(
				s3.requests.slice(before).map((r) => r.replace(/\.slot1\..*/, '.slot1')),
				// The listing, the CLI's index, its slot, the re-read, the put, the listing.
				[`GET ${index}`, `GET ${index}`, `PUT ${PREFIX}datum.slot1`, `GET ${index}`, `PUT ${index}`, `GET ${index}`]
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// index.json is read, rotated and written back. Device B reads it, and
	// before B puts its own, device A takes a whole backup: B must merge
	// into the index as A left it, not write back the one it read — which
	// would drop A's slot and leave A's file named by nothing.
	test('two devices backing up one database to one bucket keep each other’s slots', async () => {
		await withApp(async (a) => {
			await withApp(async (b) => {
				for (const app of [a, b]) assert.equal((await app.run(['occur', 'coffee'])).code, 0);
				const sheetA = await openSheet(a.page);
				const planA = await addPlan(sheetA, 'a', 'shared');
				await planA.locator('li[data-slot="0"]').waitFor();

				// A is an hour on (slot 1), B two (slot 2).
				await a.clock.forward(HOUR);
				await b.clock.forward(2 * HOUR);
				const held = s3.hold((method, key) => method === 'PUT' && /\.slot2\./.test(key));
				const sheetB = await openSheet(b.page);
				const planB = await addPlan(sheetB, 'b', 'shared');
				await sheetB.locator('[data-found]').waitFor();
				await sheetB.locator('button[data-backup="add"]').click();
				await held.reached;

				await planA.locator('button[data-backup="now"]').click();
				await planA.locator('li[data-slot="1"]').waitFor();
				held.release();
				await planB.locator('li[data-slot="2"]').waitFor();

				const objects = s3.objects('shared');
				const backups = JSON.parse(objects.get(`${PREFIX}index.json`)).dbs.datum.backups;
				assert.deepEqual(
					backups.map((x) => x.slot),
					[0, 1, 2]
				);
				// Every file the index names is there, and nothing else is.
				assert.deepEqual([...objects.keys()].sort(), [...backups.map((x) => PREFIX + x.file), `${PREFIX}index.json`].sort());
				// And B lists A's backup as its own.
				await planB.locator('li[data-slot="1"]').waitFor();
				assert.deepEqual([...a.errors, ...b.errors], []);
			});
		});
	});

	// The re-read covers a writer that finished first, not one that re-read
	// before this one put: tab A is held at its index put, after its re-read,
	// and tab B is asked for a backup. Run then, B's slot would be in an index
	// A's put replaces, its file named by nothing (da-u050c). One run a device.
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

			const objects = s3.objects('tabs');
			const whole = (slots) => {
				const backups = JSON.parse(objects.get(`${PREFIX}index.json`)).dbs.datum.backups;
				// Every file the index names is there, and nothing else is.
				assert.deepEqual([...objects.keys()].sort(), [...backups.map((x) => PREFIX + x.file), `${PREFIX}index.json`].sort());
				assert.deepEqual(
					backups.map((x) => x.slot),
					slots
				);
			};

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
			whole([0, 1]);
			assert.deepEqual(await answer.allInnerTexts(), ['Another datum.day tab is backing up. Try again when it is done.']);

			// And B's turn comes: asked again, it merges into the index as A left it.
			await planB.locator('button[data-backup="now"]').click();
			await planB.locator('li[data-slot="2"]').waitFor();
			whole([0, 1, 2]);
			assert.equal(await planB.locator('.error').count(), 0);
			assert.deepEqual([...a.errors, ...b.errors], []);
		}, { prepare: (context) => context.addInitScript(() => {
			const real = window.fetch;
			window.fetch = (url, init) => real(url, { ...init, cache: 'no-store' });
		}) });
	});
});
