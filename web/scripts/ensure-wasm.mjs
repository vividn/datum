// Put a datum-cli.wasm, and the sync worker's module (datum-sync.js, .d.ts
// and _bg.wasm, da-6kpj5), that match this tree's Rust in src/lib/wasm/.
//
// The module is a build product like target/, not a committed file
// (da-0qa4x): every push to main builds one onto the `wasm` branch
// (.forgejo/workflows/wasm.yml), one commit per main commit, holding
// the modules and `SOURCE` — the main sha they are for. Beside the local
// copy, src/lib/wasm/SOURCE names a commit whose crate inputs it was built
// from. Wired as predev, prebuild, precheck and precheck:watch, so `pnpm
// dev`, `pnpm run build`, `pnpm test`, `pnpm run check` and `pnpm run
// check:watch` never run against a module another tree built, or against
// none (da-jyktx, da-nr4jd). In order:
//
//   keep   — SOURCE's crate inputs are this tree's: nothing to do;
//   fetch  — this tree's crate inputs are those of its merge-base with
//            origin/main: take the newest `wasm` commit built from the same
//            inputs (usually the merge-base's own);
//   build  — anything else, or no such commit (not built yet, offline):
//            `build-cli-wasm.mjs`, the pinned toolchain.
//
// `--fetch-only` exits 2 instead of building: the wasm workflow and the
// deploy (.github/workflows/deploy.yml, on the push to main) install Rust
// only when it has to. A deploy of a merge that changes Rust usually builds,
// since it starts before the wasm workflow has committed that merge's module.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const repoRoot = resolve(import.meta.dirname, '..', '..');
const dir = resolve(repoRoot, 'web', 'src', 'lib', 'wasm');
/** What a build puts in `dir`, and a `wasm` commit holds beside SOURCE. */
const FILES = ['datum-cli.wasm', 'datum-sync.js', 'datum-sync.d.ts', 'datum-sync_bg.wasm'];
const built = () => FILES.every((f) => existsSync(resolve(dir, f)));
const stamp = resolve(dir, 'SOURCE');

// What the module is built from. Wider than it strictly needs to be (a crate's
// README is doc-only): a needless build costs minutes, a wrong module a day.
const INPUTS = [
	'crates',
	':(exclude,glob)crates/*/tests/**',
	'Cargo.toml',
	'Cargo.lock',
	'rust-toolchain.toml',
	'.cargo',
	'web/scripts/build-cli-wasm.mjs',
	'web/scripts/reproducible.mjs'
];

/** git's stdout, or null when it fails — every failure falls through to a build. */
function git(args, encoding = 'utf8') {
	try {
		return execFileSync('git', args, {
			cwd: repoRoot,
			env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
			encoding,
			stdio: ['ignore', 'pipe', 'ignore'],
			maxBuffer: 64 << 20,
			timeout: 120_000
		});
	} catch {
		return null;
	}
}
const sha = (rev) => git(['rev-parse', '--verify', '-q', `${rev}^{commit}`])?.trim() || null;
// Untracked files among the inputs make the tree match no commit at all.
const untracked = git(['ls-files', '--others', '--exclude-standard', '--', ...INPUTS]) !== '';
/** Does the working tree hold `rev`'s crate inputs? */
const tree = (rev) => !untracked && git(['diff', '--quiet', rev, '--', ...INPUTS]) !== null;

function done(how, source) {
	const head = sha('HEAD');
	source = head && tree(head) ? head : source;
	if (source) writeFileSync(stamp, `${source}\n`);
	else rmSync(stamp, { force: true });
	console.log(`ensure-wasm: ${how}`);
	process.exit(0);
}

const have = built() && existsSync(stamp) ? sha(readFileSync(stamp, 'utf8').trim()) : null;
if (have && tree(have)) done(`kept, built from ${have.slice(0, 10)}'s crates`, have);

const base = sha('origin/main') && git(['merge-base', 'HEAD', 'origin/main'])?.trim();
let why = base ? 'the crates differ from the merge-base with origin/main' : 'no merge-base with origin/main';
if (base && tree(base)) {
	git(['fetch', '-q', 'origin', '+refs/heads/wasm:refs/remotes/origin/wasm']);
	why = `no wasm commit on origin/wasm is built from ${base.slice(0, 10)}'s crates`;
	for (const c of (git(['rev-list', '-n', '200', 'refs/remotes/origin/wasm']) ?? '').split('\n').filter(Boolean)) {
		const source = git(['show', `${c}:SOURCE`])?.trim();
		if (!source || git(['diff', '--quiet', source, base, '--', ...INPUTS]) === null) continue;
		// A commit from before the sync module holds only the CLI's.
		const bytes = FILES.map((f) => git(['show', `${c}:${f}`], 'buffer'));
		if (bytes.some((b) => !b)) continue;
		mkdirSync(dir, { recursive: true });
		FILES.forEach((f, i) => {
			const path = resolve(dir, f);
			if (!existsSync(path) || !readFileSync(path).equals(bytes[i])) writeFileSync(path, bytes[i]);
		});
		done(`fetched ${c.slice(0, 10)} (wasm for ${source.slice(0, 10)}): the crates are ${base.slice(0, 10)}'s`, base);
	}
}

if (process.argv.includes('--fetch-only')) {
	console.log(`ensure-wasm: not built: ${why}`);
	process.exit(2);
}
console.log(`ensure-wasm: building: ${why}`);
execFileSync(process.execPath, [resolve(import.meta.dirname, 'build-cli-wasm.mjs')], { stdio: 'inherit' });
done('built', null);
