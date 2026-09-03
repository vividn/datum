// Build the browser's Rust modules and drop them in static/.
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

import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, statSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

const MODULES = [
	{ crate: 'datum-layout-wasm', artifact: 'datum_layout_wasm.wasm', out: 'datum-layout.wasm' },
	{
		crate: 'datum-parse-wasm',
		artifact: 'datum_parse_wasm.wasm',
		out: 'datum-parse.wasm',
		// `native` is default-on so a host build of the crate keeps local
		// timezone detection; the browser build turns it off, which is what
		// makes `datum-parse` compile for wasm at all.
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
	execFileSync('cargo', args, { cwd: repoRoot, stdio: 'inherit' });

	const built = resolve(repoRoot, 'target/wasm32-unknown-unknown/wasm-release', artifact);
	const outPath = resolve(here, '..', 'static', out);
	mkdirSync(dirname(outPath), { recursive: true });
	copyFileSync(built, outPath);
	// cargo marks the module executable and copyFileSync carries that mode
	// across; without this every rebuild shows up as a mode-only diff.
	chmodSync(outPath, 0o644);
	const bytes = statSync(outPath).size;
	const gz = gzipSync(readFileSync(outPath)).length;
	console.log(`wrote ${outPath} (${bytes} bytes, ${gz} gzipped)`);
}
