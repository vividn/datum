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
// ⛔ SELECTING TIME STAYS THE PRIMARY OPERATION. vividn: «When you have
// enough data, like me, everywhere has data under it, and being able to
// select time (at least in nowview) is a much more important operation
// than selecting the data.» So a plain tap and a plain drag still mean
// TIME, and every gesture here is a deliberate one — a double tap, a
// still long press, a right click, a shift+click.
//
// Pure and DOM-only: no store is touched here, so the truth table is
// `tests/inspect-pure.test.mjs` in node. The gesture that SPENDS a press
// is `inspectMark` in $lib/hover.ts, where the bar's selection lives.

/**
 * How long after a tap a second one is still the same gesture, and how
 * far from it.
 *
 * The day strip has read a double tap as «the whole day back» since
 * da-ukcf with exactly these two numbers; a second recogniser beside it
 * disagreeing by 50ms would be two gestures a finger cannot tell apart.
 */
export const DOUBLE_MS = 350;
export const DOUBLE_PX = 30;

/** A tap remembered: when, where, and on whose entry. */
export interface Tap {
	at: number;
	x: number;
	y: number;
	/** The entry the tap landed on, or null over ground that names none. */
	hid: string | null;
}

/** Before any tap at all — never near anything, never recent, on nothing. */
export const NO_TAP: Tap = { at: -Infinity, x: 0, y: 0, hid: null };

/** What a pointer event remembers of itself, for the next one to compare against. */
export const tapOf = (e: { clientX: number; clientY: number; timeStamp: number }, hid: string | null): Tap => ({
	at: e.timeStamp,
	x: e.clientX,
	y: e.clientY,
	hid
});

/**
 * Whether `e`, on the entry `hid`, is the second half of a double tap
 * begun at `last`.
 *
 * THE SAME ENTRY, and not merely the same neighbourhood. vividn's gesture is
 * «if you double tap a color» — one colour, twice — and marks are stacked
 * a lane apart: in the nowview on a phone two fields' now-chunks sit well
 * inside DOUBLE_PX of each other, so «near enough» alone read two quick
 * taps on two different fields as one double tap and inspected the second
 * (`fieldsorder.test.mjs`, which taps `mood` then `sleep`). The distance
 * and the clock stay, for the finger that misses its own mark by a pixel
 * on the way back.
 */
export const doubled = (
	last: Tap,
	e: { clientX: number; clientY: number; timeStamp: number },
	hid: string | null
): boolean =>
	!!hid &&
	last.hid === hid &&
	Math.abs(e.clientX - last.x) < DOUBLE_PX &&
	Math.abs(e.clientY - last.y) < DOUBLE_PX &&
	e.timeStamp - last.at < DOUBLE_MS;

/**
 * The mark an event landed on: the nearest element naming an entry, or
 * null over ground that names none.
 *
 * `[data-id]` and not `[data-state]`: the now chunk — «if you double tap a
 * color, that color is an open block» — carries an id and nothing else
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
 * In the tail and on a document shift picks a RANGE ($lib/selection.ts),
 * but on a picture shift is the inspect gesture ITSELF — vividn: «on desktop
 * right click or shift click or also long click» — and one modifier cannot
 * be both. Ctrl/⌘ still toggles, and a LOCKED hid pill still accumulates,
 * which is the way vividn named for gathering several: «locking the hid pill
 * lets you select other pieces of data, so you can manipulate them
 * together».
 */
export const inspectMods = (e: { ctrlKey: boolean; metaKey: boolean }) => ({
	shiftKey: false,
	ctrlKey: e.ctrlKey,
	metaKey: e.metaKey
});

/**
 * Whether a mouse event is the desk's inspect — vividn: «on desktop right
 * click or shift click or also long click».
 *
 * The right button is asked as `contextmenu` and NOT as a button number:
 * one press fires `mousedown`, `pointerup` and `contextmenu`, and an
 * inspect run on more than one of them selects the entry and then
 * deselects it again. `contextmenu` is the one of the three with a default
 * to suppress, so it is the one the gesture is taken on.
 *
 * Shift is asked on the LIFT, not the press: in the nowview shift+DRAG is
 * a range with no waiting (`atOnce`, $lib/nowpoint.ts), so shift+click can
 * only be the press that never travelled.
 */
export const inspectClick = (e: { type: string; button: number; shiftKey: boolean }): boolean =>
	e.type === 'contextmenu' || (e.button === 0 && e.shiftKey);

/** A mark as the tooltip reads it — the `data-` the CLI wrote on it. */
export interface MarkData {
	field: string | null;
	state: string | null;
	from: string | null;
	to: string | null;
	at: string | null;
}

/** What the tooltip says: the entry in four words. */
export interface Tip {
	field: string;
	state: string;
	/** The instants the entry's marks span; `null` where the picture drew none. */
	from: number | null;
	to: number | null;
}

/**
 * The tip for ONE entry, from every mark that names it.
 *
 * Several marks, one entry, on purpose: nowview draws a state's open block
 * as a band AND as the now chunk beside it, and the chunk carries an id
 * and nothing else — no field, no state, no times (da-6hg6v). So the four
 * words are gathered across the marks rather than read off whichever one
 * the finger happened to land on, and a double tap on the colour says as
 * much as a double tap on the band it continues.
 *
 * ⛔ NO COMMENT. vividn's «+ comment (maybe?)» is the one thing a picture
 * does not carry: a mark has its field, its state and its instants and
 * nothing else, so a comment would mean a `datum get` — a whole CLI
 * process — per gesture, and a tooltip that arrives after the finger has
 * gone. It is left out until a mark carries it.
 */
export function tipOf(marks: MarkData[]): Tip | null {
	if (!marks.length) return null;
	// The instants are the CLI's own `iso()` — ISO 8601, UTC, to the
	// millisecond (dayview_svg.rs) — which is what `Date.parse` is exact
	// for. Anything it cannot read is dropped rather than guessed at, so a
	// format change narrows the span instead of inventing one.
	const ms = (s: string | null) => (s ? Date.parse(s) : NaN);
	const edges = marks.flatMap((m) => [ms(m.from), ms(m.at), ms(m.to)].filter((n) => !Number.isNaN(n)));
	const said = (pick: (m: MarkData) => string | null) => marks.map(pick).find((v) => v) ?? '';
	return {
		field: said((m) => m.field),
		state: said((m) => m.state),
		from: edges.length ? Math.min(...edges) : null,
		to: edges.length ? Math.max(...edges) : null
	};
}

// ── the press an inspect has spent ─────────────────────────────────────
//
// One gesture, three listeners. A double tap is recognised on the second
// `pointerdown`, but the `click` that follows it would set the time cursor
// ($lib/hover.ts) and the nowview's own `finish` would drop a relative
// pill ($lib/layout/NowviewWidget.svelte) — both of them answering the
// same finger, and both of them making the TIME pill vividn asked this
// gesture to make an hid pill instead of.
//
// So the press is marked SPENT where the inspect happens, and the two
// later listeners ask. Cleared by the next press, which `picture` begins
// on every drawn picture; a module-level flag and not a store, because it
// lives for the length of one gesture and no view renders from it.

let spent = false;

/** This press has inspected: the click and the tap that share it do nothing. */
export const spendPress = (): void => void (spent = true);

/** Whether the press in hand was spent by an inspect. */
export const pressSpent = (): boolean => spent;

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
	const clear = () => void (spent = false);
	target?.addEventListener('pointerdown', clear, true);
	return () => target?.removeEventListener('pointerdown', clear, true);
}

/**
 * The app's one watch, armed on import and never taken down — the page
 * lives as long as the module does. The teardown above is for a caller
 * that wants the module back to itself (canary minimax, PR #71).
 */
export const unwatchPresses = watchPresses();
