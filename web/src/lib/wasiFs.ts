// The CLI's filesystem under WASI: one preopened directory whose database
// files are OPFS sync access handles (da-1vat).
//
// The shim ships an OPFS *file* (`SyncOPFSFile`) but no OPFS directory, and
// SQLite needs a directory: in rollback-journal mode — the only mode it has
// on WASI, which compiles WAL out — it creates `datum.db-journal` at the
// start of every write transaction and deletes it at commit. A sync access
// handle can be neither created nor deleted synchronously, and WASI calls
// are synchronous, so the worker opens a handle for every file SQLite will
// touch before the process starts, and this directory maps onto them:
//
//   a zero-length file reads as absent
//   creating it truncates the handle and makes it present
//   deleting it truncates the handle to zero
//
// So a journal left behind by a tab that died mid-write is still there, and
// non-empty, for the next process — which is what lets SQLite roll it back.
//
// Every other path lives in memory for the length of one process: the
// dot-file lock SQLite's WASI build takes (`datum.db.lock`, a directory —
// WASI has no fcntl locks), and anything else datum writes under HOME.
//
// The process's own stdout and stderr are here too (`Sink`), for the same
// reason the directory is: what the shim ships grows by copying itself.

import { Directory, Fd, PreopenDirectory, SyncOPFSFile, wasi, type Inode } from '@bjorn3/browser_wasi_shim';

/**
 * The databases that persist: the user's, and `_internal` — the app's own
 * datum database, which holds the saved layouts (da-wpgy). Any other `--db`
 * lives in memory for one process, as every other file does.
 */
export const DATABASES = ['datum', '_internal'];

/** Every file SQLite keeps for them in rollback-journal mode. */
export const DB_FILES = DATABASES.flatMap((db) => [`${db}.db`, `${db}.db-journal`]);

/** The part of `FileSystemSyncAccessHandle` the shim uses. */
export type Handle = ConstructorParameters<typeof SyncOPFSFile>[0];

/** `name`, preopened, with `handles` as the files SQLite creates and deletes. */
export function opfsDirectory(name: string, handles: Map<string, Handle>): PreopenDirectory {
	const backed = new Map([...handles].map(([file, handle]) => [file, new SyncOPFSFile(quotaAware(handle))]));

	class Contents extends Map<string, Inode> {
		delete(file: string): boolean {
			const opfs = backed.get(file);
			if (opfs && this.get(file) === opfs) opfs.handle.truncate(0);
			return super.delete(file);
		}
	}

	class OpfsDirectory extends Directory {
		create_entry_for_path(path: string, isDir: boolean) {
			const opfs = backed.get(path);
			if (!opfs || isDir) return super.create_entry_for_path(path, isDir);
			opfs.handle.truncate(0);
			this.contents.set(path, opfs);
			return { ret: wasi.ERRNO_SUCCESS, entry: opfs as Inode };
		}
	}

	const preopen = new PreopenDirectory(name, new Map());
	preopen.dir = new OpfsDirectory(
		new Contents([...backed].filter(([, opfs]) => opfs.handle.getSize() > 0))
	);
	return preopen;
}

const isFull = (e: unknown) => (e as { name?: string } | null)?.name === 'QuotaExceededError';

/**
 * `handle`, with a full disk told to SQLite rather than thrown through it (da-qlu7).
 *
 * The shim calls the handle with no try/catch, so a `QuotaExceededError`
 * unwound through the wasm import and trapped the process mid-write. Now:
 *
 *   write     writes 0 bytes: SQLite's unix VFS reads a short write as
 *             SQLITE_FULL, rolls the transaction back, and the command fails
 *             with "database or disk is full"
 *   truncate  a grow that does not fit leaves the file as it was: SQLite sees
 *             success, and its next write past the old end is the short
 *             write above (it only grows a file by truncate for a size hint)
 *   flush     never throws it — the quota is spent by write and truncate —
 *             so it is left alone
 *
 * Only a full disk: any other exception still traps, as a bug should.
 */
function quotaAware(handle: Handle): Handle {
	// Every call made on `handle` itself: a native handle's methods throw
	// "Illegal invocation" for any other receiver.
	return {
		getSize: () => handle.getSize(),
		read: (buffer, options) => handle.read(buffer, options),
		flush: () => handle.flush(),
		close: () => handle.close(),
		write(buffer, options) {
			try {
				return handle.write(buffer, options);
			} catch (e) {
				if (isFull(e)) return 0;
				throw e;
			}
		},
		truncate(to) {
			try {
				handle.truncate(to);
			} catch (e) {
				if (!isFull(e)) throw e;
			}
		}
	};
}

/** A sync access handle over memory, for a browser that will not give one. */
export class MemoryHandle implements Handle {
	private data = new Uint8Array(0);
	private length = 0;

	getSize() {
		return this.length;
	}
	read(buffer: ArrayBufferView, { at = 0 } = {}) {
		const view = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
		const chunk = this.data.subarray(at, Math.min(this.length, at + view.length));
		view.set(chunk);
		return chunk.length;
	}
	write(buffer: ArrayBufferView, { at = 0 } = {}) {
		const bytes = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
		this.truncate(Math.max(this.length, at + bytes.length));
		this.data.set(bytes, at);
		return bytes.length;
	}
	truncate(to: number) {
		if (to > this.data.length) {
			const grown = new Uint8Array(Math.max(to, this.data.length * 2));
			grown.set(this.data.subarray(0, this.length));
			this.data = grown;
		} else if (to < this.length) {
			this.data.fill(0, to, this.length);
		}
		this.length = to;
	}
	flush() {}
	close() {}
}

/**
 * Somewhere for a process to write, that keeps what it is handed.
 *
 * stdout and stderr were a `File` behind an `OpenFile`, and that grows by
 * allocating an array of exactly the new length and copying the old one
 * into it, on every write that extends it — so N bytes arriving in k
 * writes copy O(N·k). A 13.6 MB `db backup -` (20k entries) took 6.1 s in
 * chromium that way, against 0.43 s for the same command under node:wasi
 * — and 1.3 s now (da-0lxu). The shim has already copied each write out
 * of the wasm memory by the time it gets here, so keeping the pieces
 * costs nothing and `text()` joins them once, in order, at exit.
 *
 * A regular file, as `OpenFile` reported, rather than the character device
 * `ConsoleStdout` reports: rust reads a character device that can neither
 * seek nor tell as a terminal, and what datum prints to a terminal is not
 * what it prints to a pipe.
 */
export class Sink extends Fd {
	private readonly chunks: Uint8Array[] = [];
	private size = 0;

	/**
	 * `said`, if given, hears each write as it happens. The process holds the
	 * worker until it exits, so this is the only way what it prints — an
	 * import's progress (da-h9vj) — reaches the page while it runs.
	 *
	 * Declared as a field and assigned here rather than as a constructor
	 * parameter property: node's strip-only type stripping, which is how the
	 * tests load this file, refuses those (da-c76f).
	 */
	private readonly said?: (text: string) => void;

	constructor(said?: (text: string) => void) {
		super();
		this.said = said;
	}

	fd_fdstat_get() {
		return { ret: wasi.ERRNO_SUCCESS, fdstat: new wasi.Fdstat(wasi.FILETYPE_REGULAR_FILE, 0) };
	}

	fd_write(data: Uint8Array) {
		this.said?.(new TextDecoder().decode(data));
		this.chunks.push(data);
		this.size += data.byteLength;
		return { ret: wasi.ERRNO_SUCCESS, nwritten: data.byteLength };
	}

	/** Every write, in order, and nothing else. */
	text(): string {
		const all = new Uint8Array(this.size);
		let at = 0;
		for (const chunk of this.chunks) {
			all.set(chunk, at);
			at += chunk.byteLength;
		}
		return new TextDecoder().decode(all);
	}
}
