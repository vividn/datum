// The stats widget's logic, kept pure and away from the markup: what a
// selection is, and the exact argv it runs (dashboard D3, da-n7r0).
//
// Nothing here computes a statistic. The widget is a picker and a frame:
// the numbers are `datum stats`'s, as text and as `--json`.

import { shapeFor, type FieldListing } from './fields.js';

/** A named window, or an explicit one (`--since` / `--until`, either may be blank). */
export type StatsRange = 'all' | 'today' | 'week' | 'month' | { since: string; until: string };

/** The widget's settings — what the engine saves with the layout. */
export interface StatsSettings {
	field?: string;
	state?: string;
	range: StatsRange;
}

/** All time, no field: bare `stats` is the model (vividn, 2026-09-10). */
export const DEFAULT_STATS: StatsSettings = { range: 'all' };

/** `stats --json` for a whole field. */
export interface FieldStats {
	field: string;
	total_count: number;
	total_duration_seconds: number;
	tracked_duration_seconds: number;
	states: { state: string; count: number; duration_seconds: number; ratio: number | null }[];
}

/** `stats --json` for one `field:state`. */
export interface StateStats {
	field: string;
	state: string;
	count: number;
	duration_seconds: number;
	ratio: number | null;
}

const PERIOD_FLAG: Record<string, string> = { today: '--today', week: '--week', month: '--month' };

/** The flags a range adds. All time adds none, as bare `stats` does. */
export function rangeArgs(range: StatsRange): string[] {
	if (typeof range !== 'object' || range === null) return PERIOD_FLAG[range] ? [PERIOD_FLAG[range]] : [];
	return [
		...(range.since ? ['--since', range.since] : []),
		...(range.until ? ['--until', range.until] : [])
	];
}

/** The argv a selection runs, or null when no field is picked. */
export function statsArgv({ field, state, range }: StatsSettings): string[] | null {
	if (!field) return null;
	return ['stats', state ? `${field}:${state}` : field, ...rangeArgs(range)];
}

/** The states a field can be narrowed to — the ones the widget bar offers. */
export function statesOf(listing: FieldListing | undefined): string[] {
	const shape = shapeFor(listing?.spec ?? undefined, listing?.states);
	return shape.kind === 'states' ? shape.states : [];
}
