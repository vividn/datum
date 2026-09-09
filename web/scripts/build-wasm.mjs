// Build the browser's Rust modules and drop them in src/lib/wasm/.
//
// Two of them, both the same idea: take a crate that is already the CLI's
// own implementation of something, compile it to `wasm32-unknown-unknown`
// behind a five-function C ABI, and call it from the browser rather than
// writing a second implementation in TypeScript.
//
//   datum-layout-wasm  the dayview / nowview render (da-6h6p)
//   datum-parse-wasm   the parser and the row it builds (da-7scm)
//
// Neither needs wasm-bindgen — a module with no imports instantiates from
// bare `WebAssembly.instantiate` — so this script is just cargo plus a copy.
//
// The artifacts are committed, the way web/tests/fixtures are, so the web
// CI job stays free of the Rust toolchain. Re-run this (`npm run
// build:wasm`) whenever the crates behind them change, and commit the
// result.
//
// They land in src/lib/wasm/ rather than static/ because static/ is copied
// to the site root under the name it has here, and a runtime artifact must
// not have a stable name: the service worker serves cache-first and the
// deploy sends a year of `immutable`, so a returning browser ran the new
// build's JavaScript against the previous build's module until the HTTP
// cache expired (da-tvg0). From src/ they are `?url` imports, which Vite
// emits content-hashed under _app/immutable/.

import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, statSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

// The bytes must not depend on WHERE the build ran, because CI rebuilds
// these and compares them against the committed copies (`wasm-check` in
// .github/workflows/ci.yml). Panic payloads carry `file!()` strings, and
// for registry dependencies cargo passes those as absolute paths — so
// without this the artifact encodes `/home/cans/.cargo/...` here and
// `/home/runner/.cargo/...` on a runner, and the comparison fails for a
// reason that has nothing to do with drift. rustc already remaps its own
// standard library to `/rustc/<hash>`; these two flags do the same for
// the registry and the checkout. Both sides of the comparison run this
// script, so the flags are the same on both.
const cargoHome = process.env.CARGO_HOME ?? resolve(homedir(), '.cargo');

// The standard library needs remapping too, and it has TWO spellings depending
// on a component the pin does not mention.
//
// Without `rust-src`, rustc reports std locations as `/rustc/<commit>/library/...`.
// With it installed, rustc points them back at the local sysroot copy so that
// debuggers can find sources -- and rust-analyzer requires `rust-src`, so every
// developer with a working IDE gets the second spelling and CI gets the first.
// Measured 2026-09-08: installing rust-src and rebuilding moved 11 absolute
// paths into datum-parse.wasm and 14 into datum-layout.wasm, changing both
// hashes; two machines reproduced it identically.
//
// Both spellings are mapped to the same `/rust`, so the bytes match either way.
const rustcV = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
const commitHash = /^commit-hash:\s*(\S+)$/m.exec(rustcV)?.[1];
const sysroot = execFileSync('rustc', ['--print', 'sysroot'], { encoding: 'utf8' }).trim();
const stdRemaps = [
	`--remap-path-prefix=${sysroot}/lib/rustlib/src/rust=/rust`,
	commitHash ? `--remap-path-prefix=/rustc/${commitHash}=/rust` : ''
].filter(Boolean).join(' ');

const remap = `--remap-path-prefix=${cargoHome}=/cargo --remap-path-prefix=${repoRoot}=/datum ${stdRemaps}`;
const env = {
	...process.env,
	RUSTFLAGS: process.env.RUSTFLAGS ? `${process.env.RUSTFLAGS} ${remap}` : remap
};

const MODULES = [
	{
		crate: 'datum-layout-wasm',
		artifact: 'datum_layout_wasm.wasm',
		out: 'datum-layout.wasm',
		// Same reason as below: both crates reach `datum-parse`, whose
		// `native` feature is default-on for host builds and compiles out
		// for the browser.
		noDefaultFeatures: true
	},
	{
		crate: 'datum-parse-wasm',
		artifact: 'datum_parse_wasm.wasm',
		out: 'datum-parse.wasm',
		// `native` is default-on so a host build of the crate keeps local
		// timezone detection; the browser build turns it off, which is what
		// makes `datum-parse` compile for wasm at all.
		noDefaultFeatures: true
	},
	{
		crate: 'datum-render-wasm',
		artifact: 'datum_render_wasm.wasm',
		out: 'datum-render.wasm',
		// Off: `native` (host-only surfaces) and `db` (SQLite, which would
		// need a C toolchain for wasm32). The renderer is the pure half.
		noDefaultFeatures: true
	}
];

for (const { crate, artifact, out, noDefaultFeatures } of MODULES) {
	const args = [
		'build',
		'-p',
		crate,
		'--target',
		'wasm32-unknown-unknown',
		'--profile',
		'wasm-release'
	];
	if (noDefaultFeatures) args.push('--no-default-features');
	execFileSync('cargo', args, { cwd: repoRoot, env, stdio: 'inherit' });

	const built = resolve(repoRoot, 'target/wasm32-unknown-unknown/wasm-release', artifact);
	const outPath = resolve(here, '..', 'src', 'lib', 'wasm', out);
	mkdirSync(dirname(outPath), { recursive: true });
	copyFileSync(built, outPath);
	// cargo marks the module executable and copyFileSync carries that mode
	// across; without this every rebuild shows up as a mode-only diff.
	chmodSync(outPath, 0o644);
	const bytes = statSync(outPath).size;
	const gz = gzipSync(readFileSync(outPath)).length;
	console.log(`wrote ${outPath} (${bytes} bytes, ${gz} gzipped)`);
}
