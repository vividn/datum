// Sync's test levers and relay overrides are read from localStorage only in a
// build made for tests (da-ha2pe): `pnpm test`'s, built with
// `DATUM_TEST_LEVERS=1`, which vite turns into `__DATUM_TEST_LEVERS__`. A
// production build reads none, so a key anything on the origin planted once
// cannot hold this page's sync or route it through another relay. Each read is
// behind the constant, so production's bundle holds none of the keys at all:
// a read that escaped it would carry its key's name into the bundle.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import './support/tmpdir.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const KEYS = [
	'datum.syncFailRefresh',
	'datum.syncHoldApply',
	'datum.syncHoldApplyAfter',
	'datum.syncGateApplyAfter',
	'datum.syncHoldVerify',
	'datum.syncFailVerify',
	'datum.syncTimeoutS',
	'datum.syncRelay',
	'datum.pkarrRelay'
];
const prod = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-prod-'));
// Its own `.svelte-kit`, as kept-build's: the other files' `vite preview` serves `.svelte-kit/output`.
const kitOut = path.join(web, '.svelte-kit-prod');

before(() => {
	const env = { ...process.env, DATUM_BUILD_DIR: prod, DATUM_KIT_OUT: kitOut };
	delete env.DATUM_TEST_LEVERS;
	execFileSync(path.join(web, 'node_modules/.bin/vite'), ['build', '--logLevel', 'error'], { cwd: web, env, stdio: 'inherit' });
});

after(() => {
	fs.rmSync(prod, { recursive: true, force: true });
	fs.rmSync(kitOut, { recursive: true, force: true });
});

/** Each key, and the built files that name it. */
function named(dir) {
	const files = fs.readdirSync(dir, { recursive: true }).map((f) => path.join(dir, f)).filter((f) => /\.(js|html)$/.test(f));
	const texts = files.map((f) => [f, fs.readFileSync(f, 'utf8')]);
	return Object.fromEntries(KEYS.map((k) => [k, texts.filter(([, t]) => t.includes(k)).map(([f]) => path.relative(dir, f))]));
}

test('a production build reads none of sync’s test levers or relay overrides', () => {
	assert.deepEqual(named(prod), Object.fromEntries(KEYS.map((k) => [k, []])));
});

test('the build the suite runs reads every one of them', () => {
	// The control: the same search finds each key where the levers are read.
	const found = named(path.join(web, 'build'));
	for (const k of KEYS) assert.ok(found[k].length > 0, `${k}: not in the test build`);
});
