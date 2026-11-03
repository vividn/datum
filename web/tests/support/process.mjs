// One command of the CLI on an instance of its own, as a process of its own
// ran it before the module was a reactor (#lib/instance.ts, da-jm4sv): its
// exit code. A trap is thrown, as `WASI.start` threw it.

import { WASIProcExit } from '../../src/lib/vendor/browser_wasi_shim/index.js';

export function exec(module, wasi) {
	const instance = new WebAssembly.Instance(module, { wasi_snapshot_preview1: wasi.wasiImport });
	wasi.inst = instance;
	try {
		return instance.exports.datum_line();
	} catch (e) {
		if (e instanceof WASIProcExit) return e.code;
		throw e;
	}
}
