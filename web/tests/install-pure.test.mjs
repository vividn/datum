// datum.day/install.sh (static/install.sh, da-vpbg3), run by `sh` in a
// scratch HOME against a stub release: a `file://` directory holding a
// script per platform and a SHA256SUMS, and `uname` stubbed on PATH to be
// the machine asked about. What is checked is which binary it picks and
// that it installs nothing whose checksum does not match. `minisign` is
// stubbed too, on a PATH entry of its own (`signer`): it says what it was
// asked, and passes a signature unless STUB_BAD_SIGNATURE is set and the
// .minisig is there, so a test leaving `signer` off PATH is a machine
// without minisign.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../static/install.sh', import.meta.url));
const ASSETS = ['datum-x86_64-linux', 'datum-aarch64-linux', 'datum-x86_64-macos', 'datum-aarch64-macos'];
const sha256 = (text) => createHash('sha256').update(text).digest('hex');

// A binary that says which asset it is when asked for its version, and
// what else it was asked.
const binary = (asset) =>
	`#!/bin/sh\ncase "$1" in --version) echo "datum 3.0.1-beta (${asset})" ;; *) echo "datum ran: $*" ;; esac\n`;

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
	writeFileSync(join(release, 'SHA256SUMS.minisig'), 'a signature\n');
	writeFileSync(join(bin, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo "$STUB_S" ;; -m) echo "$STUB_M" ;; esac\n');
	chmodSync(join(bin, 'uname'), 0o755);
	const signer = join(root, 'signer');
	mkdirSync(signer);
	writeFileSync(join(signer, 'minisign'), '#!/bin/sh\necho "minisign $*" >&2\n[ -f "$3.minisig" ] && [ -z "$STUB_BAD_SIGNATURE" ]\n');
	chmodSync(join(signer, 'minisign'), 0o755);
	return { root, release, bin, signer, home: join(root, 'home') };
}

function install(box, s, m, env = {}) {
	const result = spawnSync('sh', [script, '--no-completions'], {
		encoding: 'utf8',
		env: {
			PATH: `${box.bin}:${box.signer}:${process.env.PATH}`,
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
			// It names what it installed and where, and that the place is not on PATH.
			for (const fact of [`datum 3.0.1-beta (${asset})`, run.installed]) assert.ok(run.stdout.includes(fact), `${fact}: ${run.stdout}`);
			assert.match(run.stdout, /\bPATH\b/);
		});
	}

	test('the binary then sets itself up, with the arguments install.sh was given', () => {
		const run = install(scratch(), 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		assert.match(run.stdout, /^datum ran: install --no-completions$/m);
	});

	test('$DATUM_INSTALL_DIR is where it goes, and on PATH it says nothing of PATH', () => {
		const box = scratch();
		const dir = join(box.root, 'elsewhere');
		const run = install(box, 'Linux', 'x86_64', { DATUM_INSTALL_DIR: dir, PATH: `${box.bin}:${box.signer}:${dir}:${process.env.PATH}` });
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
			assert.ok(run.stderr.includes(`${s} ${m}`), `it names the machine: ${run.stderr}`);
			assert.match(run.stderr, /\bcheckout\b/, 'and where to build one');
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
		// It names the binary and both checksums: the one it got and the one SHA256SUMS lists.
		for (const fact of ['datum-x86_64-linux', sha256(binary('datum-x86_64-linux')), sha256('something else')]) {
			assert.ok(run.stderr.includes(fact), `${fact}: ${run.stderr}`);
		}
		assert.equal(readFileSync(old, 'utf8'), 'the old one');
	});

	test('a binary SHA256SUMS does not list installs nothing', () => {
		const box = scratch({ sums: (asset) => (asset === 'datum-aarch64-macos' ? null : sha256(binary(asset))) });
		const run = install(box, 'Darwin', 'arm64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\bdatum-aarch64-macos\b/, 'it names the binary SHA256SUMS lacks');
		assert.equal(existsSync(run.installed), false);
	});
});

// The pinned key is release/minisign.pub's; datum-commands' install.rs
// holds every copy of it to that file.
const KEY = readFileSync(fileURLToPath(new URL('../../release/minisign.pub', import.meta.url)), 'utf8').split('\n')[1];
const hostMinisign = spawnSync('sh', ['-c', 'command -v minisign']).status === 0;

describe('install.sh checks SHA256SUMS is signed by the release key, with minisign', () => {
	test('it asks minisign with the pinned key, and installs on a good signature', () => {
		const run = install(scratch(), 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		assert.match(run.stderr, new RegExp(`^minisign -Vq -m \\S+/SHA256SUMS -P ${KEY.replace(/[+/]/g, '\\$&')}$`, 'm'));
		assert.equal(existsSync(run.installed), true);
	});

	test('a signature that does not verify installs nothing and keeps the datum already there', () => {
		const box = scratch();
		const old = join(box.home, '.local/bin/datum');
		mkdirSync(join(box.home, '.local/bin'), { recursive: true });
		writeFileSync(old, 'the old one');
		const run = install(box, 'Linux', 'x86_64', { STUB_BAD_SIGNATURE: '1' });
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\bSHA256SUMS\b.*\bsignature\b/, 'it names what failed to verify');
		assert.equal(readFileSync(old, 'utf8'), 'the old one');
	});

	test('a release with no signature installs nothing', () => {
		const box = scratch();
		rmSync(join(box.release, 'SHA256SUMS.minisig'));
		const run = install(box, 'Linux', 'x86_64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\S+\/SHA256SUMS\.minisig\b/, 'it names the signature it could not download');
		assert.equal(existsSync(run.installed), false);
	});

	test('without minisign it says so, and how to check by hand, and installs', { skip: hostMinisign && 'minisign is installed here' }, () => {
		const box = scratch();
		const run = install(box, 'Linux', 'x86_64', { PATH: `${box.bin}:${process.env.PATH}` });
		assert.equal(run.status, 0, run.stderr);
		assert.ok(run.stderr.includes(`minisign -Vm SHA256SUMS -P ${KEY}`), `the command to check it by hand: ${run.stderr}`);
		assert.equal(existsSync(run.installed), true);
	});
});
