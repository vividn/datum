// Which build this is (da-5172a, da-cv118): `3.0.<height>-beta` and the
// sha7, the same two things crates/datum-cli/build_id.rs gives `datum
// --version`. The gear menu shows the WEB's build; the CLI module inside it is often reused from
// an earlier main commit, and its own `datum --version` names that one.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

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
 * `{ version: '3.0.<height>-beta', commit: '<sha7>[+]' }`: valid semver,
 * the workspace's Cargo `3.0.0-beta` with the patch the height — `git
 * rev-list --count`, the number the forge shows. CI's `GITHUB_SHA` names the
 * commit a run was for even when the checkout is a detached merge. Neither
 * is wrong where git cannot say: a shallow clone counts only its own
 * truncated history, so the patch is `0` (semver needs a number), and a
 * tree with no git has commit `unknown`.
 *
 * @param {string} dir
 * @param {string | undefined} ciSha  `GITHUB_SHA`, where there is one
 */
export function buildId(dir, ciSha) {
	const toml = readFileSync(new URL('../../Cargo.toml', import.meta.url), 'utf8');
	const [, major, minor, pre] = /^version = "(\d+)\.(\d+)\.\d+(?:-(\S+))?"$/m.exec(toml) ?? [];
	const rev = ciSha ?? 'HEAD';
	const height = (!truncated(dir) && git(dir, ['rev-list', '--count', rev])) || '0';
	const sha = git(dir, ['rev-parse', '--verify', rev]);
	const dirty = !ciSha && !!git(dir, ['--no-optional-locks', 'status', '--porcelain']);
	return {
		version: `${major}.${minor}.${height}${pre ? `-${pre}` : ''}`,
		commit: sha ? `${sha.slice(0, 7)}${dirty ? '+' : ''}` : 'unknown'
	};
}

// `node web/scripts/build-id.mjs` prints what `datum --version` does for this
// checkout, which CI compares with the binary it built (da-cv118).
if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const { version, commit } = buildId(process.cwd(), process.env.GITHUB_SHA || undefined);
	console.log(`datum ${version} (${commit})`);
}
