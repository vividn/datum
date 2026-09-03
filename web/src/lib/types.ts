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
	/** ISO-8601 duration, already normalized from `dur=30m` / `dur=1:30`. */
	dur?: string;
	timeOffset?: number; // minutes offset from now
	/** Whole days back (`-y`): midnight in the local zone, P1D uncertainty. */
	dayOffset?: number;
	timeAbsolute?: string; // HH:MM
}

/**
 * The intent verbs a field spec may carry as its default.
 *
 * Serialized kebab-case, mirroring `Action` in
 * crates/datum-core/src/action.rs. `occur` is the variant's `#[default]`,
 * so a field with no spec at all behaves as a point field.
 */
export type DefaultAction = 'occur' | 'start' | 'stop' | 'switch' | 'block' | 'add' | 'note';

/**
 * The slice of a stored field spec the web app reads.
 *
 * Field specs live in `field_specs.spec_json` (schema.sql) as one JSON blob
 * so the shape can evolve without migrations; this interface names only the
 * keys the widget bar needs and passes the rest through untouched, so a
 * spec written by the CLI survives a round trip through the browser.
 *
 * The JSON key is `default_verb`, not `default_action` — kept for wire
 * compatibility with stored rows (see `FieldSpec` in
 * crates/datum-core/src/field_spec/spec.rs).
 */
export interface FieldSpec {
	field: string;
	default_verb?: DefaultAction;
	/** Allowed states for a switch field. Absent/null = unconstrained. */
	allowed_states?: string[] | null;
	color?: string | null;
	[key: string]: unknown;
}
