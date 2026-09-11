// The time cursor (da-wnt2): the one instant the page is pointed at.
//
// A click on a drawn day sets it (the `picture` action, $lib/hover.ts);
// every picture draws a dotted line where it falls, the nowview widget
// centres on it, and the bar holds it as ONE `time` pill — the argv the
// CLI reads it as, so a line typed after it is logged at it. The pill is
// the cursor's other face: its × clears the cursor, and a cleared cursor
// takes the pill away. Other pills are left alone.

import { get, writable } from 'svelte/store';
import { pills } from './bar.js';

export const cursor = writable<{ at: Date } | null>(null);

const MINUTE = 60_000;
const pad = (n: number) => String(n).padStart(2, '0');

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

/** Point the page at `at`, to the minute the pill can say; `null` clears it. */
export const setCursor = (at: Date | null): void =>
	cursor.set(at && { at: new Date(Math.round(at.getTime() / MINUTE) * MINUTE) });

cursor.subscribe((c) =>
	pills.update((all) => [
		...all.filter((p) => p.kind !== 'time'),
		...(c ? [{ id: 'cursor', kind: 'time', text: `-t ${timeArg(c.at)}` }] : [])
	])
);

pills.subscribe((all) => {
	if (get(cursor) && !all.some((p) => p.kind === 'time')) cursor.set(null);
});
