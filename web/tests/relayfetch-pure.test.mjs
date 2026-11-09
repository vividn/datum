// The relay's tarball is fetched as build-cli-wasm.mjs fetches its tools
// (da-ccplx): a 504 is tried again, a 404 is not. It comes through
// toolchain.mjs's `verified()`, so DATUM_TOOLCHAIN_CACHE is read first, and a
// miss gives up well inside the `before()` it runs in (da-or6gu).

import test, { after, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { download } from '../scripts/toolchain.mjs';
import { relayBinary } from './support/sync.mjs';

const answers = (...statuses) => {
	const asked = [];
	const get = async (url) => {
		asked.push(url);
		const status = statuses.shift();
		return new Response(status === 200 ? 'tarball' : null, { status });
	};
	return { asked, get };
};

test('a 504 then a 200 gets the bytes, after one wait of 10 s', async () => {
	const { asked, get } = answers(504, 200);
	const waits = [];
	const bytes = await download('u', { get, wait: async (ms) => waits.push(ms) });
	assert.equal(bytes.toString(), 'tarball');
	assert.deepEqual(asked, ['u', 'u']);
	assert.deepEqual(waits, [10_000]);
});

test('a 404 fails at once, with no second try', async () => {
	const { asked, get } = answers(404, 200);
	await assert.rejects(download('u', { get, wait: async () => {} }), /u: HTTP 404/);
	assert.deepEqual(asked, ['u']);
});

test('eight 503s give up, having waited 10 s more each time', async () => {
	const { asked, get } = answers(...Array(9).fill(503));
	const waits = [];
	await assert.rejects(download('u', { get, wait: async (ms) => waits.push(ms) }), /u: HTTP 503/);
	assert.equal(asked.length, 8);
	assert.deepEqual(waits, [10, 20, 30, 40, 50, 60, 70].map((s) => s * 1000));
});

// A relay of one bare binary, packed as n0 packs it.
const tmp = mkdtempSync(join(tmpdir(), 'relayfetch-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
afterEach(() => delete process.env.DATUM_TOOLCHAIN_CACHE);
const NAME = 'iroh-relay-v0-test.tar.gz';
const src = mkdtempSync(join(tmp, 'src-'));
writeFileSync(join(src, 'iroh-relay'), 'the relay');
execFileSync('tar', ['-czf', join(src, NAME), '-C', src, 'iroh-relay']);
const GOOD = readFileSync(join(src, NAME));
const RELAY_URL = `https://example.invalid/v0/${NAME}`;
const spec = { url: RELAY_URL, sha256: createHash('sha256').update(GOOD).digest('hex'), dir: 'iroh-relay-v0' };

test('a relay in DATUM_TOOLCHAIN_CACHE is unpacked with no fetch', async () => {
	const cache = mkdtempSync(join(tmp, 'cache-'));
	writeFileSync(join(cache, NAME), GOOD);
	process.env.DATUM_TOOLCHAIN_CACHE = cache;
	const { asked, get } = answers();
	const bin = await relayBinary({ spec, toolchains: mkdtempSync(join(tmp, 'toolchains-')), get });
	assert.equal(readFileSync(bin, 'utf8'), 'the relay');
	assert.deepEqual(asked, []);
});

// syncgate's before() was cut at test.mjs's 300 s mid-retry (run 11181), so it
// failed as a timeout and never named the URL.
test('a relay answering 504 throughout fails naming its URL, after 100 s of waits', async () => {
	const { asked, get } = answers(...Array(9).fill(504));
	const waits = [];
	const binary = relayBinary({ spec, toolchains: mkdtempSync(join(tmp, 'toolchains-')), get, wait: async (ms) => waits.push(ms) });
	await assert.rejects(binary, (e) => e.message === `${RELAY_URL}: HTTP 504`);
	assert.equal(asked.length, 5);
	assert.equal(waits.reduce((a, b) => a + b), 100_000);
});
