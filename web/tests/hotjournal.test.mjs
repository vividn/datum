// A write cut short leaves its journal HOT, and the next process rolls it
// back (da-dfmg8). SQLite's WASI build locks with dot-files by default,
// and that VFS reads its own lock as another process's reserved one, so no
// journal was ever hot: a tab closed, killed or crashed mid-commit left
// half-written pages that every command after read as they were —
// «database disk image is malformed», vividn's datum.day on 2026-10-12.
//
// The cut is made by hand, as SQLite would leave it: page 1 kept in a
// one-record journal (https://sqlite.org/fileformat.html#the_rollback_journal),
// then overwritten in the database.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

test('a journal a cut write left hot is rolled back by the next command', async () => {
	await withApp(async (app) => {
		for (let i = 0; i < 3; i++) assert.equal((await app.run(['occur', 'coffee', '-t', `${10 + i}:00`])).code, 0);
		const cut = await app.page.evaluate(() =>
			navigator.locks.request('datum-cli-files', async () => {
				const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('datum-cli');
				const file = await dir.getFileHandle('datum.db');
				const db = new Uint8Array(await (await file.getFile()).arrayBuffer());
				const size = (db[16] << 8) | db[17];
				const page = db.slice(0, size);
				// Header, padded to a 512-byte sector, then one record: page
				// number, the page, its checksum (the nonce plus every 200th byte).
				const journal = new Uint8Array(512 + 4 + size + 4);
				const view = new DataView(journal.buffer);
				journal.set([0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]);
				view.setUint32(8, 1);
				view.setUint32(12, 0x2a);
				view.setUint32(16, db.length / size);
				view.setUint32(20, 512);
				view.setUint32(24, size);
				view.setUint32(512, 1);
				journal.set(page, 516);
				let sum = 0x2a;
				for (let i = size - 200; i > 0; i -= 200) sum += page[i];
				view.setUint32(516 + size, sum >>> 0);
				const write = async (name, bytes, at = 0) => {
					const out = await (await dir.getFileHandle(name, { create: true })).createWritable({ keepExistingData: true });
					await out.write({ type: 'write', position: at, data: bytes });
					await out.close();
				};
				await write('datum.db-journal', journal);
				await write('datum.db', new Uint8Array(size).fill(0xa5));
				return true;
			})
		);
		assert.ok(cut);
		await app.reload();
		const count = await app.run(['count', 'coffee']);
		assert.equal(count.stderr, '');
		assert.equal(count.stdout, '3');
	});
});
