import type { Entry } from './types.js';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SqliteDb = any;

let dbInstance: SqliteDb | null = null;
let initPromise: Promise<SqliteDb> | null = null;

const SCHEMA = `
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS entries (
    id              TEXT NOT NULL PRIMARY KEY,
    slug            TEXT NOT NULL,
    content_hash    TEXT NOT NULL UNIQUE,
    field           TEXT NOT NULL,
    verb            TEXT NOT NULL,
    value           TEXT,
    extra_data      TEXT,
    unit            TEXT,
    time_start_utc  TEXT,
    time_start_offset INTEGER,
    time_start_tz   TEXT,
    time_end_utc    TEXT,
    time_end_offset INTEGER,
    time_end_tz     TEXT,
    lat             REAL,
    lon             REAL,
    location        TEXT,
    created_at      TEXT NOT NULL,
    modified_at     TEXT NOT NULL,
    device_id       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entries_field
    ON entries (field, COALESCE(time_start_utc, time_end_utc, created_at));

CREATE INDEX IF NOT EXISTS idx_entries_time
    ON entries (COALESCE(time_start_utc, time_end_utc, created_at));

CREATE TABLE IF NOT EXISTS field_specs (
    field       TEXT NOT NULL PRIMARY KEY,
    spec_json   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS aliases (
    name        TEXT NOT NULL PRIMARY KEY,
    alias_json  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_state (
    device_id   TEXT NOT NULL PRIMARY KEY,
    last_seq    TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
`;

export async function initDb(): Promise<SqliteDb> {
	if (dbInstance) return dbInstance;
	if (initPromise) return initPromise;

	initPromise = (async () => {
		const sqlite3InitModule = (await import('@sqlite.org/sqlite-wasm')).default;
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
		id: row.id as string,
		slug: row.slug as string,
		contentHash: row.content_hash as string,
		field: row.field as string,
		verb: row.verb as Entry['verb'],
		value: row.value ? JSON.parse(row.value as string) : undefined,
		extraData: row.extra_data ? JSON.parse(row.extra_data as string) : undefined,
		unit: row.unit as string | undefined,
		timeStart:
			row.time_start_utc
				? {
						utc: row.time_start_utc as string,
						offsetMinutes: row.time_start_offset as number,
						timezone: row.time_start_tz as string
					}
				: undefined,
		timeEnd:
			row.time_end_utc
				? {
						utc: row.time_end_utc as string,
						offsetMinutes: row.time_end_offset as number,
						timezone: row.time_end_tz as string
					}
				: undefined,
		lat: row.lat as number | undefined,
		lon: row.lon as number | undefined,
		location: row.location as string | undefined,
		createdAt: row.created_at as string,
		modifiedAt: row.modified_at as string,
		deviceId: row.device_id as string
	};
}

export async function insertEntry(entry: Entry): Promise<void> {
	const db = await initDb();
	db.exec({
		sql: `INSERT INTO entries (
			id, slug, content_hash, field, verb, value, extra_data, unit,
			time_start_utc, time_start_offset, time_start_tz,
			time_end_utc, time_end_offset, time_end_tz,
			lat, lon, location, created_at, modified_at, device_id
		) VALUES (
			$id, $slug, $content_hash, $field, $verb, $value, $extra_data, $unit,
			$time_start_utc, $time_start_offset, $time_start_tz,
			$time_end_utc, $time_end_offset, $time_end_tz,
			$lat, $lon, $location, $created_at, $modified_at, $device_id
		)`,
		bind: {
			$id: entry.id,
			$slug: entry.slug,
			$content_hash: entry.contentHash,
			$field: entry.field,
			$verb: entry.verb,
			$value: entry.value !== undefined ? JSON.stringify(entry.value) : null,
			$extra_data: entry.extraData !== undefined ? JSON.stringify(entry.extraData) : null,
			$unit: entry.unit ?? null,
			$time_start_utc: entry.timeStart?.utc ?? null,
			$time_start_offset: entry.timeStart?.offsetMinutes ?? null,
			$time_start_tz: entry.timeStart?.timezone ?? null,
			$time_end_utc: entry.timeEnd?.utc ?? null,
			$time_end_offset: entry.timeEnd?.offsetMinutes ?? null,
			$time_end_tz: entry.timeEnd?.timezone ?? null,
			$lat: entry.lat ?? null,
			$lon: entry.lon ?? null,
			$location: entry.location ?? null,
			$created_at: entry.createdAt,
			$modified_at: entry.modifiedAt,
			$device_id: entry.deviceId
		}
	});
}

export async function getEntriesForField(field: string, limit = 50): Promise<Entry[]> {
	const db = await initDb();
	const rows: Record<string, unknown>[] = [];
	db.exec({
		sql: `SELECT * FROM entries WHERE field = $field
		      ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC
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
		      WHERE COALESCE(time_start_utc, time_end_utc, created_at) >= $start
		        AND COALESCE(time_start_utc, time_end_utc, created_at) <= $end
		      ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC`,
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
		      ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC
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
		sql: `SELECT DISTINCT field FROM entries ORDER BY field`,
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
		        SELECT field, MAX(COALESCE(time_start_utc, time_end_utc, created_at)) as max_time
		        FROM entries GROUP BY field
		      ) latest ON e.field = latest.field
		        AND COALESCE(e.time_start_utc, e.time_end_utc, e.created_at) = latest.max_time
		      ORDER BY e.field`,
		rowMode: 'object',
		callback: (row: Record<string, unknown>) => rows.push(row)
	});
	return rows.map(rowToEntry);
}
