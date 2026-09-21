// The stats widget's logic, kept pure and away from the markup: what its
// settings are, and the exact argv it runs (da-n7r0, da-ptuwr).
//
// Nothing here computes a statistic. The widget is a frame: the numbers
// are `datum stats`'s, as text and as `--json`.
//
// There is no field in the settings any more (da-ptuwr, vividn: "Remove the
// field selector in there, and just show general stats across all fields
// until a field filter is applied via another widget"). A bare `stats` is
// the whole database — every field, ranked by the time it was tracked —
// and the FIELD PILL another widget mints ($lib/bar.ts) is the only thing
// that narrows it. So the argv takes the field as an argument rather than
// reading it out of the settings: what the widget shows is the reader's
// filter, not a selection of its own, and there is nothing left to save.

/** A named window, or an explicit one (`--since` / `--until`, either may be blank). */
export type StatsRange = 'all' | 'today' | 'week' | 'month' | { since: string; until: string };

/** The widget's settings: the window, and nothing else (da-ptuwr). */
export interface StatsSettings {
	range: StatsRange;
}

/** All time, every field: bare `stats` is the model (vividn, 2026-09-10). */
export const DEFAULT_STATS: StatsSettings = { range: 'all' };

/** `stats --json` for a whole field — what a field pill narrows the widget to. */
export interface FieldStats {
	field: string;
	total_count: number;
	total_duration_seconds: number;
	tracked_duration_seconds: number;
	states: { state: string; count: number; duration_seconds: number; ratio: number | null }[];
}

/** One field's row of a bare `stats --json`. */
export interface FieldRow {
	field: string;
	/** The field's resolved `#rrggbb`, the colour it wears everywhere else. */
	color: string;
	count: number;
	duration_seconds: number;
	/** Its share of `tracked_duration_seconds`, or null when nothing was tracked. */
	ratio: number | null;
}

/** Bare `stats --json`: every field, ranked. */
export interface AllFieldsStats {
	fields: FieldRow[];
	total_count: number;
	tracked_duration_seconds: number;
}

/** Which of the two a `stats --json` is: the all-fields table has `fields`. */
export const isAllFields = (s: FieldStats | AllFieldsStats | null): s is AllFieldsStats =>
	!!s && Array.isArray((s as AllFieldsStats).fields);

const PERIOD_FLAG: Record<string, string> = { today: '--today', week: '--week', month: '--month' };

/** The flags a range adds. All time adds none, as bare `stats` does. */
export function rangeArgs(range: StatsRange): string[] {
	if (typeof range !== 'object' || range === null) return PERIOD_FLAG[range] ? [PERIOD_FLAG[range]] : [];
	return [
		...(range.since ? ['--since', range.since] : []),
		...(range.until ? ['--until', range.until] : [])
	];
}

/**
 * The argv the widget runs: `stats` over every field, or over the one a
 * field pill has narrowed the page to.
 */
export function statsArgv({ range }: StatsSettings, field?: string | null): string[] {
	return ['stats', ...(field ? [field] : []), ...rangeArgs(range)];
}
