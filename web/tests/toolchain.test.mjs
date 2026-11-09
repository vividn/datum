// build-cli-wasm.mjs's read-through toolchain cache, in node (da-waoz2):
// `tool()` against a fake fetch, a real tarball and DATUM_TOOLCHAIN_CACHE, and
// `verified()`, the relay's way in (da-or6gu), held to the same cases.

import test, { after, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { tool, verified } from '../scripts/toolchain.mjs';

const tmp = mkdtempSync(join(tmpdir(), 'toolchain-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
const cwd = process.cwd();
afterEach(() => {
	delete process.env.DATUM_TOOLCHAIN_CACHE;
	process.chdir(cwd);
});

// A tool of one file, packed as the releases are: its directory at the top.
const NAME = 'tool-1.0-x86_64-linux.tar.gz';
const src = mkdtempSync(join(tmp, 'src-'));
mkdirSync(join(src, 'tool-1.0'));
writeFileSync(join(src, 'tool-1.0', 'bin'), 'the tool');
execFileSync('tar', ['-czf', join(src, NAME), '-C', src, 'tool-1.0']);
const GOOD = readFileSync(join(src, NAME));
const SPEC = { url: `https://example.invalid/releases/1.0/${NAME}`, sha256: createHash('sha256').update(GOOD).digest('hex'), dir: 'tool-1.0' };

/** A fetch answering GOOD, and how many times it was asked. */
function fake() {
	const get = async () => {
		get.calls++;
		return new Response(GOOD);
	};
	get.calls = 0;
	return get;
}

/** Each way in, given a fetch: `tool()`'s unpacked tool, `verified()`'s bytes. */
const WAYS = {
	tool: async (get) => {
		const home = await tool(SPEC, mkdtempSync(join(tmp, 'toolchains-')), get);
		assert.equal(readFileSync(join(home, 'bin'), 'utf8'), 'the tool');
	},
	verified: async (get) => assert.deepEqual(await verified(SPEC, { get }), GOOD)
};

const dir = () => mkdtempSync(join(tmp, 'cache-'));

for (const [way, take] of Object.entries(WAYS)) {
	/** `way` with a fresh fetch: how many times it was asked. */
	const run = async () => {
		const get = fake();
		await take(get);
		return get.calls;
	};

	test(`${way}: unset or empty, no cache is read or written`, async () => {
		assert.equal(await run(), 1);
		// Empty must not read the cwd as the cache: a good tarball there is not taken.
		const here = dir();
		writeFileSync(join(here, NAME), GOOD);
		process.chdir(here);
		process.env.DATUM_TOOLCHAIN_CACHE = '';
		assert.equal(await run(), 1);
		assert.deepEqual(readdirSync(here), [NAME]);
	});

	test(`${way}: a cached tarball with the pinned sha256 is used, and nothing is fetched`, async () => {
		const cache = dir();
		writeFileSync(join(cache, NAME), GOOD);
		process.env.DATUM_TOOLCHAIN_CACHE = cache;
		assert.equal(await run(), 0);
	});

	test(`${way}: a cached tarball with another sha256 is fetched again, and the good bytes replace it`, async () => {
		const cache = dir();
		writeFileSync(join(cache, NAME), 'not the tool');
		process.env.DATUM_TOOLCHAIN_CACHE = cache;
		assert.equal(await run(), 1);
		assert.deepEqual(readFileSync(join(cache, NAME)), GOOD);
		assert.deepEqual(readdirSync(cache), [NAME]);
	});

	test(`${way}: a miss is fetched and kept, and the next build fetches nothing`, async () => {
		const cache = dir();
		process.env.DATUM_TOOLCHAIN_CACHE = cache;
		assert.equal(await run(), 1);
		assert.deepEqual(readdirSync(cache), [NAME]);
		assert.deepEqual(readFileSync(join(cache, NAME)), GOOD);
		assert.equal(await run(), 0);
	});

	test(`${way}: a download that fails the check is not kept`, async () => {
		const cache = dir();
		process.env.DATUM_TOOLCHAIN_CACHE = cache;
		await assert.rejects(take(async () => new Response('not the tool')), /sha256 .*, pinned/);
		assert.deepEqual(readdirSync(cache), []);
	});

	test(`${way}: a cache that cannot be written still gives the build its tool`, async () => {
		const file = join(dir(), 'a-file');
		writeFileSync(file, '');
		// Missing, and under a file (ENOTDIR, which root cannot write either).
		for (const cache of [join(tmp, 'missing'), join(file, 'cache')]) {
			process.env.DATUM_TOOLCHAIN_CACHE = cache;
			assert.equal(await run(), 1);
			assert.equal(existsSync(cache), false);
		}
	});
}
