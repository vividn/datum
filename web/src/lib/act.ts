// What a tap on a button DOES, and the one place its outline is decided
// (da-y0ocz). Rule 1 of the design language: the outline says what a tap
// does — DOUBLE, it acts now; SINGLE, it arms, opens or fills; NONE, it is
// display. vividn: «is there anyway to help enforce the double line semantic
// … codewise? like hooking up a button to do something automatically gives
// it a double line».
//
// So a button says its KIND — `use:act={kind}` — and never its border: the
// action writes `data-act`, and the double outline is drawn on
// `[data-act='run']` and nowhere else. How it is drawn stays each bar's
// own (`.command:global([data-act='run'])`, in its colour and padding), but
// WHETHER is only ever this. An attribute and not a class, because Svelte
// writes a dynamic `class=` wholesale and would wipe a class put there
// behind its back. `act-guard.test.mjs` holds the source to it and
// `act-taps-*.test.mjs` hold the look to what a tap did.

/**
 * `run` — something is written or run at once; `fill` — a line is left in
 * the prompt; `arm` — the next tap means something else; `open` — a list,
 * a form or a menu takes over, or gives way again.
 */
export type ActKind = 'run' | 'fill' | 'arm' | 'open';

/** No kind is display: the attribute is taken off, as a menu's plain item has none. */
export function act(node: HTMLElement, kind?: ActKind) {
	const set = (to?: ActKind) => (to ? (node.dataset.act = to) : delete node.dataset.act);
	set(kind);
	return { update: set };
}
