// The time cursor (da-wnt2): the one instant the page is pointed at.
//
// A click on a drawn day sets it (the `picture` action, $lib/hover.ts);
// every picture draws a dotted line where it falls, the nowview widget
// centres on it, and the bar holds it as ONE `time` pill — the argv the
// CLI reads it as, so a line typed after it is logged at it. The pill is
// the cursor's other face: its × clears the cursor, and a cleared cursor
// takes the pill away. Other pills are left alone.
//
// That one pill is DRAWN in two sections (da-ui4i): a date — `-y`, `-yy`,
// `sep14`, `2025-09-14` — and a time, divided by a hairline, each
// pressable on its own. Only the drawing is short: the pill's text, and so
// the argv, stays the `-t` the CLI reads, because `sep14` is a date the
// CLI searches for in the PAST by default (`apr29` in
// datum-parse/src/time/date.rs) while the cursor may be pointed later in
// this year, and a pill must mean the instant it was set to.

import { get, writable } from 'svelte/store';
import { pills, type PillSection } from './bar.js';

export const cursor = writable<{ at: Date } | null>(null);

const MINUTE = 60_000;
const DAY = 24 * 60 * 60_000;
const pad = (n: number) => String(n).padStart(2, '0');
const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'];

/**
 * `-t` as the CLI parses it, in the page's zone: `HH:MM` today, the
 * dated `YYYY-MM-DDTHH:MM` another day (`-t "YYYY-MM-DD HH:MM"` it
 * does not read).
 */
export function timeArg(at: Date, now = new Date()): string {
	const hm = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
	const day = `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
	return at.toDateString() === now.toDateString() ? hm : `${day}T${hm}`;
}

/** The instant `-t <arg>` names, as `timeArg` writes it; `null` if it is not one. */
function timeArgAt(arg: string, now = new Date()): Date | null {
	const m = /^(?:(\d{4})-(\d\d)-(\d\d)T)?(\d\d):(\d\d)$/.exec(arg);
	if (!m) return null;
	const [y, mo, d] = m[1] ? [+m[1], +m[2] - 1, +m[3]] : [now.getFullYear(), now.getMonth(), now.getDate()];
	return new Date(y, mo, d, +m[4], +m[5]);
}

/** Midnight of the day `at` falls in, for counting calendar days. */
const midnight = (at: Date) => new Date(at.getFullYear(), at.getMonth(), at.getDate()).getTime();

/**
 * How the cursor's DATE is drawn (da-ui4i): nothing at all today, the `-y`
 * and `-yy` the CLI itself takes for the two days before it, the short
 * `sep14` for any other date this year, and the full `2025-09-14` once the
 * year is not this one — «sufficiently far back» said as one rule.
 *
 * vividn, 2026-09-16: "for other dates, use a short from like sep14 to keep
 * it small unless it is sufficiently far back in time."
 */
export function dateLabel(at: Date, now = new Date()): string | null {
	const days = Math.round((midnight(at) - midnight(now)) / DAY);
	if (days === 0) return null;
	if (days === -1) return '-y';
	if (days === -2) return '-yy';
	if (at.getFullYear() !== now.getFullYear())
		return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
	return `${MONTHS[at.getMonth()]}${at.getDate()}`;
}

/**
 * The pill's two sections: the date, when there is one to say, and the
 * time. Pressing the date leaves the time behind — today's date implied,
 * which is what a bare `-t HH:MM` means — and pressing the time takes the
 * whole pill, and so the cursor, away.
 */
export function pillSections(at: Date, now = new Date()): PillSection[] {
	const time = { text: `${pad(at.getHours())}:${pad(at.getMinutes())}` };
	const date = dateLabel(at, now);
	return date === null ? [time] : [{ text: date, rest: { text: `-t ${time.text}`, sections: [time] } }, time];
}

/** Point the page at `at`, to the minute the pill can say; `null` clears it. */
export const setCursor = (at: Date | null): void =>
	cursor.set(at && { at: new Date(Math.round(at.getTime() / MINUTE) * MINUTE) });

cursor.subscribe((c) =>
	pills.update((all) => [
		...all.filter((p) => p.kind !== 'time'),
		...(c
			? [
					{
						id: 'cursor',
						kind: 'time',
						text: `-t ${timeArg(c.at)}`,
						sections: pillSections(c.at)
					}
				]
			: [])
	])
);

// The pill is the cursor's other face, and a press on one of its sections
// changes the pill: no time pill at all clears the cursor, and a pill that
// no longer says the cursor's instant MOVES it — dropping the date section
// leaves `-t 09:00`, which is today at 09:00 and has to be where the page
// is pointed too. Read back off the text rather than tracked beside it, so
// the pill and the cursor cannot come to disagree.
pills.subscribe((all) => {
	const c = get(cursor);
	if (!c) return;
	const pill = all.find((p) => p.kind === 'time');
	if (!pill) return cursor.set(null);
	if (pill.text === `-t ${timeArg(c.at)}`) return;
	const at = timeArgAt(pill.text.slice(3));
	if (at) setCursor(at);
});
