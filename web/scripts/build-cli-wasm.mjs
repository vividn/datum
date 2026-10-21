// Build the real `datum` CLI for wasm32-wasip1 and drop it in src/lib/wasm/.
//
// This is route C of da-1vat, datum.day's only runner since da-g8o8: the
// whole CLI, run in the browser one process per command under a WASI shim
// ($lib/cli.ts). It needs a C compiler — rusqlite's `bundled` compiles
// SQLite's C, and for wasm32-wasip1 that means clang plus a wasi sysroot —
// and it is shrunk with wasm-opt.
//
// Both toolchains are PINNED releases, downloaded into target/toolchains/
// and checked against the sha256 below before use. Nothing is installed
// anywhere else: no /opt, no root, no distro package, and the default
// `cargo build/test/clippy` never reach this file. Where the pinned
// tarballs do not exist (they are x86_64-linux), the failure is "the CLI
// module does not build here", never "the repo does not build".
//
// The artifact is not committed (da-0qa4x): `ensure-wasm.mjs` fetches the
// one the `wasm` branch holds for this tree's crates, and runs this when
// there is none. The bytes are reproducible, so either is the same module.
//
// It builds the sync worker's module too (da-6kpj5): `datum-sync-wire` for
// wasm32-unknown-unknown, its JavaScript face by wasm-bindgen (a third pinned
// tool, whose version must be Cargo.lock's `wasm-bindgen`), shrunk the same
// way. ring's C compiles with the same pinned clang, so those bytes are
// reproducible for the same reason the CLI's are.
//
// Usage:  pnpm run build:cli-wasm
//         node scripts/build-cli-wasm.mjs --check   (compiles, writes nothing)

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { cargoHome, env, repoRoot } from './reproducible.mjs';

const TOOLS = {
	// clang 18.1.2. 24, not older: 22's wasm32-wasip1 sysroot has no
	// pthread.h, which SQLite includes (rusqlite only turns threading off
	// for the retired `wasm32-wasi` target name). sha256 recorded from the
	// first download; wasi-sdk publishes none.
	wasiSdk: {
		url: 'https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-24/wasi-sdk-24.0-x86_64-linux.tar.gz',
		sha256: 'c6c38aab56e5de88adf6c1ebc9c3ae8da72f88ec2b656fb024eda8d4167a0bc5',
		dir: 'wasi-sdk-24.0-x86_64-linux'
	},
	// sha256 as published beside the release.
	binaryen: {
		url: 'https://github.com/WebAssembly/binaryen/releases/download/version_123/binaryen-version_123-x86_64-linux.tar.gz',
		sha256: 'e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe',
		dir: 'binaryen-version_123'
	}
};

TOOLS.wasmBindgen = {
	url: 'https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.118/wasm-bindgen-0.2.118-x86_64-unknown-linux-musl.tar.gz',
	sha256: '00b519c9fc2d6e087265da1a00f29160bfcc6a823993482bc2e691910287427b',
	dir: 'wasm-bindgen-0.2.118-x86_64-unknown-linux-musl'
};

const toolchains = resolve(repoRoot, 'target', 'toolchains');
// Where cargo writes, as cargo resolves it: CARGO_TARGET_DIR (relative to the
// cwd it runs in, repoRoot) when set, else <repoRoot>/target.
const targetDir = resolve(repoRoot, process.env.CARGO_TARGET_DIR || 'target');
const out = resolve(repoRoot, 'web', 'src', 'lib', 'wasm', 'datum-cli.wasm');

/** `url`'s bytes, asking up to three times: one HTTP 500 from GitHub failed a CI run (da-wh359). */
async function download(url) {
	for (let attempt = 1; ; attempt++) {
		console.log(`fetching ${url}`);
		try {
			const res = await fetch(url);
			if (!res.ok) throw new Error(`HTTP ${res.status}`);
			return Buffer.from(await res.arrayBuffer());
		} catch (e) {
			if (attempt === 3) throw new Error(`${url}: ${e.message}`);
			console.log(`${e.message}; again in ${attempt * 10}s`);
			await new Promise((r) => setTimeout(r, attempt * 10_000));
		}
	}
}

/** The unpacked tool's directory, downloading and verifying it first if needed. */
async function tool({ url, sha256, dir }) {
	const home = resolve(toolchains, dir);
	if (existsSync(home)) return home;
	mkdirSync(toolchains, { recursive: true });
	const tarball = resolve(toolchains, url.split('/').pop());
	if (!existsSync(tarball)) {
		writeFileSync(tarball, await download(url));
	}
	const actual = createHash('sha256').update(readFileSync(tarball)).digest('hex');
	if (actual !== sha256) throw new Error(`${tarball}: sha256 ${actual}, pinned ${sha256}`);
	execFileSync('tar', ['-xzf', tarball, '-C', toolchains], { stdio: 'inherit' });
	return home;
}

const sdk = await tool(TOOLS.wasiSdk);
const cli = ['-p', 'datum-cli', '--bin', 'datum', '--no-default-features', '--target', 'wasm32-wasip1', '--profile', 'wasm-release'];
const cliEnv = {
	CC_wasm32_wasip1: `${sdk}/bin/clang`,
	AR_wasm32_wasip1: `${sdk}/bin/ar`,
	// The C half gets the same treatment as the Rust half: no
	// absolute path of this machine may reach the bytes.
	CFLAGS_wasm32_wasip1: `--sysroot=${sdk}/share/wasi-sysroot -ffile-prefix-map=${cargoHome}=/cargo`
};
const wire = ['-p', 'datum-sync-wire', '--lib', '--target', 'wasm32-unknown-unknown', '--profile', 'wasm-release'];
const wireEnv = {
	CC_wasm32_unknown_unknown: `${sdk}/bin/clang`,
	AR_wasm32_unknown_unknown: `${sdk}/bin/ar`,
	CFLAGS_wasm32_unknown_unknown: `-ffile-prefix-map=${cargoHome}=/cargo`
};
const cargo = (args, extra) => execFileSync('cargo', args, { cwd: repoRoot, env: { ...env, ...extra }, stdio: 'inherit' });

// `--check`: does this tree still compile for datum.day? `cargo check` of both
// modules' crates, no module written: what a PR that changes crates/ runs in
// place of the web suite (da-q587o).
if (process.argv.includes('--check')) {
	cargo(['check', ...cli], cliEnv);
	cargo(['check', ...wire], wireEnv);
	process.exit(0);
}

const binaryen = await tool(TOOLS.binaryen);
const wasmBindgen = await tool(TOOLS.wasmBindgen);

cargo(['build', ...cli], cliEnv);

mkdirSync(resolve(out, '..'), { recursive: true });
// ensure-wasm.mjs's record of what the module was built from; it writes it
// again after a build it ran, and a build by hand is from nobody knows what.
rmSync(resolve(out, '..', 'SOURCE'), { force: true });
/** `wasm-opt -Oz` `from` into `to`. */
function shrink(from, to) {
	execFileSync(
		`${binaryen}/bin/wasm-opt`,
		[
			from,
			'-Oz',
			// What rustc's generic wasm CPU emits. The profile's `strip` drops
			// the target_features section wasm-opt would otherwise read them
			// from, and without it wasm-opt validates against the 2017 MVP.
			'--enable-bulk-memory',
			'--enable-bulk-memory-opt',
			'--enable-nontrapping-float-to-int',
			'--enable-sign-ext',
			'--enable-mutable-globals',
			'--enable-multivalue',
			'--enable-reference-types',
			'--strip-debug',
			'--strip-producers',
			'-o',
			to
		],
		{ stdio: 'inherit' }
	);
	chmodSync(to, 0o644);
}

shrink(resolve(targetDir, 'wasm32-wasip1/wasm-release/datum.wasm'), out);

const bytes = statSync(out).size;
const gz = gzipSync(readFileSync(out)).length;
console.log(`wrote ${out} (${bytes} bytes, ${gz} gzipped)`);

// The sync worker's module: the wire crate and its wasm-bindgen glue, named
// `datum-sync.js` (typed by `datum-sync.d.ts`) and `datum-sync_bg.wasm`
// beside the CLI's.
cargo(['build', ...wire], wireEnv);
const wasmDir = resolve(out, '..');
execFileSync(
	`${wasmBindgen}/wasm-bindgen`,
	[
		resolve(targetDir, 'wasm32-unknown-unknown/wasm-release/datum_sync_wire.wasm'),
		'--target',
		'web',
		'--out-name',
		'datum-sync',
		'--out-dir',
		wasmDir
	],
	{ stdio: 'inherit' }
);
const sync = resolve(wasmDir, 'datum-sync_bg.wasm');
shrink(sync, sync);
// Its .d.ts types it; the glue itself is generated and not the checker's.
const glue = resolve(wasmDir, 'datum-sync.js');
writeFileSync(glue, `// @ts-nocheck\n${readFileSync(glue, 'utf8')}`);
rmSync(resolve(wasmDir, 'datum-sync_bg.wasm.d.ts'), { force: true });
console.log(`wrote ${sync} (${statSync(sync).size} bytes, ${gzipSync(readFileSync(sync)).length} gzipped)`);
