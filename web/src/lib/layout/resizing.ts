// Resizing without a render loop (da-jgrn).
//
// Every drawn widget re-runs a CLI command per size it is shown at, and a
// border dragged at pointer rate would be sixty of them a second per pane:
// that was the jitter vividn saw. So a size change reaches a widget through
// `settle`, which holds it while a border is being dragged and lets it
// through once on the drop — the leading edge — and otherwise (a window
// resize, a run of keyboard nudges) waits for the last of a burst and
// applies once, the trailing edge. Meanwhile the widget keeps the picture
// it has, scaled to its box (CliOutput.svelte reads `resizing` for that).

import { get, writable } from 'svelte/store';

/** True from the moment a border is grabbed until it is dropped. */
export const resizing = writable(false);

/** How long a burst of resizes has to be over before one redraw. */
export const SETTLE_MS = 150;

/**
 * Wrap `apply` so a burst of boxes becomes one call with the last of them.
 *
 * Returns the sink for measured boxes and a teardown. Boxes arriving
 * during a drag are held; the drop flushes the newest at once. Outside a
 * drag the newest is applied `SETTLE_MS` after the last one arrives.
 */
export function settle<T>(apply: (box: T) => void): { push: (box: T) => void; stop: () => void } {
	let pending: T | undefined;
	let timer: ReturnType<typeof setTimeout> | undefined;
	/** Nothing drawn yet: the first box is applied at once, not 150ms late. */
	let first = true;

	const flush = () => {
		if (timer !== undefined) clearTimeout(timer);
		timer = undefined;
		if (pending === undefined) return;
		const box = pending;
		pending = undefined;
		apply(box);
	};

	const unsubscribe = resizing.subscribe((dragging) => {
		if (!dragging) flush();
	});

	return {
		push(box) {
			pending = box;
			if (get(resizing)) return;
			if (first) {
				first = false;
				flush();
				return;
			}
			if (timer !== undefined) clearTimeout(timer);
			timer = setTimeout(flush, SETTLE_MS);
		},
		stop() {
			unsubscribe();
			if (timer !== undefined) clearTimeout(timer);
		}
	};
}
