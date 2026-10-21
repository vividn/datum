// datum.day/install.sh (static/install.sh, da-vpbg3), run by `sh` in a
// scratch HOME against a stub release: a `file://` directory holding a
// script per platform and a SHA256SUMS, and `uname` stubbed on PATH to be
// the machine asked about. What is checked is which binary it picks and
// that it installs nothing whose checksum does not match.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../static/install.sh', import.meta.url));
const ASSETS = ['datum-x86_64-linux', 'datum-aarch64-linux', 'datum-x86_64-macos', 'datum-aarch64-macos'];
const sha256 = (text) => createHash('sha256').update(text).digest('hex');

// A binary that says which asset it is when asked for its version.
const binary = (asset) => `#!/bin/sh\necho "datum 3.0.1-beta (${asset})"\n`;

function scratch({ sums = (asset) => sha256(binary(asset)) } = {}) {
	const root = mkdtempSync(join(tmpdir(), 'datum-install-'));
	const release = join(root, 'release');
	const bin = join(root, 'bin');
	mkdirSync(release);
	mkdirSync(bin);
	mkdirSync(join(root, 'home'));
	for (const asset of ASSETS) writeFileSync(join(release, asset), binary(asset));
	const lines = ASSETS.map((asset) => [sums(asset), asset]).filter(([sum]) => sum);
	writeFileSync(join(release, 'SHA256SUMS'), lines.map(([sum, asset]) => `${sum}  ${asset}\n`).join(''));
	writeFileSync(join(bin, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo "$STUB_S" ;; -m) echo "$STUB_M" ;; esac\n');
	chmodSync(join(bin, 'uname'), 0o755);
	return { root, release, bin, home: join(root, 'home') };
}

function install(box, s, m, env = {}) {
	const result = spawnSync('sh', [script, '--no-completions'], {
		encoding: 'utf8',
		env: {
			PATH: `${box.bin}:${process.env.PATH}`,
			HOME: box.home,
			XDG_CONFIG_HOME: join(box.home, '.config'),
			DATUM_RELEASE_URL: `file://${box.release}`,
			STUB_S: s,
			STUB_M: m,
			...env
		}
	});
	return { ...result, installed: join(env.DATUM_INSTALL_DIR ?? join(box.home, '.local/bin'), 'datum') };
}

describe('install.sh picks the binary for the machine', () => {
	for (const [s, m, asset] of [
		['Linux', 'x86_64', 'datum-x86_64-linux'],
		['Linux', 'amd64', 'datum-x86_64-linux'],
		['Linux', 'aarch64', 'datum-aarch64-linux'],
		['Darwin', 'arm64', 'datum-aarch64-macos'],
		['Darwin', 'x86_64', 'datum-x86_64-macos']
	]) {
		test(`${s} ${m} gets ${asset}`, () => {
			const box = scratch();
			const run = install(box, s, m);
			assert.equal(run.status, 0, run.stderr);
			assert.equal(readFileSync(run.installed, 'utf8'), binary(asset));
			assert.match(run.stdout, new RegExp(`installed datum 3\\.0\\.1-beta \\(${asset}\\) at ${run.installed}`));
			assert.match(run.stdout, /\.local\/bin is not on your PATH/);
		});
	}

	test('$DATUM_INSTALL_DIR is where it goes, and on PATH it says nothing of PATH', () => {
		const box = scratch();
		const dir = join(box.root, 'elsewhere');
		const run = install(box, 'Linux', 'x86_64', { DATUM_INSTALL_DIR: dir, PATH: `${box.bin}:${dir}:${process.env.PATH}` });
		assert.equal(run.status, 0, run.stderr);
		assert.equal(run.installed, join(dir, 'datum'));
		assert.equal(readFileSync(run.installed, 'utf8'), binary('datum-x86_64-linux'));
		assert.doesNotMatch(run.stdout, /PATH/);
	});

	for (const [s, m] of [
		['FreeBSD', 'x86_64'],
		['Linux', 'riscv64']
	]) {
		test(`${s} ${m} is refused, naming the build from a checkout`, () => {
			const box = scratch();
			const run = install(box, s, m);
			assert.notEqual(run.status, 0);
			assert.match(run.stderr, new RegExp(`no prebuilt datum for ${s} ${m}; build it from a checkout`));
			assert.equal(existsSync(run.installed), false);
		});
	}
});

describe('install.sh refuses a binary SHA256SUMS does not vouch for', () => {
	test('a checksum mismatch installs nothing and keeps the datum already there', () => {
		const box = scratch({ sums: (asset) => sha256(asset === 'datum-x86_64-linux' ? 'something else' : binary(asset)) });
		const old = join(box.home, '.local/bin/datum');
		mkdirSync(join(box.home, '.local/bin'), { recursive: true });
		writeFileSync(old, 'the old one');
		const run = install(box, 'Linux', 'x86_64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /datum-x86_64-linux does not match SHA256SUMS \(sha256 [0-9a-f]{64}, expected [0-9a-f]{64}\); nothing installed/);
		assert.equal(readFileSync(old, 'utf8'), 'the old one');
	});

	test('a binary SHA256SUMS does not list installs nothing', () => {
		const box = scratch({ sums: (asset) => (asset === 'datum-aarch64-macos' ? null : sha256(binary(asset))) });
		const run = install(box, 'Darwin', 'arm64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /SHA256SUMS lists no datum-aarch64-macos; nothing installed/);
		assert.equal(existsSync(run.installed), false);
	});
});
