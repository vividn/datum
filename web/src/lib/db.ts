import { SCHEMA } from './schema.generated.js';
import type { Entry, StoredVerb } from './types.js';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SqliteDb = any;

let dbInstance: SqliteDb | null = null;
let initPromise: Promise<SqliteDb> | null = null;

/** Where the open database actually lives. Null until [`initDb`] resolves. */
export type StorageKind = 'opfs' | 'memory';
let storage: StorageKind | null = null;

/**
 * Whether entries survive a reload. Null before the database is open.
 *
 * The UI states this rather than assuming it — see $lib/BetaBanner.svelte.
 * A promise the app makes about the user's data has to be read off the
 * storage that is actually in use.
 */
export function storageKind(): StorageKind | null {
	return storage;
}

export async function initDb(): Promise<SqliteDb> {
	if (dbInstance) return dbInstance;
	if (initPromise) return initPromise;

	initPromise = (async () => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const sqlite3InitModule = ((await import('@sqlite.org/sqlite-wasm')) as any).default;
		const sqlite3 = await sqlite3InitModule({
			print: console.log,
			printErr: console.error
		});

		const db = await openPersistent(sqlite3);

		db.exec(SCHEMA);
		dbInstance = db;
		return db;
	})();

	return initPromise;
}

/**
 * Open the on-device database, preferring durable storage.
 *
 * The SAH-pool VFS, not `oo1.OpfsDb`: the plain OPFS VFS needs
 * `Atomics.wait()`, so it only runs in a Worker. Both need a
 * cross-origin-isolated document — see the COOP/COEP headers in
 * vite.config.ts, which the deployment must send too.
 *
 * On the main thread this currently always falls back: every OPFS VFS
 * needs `FileSystemFileHandle.createSyncAccessHandle`, which the platform
 * exposes to dedicated workers only, so the pool install fails with
 * "Missing required OPFS APIs". Real persistence needs SQLite moved into a
 * Worker (da-5iqw); the feature detection stays so this starts working the
 * moment it runs somewhere the handle exists.
 *
 * The in-memory fallback keeps the app usable meanwhile. It loses
 * everything on reload, so it warns and records `memory` as the storage
 * kind rather than letting the UI imply durability it doesn't have.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function openPersistent(sqlite3: any): Promise<SqliteDb> {
	if (typeof sqlite3.installOpfsSAHPoolVfs === 'function') {
		try {
			const pool = await sqlite3.installOpfsSAHPoolVfs({});
			storage = 'opfs';
			return new pool.OpfsSAHPoolDb('/datum.db');
		} catch (e) {
			console.warn('OPFS SAH pool unavailable:', e);
		}
	}
	console.warn('no durable storage available — entries will not survive a reload');
	storage = 'memory';
	return new sqlite3.oo1.DB(':memory:', 'c');
}

function rowToEntry(row: Record<string, unknown>): Entry {
	return {
		hid: row.hid as string,
		slug: row.slug as string,
		contentHash: row.content_hash as string,
		field: row.field as string,
		state: row.state ? JSON.parse(row.state as string) : undefined,
		dur: (row.dur as string | null) ?? undefined,
		followingState: row.following_state ? JSON.parse(row.following_state as string) : undefined,
		verb: row.verb as StoredVerb,
		data: row.data ? JSON.parse(row.data as string) : undefined,
		occurTime: {
			utc: row.occur_time as string,
			offsetMinutes: (row.occur_time_offset as number | null) ?? 0,
			timezone: (row.occur_time_tz as string | null) ?? undefined
		},
		occurTimeUncertainty: (row.occur_time_uncertainty as string | null) ?? undefined,
		lat: (row.lat as number | null) ?? undefined,
		lon: (row.lon as number | null) ?? undefined,
		location: (row.location as string | null) ?? undefined,
		createTime: row.create_time as string,
		modifyTime: row.modify_time as string,
		deviceId: row.device_id as string,
		rawInput: (row.raw_input as string | null) ?? ''
	};
}

export async function insertEntry(entry: Entry): Promise<void> {
	const db = await initDb();
	db.exec({
		sql: `INSERT INTO entries (
			hid, slug, content_hash, field,
			state, dur, following_state, verb, data,
			occur_time, occur_time_offset, occur_time_tz, occur_time_uncertainty,
			lat, lon, location,
			create_time, modify_time, device_id, raw_input
		) VALUES (
			$hid, $slug, $content_hash, $field,
			$state, $dur, $following_state, $verb, $data,
			$occur_time, $occur_time_offset, $occur_time_tz, $occur_time_uncertainty,
			$lat, $lon, $location,
			$create_time, $modify_time, $device_id, $raw_input
		)`,
		bind: {
			$hid: entry.hid,
			$slug: entry.slug,
			$content_hash: entry.contentHash,
			$field: entry.field,
			$state: entry.state !== undefined ? JSON.stringify(entry.state) : null,
			$dur: entry.dur ?? null,
			$following_state:
				entry.followingState !== undefined ? JSON.stringify(entry.followingState) : null,
			$verb: entry.verb,
			$data: entry.data !== undefined ? JSON.stringify(entry.data) : null,
			$occur_time: entry.occurTime.utc,
			$occur_time_offset: entry.occurTime.offsetMinutes,
			$occur_time_tz: entry.occurTime.timezone ?? null,
			$occur_time_uncertainty: entry.occurTimeUncertainty ?? null,
			$lat: entry.lat ?? null,
			$lon: entry.lon ?? null,
			$location: entry.location ?? null,
			$create_time: entry.createTime,
			$modify_time: entry.modifyTime,
			$device_id: entry.deviceId,
			$raw_input: entry.rawInput
		}
	});
}

export async function getEntriesForField(field: string, limit = 50): Promise<Entry[]> {
	const db = await initDb();
	const rows: Record<string, unknown>[] = [];
	db.exec({
		sql: `SELECT * FROM entries
		      WHERE field = $field AND deleted_at IS NULL
		      ORDER BY occur_time DESC
		      LIMIT $limit`,
		bind: { $field: field, $limit: limit },
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => rows.push(row)
	});
	return rows.map(rowToEntry);
}

export async function getEntriesInRange(start: Date, end: Date): Promise<Entry[]> {
	const db = await initDb();
	const rows: Record<string, unknown>[] = [];
	db.exec({
		sql: `SELECT * FROM entries
		      WHERE occur_time >= $start AND occur_time <= $end
		        AND deleted_at IS NULL
		      ORDER BY occur_time DESC`,
		bind: { $start: start.toISOString(), $end: end.toISOString() },
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => rows.push(row)
	});
	return rows.map(rowToEntry);
}

export async function getRecentEntries(limit = 20): Promise<Entry[]> {
	const db = await initDb();
	const rows: Record<string, unknown>[] = [];
	db.exec({
		sql: `SELECT * FROM entries
		      WHERE deleted_at IS NULL
		      ORDER BY occur_time DESC
		      LIMIT $limit`,
		bind: { $limit: limit },
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => rows.push(row)
	});
	return rows.map(rowToEntry);
}

export async function getAllFields(): Promise<string[]> {
	const db = await initDb();
	const fields: string[] = [];
	db.exec({
		sql: `SELECT DISTINCT field FROM entries WHERE deleted_at IS NULL ORDER BY field`,
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => fields.push(row.field as string)
	});
	return fields;
}

export async function getLatestPerField(): Promise<Entry[]> {
	const db = await initDb();
	const rows: Record<string, unknown>[] = [];
	db.exec({
		sql: `SELECT e.* FROM entries e
		      INNER JOIN (
		        SELECT field, MAX(occur_time) AS max_time
		        FROM entries
		        WHERE deleted_at IS NULL
		        GROUP BY field
		      ) latest ON e.field = latest.field AND e.occur_time = latest.max_time
		      WHERE e.deleted_at IS NULL
		      ORDER BY e.field`,
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => rows.push(row)
	});
	return rows.map(rowToEntry);
}
