// A database SQLite cannot read has a way out that does not go through
// SQLite (da-0v940): the error says where the file is, keeps a copy of its
// bytes, and resets it — with plain OPFS calls, no CLI process.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';

const OPFS_DIR = 'datum-cli';

before(startApp);
after(stopApp);

/** `datum.db` replaced by 4 KB of random bytes — 4000, not a whole tar block, so the copy shows its padding — under the files' lock; the bytes. */
const garble = (page) =>
	page.evaluate(
		(dir) =>
			navigator.locks.request('datum-cli-files', async () => {
				const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
				const writable = await (await files.getFileHandle('datum.db')).createWritable();
				const bytes = crypto.getRandomValues(new Uint8Array(4000));
				await writable.write(bytes);
				await writable.close();
				return [...bytes];
			}),
		OPFS_DIR
	);

/** Whether `locator` is drawn in the colour the CSS token `token` resolves to (da-ad2lt). */
const inks = (locator, token) =>
	locator.evaluate((el, token) => {
		const probe = el.appendChild(document.createElement('span'));
		probe.style.color = `var(${token})`;
		const same = getComputedStyle(probe).color === getComputedStyle(el).color;
		probe.remove();
		return same;
	}, token);

/** The page after a reload onto the damaged file: its notice, once a command has failed on it. */
async function damaged(page) {
	await page.reload();
	const notice = page.locator('section.damaged');
	await notice.waitFor();
	return notice;
}

/** Press Reset and confirm it, typing the database's name. */
async function reset(page, notice) {
	await notice.getByRole('button', { name: 'Reset this database' }).click();
	const dialog = page.locator('dialog.confirm');
	await dialog.locator('input').fill('datum');
	// A confirm's action is the danger, and red.
	const yes = dialog.getByRole('button', { name: 'reset', exact: true });
	assert.ok(await inks(yes, '--error'));
	await yes.click();
}

describe('a damaged database', () => {
	test('names where it is, hands over its bytes, and resets to a working empty database', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['occur', 'coffee']);
			const bytes = await garble(page);
			const notice = await damaged(page);

			const text = await notice.textContent();
			assert.match(text, /datum-cli/);
			assert.match(text, /OPFS/);
			assert.match(text, /clearing site data in the browser may not/);

			const [download] = await Promise.all([
				page.waitForEvent('download'),
				notice.getByRole('button', { name: 'Download a copy' }).click()
			]);
			assert.equal(download.suggestedFilename(), 'datum-damaged.tar');
			// Read by the system's tar, which checks every header's checksum, and
			// with no complaint: a short padding only warns («A lone zero block»).
			const tar = spawnSync('tar', ['-xOf', await download.path(), 'datum.db']);
			assert.equal(tar.stderr.toString(), '');
			assert.equal(tar.status, 0);
			const copy = tar.stdout;
			assert.deepEqual([...copy], bytes);

			await Promise.all([page.waitForEvent('load'), reset(page, notice)]);
			await ready(page);
			assert.equal(await page.locator('section.damaged').count(), 0);
			assert.equal((await app.run(['count'])).stdout.trim(), '0');
			assert.equal((await app.run(['occur', 'tea'])).code, 0);
		});
	});

	test('says to close the other tab holding the files, and resets once it lets go', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const other = await app.newTab();
			await garble(page);
			const notice = await damaged(page);
			// The other tab keeps the files, as one the system froze mid-command would.
			await other.page.evaluate(() => {
				navigator.locks.request('datum-cli-files', () => new Promise((resolve) => (window.letGo = resolve)));
			});

			await reset(page, notice);
			await notice.locator('.said').waitFor();
			assert.match(await notice.locator('.said').textContent(), /Close every other datum\.day tab and window/);

			await other.page.evaluate(() => window.letGo());
			await Promise.all([page.waitForEvent('load'), reset(page, notice)]);
			await ready(page);
			assert.equal((await app.run(['count'])).stdout.trim(), '0');
			// The other tab, still open, is on the new database too.
			assert.equal((await other.run(['count'])).stdout.trim(), '0');
		});
	});
});
