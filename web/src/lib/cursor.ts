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
//
// A selection is not always an absolute instant (da-rskp). Pointed INTO
// the nowview while its window is on now, it is RELATIVE: `rel` whole
// minutes back from whenever the command finally runs — `-q` for five,
// `-t -7` for seven. The axis it was dropped on is relative too, so the
// white line stays on its gridline as the minutes pass, and `occur coffee`
// typed after it is logged that long before the moment of the Enter, not
// before the moment of the click. On an ABSOLUTE axis — the dayview's —
// the same selection is drawn at now − `rel`, and moves with the minute:
// that is what `pointed` is for, and every picture draws from it.
//
// A selection may also have a LENGTH: `span` whole minutes ENDING at the
// selection, drawn as a white band and carried as a second pill, because
// a duration runs back from the entry's time (da-zwct). `rel: 0` is the
// length that ends NOW — the nowview's own gesture — and makes the length
// pill alone, since the entry's time is already now and needs no `-t` at
// all.
//
// HOW a range spells itself is the view it was MADE in (da-ku7y), and
// nothing else: the two spellings are the same block to the CLI, and
// which one a person reads is what teaches them the flag they did not
// know. A range dragged in the nowview is minutes, so it says `-t` and
// `dur=30m`. One dragged across a day of the strip is two clock times
// read off a wall — usually long, usually remembered as "nine to
// half ten" — so it says `start_time=09:00 end_time=10:30`, which is the
// pair the CLI lifts onto occur_time and dur (da-mjwq). The other view
// DRAWS the range all the same; it does not re-spell it.

import { derived, get, writable } from 'svelte/store';
import { pills, selected, type Pill, type PillSection } from './bar.js';
import { minute } from './clock.js';
import { lanes } from './lanes.js';
import { relArg } from './nowpoint.js';

/**
 * Where the page is pointed.
 *
 * `at` is always the instant, resolved when the selection was made; for a
 * relative one it goes stale by a minute at a time, which is why readers
 * take `pointed` below rather than this. `rel`, `span` and `from` are the
 * three things a selection can carry beyond its instant.
 */
export interface Cursor {
	/** The instant pointed at. */
	at: Date;
	/** Minutes back from NOW, to the second, when the selection is relative. */
	rel?: number;
	/** Whole minutes of range ENDING at the selection — the `dur=` pill. */
	span?: number;
	/**
	 * What made it. The nowview does not re-centre its own window on its
	 * own click; the day strip is how a range comes to be spelled with the
	 * CLI's two keys rather than with `dur=`.
	 */
	from?: 'nowview' | 'daystrip';
}

export const cursor = writable<Cursor | null>(null);

const MINUTE = 60_000;
const DAY = 24 * 60 * 60_000;
const pad = (n: number) => String(n).padStart(2, '0');
const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'];

/** The clock time `at` reads on the page's wall, with its seconds when it has any. */
const clock = (at: Date) =>
	`${pad(at.getHours())}:${pad(at.getMinutes())}${at.getSeconds() ? `:${pad(at.getSeconds())}` : ''}`;

/**
 * The UTC offset `at` must say for its clock time to be read as it, or
 * nothing (da-fir4a). A clock the day reads twice, the hour the clocks go
 * back over, is read at its FIRST occurrence by the CLI's `-t` and by
 * `new Date` alike, so only a later occurrence says its own: `+01:00`.
 */
function offset(at: Date): string {
	const first = new Date(at.getFullYear(), at.getMonth(), at.getDate(), at.getHours(), at.getMinutes());
	const east = -at.getTimezoneOffset();
	if (east === -first.getTimezoneOffset()) return '';
	return `${east < 0 ? '-' : '+'}${pad(Math.floor(Math.abs(east) / 60))}:${pad(Math.abs(east) % 60)}`;
}

/**
 * `-t` as the CLI parses it, in the page's zone: `HH:MM` today (`HH:MM:SS`
 * off a whole minute), the dated `YYYY-MM-DDTHH:MM` another day (`-t "YYYY-MM-DD HH:MM"` it
 * does not read), and dated with its offset when the clock time alone
 * would be read as an earlier instant.
 */
export function timeArg(at: Date, now = new Date()): string {
	const day = `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
	const off = offset(at);
	return !off && at.toDateString() === now.toDateString() ? clock(at) : `${day}T${clock(at)}${off}`;
}

/** The instant `-t <arg>` names, as `timeArg` writes it; `null` if it is not one. */
export function timeArgAt(arg: string, now = new Date()): Date | null {
	// With its offset it is ISO, which `Date` reads as the CLI does.
	if (/^\d{4}-\d\d-\d\dT\d\d:\d\d(?::\d\d)?[+-]\d\d:\d\d$/.test(arg)) return new Date(arg);
	const m = /^(?:(\d{4})-(\d\d)-(\d\d)T)?(\d\d):(\d\d)(?::(\d\d))?$/.exec(arg);
	if (!m) return null;
	const [y, mo, d] = m[1] ? [+m[1], +m[2] - 1, +m[3]] : [now.getFullYear(), now.getMonth(), now.getDate()];
	return new Date(y, mo, d, +m[4], +m[5], +(m[6] ?? 0));
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
 * whole pill, and so the cursor, away. A time that says its offset in the
 * argv says it here too, or the two lanes of a repeated hour read alike.
 */
export function pillSections(at: Date, now = new Date()): PillSection[] {
	const bare = { text: clock(at) };
	const time = { text: bare.text + offset(at) };
	const date = dateLabel(at, now);
	return date === null ? [time] : [{ text: date, rest: { text: `-t ${bare.text}`, sections: [bare] } }, time];
}

/** To the nearest `step` seconds: a whole minute unless the view says finer. */
const toStep = (at: Date, step = 60) => new Date(Math.round(at.getTime() / (step * 1000)) * step * 1000);

/**
 * The instant `rel` minutes back from now, from now floored to the unit
 * `rel` is in: the minute for whole minutes, the second otherwise, so a
 * `-t -20s` is drawn 20 s before now and not up to 79.
 */
const back = (rel: number, now = Date.now()) => {
	const unit = Number.isInteger(rel) ? MINUTE : 1000;
	return new Date(Math.floor(now / unit) * unit - Math.round(rel * MINUTE));
};

/** Whole minutes, never negative, and 0 read as «none» for a span. */
const whole = (n: number | undefined) => (n === undefined ? undefined : Math.max(0, Math.round(n)));

/**
 * Whether this selection is a range that says its two EDGES — the day
 * strip's spelling (da-ku7y). Anything else says an instant and a length.
 */
const edged = (c: Cursor): boolean => c.from === 'daystrip' && !!c.span;

/** Where a range begins: its length before the instant it ends at. */
const startOf = (c: Cursor): Date => new Date(+c.at - (c.span ?? 0) * MINUTE);

/**
 * The instant a selection puts on the command line, or `null` when it has
 * none to put there: `rel: 0` ends its range at now, and now is what an
 * entry's time already is.
 */
export function cursorArg(c: Cursor, now = new Date()): string | null {
	if (c.rel === 0) return null;
	if (c.rel !== undefined) return relArg(c.rel);
	// The strip's range ENDS here, and `end_time=` is the CLI's word for
	// that — the same `-t` value, under the key that says which edge it is.
	return `${edged(c) ? 'end_time=' : '-t '}${timeArg(c.at, now)}`;
}

/** The LENGTH pill's id — the second half of a range, however it is spelled. */
export const SPAN_ID = 'cursor-span';

/** What the length pill says: the range's other edge, or the minutes between them. */
const spanArg = (c: Cursor, now = new Date()): string =>
	edged(c) ? `start_time=${timeArg(startOf(c), now)}` : `dur=${c.span}m`;

/**
 * The pills a selection is: its time, when it has one, and its length,
 * when it has one — the edged range reading `start_time=` first, because
 * that is the order a person says a block in and the order the two words
 * teach best.
 */
function cursorPills(c: Cursor): Pill[] {
	const arg = cursorArg(c);
	const time =
		arg === null
			? []
			: [
					{
						id: 'cursor',
						kind: 'time',
						text: arg,
						// A relative pill is one short word already, and a keyed
						// one says which edge it is in the word itself; only the
						// bare absolute pill wants dividing (da-ui4i).
						...(c.rel === undefined && !edged(c) ? { sections: pillSections(c.at) } : {})
					}
				];
	const span = c.span ? [{ id: SPAN_ID, kind: 'dur', text: spanArg(c) }] : [];
	return edged(c) ? [...span, ...time] : [...time, ...span];
}

/**
 * Point the page at a selection, or clear it with `null`.
 *
 * `rel` minutes back from now makes it RELATIVE, `at` makes it absolute,
 * and `span` gives it a length ending there. `rel` is rounded to the
 * second, and `at` to the `step` seconds the view that made it says
 * (`tapStep`), a whole minute when it says none; `span` is whole minutes.
 * So nothing downstream has to wonder.
 */
export function pointAt(sel: (Partial<Cursor> & { at?: Date; step?: number }) | null): void {
	if (!sel) return void cursor.set(null);
	const rel = sel.rel === undefined ? undefined : Math.max(0, Math.round(sel.rel * 60)) / 60;
	const at = rel === undefined ? sel.at && toStep(sel.at, sel.step) : back(rel);
	if (!at) return void cursor.set(null);
	const span = whole(sel.span) || undefined;
	// A selection AT now with no length is not a selection: now is the
	// yellow line's, and a white one over it would say the same thing twice.
	if (rel === 0 && !span) return void cursor.set(null);
	cursor.set({ at, ...(rel === undefined ? {} : { rel }), ...(span ? { span } : {}), ...(sel.from ? { from: sel.from } : {}) });
}

/** Point the page at `at`, to the minute; `null` clears it. */
export const setCursor = (at: Date | null): void => pointAt(at && { at });

/**
 * WHERE the page is pointed, now: the selection with its instant resolved
 * against the clock.
 *
 * A relative selection's instant is now − `rel`, so it moves on every
 * minute tick — which is exactly what an ABSOLUTE axis has to draw
 * (the dayview's line creeps FORWARD as the minutes pass — «seven
 * minutes ago» is a later instant every minute) and what a RELATIVE
 * one gets for free (nowview's window moves with it, so the line stays
 * on its gridline). An absolute selection is handed through as the same
 * OBJECT, so a reader comparing by identity sees nothing change — but
 * every subscriber is still CALLED on each tick (Svelte's `safe_not_equal`
 * counts every object as changed), so a subscriber that acts rather than
 * draws must key on what it cares about. `follow` in NowviewWidget takes
 * `cursor` itself for exactly that reason (vremya on PR #48).
 */
export const pointed = derived([cursor, minute], ([c]) =>
	!c || c.rel === undefined ? c : { ...c, at: back(c.rel) }
);

cursor.subscribe((c) => {
	// The lanes a press published are the selection's other half, and a
	// selection CLEARED clears them (da-qu16, $lib/lanes.ts): the pill's
	// ×, `now →` and the reconciler below are every door out of one, and
	// all three pass through here. A press that lands at now clears the
	// selection and still re-orders — which is why the pictures publish
	// their lanes AFTER they point.
	if (!c && !get(selected).length) lanes.set(null);
	pills.update((all) => [...all.filter((p) => p.kind !== 'time' && p.id !== SPAN_ID), ...(c ? cursorPills(c) : [])]);
});

// And an ENTRY selected by a tap publishes them too (da-xg1vi: a tap on a
// mark is its entry, and its height still orders the fields), so the last
// of the two selections to go clears them.
selected.subscribe((hids) => {
	if (!hids.length && !get(cursor)) lanes.set(null);
});

// The pills are the selection's other face, and a press on one of them
// changes the selection: the `dur=` pill's × takes the range's length away
// and leaves the instant (and takes the whole selection, when the range
// was the whole of it — a `rel: 0` one has no time pill to fall back to);
// no time pill at all clears the selection; and a time pill that no longer
// says the selection's instant MOVES it — dropping the date section leaves
// `-t 09:00`, which is today at 09:00 and has to be where the page is
// pointed too. Read back off the text rather than tracked beside it, so
// the pills and the selection cannot come to disagree.
//
// THE ORDER MATTERS, and getting it wrong wrote a wrong time into the next
// entry (vremya on PR #48, da-rskp). A command spends BOTH of a range's
// pills in one update — the strip's `start_time=` / `end_time=` pair as
// much as the nowview's. Answering the missing length first — «a span and
// no length pill» → keep the instant, drop the length — put the cursor back, and
// `cursor.subscribe` then drew a fresh time pill over a line the person had
// just watched be spent; the next plain `occur` was logged at it. So the
// TIME pill's absence is asked about FIRST, because it takes the whole
// selection, length and all, and subsumes the other case.
pills.subscribe((all) => {
	const c = get(cursor);
	if (!c) return;
	const want = cursorArg(c);
	const pill = all.find((p) => p.kind === 'time');
	if (want !== null && !pill) return void cursor.set(null);
	if (c.span !== undefined && !all.some((p) => p.id === SPAN_ID))
		return void (want === null ? cursor.set(null) : pointAt({ ...c, span: undefined }));
	if (want === null || !pill || pill.text === want) return;
	// A relative pill has one section and no other text it can become, so
	// a press on it is the press that takes it away — already handled
	// above. Only the absolute pill's date section leaves something behind.
	const at = c.rel === undefined && pill.text.startsWith('-t ') ? timeArgAt(pill.text.slice(3)) : null;
	if (at) pointAt({ ...c, at, rel: undefined, step: 1 });
	else cursor.set(null);
});
