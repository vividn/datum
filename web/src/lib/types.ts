export interface DatumTime {
	utc: string; // ISO 8601
	offsetMinutes: number;
	timezone: string;
}

// Stored verb (shape-derived classifier). Maps 1:1 to the
// `entries.verb` column in crates/datum-core/src/schema.sql.
export type StoredVerb = 'point' | 'switch' | 'block' | 'block-switch' | 'point-switch';

// Input verb vocabulary the parser accepts. The stored verb is derived
// from the (state, dur, following_state) shape at insert time; the input
// verb is decorative and lives in `rawInput`.
export type InputVerb = 'occur' | 'switch' | 'start' | 'stop' | 'add' | 'note';

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
	unit?: string;
	occurTime: DatumTime;
	occurTimePrecision?: string;
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
	unit?: string;
	data?: Record<string, unknown>;
	timeOffset?: number; // minutes offset from now
	timeAbsolute?: string; // HH:MM
}
