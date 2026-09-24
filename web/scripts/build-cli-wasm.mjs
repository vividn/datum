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
// Usage:  pnpm run build:cli-wasm

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

const toolchains = resolve(repoRoot, 'target', 'toolchains');
const out = resolve(repoRoot, 'web', 'src', 'lib', 'wasm', 'datum-cli.wasm');

/** The unpacked tool's directory, downloading and verifying it first if needed. */
async function tool({ url, sha256, dir }) {
	const home = resolve(toolchains, dir);
	if (existsSync(home)) return home;
	mkdirSync(toolchains, { recursive: true });
	const tarball = resolve(toolchains, url.split('/').pop());
	if (!existsSync(tarball)) {
		console.log(`fetching ${url}`);
		const res = await fetch(url);
		if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
		writeFileSync(tarball, Buffer.from(await res.arrayBuffer()));
	}
	const actual = createHash('sha256').update(readFileSync(tarball)).digest('hex');
	if (actual !== sha256) throw new Error(`${tarball}: sha256 ${actual}, pinned ${sha256}`);
	execFileSync('tar', ['-xzf', tarball, '-C', toolchains], { stdio: 'inherit' });
	return home;
}

const sdk = await tool(TOOLS.wasiSdk);
const binaryen = await tool(TOOLS.binaryen);

execFileSync(
	'cargo',
	[
		'build',
		'-p',
		'datum-cli',
		'--bin',
		'datum',
		'--no-default-features',
		'--target',
		'wasm32-wasip1',
		'--profile',
		'wasm-release'
	],
	{
		cwd: repoRoot,
		env: {
			...env,
			CC_wasm32_wasip1: `${sdk}/bin/clang`,
			AR_wasm32_wasip1: `${sdk}/bin/ar`,
			// The C half gets the same treatment as the Rust half: no
			// absolute path of this machine may reach the bytes.
			CFLAGS_wasm32_wasip1: `--sysroot=${sdk}/share/wasi-sysroot -ffile-prefix-map=${cargoHome}=/cargo`
		},
		stdio: 'inherit'
	}
);

mkdirSync(resolve(out, '..'), { recursive: true });
// ensure-wasm.mjs's record of what the module was built from; it writes it
// again after a build it ran, and a build by hand is from nobody knows what.
rmSync(resolve(out, '..', 'SOURCE'), { force: true });
execFileSync(
	`${binaryen}/bin/wasm-opt`,
	[
		resolve(repoRoot, 'target/wasm32-wasip1/wasm-release/datum.wasm'),
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
		out
	],
	{ stdio: 'inherit' }
);
chmodSync(out, 0o644);
const bytes = statSync(out).size;
const gz = gzipSync(readFileSync(out)).length;
console.log(`wrote ${out} (${bytes} bytes, ${gz} gzipped)`);
