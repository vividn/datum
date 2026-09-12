// What a process writes, in node, with no browser.
//
// `Sink` ($lib/wasiFs.ts) is what the CLI's stdout and stderr are, and it
// replaced the shim's `File` behind an `OpenFile` — which reallocated the
// whole stream on every write that extended it, so a 13.6 MB `db backup -`
// (20k entries) took 6.1 s in chromium where it now takes 1.3 s (da-0lxu).
//
// Two claims, and the first is the one with teeth: the bytes that come out
// are the bytes that went in, in order, with nothing added — `db restore`
// reads exactly what `db backup -` wrote, so a stray byte here is a
// corrupted export. The second is the fix itself, counted rather than
// timed: every byte is copied once, at the join.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { Sink } from '../src/lib/wasiFs.ts';

const utf8 = new TextEncoder();

/** What the shim hands an Fd: a copy, already out of the wasm memory. */
const write = (sink, bytes) => sink.fd_write(bytes instanceof Uint8Array ? bytes : utf8.encode(bytes));

describe('Sink', () => {
	test('nothing written is the empty string', () => {
		assert.equal(new Sink().text(), '');
	});

	test('every write, in order, and nothing of its own', () => {
		const sink = new Sink();
		const lines = ['{"type":"daydumb_backup"}\n', '{"field":"coffee"}\n', '{"field":"mood"}\n'];
		for (const line of lines) assert.deepEqual(write(sink, line), { ret: 0, nwritten: utf8.encode(line).length });
		assert.equal(sink.text(), lines.join(''));
	});

	test('an empty write is a write of nothing', () => {
		const sink = new Sink();
		write(sink, 'a');
		assert.deepEqual(write(sink, new Uint8Array(0)), { ret: 0, nwritten: 0 });
		write(sink, 'b');
		assert.equal(sink.text(), 'ab');
	});

	// The writes arrive at whatever boundary the CLI's BufWriter fills at,
	// which is not a character boundary: a decoder per chunk would put a
	// replacement character in the middle of a field name.
	test('a character split across two writes survives it', () => {
		const bytes = utf8.encode('très');
		for (let at = 1; at < bytes.length; at++) {
			const sink = new Sink();
			write(sink, bytes.subarray(0, at));
			write(sink, bytes.subarray(at));
			assert.equal(sink.text(), 'très', `split at ${at}`);
		}
	});

	// The defect was a copy per extending write. Counted, not timed: 16 MB
	// arriving in 4 KB writes copies 16 MB — the same 16 MB the old shape
	// copied 32 GB for.
	test('every byte is copied once, however many writes it came in', () => {
		const chunk = new Uint8Array(4096).fill(65);
		const writes = 4096;
		const sink = new Sink();
		for (let i = 0; i < writes; i++) write(sink, chunk);

		const set = Uint8Array.prototype.set;
		let copied = 0;
		Uint8Array.prototype.set = function (from, at) {
			copied += from.length;
			return set.call(this, from, at);
		};
		try {
			assert.equal(sink.text().length, chunk.length * writes);
		} finally {
			Uint8Array.prototype.set = set;
		}
		assert.equal(copied, chunk.length * writes);
	});
});
