// The environment the wasm artifact is built in.
//
// Used by build-cli-wasm.mjs, because ensure-wasm.mjs takes the module the
// `wasm` branch holds as the one this tree would build, and this is the rule
// that makes that honest.

import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

// The bytes must not depend on WHERE the build ran, because a module built
// on the forge's runner stands in for one built here. Panic payloads carry `file!()` strings, and
// for registry dependencies cargo passes those as absolute paths — so
// without this the artifact encodes `/home/cans/.cargo/...` here and
// `/home/runner/.cargo/...` on a runner, and the comparison fails for a
// reason that has nothing to do with drift. rustc already remaps its own
// standard library to `/rustc/<hash>`; these two flags do the same for
// the registry and the checkout. Every build runs this script, so the flags
// are the same on every machine.
export const cargoHome = process.env.CARGO_HOME ?? resolve(homedir(), '.cargo');

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
export const env = {
	...process.env,
	RUSTFLAGS: process.env.RUSTFLAGS ? `${process.env.RUSTFLAGS} ${remap}` : remap
};

