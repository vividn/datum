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
// So no renderer anywhere in the app calls a locale-sensitive formatter
// for an instant — not a time of day, and not a date either: the /now
// card past 24 hours asked `toLocaleDateString()` and printed `9/4/2026`
// long after the time half was fixed, because the fix named only the call
// it had found (da-6eou). Both conventions live here now. The zone is
// still the browser's, which is what `getHours` reads; only the
// convention is pinned. The one clock that is not the browser's is a day's
// own, the one it was lived by, which `clockLabel` takes as an offset.

import { readable } from 'svelte/store';
import type { DatumTime } from './types.js';

/**
 * The time, live: what every prompt's clock shows, and "what today is"
 * for a command's stamp.
 *
 * It ticks per second, and only while someone could be looking at it: a
 * backgrounded tab that keeps a timer alive is a tab that keeps a phone
 * awake for a label nobody can read. Coming back visible re-reads the
 * time first, so the clock is never a second late for the length of a
 * tick.
 */
export const now = readable(new Date(), (set) => {
	let timer: ReturnType<typeof setInterval> | undefined;
	const stop = () => clearInterval(timer);
	const start = () => {
		stop();
		set(new Date());
		timer = setInterval(() => set(new Date()), 1000);
	};
	const visibility = () => (document.hidden ? stop() : start());
	start();
	document.addEventListener('visibilitychange', visibility);
	return () => {
		stop();
		document.removeEventListener('visibilitychange', visibility);
	};
});

/**
 * The local midnight `date` falls in — the start of the day it is in,
 * in the browser's own zone, the way `local_day_bounds_ms` takes a
 * day's start in the host's.
 */
export function startOfDay(date: Date): Date {
	const day = new Date(date);
	day.setHours(0, 0, 0, 0);
	return day;
}

/** `HH:MM`, 24-hour: in the browser's zone, or on the clock `offset` seconds east of UTC. */
export function clockLabel(date: Date, offset?: number): string {
	if (offset !== undefined) return new Date(+date + offset * 1000).toISOString().slice(11, 16);
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

/** `HH:MM:SS` in the browser's zone, 24-hour — `clockLabel` to the second. */
export function clockSecondsLabel(date: Date): string {
	return `${clockLabel(date)}:${String(date.getSeconds()).padStart(2, '0')}`;
}

/** The `DatumTime` an instant is when read in the browser's own zone. */
export function localTime(date: Date): DatumTime {
	return { utc: date.toISOString(), offsetMinutes: -date.getTimezoneOffset() };
}

/**
 * When something happened, for a reader watching it happen: the clock
 * alone while it is still today, the date in front of it once it isn't.
 *
 * The second half is the tail's rule for a row older than the day, and it
 * is `dateLabel` that applies it — the one date renderer, rather than a
 * second one grown next to it (da-6eou).
 */
export function stampLabel(date: Date, now: Date): string {
	const clock = clockSecondsLabel(date);
	if (startOfDay(date).getTime() === startOfDay(now).getTime()) return clock;
	return `${dateLabel(localTime(date))} ${clock}`;
}

/**
 * Ten seconds, counted — one tick on each wall-clock ten-second boundary.
 *
 * What nowview hangs off (da-fprk0), and the one timer under `minute`.
 * vividn wanted to «watch time pass smoothly in the nowview without needing
 * to enter in more data», and its default 30 minutes across a pane is a
 * pixel every couple of seconds, so a minute's step is a jump. A run of
 * it is cheap — 52 ms native on 36k entries — so it is re-run rather than
 * moved by hand, and the picture stays the CLI's.
 *
 * ONE timer for the app, not one per pane: a `readable`'s body runs on
 * the first subscriber and stops with the last. The value is a COUNT
 * rather than a Date — two subscribers wanting different things of the
 * same instant would be asking the browser what time it is, which is the
 * CLI's question, not the web's (see the note at the head of this file).
 * The web only decides when to ask again.
 *
 * Aligned to the wall clock, not 10 s from whenever the page loaded, and
 * re-armed to the next boundary after every tick, so a browser that fires
 * it a millisecond early neither counts a period twice (the period it
 * lands in is compared, not assumed) nor drifts.
 *
 * Paused while the document is hidden: a backgrounded tab that wakes the
 * CPU to redraw a picture nobody can see is a tab that costs a phone its
 * battery. Coming back visible ticks once if a boundary passed while away
 * — one run, however long the tab was gone.
 */
export const tenSeconds = readable(0, (set) => {
	let ticks = 0;
	let at = Math.floor(Date.now() / 10_000);
	let timer: ReturnType<typeof setTimeout> | undefined;
	const stop = () => clearTimeout(timer);
	const arm = () => {
		// Cleared first: two `visible` events in a row would otherwise leave
		// two timers running, and the tick would double from then on.
		stop();
		timer = setTimeout(tick, 10_000 - (Date.now() % 10_000));
	};
	function tick(): void {
		const now = Math.floor(Date.now() / 10_000);
		if (now !== at) {
			at = now;
			set(++ticks);
		}
		arm();
	}
	const visibility = () => (document.hidden ? stop() : tick());
	arm();
	document.addEventListener('visibilitychange', visibility);
	return () => {
		stop();
		document.removeEventListener('visibilitychange', visibility);
	};
});

/**
 * The minute, counted — one tick on each wall-clock minute boundary.
 *
 * What a view of NOW hangs off (da-r61i). `datum now` and `datum dayview`
 * read the clock inside the wasm process and draw it: elapsed durations,
 * and the now-line. Run once by their pane, they show the minute they
 * were run in and stand still until something else — a write, a change
 * next door, a resize — happens to re-run them. So the panes whose
 * picture is of now subscribe to this and re-run on it.
 *
 * Counted off `tenSeconds`, not a timer of its own, so it pauses with it
 * and a minute boundary is ONE timer callback that moves both: what a
 * minute re-draws lands in the same flush as nowview's beat and costs it
 * one run, where two timers firing a moment apart cost it two (da-fprk0,
 * tests/tick.test.mjs's write-and-tick bound, 12 of 16 under load).
 */
export const minute = readable(0, (set) => {
	let ticks = 0;
	let at = Math.floor(Date.now() / 60_000);
	return tenSeconds.subscribe(() => {
		const now = Math.floor(Date.now() / 60_000);
		if (now !== at) {
			at = now;
			set(++ticks);
		}
	});
});
