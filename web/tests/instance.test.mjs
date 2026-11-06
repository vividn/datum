// One CLI instance answering command after command, in node, with the real
// CLI (#lib/instance.ts, da-jm4sv).
//
// The worker keeps one instance, and the instance keeps each database's
// connection open between commands, SQLite's cache with it. These hold it to
// what a process per command gave: the same output, exit codes and env per
// command; and to what keeping a connection must not cost, which is the
// data-safety core: another tab's write made between two of its commands is
// read, and never written over.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { File as MemFile, OpenFile } from '../src/lib/vendor/browser_wasi_shim/index.js';
import { Instance } from '../src/lib/instance.ts';
import { cached, DB_FILES, forgetCached, MemoryHandle, opfsDirectory, Sink } from '../src/lib/wasiFs.ts';
import { masker, PINNED_DAY } from './support/stdout.mjs';

const module = await WebAssembly.compile(await readFile(new URL('../src/lib/wasm/datum-cli.wasm', import.meta.url)));

/** The gesture's instant every command here runs at (`FROZEN_AT`). */
const AT = Date.parse(`${PINNED_DAY}T12:00:00Z`);

/** A file in memory that counts the reads made of it. */
class Counted extends MemoryHandle {
	reads = 0;
	read(buffer, options) {
		this.reads++;
		return super.read(buffer, options);
	}
}

const disk = () => new Map(DB_FILES.map((name) => [name, new Counted()]));

/** The files as one burst of the worker holds them: every `.db` behind its cache. */
const hold = (files) => new Map([...files].map(([name, file]) => [name, name.endsWith('.db') ? cached(name, file) : file]));

const release = (held) => {
	for (const handle of held.values()) handle.close();
};

/**
 * `datum <argv>` on `cli` against `handles`, as #lib/cli.ts runs it: `keep`,
 * the database's connection kept for the next command.
 */
async function run(cli, handles, argv, { env = ['TZ=UTC'], stdin = '', keep = true } = {}) {
	const stdout = new Sink();
	const stderr = new Sink();
	const fds = [new OpenFile(new MemFile(new TextEncoder().encode(stdin))), stdout, stderr, opfsDirectory('/data', handles, undefined, cli.inodes)];
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', 'DATUM_CLIENT_ID=instance', ...env];
	if (keep) vars.push('DATUM_KEEP_OPEN=/data/datum.db');
	const { code, dead } = await cli.run(['datum', ...argv], vars, fds, AT);
	return { code, dead, stdout: stdout.text(), stderr: stderr.text() };
}

/** `datum <argv>` as a process of its own: another tab's, or the old worker's. */
const alone = async (handles, argv, opts) => run(await Instance.start(module), handles, argv, { ...opts, keep: false });

const day = (clock) => `${PINNED_DAY} ${clock}`;

/** The short hid after UNDONE, which the masker leaves. */
const undone = (text) => text.replace(/\bUNDONE [0-9a-z]{5}\b/g, 'UNDONE <id>');

describe('one instance', () => {
	test('prints, exits and reads its env as a process per command did', async () => {
		// Most entries in one zone: `tail` heads its time column with the most
		// common offset, and two tied pick one at random, process or not.
		const script = [
			[['occur', 'tea', '-t', day('06:00')]],
			[['occur', 'coffee', '-t', day('07:00')]],
			[['start', 'walk', '-t', day('08:00')], { env: ['TZ=Europe/Berlin'] }],
			// A refused re-log of the same entry: EXISTS, exit 11, on a clean run.
			[['occur', 'coffee', '-t', day('07:00'), '--conflict', 'refuse']],
			[['tail'], { env: ['TZ=America/Chicago'] }],
			[['now', '--json']],
			[['fields', '--json']],
			[['stop', 'walk', '-t', day('09:30')], { env: ['TZ=Asia/Tokyo'] }],
			[['undo']],
			[['tail', '--json']],
			[['zzz']],
			[['note', 'jot', 'a', 'line', '-t', day('10:00')], { stdin: 'what a later command must not read\n' }],
			[['dayview', '-d', PINNED_DAY]],
			[['stats', '--show', 'all']]
		];
		const kept = await Instance.start(module);
		const [mine, theirs] = [disk(), disk()];
		const [maskMine, maskTheirs] = [masker(), masker()];
		const codes = [];
		for (const [argv, opts] of script) {
			const a = await run(kept, mine, argv, opts);
			const b = await alone(theirs, argv, opts);
			assert.equal(a.code, b.code, `${argv.join(' ')}: ${a.stderr}`);
			assert.equal(maskMine(undone(a.stdout)), maskTheirs(undone(b.stdout)), argv.join(' '));
			assert.equal(maskMine(a.stderr), maskTheirs(b.stderr), argv.join(' '));
			assert.equal(a.dead, false, argv.join(' '));
			codes.push(a.code);
		}
		// The script reaches the exits that are not 0: the duplicate's, a usage error's.
		assert.ok(codes.includes(11) && codes.some((code) => code !== 0 && code !== 11), `codes: ${codes}`);
	});

	test('keeps the connection, and its cache: a read again reads the file almost nothing', async () => {
		const files = disk();
		const kept = await Instance.start(module);
		for (let i = 0; i < 30; i++) await run(kept, files, ['occur', `f${i % 3}`, `n=${i}`, '-t', day(`0${i % 10}:${10 + i}`)]);
		const db = files.get('datum.db');
		await run(kept, files, ['tail', '--json']);
		db.reads = 0;
		await run(kept, files, ['tail', '--json']);
		const again = db.reads;
		db.reads = 0;
		await alone(files, ['tail', '--json']);
		// The header, which a connection reads as it takes its lock again.
		assert.ok(again <= 2, `the kept connection read the file ${again} times`);
		assert.ok(db.reads > 5 * Math.max(1, again), `a new connection read it ${db.reads} times, the kept one ${again}`);
	});

	test("reads another tab's write made between two of its commands, and never writes over it", async () => {
		forgetCached();
		const files = disk();
		const kept = await Instance.start(module);
		const burst = async (argv) => {
			const held = hold(files);
			const result = await run(kept, held, argv);
			release(held);
			assert.equal(result.code, 0, `${argv.join(' ')}: ${result.stderr}`);
			return result.stdout;
		};
		const tab = async (argv) => {
			const held = hold(files);
			const result = await alone(held, argv);
			release(held);
			assert.equal(result.code, 0, `${argv.join(' ')}: ${result.stderr}`);
			return result.stdout;
		};
		for (let i = 0; i < 20; i++) await burst(['occur', 'mine', `n=${i}`, '-t', day(`07:${10 + i}`)]);
		await burst(['tail', '--json']);
		// Every write a page or more: the other tab's land on pages the kept
		// connection has in its cache.
		const text = 'x'.repeat(3000);
		for (let i = 0; i < 4; i++) {
			await tab(['occur', 'theirs', `n=${i}`, `text=${text}`, '-t', day(`08:${10 + i}`)]);
			const read = JSON.parse(await burst(['tail', '-n', '100', '--json'])).rows;
			assert.equal(read.filter((row) => row.field === 'theirs').length, i + 1, `the other tab's write ${i}`);
			await burst(['occur', 'mine', `n=${100 + i}`, `text=${text}`, '-t', day(`09:${10 + i}`)]);
		}
		const all = JSON.parse(await tab(['tail', '-n', '100', '--json']));
		const count = (field) => all.rows.filter((row) => row.field === field).length;
		assert.equal(count('theirs'), 4);
		assert.equal(count('mine'), 24);
		await tab(['check']);
	});

	test('a file removed under it is made anew, as a new process makes it', async () => {
		const kept = await Instance.start(module);
		const files = disk();
		for (let i = 0; i < 3; i++) assert.equal((await run(kept, files, ['occur', 'tea', '-t', day(`0${i}:00`)])).code, 0);
		// New, empty handles under the same names: what a removal of the files leaves.
		const empty = disk();
		const made = await run(kept, empty, ['occur', 'tea', '-t', day('09:00')]);
		assert.equal(made.code, 0, made.stderr);
		const rows = JSON.parse((await run(kept, empty, ['tail', '--json'])).stdout).rows;
		assert.deepEqual(rows.map((row) => row.slug), [`tea:${PINNED_DAY}T09:00:00.000Z`]);
	});

	test('a command that traps ends it', async () => {
		const files = disk();
		const kept = await Instance.start(module);
		assert.equal((await run(kept, files, ['occur', 'coffee'])).code, 0);
		// A handle that throws traps the command: the shim calls it with no try/catch.
		const broken = new Map([...files].map(([name, file]) => [name, { getSize: () => file.getSize(), read: () => { throw new TypeError('not a disk'); } }]));
		const trapped = await run(kept, broken, ['tail']);
		assert.deepEqual([trapped.code, trapped.dead], [134, true], trapped.stderr);
		await assert.rejects(run(kept, files, ['tail']), /ended/);
	});
});
