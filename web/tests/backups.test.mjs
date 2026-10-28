// «Backups» (da-9ow8m, #lib/backups.ts, layout/BackupsWidget.svelte): the
// gear's place of Tower of Hanoi plans (da-ea5q2), driven here against `memory` — a
// bucket in the tab, offered on the DEBUG database alone (vremya on #217),
// which is why the plan tests open `?db=DEBUG`. The S3 destination is
// s3.test.mjs's. Every number the widget shows is the CLI's, so what is asserted
// is what it SAYS and what the database HOLDS, never slot arithmetic.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const HOUR = 3_600_000;

// On a phone, where a place is a tab whatever da-jozgv makes it on a desktop.
/** The one database the in-memory destination is offered on. */
const DEBUG = { ...PHONE, query: '?db=DEBUG' };

const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

/** The gear's «Backups»: its place, as a tab. */
async function openSheet(page) {
	await page.locator('.root-bar button.gear').click();
	await page.getByRole('menuitem', { name: 'Backups', exact: true }).click();
	const sheet = page.locator('section.pane[data-title="backups"]');
	await sheet.locator('.backups').waitFor();
	return sheet;
}

/** Point the new plan at the tab's memory bucket `bucket`. */
async function inMemory(sheet, bucket) {
	await sheet.locator('select[data-plan-field="kind"]').selectOption('memory');
	await sheet.locator('input[data-plan-field="bucket"]').fill(bucket);
	await sheet.locator('input[data-plan-field="bucket"]').dispatchEvent('change');
}

/** Add an hourly, uncapped plan named `name` on bucket `bucket`, for this page's database. */
async function addPlan(sheet, name, bucket) {
	await sheet.locator('input[data-plan-field="name"]').fill(name);
	await inMemory(sheet, bucket);
	await sheet.locator('button[data-backup="add"]').click();
	return sheet.locator(`section.plan[data-plan="${name}"]`);
}

/** What the sheet says about slot `n` of a plan: its age. */
const ageOf = (plan, n) => plan.locator(`li[data-slot="${n}"] .age`).innerText();

describe('«Backups»', () => {
	test('is the gear’s, a place: a tab beside this one, which back closes; the root tab menu has no export backup…', async () => {
		await withApp(async ({ page, errors }) => {
			const [first, ...rest] = await rootTabs(page);
			const sheet = await openSheet(page);
			assert.match(await sheet.innerText(), /not encrypted/);
			assert.match(await sheet.innerText(), /closed browser takes no backups/);
			assert.deepEqual(await rootTabs(page), [first, 'backups', ...rest]);
			assert.equal(await page.locator('dialog.sheet').count(), 0, 'a modal opened as well');
			await page.goBack();
			await until(() => rootTabs(page), (t) => t.join() === [first, ...rest].join());
			await sheet.waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').click();
			const items = await page.locator('[role="menu"] button').allInnerTexts();
			assert.ok(items.length > 0);
			assert.ok(!items.some((t) => /export backup/.test(t)), `the root menu reads ${items.join(' | ')}`);
			assert.deepEqual(errors, []);
		}, PHONE);
	});

	test('in a pane, it is just the widget', async () => {
		await withApp(
			async ({ page, errors }) => {
				const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
				await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'backups'), tab);
				const pane = page.locator('section.pane[data-title="backups"]');
				await pane.locator('.add fieldset label', { hasText: 'datum' }).waitFor();
				assert.equal(await pane.locator('button[data-backup="save"]').count(), 1);
				assert.deepEqual(await rootTabs(page), ['main']);
				assert.deepEqual(errors, []);
			},
			{ ...DESKTOP, desk: null }
		);
	});

	// Off the DEBUG database the tab's memory is not offered: a tab's bucket
	// behind a plan that survives the reload would look like protection and
	// hold nothing.
	test('offers no memory destination off the DEBUG database, and both file buttons', async () => {
		await withApp(async ({ page }) => {
			const sheet = await openSheet(page);
			assert.equal(await sheet.locator('option', { hasText: 'memory' }).count(), 0);
			assert.equal(await sheet.locator('button[data-backup="save"]').count(), 1);
			assert.equal(await sheet.locator('button[data-backup="restore-file"]').count(), 1);
		}, PHONE);
	});

	// `_internal` holds every plan's keys, to this bucket and to others: a
	// backup of it would carry them all into a bucket as plain JSONL.
	test('never offers _internal as a database to back up', async () => {
		await withApp(async ({ page }) => {
			// The first open reads the plans from `_internal`, so by the second it is in the pool.
			await openSheet(page);
			await page.goBack();
			await until(() => rootTabs(page), (t) => !t.includes('backups'));
			const sheet = await openSheet(page);
			const offered = sheet.locator('.add fieldset label');
			await offered.first().waitFor();
			const names = await offered.allInnerTexts();
			assert.ok(names.includes('datum'), `offered: ${names.join(', ')}`);
			assert.ok(!names.includes('_internal'), `offered: ${names.join(', ')}`);
		}, PHONE);
	});

	test('a plan writes slot 0, then the next period’s slot, and shows both with their ages', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			// The explanation is the CLI's `--plan`: files and reach, per span.
			await sheet.locator('.add .explain').waitFor();
			assert.match(await sheet.locator('.add .explain').innerText(), /after 1 d, \d+ files reaching back/);

			// A new plan is due at once: its first backup is slot 0.
			const plan = await addPlan(sheet, 'hourly', 'b1');
			await plan.locator('li[data-slot="0"]').waitFor();
			assert.equal(await ageOf(plan, 0), '0 s ago');
			assert.match(await plan.locator('[data-last]').innerText(), /^last backup 0 s ago$/);

			// An hour on, «Back up now» takes the next period's: slot 1.
			await app.clock.forward(HOUR);
			await plan.locator('button[data-backup="now"]').click();
			await plan.locator('li[data-slot="1"]').waitFor();
			assert.equal(await ageOf(plan, 1), '0 s ago');
			assert.equal(await ageOf(plan, 0), '1 h ago');
			// Newest first.
			assert.deepEqual(await plan.locator('li[data-slot]').evaluateAll((li) => li.map((l) => l.dataset.slot)), ['1', '0']);

			// Another plan on the same bucket is told what is already there
			// before it writes anything.
			await inMemory(sheet, 'b1');
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			const found = '2 backups of DEBUG found, newest 0 s ago Add plan continues that rotation.';
			assert.equal(await sheet.locator('[data-found]').innerText(), found);

			// And «Add plan» without looking first says the same, and writes nothing yet.
			await inMemory(sheet, 'b1');
			await sheet.locator('input[data-plan-field="name"]').fill('again');
			await sheet.locator('button[data-backup="add"]').click();
			await sheet.locator('[data-found]').waitFor();
			assert.equal(await sheet.locator('[data-found]').innerText(), found);
			assert.equal(await sheet.locator('section.plan[data-plan="again"]').count(), 0, 'the plan was added unseen');
			// Added, it reads the same rotation.
			await sheet.locator('button[data-backup="add"]').click();
			const again = sheet.locator('section.plan[data-plan="again"]');
			await again.locator('li[data-slot="1"]').waitFor();
			assert.deepEqual(await again.locator('li[data-slot]').evaluateAll((li) => li.map((l) => l.dataset.slot)), ['1', '0']);
			assert.equal(await ageOf(again, 1), '0 s ago');
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// da-r6hsj: a device whose client id changed (cleared site data, a fresh
	// install) leaves its earlier folder under its own label, which listed as
	// another device's with the same name.
	test('another folder under this device’s label says it may be this device, earlier; one under another label does not', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b6');
			await plan.locator('li[data-slot="0"]').waitFor();
			// This device's folder, copied as two others': its own label, and another's.
			const label = await page.evaluate(async () => {
				const bucket = window.datum.backupKinds.memory.open({ bucket: 'b6' });
				const at = `client-${localStorage.getItem('datum-client-id')}/`;
				const index = JSON.parse(new TextDecoder().decode(await bucket.get(`${at}index.json`)));
				const file = index.dbs.DEBUG.backups[0].file;
				for (const [folder, label] of [['client-earlier/', index.label], ['client-other/', 'Firefox on macOS']]) {
					await bucket.put(folder + file, await bucket.get(at + file));
					await bucket.put(`${folder}index.json`, new TextEncoder().encode(JSON.stringify({ ...index, label })));
				}
				return index.label;
			});
			await inMemory(sheet, 'b6');
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			assert.equal(
				await sheet.locator('[data-found]').innerText(),
				`1 backup of DEBUG found, newest 0 s ago; 1 backup of DEBUG from ${label} (this device, earlier?) found, newest 0 s ago; ` +
					'1 backup of DEBUG from Firefox on macOS found, newest 0 s ago Add plan continues that rotation.'
			);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// da-5okjx: a run removed the backup it replaced on the destination's
	// yes alone, so a put cut short lost the good backup with the bad.
	test('a slot that arrives short fails the run and keeps the backup it would replace; a whole one replaces it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b5');
			await plan.locator('li[data-slot="0"]').waitFor();
			await app.clock.forward(HOUR);
			await plan.locator('button[data-backup="now"]').click();
			await plan.locator('li[data-slot="1"]').waitFor();

			/** The file this device's index in the bucket names for slot 1. */
			const slotOne = () =>
				page.evaluate(async () => {
					const at = `client-${localStorage.getItem('datum-client-id')}/`;
					const index = await window.datum.backupKinds.memory.open({ bucket: 'b5' }).get(`${at}index.json`);
					return at + JSON.parse(new TextDecoder().decode(index)).dbs.DEBUG.backups.find((b) => b.slot === 1).file;
				});
			const held = (file) => page.evaluate(async (file) => (await window.datum.backupKinds.memory.open({ bucket: 'b5' }).get(file)) !== null, file);
			const good = await slotOne();

			// The destination says yes to every put, and keeps a slot file but for its last byte.
			await page.evaluate(() => {
				const memory = window.datum.backupKinds.memory;
				const whole = memory.open;
				memory.open = (config) => {
					const dest = whole(config);
					return { ...dest, put: (file, bytes) => dest.put(file, file.endsWith('index.json') ? bytes : bytes.subarray(0, -1)) };
				};
				memory.open.whole = whole;
			});
			// The same period: this run replaces slot 1's file.
			await app.clock.forward(60_000);
			await plan.locator('button[data-backup="now"]').click();
			const alert = plan.locator('p.error[role="alert"]');
			await alert.waitFor();
			const [, got, want] = (await alert.innerText()).match(
				/^DEBUG\.slot1\.\S+ reached the destination as (\d+) of (\d+) bytes; the backup it would replace is kept$/
			);
			assert.equal(Number(got), Number(want) - 1);
			assert.equal(await slotOne(), good, 'the index names the short file');
			assert.equal(await held(good), true, 'the good backup was removed');

			// Whole again, the next run replaces it as before.
			await page.evaluate(() => {
				const memory = window.datum.backupKinds.memory;
				memory.open = memory.open.whole;
			});
			await app.clock.forward(60_000);
			await plan.locator('button[data-backup="now"]').click();
			await until(slotOne, (file) => file !== good);
			assert.equal(await held(good), false, 'the replaced backup was kept');
			await alert.waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	test('restore shows the dry run’s counts, and after yes the database is the backup’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b2');
			await plan.locator('li[data-slot="0"]').waitFor();
			const backedUp = (await app.allEntries(500)).map((e) => e.hid).sort();

			await app.clock.forward(1000);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			assert.equal((await app.allEntries(500)).length, backedUp.length + 1);

			await plan.locator('li[data-slot="0"] button', { hasText: 'Restore' }).click();
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			assert.match(await confirm.innerText(), /: 1 deleted, 0 added, 0 changed\./);
			await confirm.getByRole('button', { name: 'restore', exact: true }).click();
			await sheet.locator('[role="status"]', { hasText: 'DEBUG restored' }).waitFor();
			assert.deepEqual((await app.allEntries(500)).map((e) => e.hid).sort(), backedUp);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// da-vceux: the dry run's `warnings` and the real run's `⚠` lines were
	// dropped, so a backup whose header disagrees with its lines was
	// confirmed and reported as if it were whole.
	test('restore names the backup’s warnings, in the confirm and in the note after it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			// The backup the destination is given declares one entry more than it holds.
			await page.evaluate(() => {
				const lines = (bytes) => new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
				const previous = window.datum.setRunner(async (...args) => {
					const result = await previous(...args);
					if (!args[0].includes('--to')) return result;
					for (const [file, bytes] of Object.entries(result.stage ?? {})) {
						if (file === 'index.json') continue;
						const [head, ...rest] = (await new Response(lines(bytes)).text()).split('\n');
						const header = JSON.parse(head);
						header.entry_count += 1;
						const text = [JSON.stringify(header), ...rest].join('\n');
						const gz = new Blob([text]).stream().pipeThrough(new CompressionStream('gzip'));
						result.stage[file] = new Uint8Array(await new Response(gz).arrayBuffer());
					}
					return result;
				});
			});
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b4');
			await plan.locator('li[data-slot="0"]').waitFor();

			await plan.locator('li[data-slot="0"] button', { hasText: 'Restore' }).click();
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			const mismatch = /⚠ Entry-count mismatch: header declared (\d+), restored (\d+)/;
			const asked = await confirm.innerText();
			assert.match(asked, mismatch);
			const [, declared, restored] = asked.match(mismatch);
			assert.equal(Number(declared), Number(restored) + 1, asked);
			await confirm.getByRole('button', { name: 'restore', exact: true }).click();
			const note = sheet.locator('[role="status"]', { hasText: 'DEBUG restored' });
			await note.waitFor();
			assert.match(await note.innerText(), mismatch);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// da-00nmm: «Restore from a file» showed stdout's last line alone.
	test('restore from a file names the backup’s warnings in the note after it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const backup = await app.run(['db', 'backup', '-']);
			assert.equal(backup.code, 0, backup.stderr);
			const [head, ...rest] = backup.stdout.split('\n');
			const header = JSON.parse(head);
			header.entry_count += 1;
			const buffer = Buffer.from([JSON.stringify(header), ...rest].join('\n'));

			const sheet = await openSheet(page);
			const [chooser] = await Promise.all([
				page.waitForEvent('filechooser'),
				sheet.locator('button[data-backup="restore-file"]').click()
			]);
			await chooser.setFiles({ name: 'backup.jsonl', mimeType: 'application/jsonl', buffer });
			// It deletes nothing, but its warning asks first (da-fnjku).
			const mismatch = /⚠ Entry-count mismatch: header declared (\d+), restored (\d+)/;
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			assert.match(await confirm.innerText(), mismatch);
			await confirm.getByRole('button', { name: 'restore', exact: true }).click();
			const note = sheet.locator('[role="status"]', { hasText: 'Restore:' });
			await note.waitFor();
			assert.match(await note.innerText(), mismatch);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// da-4gfdi: «Delete plan» deleted at once from a button drawn as display.
	// It opens a confirm now, so it is SINGLE. act-taps-dialogs walks the
	// place as the desktop's modal; its kinds are checked here on a phone's tab too.
	test('«Delete plan» opens a confirm naming the plan: cancel keeps it, delete removes it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b3');
			await plan.locator('li[data-slot="0"]').waitFor();

			const kinds = await sheet
				.locator('.backups button')
				.evaluateAll((bs) => bs.map((b) => `${b.textContent.trim()}: ${b.dataset.act ?? 'no kind'}`));
			assert.ok(kinds.length >= 7, `found only ${kinds.length} buttons`);
			assert.deepEqual(kinds.filter((k) => k.endsWith('no kind')), [], 'give every button in Backups use:act');
			assert.ok(kinds.includes('Delete plan: open'), kinds.join(' | '));

			const remove = plan.locator('button[data-backup="delete"]');
			const confirm = page.locator('dialog.confirm');
			await remove.click();
			await confirm.waitFor();
			assert.match(await confirm.innerText(), /Delete the plan hourly from this device\? The backups already at its destination stay there\./);
			await confirm.getByRole('button', { name: 'cancel', exact: true }).click();
			await confirm.waitFor({ state: 'detached' });
			assert.equal(await plan.count(), 1, 'cancel deleted the plan');

			await remove.click();
			await confirm.getByRole('button', { name: 'delete', exact: true }).click();
			await plan.waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// canary-qwen-3-8-2-4t on #354: `db restore` opens it scrolled to the
	// backups to restore from (`toRestore`), and `db backup` does not — on a
	// box short enough that they start below the fold, which the `db backup`
	// arm is what proves.
	for (const [sub, shown] of [
		['backup', false],
		['restore', true]
	]) {
		test(`db ${sub} opens it ${shown ? 'scrolled to' : 'above'} the backups to restore from`, async () => {
			await withApp(
				async (app) => {
					const plan = { name: 'r', kind: 'memory', config: { bucket: 'r' }, dbs: ['DEBUG'], every: 3600, slots: null };
					const argv = ['--db', '_internal', 'occur', 'backup_plan', '--id', '%name', '-k', 'name="r"', '-k', `plan=${JSON.stringify(plan)}`];
					assert.equal((await app.run(argv)).code, 0);
					const { page } = app;
					const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
					await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'command'), tab);
					await page.locator('.bar.commands button[data-command="db"]').click();
					await page.locator(`.bar.commands button[data-command="db ${sub}"]`).click();
					const restore = page.locator('section.pane[data-title="backups"] [data-restore="DEBUG"]');
					await restore.waitFor();
					const inView = () =>
						restore.evaluate((el) => {
							let box = el.parentElement;
							while (box && getComputedStyle(box).overflowY !== 'auto' && getComputedStyle(box).overflowY !== 'scroll') box = box.parentElement;
							const r = el.getBoundingClientRect();
							const b = box.getBoundingClientRect();
							return r.top >= b.top && r.top < b.bottom;
						});
					if (shown) {
						await until(inView, (v) => v);
						// da-p3bc8: the plan's reach, drawn above them, landed after the
						// scroll and pushed them back out of view on a loaded runner.
						assert.equal(await page.locator('section.plan[data-plan="r"] .explain').count(), 1, 'scrolled before the reach was drawn');
					} else {
						await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
						assert.equal(await inView(), false, 'the backups start in view: the box is too tall to tell');
					}
					assert.deepEqual(app.errors, []);
				},
				{ viewport: { width: 1280, height: 420 }, query: '?db=DEBUG' }
			);
		});
	}
});
