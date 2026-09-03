export interface DatumTime {
	utc: string; // ISO 8601
	offsetMinutes: number;
	// IANA name, or undefined when the zone is unknown (a legacy import can
	// carry an offset with no zone — da-vfbq). Undefined means *unknown*,
	// not UTC: naming a zone the row doesn't carry contradicts the offset
	// stored beside it. Mirrors `DatumTime.timezone: Option<String>` in
	// crates/datum-parse/src/time/datum_time.rs.
	timezone?: string;
}

// Stored verb (shape-derived classifier). Maps 1:1 to the
// `entries.verb` column in crates/datum-core/src/schema.sql.
export type StoredVerb = 'point' | 'switch' | 'block' | 'block-switch' | 'point-switch';

// Input verb vocabulary the parser accepts. The stored verb is derived
// from the (state, dur, following_state) shape at insert time; the input
// verb is decorative and lives in `rawInput`.
export type InputVerb = 'occur' | 'switch' | 'start' | 'stop' | 'add' | 'note';

/**
 * A number carrying a unit, as datum annotates one: `{"v": 75.2, "u": "kg"}`.
 * Built by `coerce.rs` on the Rust side (crates/datum-parse/src/input/
 * data_args/coerce.rs) and stored inside `state` / `data` — there is no
 * `unit` column in schema.sql, and a bare number is equally valid.
 */
export interface Annotated {
	v: number;
	u: string;
}

export function isAnnotated(value: unknown): value is Annotated {
	return (
		typeof value === 'object' &&
		value !== null &&
		typeof (value as Annotated).v === 'number' &&
		typeof (value as Annotated).u === 'string'
	);
}

export interface Entry {
	hid: string;
	slug: string;
	contentHash: string;
	field: string;
	state?: unknown;
	dur?: string; // ISO-8601 duration string (e.g. "PT30M")
	followingState?: unknown;
	verb: StoredVerb;
	data?: unknown;
	occurTime: DatumTime;
	occurTimeUncertainty?: string;
	lat?: number;
	lon?: number;
	location?: string;
	createTime: string;
	modifyTime: string;
	deviceId: string;
	rawInput: string;
}

export interface ParsedInput {
	verb: InputVerb;
	field: string;
	state?: unknown;
	/** Unit for a numeric `state`; folded into an [`Annotated`] at insert. */
	unit?: string;
	data?: Record<string, unknown>;
	timeOffset?: number; // minutes offset from now
	timeAbsolute?: string; // HH:MM
}
