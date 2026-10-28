// A command's reads of the database, in node, with the real CLI (da-o3sns).
//
// In a private window every sync access handle call is an IPC of ~1 ms, and
// a command made hundreds: SQLite let go of its lock after every statement,
// and took it again by re-reading the file's header and size. Now the CLI
// holds its lock for the connection's life on wasm, and the worker keeps
// what it has read (#lib/wasiFs.ts `cached`). These hold both to what they
// are for — fewer reads — and to what they must not cost: what a command
// prints, and another tab's writes.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { File as MemFile, OpenFile, WASI } from '@bjorn3/browser_wasi_shim';
import { cached, DB_FILES, forgetCached, MemoryHandle, opfsDirectory, Sink } from '../src/lib/wasiFs.ts';

const module = await WebAssembly.compile(await readFile(new URL('../src/lib/wasm/datum-cli.wasm', import.meta.url)));

/** A file in memory that counts the reads made of it. */
class Counted extends MemoryHandle {
	reads = [];
	read(buffer, options) {
		this.reads.push(options?.at ?? 0);
		return super.read(buffer, options);
	}
}

/** One origin's files, as the worker opens them. */
const disk = () => new Map(DB_FILES.map((name) => [name, new Counted()]));

/** Another origin's files, byte for byte these. */
function copy(files) {
	const other = disk();
	for (const [name, file] of files) {
		const bytes = new Uint8Array(file.getSize());
		file.read(bytes, { at: 0 });
		other.get(name).write(bytes, { at: 0 });
	}
	return other;
}

/** The files as the worker holds them: every `.db` behind its cache. */
const hold = (files) => new Map([...files].map(([name, file]) => [name, name.endsWith('.db') ? cached(name, file) : file]));

const release = (held) => {
	for (const handle of held.values()) handle.close();
};

/** `datum <argv>` against `handles`, as #lib/cli.ts runs it. */
function run(handles, argv) {
	const stdout = new Sink();
	const stderr = new Sink();
	const fds = [new OpenFile(new MemFile(new Uint8Array())), stdout, stderr, opfsDirectory('/data', handles)];
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', 'TZ=UTC'];
	const wasi = new WASI(['datum', ...argv], vars, fds, { debug: false });
	const code = wasi.start(new WebAssembly.Instance(module, { wasi_snapshot_preview1: wasi.wasiImport }));
	assert.equal(code, 0, stderr.text());
	return stdout.text();
}

const READS = [['tail', '--json'], ['now', '--json', '-t', '2026-10-07 12:00'], ['fields', '--json'], ['nowview', '-o', '-', '-t', '2026-10-07 12:00']];

/** A database of a few days, `n` entries a day. */
function seed(handles, n = 20) {
	for (const day of ['2026-10-05', '2026-10-06', '2026-10-07'])
		for (let i = 0; i < n; i++) run(handles, ['occur', `f${i % 5}`, `n=${i}`, '-t', `${day} ${String(i % 24).padStart(2, '0')}:00`]);
}

describe('the cache', () => {
	test('reads what the file holds, whatever is written, grown or cut around it', () => {
		forgetCached();
		let seed = 7;
		// xorshift32: an LCG's low bits cycle in a few steps, and its product overflows a double.
		const random = (n) => ((seed ^= seed << 13), (seed ^= seed >>> 17), (seed ^= seed << 5), (seed >>> 0) % n);
		const plain = new MemoryHandle();
		const raw = new MemoryHandle();
		let held = cached('fuzz.db', raw);
		// Offsets that cluster at the chunk edges, the end of the file, and
		// where the end was before it last moved.
		let end = 0;
		const offset = () =>
			[random(5 * 4096), plain.getSize() + random(3) * 4096, end, random(5) * 4096][random(4)] + random(3) - 1;
		for (let step = 0; step < 3000; step++) {
			const op = random(10);
			const at = Math.max(0, offset());
			if (op >= 5) end = plain.getSize();
			if (op < 5) {
				const a = new Uint8Array(random(3) ? 4096 : random(9000));
				const b = new Uint8Array(a.length);
				assert.equal(held.read(a, { at }), plain.read(b, { at }), `step ${step}: read ${a.length} at ${at}`);
				assert.deepEqual(a, b, `step ${step}: read ${a.length} at ${at}`);
			} else if (op < 9) {
				const bytes = Uint8Array.from({ length: random(3) ? 4096 : random(9000) }, () => random(256));
				held.write(bytes, { at });
				plain.write(bytes, { at });
			} else if (random(2)) {
				const to = Math.max(0, offset());
				held.truncate(to);
				plain.truncate(to);
			} else {
				// A release and a hold again, as between bursts.
				held.close();
				held = cached('fuzz.db', raw);
			}
			assert.equal(held.getSize(), plain.getSize(), `step ${step}`);
		}
	});
});

describe("a command's reads", () => {
	test('take the lock once a connection: a read command reads the file tens of times, not hundreds', () => {
		forgetCached();
		const files = disk();
		seed(files);
		const db = files.get('datum.db');
		for (const argv of READS) {
			db.reads.length = 0;
			run(files, argv);
			// The header check SQLite makes on taking its lock: a read at
			// offset 24. Two connections a process (the alias pre-pass and
			// the command) — 150 and more when it let go per statement.
			const checks = db.reads.filter((at) => at === 24).length;
			assert.ok(checks <= 2, `${argv[0]}: ${checks} header checks`);
			assert.ok(db.reads.length < 60, `${argv[0]}: ${db.reads.length} reads`);
		}
	});

	test('through the cache: the same output, and a command again reads nothing', () => {
		forgetCached();
		const plain = disk();
		seed(plain);
		const warm = copy(plain);
		const held = hold(warm);
		for (const argv of READS) {
			const expected = run(plain, argv);
			assert.equal(run(held, argv), expected, argv.join(' '));
			warm.get('datum.db').reads.length = 0;
			assert.equal(run(held, argv), expected, argv.join(' '));
			assert.equal(warm.get('datum.db').reads.length, 0, `${argv[0]} again read the file`);
		}
		// After a write through it, it reads what the file now holds.
		run(held, ['occur', 'coffee', '-t', '2026-10-07 13:00']);
		const truth = copy(warm);
		for (const argv of READS) assert.equal(run(held, argv), run(truth, argv), `after a write: ${argv.join(' ')}`);
		release(held);
	});

	test("is kept across a release only while the file is as it was: another tab's write drops it", () => {
		forgetCached();
		const files = disk();
		seed(files);
		let held = hold(files);
		const before = run(held, ['tail', '--json']);
		release(held);

		// Nothing changed: the next hold reads nothing it read before.
		held = hold(files);
		files.get('datum.db').reads.length = 0;
		assert.equal(run(held, ['tail', '--json']), before);
		assert.ok(files.get('datum.db').reads.length <= 2, `${files.get('datum.db').reads.length} reads`);
		release(held);

		// Another tab writes while this one holds nothing.
		run(files, ['occur', 'coffee', 'by=other-tab', '-t', '2026-10-07 23:00']);
		held = hold(files);
		assert.match(run(held, ['tail', '--json']), /other-tab/);
		release(held);
	});
});
