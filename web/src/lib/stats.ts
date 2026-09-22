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

/**
 * The same four windows in words, for the line that says what the numbers
 * cover. Keyed by the named ranges themselves, so a fifth one cannot be
 * added without a word for it.
 */
const PERIOD_LABEL: Record<Exclude<StatsRange, object>, string> = {
	all: 'all time',
	today: 'today',
	week: 'this week',
	month: 'this month'
};

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

/**
 * A span of seconds in the CLI's own spelling — `45s`, `2m5s`, `4m`, `1h`,
 * `1h2m`, `26h`.
 *
 * A mirror of `format_human_duration(secs, DurationPrecision::Seconds)`
 * (`crates/datum-parse/src/time/dur.rs`), which is what every duration the
 * CLI prints into this widget's body is written with: two units at most,
 * never three, and seconds fall off above the hour. The tiles say the same
 * numbers as the text under them, so they say them the same way.
 */
export function formatDur(secs: number): string {
	const n = Math.abs(Math.trunc(secs));
	const [h, m, s] = [Math.floor(n / 3600), Math.floor((n % 3600) / 60), n % 60];
	if (h) return m ? `${h}h${m}m` : `${h}h`;
	if (m) return s ? `${m}m${s}s` : `${m}m`;
	return `${s}s`;
}

/**
 * The tiles over the all-fields table: what the database itself amounts to
 * in the window (da-vpuxg).
 *
 * vividn, 2026-09-22: "when nothing is selected, maybe can show total tracked
 * fields, time spent tracking […] let's just get some nice top level
 * statistics there when nothing is [selected]".
 *
 * Same three tiles as a field's own, one level out: how many entries, how
 * much time, and how many parts the subject has — states within a field,
 * fields within a database. `tracked` and not `time`, because that is the
 * number: fields run in parallel, so it is a sum of their tracked time and
 * not a span of clock, and it is what the share bar's ratios divide.
 *
 * Nothing here computes a statistic: all three are `stats --json`'s own,
 * the one command the widget already runs.
 */
export function summaryTiles(all: AllFieldsStats): [string, string][] {
	return [
		['entries', String(all.total_count)],
		['tracked', formatDur(all.tracked_duration_seconds)],
		['fields', String(all.fields.length)]
	];
}

/**
 * The tiles over one field's own table — the same three, one level in:
 * its entries, the time it has taken, and the states it has been in.
 *
 * `time` here and `tracked` there because they are different numbers: a
 * field's own total is measured over every state it holds, the all-fields
 * total only over the states that count.
 */
export function fieldTiles(s: FieldStats): [string, string][] {
	return [
		['entries', String(s.total_count)],
		['time', formatDur(s.total_duration_seconds)],
		['states', String(s.states.length)]
	];
}

/**
 * The window in words — what the numbers cover, said on screen beside the
 * database they cover it in.
 *
 * A figure whose scope the reader has to guess at is worse than no figure,
 * and `fields` is the figure that invites the guess: the table leaves out a
 * field with nothing in the window, so "3 fields" is three fields THIS
 * WEEK, not three fields ever.
 */
export function windowLabel(range: StatsRange): string {
	if (typeof range !== 'object' || range === null) return PERIOD_LABEL[range];
	if (range.since && range.until) return `${range.since} → ${range.until}`;
	if (range.since) return `since ${range.since}`;
	if (range.until) return `until ${range.until}`;
	return 'all time';
}
