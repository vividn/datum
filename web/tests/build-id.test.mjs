// The gear's build stamp, in node (da-5172a): `scripts/build-id.mjs` against
// repos of a known shape — the same cases `crates/datum-cli/tests/version.rs`
// puts to the CLI's `build_id.rs`.

import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildId } from '../scripts/build-id.mjs';

const IDENT = ['-c', 'user.name=t', '-c', 'user.email=t@t', '-c', 'commit.gpgsign=false'];
const git = (dir, ...args) => execFileSync('git', [...IDENT, ...args], { cwd: dir, encoding: 'utf8' }).trim();

/** A repo of `n` commits, and its HEAD. */
function repo(n) {
	const dir = mkdtempSync(join(tmpdir(), 'build-id-'));
	git(dir, 'init', '-q');
	for (let i = 0; i < n; i++) git(dir, 'commit', '-q', '--allow-empty', '-m', `${i}`);
	return { dir, head: git(dir, 'rev-parse', 'HEAD') };
}

test('the height is every commit reachable, and a dirty tree says so', () => {
	const { dir, head } = repo(5);
	assert.deepEqual(buildId(dir, undefined), { version: '3.beta.5', commit: head.slice(0, 7) });
	writeFileSync(join(dir, 'new'), '');
	assert.equal(buildId(dir, undefined).commit, `${head.slice(0, 7)}+`);
	// CI names its commit, and a checkout it made is not dirty for us.
	const first = git(dir, 'rev-list', '--max-parents=0', 'HEAD');
	assert.deepEqual(buildId(dir, first), { version: '3.beta.1', commit: first.slice(0, 7) });
});

test('a shallow clone says unknown unless it holds the root', () => {
	const { dir: src, head } = repo(3);
	for (const [depth, height] of [['1', 'unknown'], ['2', 'unknown'], ['3', '3']]) {
		const dir = mkdtempSync(join(tmpdir(), 'build-id-'));
		git(dir, 'clone', '-q', '--depth', depth, `file://${src}`, 'c');
		assert.deepEqual(buildId(join(dir, 'c'), undefined), { version: `3.beta.${height}`, commit: head.slice(0, 7) }, `--depth ${depth}`);
	}
});

test('no git says unknown', () => {
	const dir = mkdtempSync(join(tmpdir(), 'build-id-'));
	assert.deepEqual(buildId(dir, undefined), { version: '3.beta.unknown', commit: 'unknown' });
});
