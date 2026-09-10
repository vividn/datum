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
// WASI has no fcntl locks), and anything datum writes under HOME.

import { Directory, PreopenDirectory, SyncOPFSFile, wasi, type Inode } from '@bjorn3/browser_wasi_shim';

/** Every file SQLite keeps beside a database in rollback-journal mode. */
export const DB_FILES = ['datum.db', 'datum.db-journal'];

/** The part of `FileSystemSyncAccessHandle` the shim uses. */
export type Handle = ConstructorParameters<typeof SyncOPFSFile>[0];

/** `name`, preopened, with `handles` as the files SQLite creates and deletes. */
export function opfsDirectory(name: string, handles: Map<string, Handle>): PreopenDirectory {
	const backed = new Map([...handles].map(([file, handle]) => [file, new SyncOPFSFile(handle)]));

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
