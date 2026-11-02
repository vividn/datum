// datum.day serves the repo's kits (da-fj3v1): `kits/<name>.toml` in the built
// site is the repo's file byte for byte, and `kits/index.html` lists each with
// its version. The built tree is what deploy.sh uploads, so it is read here,
// and held to deploy.sh's own check.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const web = fileURLToPath(new URL('..', import.meta.url));
const kits = fileURLToPath(new URL('../../kits/', import.meta.url));
const built = fileURLToPath(new URL('../build/kits/', import.meta.url));

test('every repo kit is in the built site byte for byte', () => {
	const names = readdirSync(kits).filter((f) => f.endsWith('.toml'));
	assert.ok(names.includes('chores.toml'), 'kits/chores.toml is in the repo');
	for (const name of names) {
		assert.ok(readFileSync(`${built}${name}`).equals(readFileSync(`${kits}${name}`)), `build/kits/${name} differs`);
	}
	execFileSync('bash', ['deploy.sh', '--check-dist', 'build'], { cwd: web });
});

test('the listing names each kit with its version from its [kit] table', () => {
	const listing = readFileSync(`${built}index.html`, 'utf8');
	const version = readFileSync(`${kits}chores.toml`, 'utf8').match(/^version = "(.*)"$/m)[1];
	assert.match(listing, new RegExp(`<a href="chores\\.toml">chores\\.toml</a> ${version.replace('.', '\\.')}</dt>`));
	assert.match(listing, /<dd>Chores as datum v2 kept them/);
});
