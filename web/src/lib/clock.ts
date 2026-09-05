// The one clock convention.
//
// datum is 24-hour everywhere — the CLI's `format_timestamp_ref`
// (crates/datum-commands/src/render/action.rs) builds `HH:MM:SS` out of
// `local.hour()`, and the dayview/nowview axis labels do the same. The
// browser has an easy way to get this wrong: `toLocaleTimeString([])`
// asks the *reader's* locale, so on en-US the tail printed "04:07 PM"
// beside a nowview axis reading "16:07" — the same instant in two
// conventions, in one screenshot (da-6o81).
//
// So no renderer here calls a locale-sensitive formatter for a time of
// day. The zone is still the browser's, which is what `getHours` reads;
// only the convention is pinned.

import type { DatumTime } from './types.js';

/** `HH:MM` in the browser's zone, 24-hour. */
export function clockLabel(date: Date): string {
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/**
 * `YYYY-MM-DD` in the entry's own zone.
 *
 * The date is read through the row's stored offset rather than the
 * browser's, the way the CLI's `format_timestamp_ref` renders in
 * `t.local_time()`: a date-only entry is anchored at midnight in the zone
 * it was logged in, so a reader in a zone behind that would otherwise be
 * shown the day before the one the entry names.
 */
export function dateLabel(time: DatumTime): string {
	const local = new Date(new Date(time.utc).getTime() + time.offsetMinutes * 60_000);
	return local.toISOString().slice(0, 10);
}
