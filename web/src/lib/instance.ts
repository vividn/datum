// The CLI as one long-lived instance (da-jm4sv).
//
// `datum-cli.wasm` is the CLI's library built as a WASI reactor
// (scripts/build-cli-wasm.mjs): no `_start`, and an export, `datum_line`,
// that runs one command as `main` would and returns its exit code. So one
// instance answers command after command: before each, its argv, env,
// stdin, stdout, stderr, `/data` and wall clock are set here, as a new
// process's would be. What it keeps between them is the CLI's to keep — a
// database's connection, SQLite's cache and prepared statements, for the
// files `DATUM_KEEP_OPEN` names — and a command that leaves it in doubt ends
// it: a trap (`panic = "abort"`), a `proc_exit` from inside a command.
//
// A connection kept open keeps its file descriptors, and those outlive the
// sync access handles of the burst that opened them: so the files' inodes
// are this instance's (`inodes`, which #lib/wasiFs.ts `opfsDirectory`
// points at each burst's handles), not one process's.

import { WASI, WASIProcExit, wasi as defs, type Fd, type SyncOPFSFile } from './vendor/browser_wasi_shim/index.js';

/** What a command ended with, and whether the instance may run another. */
export interface Ran {
	code: number;
	/** The instance ended inside the command, by a trap or an exit: start another. */
	dead: boolean;
	/** It trapped, rather than exited. */
	trapped: boolean;
}

export class Instance {
	/** Each file's inode, by name in `/data`: what a kept descriptor points at. */
	readonly inodes = new Map<string, SyncOPFSFile>();
	private readonly wasi = new WASI([], [], [], { debug: false });
	private exports!: { memory: WebAssembly.Memory; datum_line: () => number | Promise<number> };
	/** The wall clock of the running command, in ms (`FROZEN_AT`); its own when unset. */
	private at: number | undefined;
	private dead = false;

	/** A new instance of `module`, its exports through `WebAssembly.instantiate`. */
	static async start(module: WebAssembly.Module): Promise<Instance> {
		const cli = new Instance();
		const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: cli.wasi.wasiImport });
		cli.wasi.inst = instance as unknown as WASI['inst'];
		cli.exports = instance.exports as unknown as Instance['exports'];
		return cli;
	}

	private constructor() {
		const wasi = this.wasi;
		// The shim (0.4.2, the latest) sizes argv in UTF-16 units and writes it
		// as UTF-8, so a non-ASCII argument overran its buffer into the next one:
		// `spec 日本語 --verb occur` arrived as `occuS`. Sized in
		// bytes here, the way it already sizes the environment.
		wasi.wasiImport.args_sizes_get = (count: number, size: number) => {
			const view = new DataView(this.exports.memory.buffer);
			view.setUint32(count, wasi.args.length, true);
			view.setUint32(size, wasi.args.reduce((n, arg) => n + new TextEncoder().encode(arg).length + 1, 0), true);
			return 0;
		};
		// The gesture's instant is the command's wall clock (`FROZEN_AT`): its
		// entry's time and its create time are when the person acted, not when
		// the command got to run. MONOTONIC stays real, so what times itself works.
		const clock = wasi.wasiImport.clock_time_get;
		wasi.wasiImport.clock_time_get = (id: number, precision: bigint, time: number) => {
			if (id !== defs.CLOCKID_REALTIME || this.at === undefined) return clock(id, precision, time);
			new DataView(this.exports.memory.buffer).setBigUint64(time, BigInt(this.at) * 1_000_000n, true);
			return 0;
		};
	}

	/** The instance's linear memory, in bytes: it grows and never shrinks. */
	get memory(): number {
		return this.exports.memory.buffer.byteLength;
	}

	/**
	 * Run `args` (argv, `datum` first) with `env`, and `fds` as its stdin,
	 * stdout, stderr and `/data`. Never twice at once, nor on an instance
	 * that died. `datum_line` is awaited, though it never yields: a test
	 * holds a command there (queue-order.test.mjs).
	 */
	async run(args: string[], env: string[], fds: [Fd, Fd, Fd, Fd], at?: number): Promise<Ran> {
		if (this.dead) throw new Error('this datum instance has ended: start another');
		this.wasi.args = args;
		this.wasi.env = env;
		fds.forEach((fd, i) => (this.wasi.fds[i] = fd));
		this.at = at;
		try {
			return { code: await this.exports.datum_line(), dead: false, trapped: false };
		} catch (e) {
			this.dead = true;
			if (e instanceof WASIProcExit) return { code: e.code, dead: true, trapped: false };
			// A trap: `panic = "abort"` in the wasm profile, after the panic
			// message reached stderr. 134 is what a shell reports for SIGABRT.
			console.warn('datum trapped:', e);
			return { code: 134, dead: true, trapped: true };
		} finally {
			// A closed descriptor's slot is never used again; the ones at the end go.
			const all = this.wasi.fds;
			while (all.length > 4 && all[all.length - 1] === undefined) all.pop();
		}
	}
}
