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

/** `HH:MM` in the browser's zone, 24-hour. */
export function clockLabel(date: Date): string {
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}
