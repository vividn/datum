// Which build this is (da-5172a): `3.beta.<height>` and the sha7, the same
// two things crates/datum-cli/build_id.rs gives `datum --version`. The gear
// menu shows the WEB's build; the CLI module inside it is often reused from
// an earlier main commit, and its own `datum --version` names that one.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

/** @param {string} dir @param {string[]} args @returns {string | null} */
function git(dir, args) {
	try {
		return execFileSync('git', args, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
	} catch {
		return null;
	}
}

/** A shallow boundary commit whose own object names a parent the clone left out. */
function truncated(/** @type {string} */ dir) {
	let boundary = '';
	try {
		boundary = readFileSync(git(dir, ['rev-parse', '--path-format=absolute', '--git-path', 'shallow']) ?? '', 'utf8');
	} catch {
		// No shallow file: the history is whole, or there is no git at all.
	}
	return boundary
		.split('\n')
		.filter(Boolean)
		.some((sha) => /^parent /m.test(git(dir, ['cat-file', 'commit', sha]) ?? 'parent '));
}

/**
 * `{ version: '3.beta.<height>', commit: '<sha7>[+]' }`. The height is `git
 * rev-list --count`, the number the forge shows; `3.beta` is the workspace's
 * Cargo `3.0.0-beta`. CI's `GITHUB_SHA` names the commit a run was for even
 * when the checkout is a detached merge. Either half is `unknown` rather
 * than wrong: a shallow clone counts only its own truncated history.
 *
 * @param {string} dir
 * @param {string | undefined} ciSha  `GITHUB_SHA`, where there is one
 */
export function buildId(dir, ciSha) {
	const toml = readFileSync(new URL('../../Cargo.toml', import.meta.url), 'utf8');
	const [, major, pre] = /^version = "(\d+)\.\d+\.\d+-(\w+)"$/m.exec(toml) ?? [];
	const rev = ciSha ?? 'HEAD';
	const height = (!truncated(dir) && git(dir, ['rev-list', '--count', rev])) || 'unknown';
	const sha = git(dir, ['rev-parse', '--verify', rev]);
	const dirty = !ciSha && !!git(dir, ['--no-optional-locks', 'status', '--porcelain']);
	return { version: `${major}.${pre}.${height}`, commit: sha ? `${sha.slice(0, 7)}${dirty ? '+' : ''}` : 'unknown' };
}
