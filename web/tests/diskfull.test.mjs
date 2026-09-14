// A full disk, in node, with the real CLI (da-qlu7).
//
// OPFS `write` throws `QuotaExceededError` when the origin's storage is
// full, and the shim let that unwind through the wasm import: the process
// trapped mid-write and SQLite never heard. `opfsDirectory` ($lib/wasiFs.ts)
// now hands SQLite a short write instead, which it reads as SQLITE_FULL.
// These hold that to what matters: the command fails cleanly, and the
// database the next command opens is the one from before.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { File as MemFile, OpenFile, WASI } from '@bjorn3/browser_wasi_shim';
import { DB_FILES, MemoryHandle, opfsDirectory, Sink } from '../src/lib/wasiFs.ts';

const module = await WebAssembly.compile(await readFile(new URL('../src/lib/wasm/datum-cli.wasm', import.meta.url)));

/** Handles sharing one quota, as an origin's files do; `limit` is in bytes. */
function disk(fail = () => new DOMException('storage is full', 'QuotaExceededError')) {
	const quota = { limit: Infinity };
	const files = new Map(DB_FILES.map((name) => [name, null]));
	const used = () => [...files.values()].reduce((n, file) => n + file.getSize(), 0);
	class Quota extends MemoryHandle {
		write(buffer, { at = 0 } = {}) {
			if (used() + Math.max(0, at + buffer.byteLength - this.getSize()) > quota.limit) throw fail();
			return super.write(buffer, { at });
		}
		truncate(to) {
			if (used() + to - this.getSize() > quota.limit) throw fail();
			super.truncate(to);
		}
	}
	for (const name of files.keys()) files.set(name, new Quota());
	return { handles: files, quota, used };
}

/** `datum <argv>` against `handles`, as $lib/cli.ts runs it. */
function run(handles, argv, stdin = '') {
	const stdout = new Sink();
	const stderr = new Sink();
	const fds = [new OpenFile(new MemFile(new TextEncoder().encode(stdin))), stdout, stderr, opfsDirectory('/data', handles)];
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', 'TZ=UTC'];
	const wasi = new WASI(['datum', ...argv], vars, fds, { debug: false });
	const instance = new WebAssembly.Instance(module, { wasi_snapshot_preview1: wasi.wasiImport });
	const code = wasi.start(instance);
	return { code, stdout: stdout.text(), stderr: stderr.text() };
}

const ok = (result) => (assert.equal(result.code, 0, result.stderr), result);

describe('a full disk', () => {
	test('fails the write cleanly, and the next command finds the database from before', () => {
		const { handles, quota, used } = disk();
		for (const time of ['0700', '0800', '0900']) ok(run(handles, ['occur', 'coffee', '-t', time]));
		const before = ok(run(handles, ['db', 'backup', '-'])).stdout.split('\n').slice(1).join('\n');
		const big = ['occur', 'note', `text=${'x'.repeat(200_000)}`, '-t', '1000'];

		// From no room at all up to room for it: that crosses the journal's
		// growth, and then the database's, whose rollback rewrites pages the
		// file already has.
		let failed = 0;
		for (let room = 0; ; room += 4096) {
			quota.limit = used() + room;
			const full = run(handles, big);
			if (full.code === 0) break;
			failed++;
			assert.equal(full.code, 1, `room ${room}: ${full.stderr}`);
			assert.match(full.stderr, /database or disk is full/, `room ${room}`);

			quota.limit = Infinity;
			ok(run(handles, ['check']));
			const after = ok(run(handles, ['db', 'backup', '-'])).stdout.split('\n').slice(1).join('\n');
			assert.equal(after, before, `room ${room}`);
		}
		assert.ok(failed > 1, `only ${failed} limits failed`);
	});

	test('is the only thing taken for one: any other error still traps', () => {
		const { handles, quota, used } = disk(() => new TypeError('not a full disk'));
		ok(run(handles, ['occur', 'coffee', '-t', '0700']));
		quota.limit = used();
		assert.throws(() => run(handles, ['occur', 'note', `text=${'x'.repeat(10_000)}`]));
	});
});
