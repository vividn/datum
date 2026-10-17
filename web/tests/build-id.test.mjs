// The gear's build stamp, in node (da-5172a): `scripts/build-id.mjs` against
// repos of a known shape — the same cases `crates/datum-cli/tests/version.rs`
// puts to the CLI's `build_id.rs`.

import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildId } from '../scripts/build-id.mjs';

const IDENT = ['-c', 'user.name=t', '-c', 'user.email=t@t', '-c', 'commit.gpgsign=false'];
/** Every repo this file makes, removed after it (da-yqc4v). */
const tmp = mkdtempSync(join(tmpdir(), 'build-id-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
const git = (dir, ...args) => execFileSync('git', [...IDENT, ...args], { cwd: dir, encoding: 'utf8' }).trim();

/** A repo of `n` commits, and its HEAD. */
function repo(n) {
	const dir = mkdtempSync(join(tmp, 'repo-'));
	git(dir, 'init', '-q');
	for (let i = 0; i < n; i++) git(dir, 'commit', '-q', '--allow-empty', '-m', `${i}`);
	return { dir, head: git(dir, 'rev-parse', 'HEAD') };
}

test('the height is every commit reachable, and a dirty tree says so', () => {
	const { dir, head } = repo(5);
	assert.deepEqual(buildId(dir, undefined), { version: '3.0.5-beta', commit: head.slice(0, 7) });
	writeFileSync(join(dir, 'new'), '');
	assert.equal(buildId(dir, undefined).commit, `${head.slice(0, 7)}+`);
	// CI names its commit, and a checkout it made is not dirty for us.
	const first = git(dir, 'rev-list', '--max-parents=0', 'HEAD');
	assert.deepEqual(buildId(dir, first), { version: '3.0.1-beta', commit: first.slice(0, 7) });
});

test('a shallow clone is patch 0 unless it holds the root', () => {
	const { dir: src, head } = repo(3);
	for (const [depth, height] of [['1', '0'], ['2', '0'], ['3', '3']]) {
		const dir = mkdtempSync(join(tmp, 'repo-'));
		git(dir, 'clone', '-q', '--depth', depth, `file://${src}`, 'c');
		assert.deepEqual(buildId(join(dir, 'c'), undefined), { version: `3.0.${height}-beta`, commit: head.slice(0, 7) }, `--depth ${depth}`);
	}
});

test('no git is patch 0 and commit unknown', () => {
	const dir = mkdtempSync(join(tmp, 'repo-'));
	assert.deepEqual(buildId(dir, undefined), { version: '3.0.0-beta', commit: 'unknown' });
});
