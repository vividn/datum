// Inspecting a mark: which entry a gesture in a picture landed on, and
// what a tooltip says about it (da-pzzoz).
//
// vividn, 2026-09-21: «it selects the entry that generated that block. So a
// state change block will be data-from, but an entry with a duration will
// be data-to, so it depends.» Which end holds the entry is NOT decided
// here and cannot be: two blocks may cover the very same span with their
// entries at opposite ends of it. The CLI resolved it in the walk that
// drew them and wrote the answer onto the mark as `data-id` (da-6hg6v),
// so everything below reads that attribute and derives nothing.
//
// A TAP ON A MARK INSPECTS IT (da-xg1vi). vividn, 2026-10-22: «single tap
// → select a piece of data», and a time is the long tap's, the right
// click's, the tap on ground and the time axis's — «for quickly selecting a
// time, a user can touch in the time axis, so that won't slow them down».
// The whole table is at the head of #lib/nowpoint.ts. (It replaces
// da-pzzoz's, where a plain tap always meant TIME and the inspect was a
// double tap or a still long press.)
//
// Pure and DOM-only: no store is touched here, so the truth table is
// `tests/inspect-pure.test.mjs` in node. The gesture that SPENDS a press
// is `inspectMark` in #lib/hover.ts, where the bar's selection lives.

/**
 * The mark an event landed on: the nearest element naming an entry, or
 * null over ground that names none.
 *
 * `[data-id]` and not `[data-state]`: the now chunk — «if you double tap a
 * color, that color is an open block», vividn's words from when the tap was
 * a double one — carries an id and nothing else
 * (da-6hg6v), and a dot with no single entry behind it carries a state and
 * no id. What can be inspected is exactly what names an entry.
 */
export const markOf = (target: EventTarget | null): Element | null =>
	target instanceof Element ? target.closest('[data-id]') : null;

/** The entry a mark names. */
export const hidOf = (mark: Element | null): string | null => mark?.getAttribute('data-id') ?? null;

/**
 * Every entry `root` draws, in the order it drew them — the `order` a
 * shift-range would run over, and the reason a picture can be one.
 */
export const marksIn = (root: ParentNode): Element[] => [...root.querySelectorAll('[data-id]')];

/**
 * The modifiers an inspect selects WITH, which is the event's own with
 * shift taken off it.
 *
 * In the tail and on a document shift picks a RANGE (#lib/selection.ts),
 * but on a picture shift+drag is a range of TIME (`atOnce`,
 * #lib/nowpoint.ts), and a shift+click that never traveled is a click
 * like any other. Ctrl/⌘ still toggles, and a LOCKED hid pill still accumulates,
 * which is the way vividn named for gathering several: «locking the hid pill
 * lets you select other pieces of data, so you can manipulate them
 * together».
 */
export const inspectMods = (e: { ctrlKey: boolean; metaKey: boolean }) => ({
	shiftKey: false,
	ctrlKey: e.ctrlKey,
	metaKey: e.metaKey
});

// ── the press an inspect has spent ─────────────────────────────────────
//
// One gesture, several listeners. A widget that has taken a press — the
// strip's long tap, which selects a time — and the `click` the browser
// sends after it (#lib/hover.ts) would both answer the same finger, and
// the click's answer over a mark is the entry, not the time.
//
// So the press is marked SPENT where it is answered, and the later
// listeners ask. Cleared by the next press, which `picture` begins
// on every drawn picture; a module-level flag and not a store, because it
// lives for the length of one gesture and no view renders from it.

let spent = false;

/** This press has been answered: the click and the tap that share it do nothing. */
export const spendPress = (): void => void (spent = true);

/** Whether the press in hand has been answered. */
export const pressSpent = (): boolean => spent;

// ── the press that traveled ───────────────────────────────────────────
//
// vividn, 2026-10-22: «in general panning around shouldn't cause data to
// be selected or any tool tips to pop up» (da-xg1vi). A press that moved
// past a click — a scroll, a pan, a pinch — or that the browser took for
// its own scroll (`pointercancel`) has TRAVELED, and a picture asks
// before it labels a mark or selects anything with its click. Watched beside the spent flag, and for
// as long: until the next press.

/** How far a press may travel and still be a click, in pixels: past it, it was a drag or a scroll. */
export const CLICK_SLOP = 5;

let traveled = false;
/** Where each pointer still down went down. */
const downs = new Map<number, { x: number; y: number }>();

/** Whether the press in hand, or the one just lifted, moved past a click. */
export const pressTraveled = (): boolean => traveled;

/**
 * A new press clears it — from the DOCUMENT and in the CAPTURE phase, so
 * it runs before every handler that might spend it, whatever the press
 * landed on.
 *
 * Not from the `picture` action's own `pointerdown`, which was the first
 * try: a band is bigger than the picture inside it, and a press that
 * landed on the band's own padding would then have left a spent flag
 * standing and swallowed the tap after it.
 */
export function watchPresses(
	target: Document | null = typeof document === 'undefined' ? null : document
): () => void {
	const clear = (e: Event) => {
		const { pointerId, clientX: x, clientY: y, isPrimary } = e as PointerEvent;
		spent = false;
		// The first pointer down starts afresh: a lift the document never
		// heard (its target gone from the page) must not outlive its press.
		if (isPrimary) downs.clear();
		// A pointer down while ANOTHER is still down is a pinch, and a pinch
		// is no click. Read before this one is recorded, so a press is never
		// its own second finger.
		traveled = downs.size > 0;
		downs.set(pointerId, { x, y });
	};
	const move = (e: Event) => {
		const { pointerId, clientX, clientY } = e as PointerEvent;
		const at = downs.get(pointerId);
		if (at && Math.hypot(clientX - at.x, clientY - at.y) > CLICK_SLOP) traveled = true;
	};
	const up = (e: Event) => void downs.delete((e as PointerEvent).pointerId);
	const cancel = (e: Event) => {
		traveled = true;
		up(e);
	};
	const on: [string, (e: Event) => void][] = [
		['pointerdown', clear],
		['pointermove', move],
		['pointerup', up],
		['pointercancel', cancel]
	];
	for (const [type, handler] of on) target?.addEventListener(type, handler, true);
	return () => {
		for (const [type, handler] of on) target?.removeEventListener(type, handler, true);
	};
}

/**
 * The app's one watch, armed on import and never taken down — the page
 * lives as long as the module does. The teardown above is for a caller
 * that wants the module back to itself (canary minimax, PR #71).
 */
export const unwatchPresses = watchPresses();
