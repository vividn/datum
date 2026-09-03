// Build the browser layout module and drop it in static/.
//
// `crates/datum-layout-wasm` is `datum-layout` behind a four-function C ABI
// (da-6h6p): the same compute+render the CLI runs for `datum dayview` and
// `datum nowview`, reachable from the browser. It needs no wasm-bindgen —
// a `wasm32-unknown-unknown` module with no imports instantiates from bare
// `WebAssembly.instantiate` — so this script is just cargo plus a copy.
//
// The artifact is committed, the way web/tests/fixtures are, so the web CI
// job stays free of the Rust toolchain. Re-run this (`npm run build:wasm`)
// whenever the layout crates change, and commit the result.

import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const outPath = resolve(here, '..', 'static/datum-layout.wasm');

const args = [
	'build',
	'-p',
	'datum-layout-wasm',
	'--target',
	'wasm32-unknown-unknown',
	'--profile',
	'wasm-release'
];

execFileSync('cargo', args, { cwd: repoRoot, stdio: 'inherit' });

const built = resolve(
	repoRoot,
	'target/wasm32-unknown-unknown/wasm-release/datum_layout_wasm.wasm'
);
mkdirSync(dirname(outPath), { recursive: true });
copyFileSync(built, outPath);
// cargo marks the module executable and copyFileSync carries that mode
// across; without this every rebuild shows up as a mode-only diff.
chmodSync(outPath, 0o644);
console.log(`wrote ${outPath} (${statSync(outPath).size} bytes)`);
