// The relay's tarball is fetched as build-cli-wasm.mjs fetches its tools
// (da-ccplx): a 504 is tried again, a 404 is not.

import test from 'node:test';
import assert from 'node:assert/strict';
import { download } from './support/sync.mjs';

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
