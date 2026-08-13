import { SCHEMA } from './schema.generated.js';
import type { Entry, StoredVerb } from './types.js';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SqliteDb = any;

let dbInstance: SqliteDb | null = null;
let initPromise: Promise<SqliteDb> | null = null;

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

		let db: SqliteDb;
		if ('opfs' in sqlite3) {
			db = new sqlite3.oo1.OpfsDb('/datum.db');
		} else {
			console.warn('OPFS not available, using in-memory database');
			db = new sqlite3.oo1.DB(':memory:', 'c');
		}

		db.exec(SCHEMA);
		dbInstance = db;
		return db;
	})();

	return initPromise;
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
		unit: (row.unit as string | null) ?? undefined,
		occurTime: {
			utc: row.occur_time as string,
			offsetMinutes: (row.occur_time_offset as number | null) ?? 0,
			timezone: (row.occur_time_tz as string | null) ?? undefined
		},
		occurTimePrecision: (row.occur_time_precision as string | null) ?? undefined,
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
			state, dur, following_state, verb, data, unit,
			occur_time, occur_time_offset, occur_time_tz, occur_time_precision,
			lat, lon, location,
			create_time, modify_time, device_id, raw_input
		) VALUES (
			$hid, $slug, $content_hash, $field,
			$state, $dur, $following_state, $verb, $data, $unit,
			$occur_time, $occur_time_offset, $occur_time_tz, $occur_time_precision,
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
			$unit: entry.unit ?? null,
			$occur_time: entry.occurTime.utc,
			$occur_time_offset: entry.occurTime.offsetMinutes,
			$occur_time_tz: entry.occurTime.timezone ?? null,
			$occur_time_precision: entry.occurTimePrecision ?? null,
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
