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
// selection, drawn as a white band and carried as a second `dur=` pill,
// because a duration runs back from the entry's time (da-zwct). `rel: 0`
// is the length that ends NOW — the nowview's own gesture — and makes the
// `dur=` pill alone, since the entry's time is already now and needs no
// `-t` at all.

import { derived, get, writable } from 'svelte/store';
import { pills, type Pill, type PillSection } from './bar.js';
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
	/** Whole minutes back from NOW, when the selection is relative. */
	rel?: number;
	/** Whole minutes of range ENDING at the selection — the `dur=` pill. */
	span?: number;
	/** What set it: a nowview does not re-centre its own window on its own click. */
	from?: 'nowview';
}

export const cursor = writable<Cursor | null>(null);

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

/** To the minute a pill can say — every selection is whole minutes. */
const toMinute = (at: Date) => new Date(Math.round(at.getTime() / MINUTE) * MINUTE);

/** The instant `rel` whole minutes back from now. */
const back = (rel: number, now = Date.now()) => new Date(Math.floor(now / MINUTE) * MINUTE - rel * MINUTE);

/** Whole minutes, never negative, and 0 read as «none» for a span. */
const whole = (n: number | undefined) => (n === undefined ? undefined : Math.max(0, Math.round(n)));

/**
 * The `-t` a selection puts on the command line, or `null` when it has
 * none to put there: `rel: 0` ends its range at now, and now is what an
 * entry's time already is.
 */
export function cursorArg(c: Cursor, now = new Date()): string | null {
	if (c.rel === 0) return null;
	return c.rel === undefined ? `-t ${timeArg(c.at, now)}` : relArg(c.rel);
}

/** The `dur=` pill's id — the range's half of a selection. */
export const DUR_ID = 'cursor-dur';

/** The pills a selection is: its time, when it has one, and its length, when it has one. */
function cursorPills(c: Cursor): Pill[] {
	const arg = cursorArg(c);
	return [
		...(arg === null
			? []
			: [
					{
						id: 'cursor',
						kind: 'time',
						text: arg,
						// A relative pill is one short word already; only the
						// absolute one is long enough to want dividing (da-ui4i).
						...(c.rel === undefined ? { sections: pillSections(c.at) } : {})
					}
				]),
		...(c.span ? [{ id: DUR_ID, kind: 'dur', text: `dur=${c.span}m` }] : [])
	];
}

/**
 * Point the page at a selection, or clear it with `null`.
 *
 * `rel` minutes back from now makes it RELATIVE, `at` makes it absolute,
 * and `span` gives it a length ending there. Everything is rounded to
 * whole minutes here, so nothing downstream has to wonder.
 */
export function pointAt(sel: (Partial<Cursor> & { at?: Date }) | null): void {
	if (!sel) return void cursor.set(null);
	const rel = whole(sel.rel);
	const at = rel === undefined ? sel.at && toMinute(sel.at) : back(rel);
	if (!at) return void cursor.set(null);
	const span = whole(sel.span) || undefined;
	// A selection AT now with no length is not a selection: now is the
	// yellow line's, and a white one over it would say the same thing twice.
	if (rel === 0 && !span) return void cursor.set(null);
	cursor.set({ at, ...(rel === undefined ? {} : { rel }), ...(span ? { span } : {}), ...(sel.from ? { from: sel.from } : {}) });
}

/** Point the page at `at`, to the minute the pill can say; `null` clears it. */
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
	if (!c) lanes.set(null);
	pills.update((all) => [...all.filter((p) => p.kind !== 'time' && p.id !== DUR_ID), ...(c ? cursorPills(c) : [])]);
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
// pills in one update. Answering the missing `dur=` first — «a span and no
// dur pill» → keep the instant, drop the length — put the cursor back, and
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
	if (c.span !== undefined && !all.some((p) => p.id === DUR_ID))
		return void (want === null ? cursor.set(null) : pointAt({ ...c, span: undefined }));
	if (want === null || !pill || pill.text === want) return;
	// A relative pill has one section and no other text it can become, so
	// a press on it is the press that takes it away — already handled
	// above. Only the absolute pill's date section leaves something behind.
	const at = c.rel === undefined && pill.text.startsWith('-t ') ? timeArgAt(pill.text.slice(3)) : null;
	if (at) pointAt({ ...c, at, rel: undefined });
	else cursor.set(null);
});
