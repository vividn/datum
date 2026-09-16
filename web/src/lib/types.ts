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

/**
 * The intent verbs a field spec may carry as its default.
 *
 * Serialized kebab-case, mirroring `Action` in
 * crates/datum-core/src/action.rs. Every spec carries one (da-c8mh): the
 * first log of a field writes the spec that says what a press of it means,
 * so a spec without a verb does not exist.
 */
export type DefaultAction = 'occur' | 'start' | 'stop' | 'switch' | 'block' | 'add' | 'note';

/**
 * The slice of a field spec the web app reads, as `datum fields --json`
 * reports it — only the keys the widget bar needs are named.
 *
 * The JSON key is `verb` (see `FieldSpec` in
 * crates/datum-core/src/field_spec/spec.rs).
 */
export interface FieldSpec {
	field: string;
	/** What a bare press of the field means. Always present (da-c8mh). */
	verb: DefaultAction;
	/** Allowed states for a switch field. Absent/null = unconstrained. */
	allowed_states?: string[] | null;
	color?: string | null;
	/** Per-state colours of a switch field, by state name. */
	state_colors?: Record<string, string>;
	[key: string]: unknown;
}
