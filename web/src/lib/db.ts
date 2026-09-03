// The main thread's handle on the database.
//
// SQLite itself lives in $lib/db.worker.ts, because OPFS is reachable only
// from a dedicated worker (da-5iqw). This file is the client side of that:
// one statement out, rows back. Its exported functions were already async,
// so nothing calling them had to change when the database moved.

import type { DbCommand, DbResponse, Row, StorageKind } from './dbProtocol.js';
import type { Entry, FieldSpec, StoredVerb } from './types.js';

export type { StorageKind };

let worker: Worker | null = null;
let nextId = 0;
const pending = new Map<number, (response: DbResponse) => void>();
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

/**
 * Start the worker, lazily.
 *
 * Lazily because this module is imported during prerendering, where there
 * is no `Worker` — every caller reaches the database from `onMount` or a
 * later event, so the worker is only ever constructed in the browser.
 */
function ensureWorker(): Worker {
	if (worker) return worker;
	const started = new Worker(new URL('./db.worker.ts', import.meta.url), { type: 'module' });
	started.onmessage = (event: MessageEvent<DbResponse>) => {
		const settle = pending.get(event.data.id);
		pending.delete(event.data.id);
		settle?.(event.data);
	};
	// A worker that fails to load answers nothing, so its requests would
	// hang forever. Fail them loudly and drop the worker, so the next call
	// gets a fresh one instead of talking to a corpse.
	started.onerror = (event) => {
		for (const [id, settle] of [...pending]) {
			pending.delete(id);
			settle({ id, ok: false, error: `database worker failed: ${event.message}` });
		}
		if (worker === started) worker = null;
	};
	worker = started;
	return started;
}

/** Send one request to the worker and resolve with the rows it returns. */
function send(command: DbCommand): Promise<Row[]> {
	const id = ++nextId;
	const started = ensureWorker();
	return new Promise<Row[]>((resolve, reject) => {
		pending.set(id, (response) => {
			if (!response.ok) return reject(new Error(response.error));
			storage = response.storage;
			resolve(response.rows);
		});
		started.postMessage({ ...command, id });
	});
}

/** Open the database (idempotent) and record where it landed. */
export async function initDb(): Promise<void> {
	await send({ op: 'open' });
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
	await send({
		op: 'exec',
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
	const rows = await send({
		op: 'exec',
		sql: `SELECT * FROM entries
		      WHERE field = $field AND deleted_at IS NULL
		      ORDER BY occur_time DESC
		      LIMIT $limit`,
		bind: { $field: field, $limit: limit }
	});
	return rows.map(rowToEntry);
}

export async function getEntriesInRange(start: Date, end: Date): Promise<Entry[]> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT * FROM entries
		      WHERE occur_time >= $start AND occur_time <= $end
		        AND deleted_at IS NULL
		      ORDER BY occur_time DESC`,
		bind: { $start: start.toISOString(), $end: end.toISOString() }
	});
	return rows.map(rowToEntry);
}

export async function getRecentEntries(limit = 20): Promise<Entry[]> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT * FROM entries
		      WHERE deleted_at IS NULL
		      ORDER BY occur_time DESC
		      LIMIT $limit`,
		bind: { $limit: limit }
	});
	return rows.map(rowToEntry);
}

export async function getAllFields(): Promise<string[]> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT DISTINCT field FROM entries WHERE deleted_at IS NULL ORDER BY field`
	});
	return rows.map((row) => row.field as string);
}

export async function getLatestPerField(): Promise<Entry[]> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT e.* FROM entries e
		      INNER JOIN (
		        SELECT field, MAX(occur_time) AS max_time
		        FROM entries
		        WHERE deleted_at IS NULL
		        GROUP BY field
		      ) latest ON e.field = latest.field AND e.occur_time = latest.max_time
		      WHERE e.deleted_at IS NULL
		      ORDER BY e.field`
	});
	return rows.map(rowToEntry);
}

/**
 * The entry that decides each field's state at `at` — one row per field,
 * the newest at-or-before that instant.
 *
 * The browser's copy of `Db::batch_state_at`'s query, down to its
 * ordering: `occur_time DESC, slug DESC`, live rows only
 * (`STATE_ORDER_NEWEST_FIRST` / `STATE_PARTICIPATION_SQL` in
 * crates/datum-core/src/db/types.rs). The trailing-edge rule that turns
 * the winning row into a state is Rust's `db::now::effective_state`,
 * reached through `crates/datum-layout-wasm` (da-t7ud) — held apart from
 * this query the same way Rust holds it apart.
 */
export async function getStateEntriesAt(at: Date): Promise<Entry[]> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT * FROM (
		        SELECT *, ROW_NUMBER() OVER (
		          PARTITION BY field ORDER BY occur_time DESC, slug DESC
		        ) AS rn
		        FROM entries
		        WHERE occur_time IS NOT NULL AND deleted_at IS NULL
		          AND occur_time <= $at
		      ) WHERE rn = 1`,
		bind: { $at: at.toISOString() }
	});
	return rows.map(rowToEntry);
}

/**
 * Every stored field spec, keyed by field.
 *
 * The spec is the field's declared shape — what the widget bar reads to
 * decide whether a press means `occur`, `start` or `switch` (da-nz29). A
 * field with no row here has no declared shape; the caller falls back to
 * what the log shows rather than guessing from the field's name.
 */
export async function getFieldSpecs(): Promise<Map<string, FieldSpec>> {
	const rows = await send({ op: 'exec', sql: `SELECT field, spec_json FROM field_specs` });
	const specs = new Map<string, FieldSpec>();
	for (const row of rows) {
		try {
			specs.set(row.field as string, JSON.parse(row.spec_json as string) as FieldSpec);
		} catch {
			// A spec row we can't parse is a spec we don't have. Skipping it
			// costs one widget its declared shape; throwing would cost the
			// whole bar.
		}
	}
	return specs;
}

/**
 * Write a field spec, replacing any existing row for the field.
 *
 * `tags` and `render_json` are mirrors of keys inside `spec_json` (see
 * schema.sql) — kept in step here so a SQL-level filter reads the same
 * truth as the blob.
 */
export async function saveFieldSpec(spec: FieldSpec): Promise<void> {
	await send({
		op: 'exec',
		sql: `INSERT OR REPLACE INTO field_specs (field, spec_json, kit_id, tags, render_json)
		      VALUES ($field, $spec_json, NULL, $tags, $render_json)`,
		bind: {
			$field: spec.field,
			$spec_json: JSON.stringify(spec),
			$tags: JSON.stringify(spec.tags ?? []),
			$render_json: spec.render !== undefined ? JSON.stringify(spec.render) : null
		}
	});
}

/**
 * The distinct states each field has actually been logged with.
 *
 * Evidence, not declaration: used only where a field has no spec, so the
 * bar can tell a two-state mood field from a point field without inventing
 * meaning from the field's name.
 */
export async function getObservedStates(): Promise<Map<string, unknown[]>> {
	const rows = await send({
		op: 'exec',
		sql: `SELECT DISTINCT field, state FROM entries
		      WHERE deleted_at IS NULL AND state IS NOT NULL
		      ORDER BY field`
	});
	const observed = new Map<string, unknown[]>();
	for (const row of rows) {
		const field = row.field as string;
		let value: unknown;
		try {
			value = JSON.parse(row.state as string);
		} catch {
			continue;
		}
		const seen = observed.get(field);
		if (seen) seen.push(value);
		else observed.set(field, [value]);
	}
	return observed;
}
