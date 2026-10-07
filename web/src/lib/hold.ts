// THE ONE HOLD (da-efr4i).
//
// The app has one long press and it had one implementation, written
// inside the fields widget (da-8810n): a timer armed on `pointerdown`,
// cleared by the lift, with `contextmenu` — the right button, and the
// menu key — and shift+click as the fine pointer's ways of saying the
// same thing. Since the widget TITLES want the gesture too (da-efr4i),
// «how long is long» lives here, once, rather than in two components that
// would drift apart.
//
// It is an action on a CONTAINER, and `within` names which of that
// container's descendants can be held — which is what lets shift+click be
// taken before the button under it sees it: a capture listener on an
// ancestor runs first, whatever order the two were registered in, where on
// the pressed element itself the order would be the framework's to decide.
//
// THE CLICK A LONG PRESS LEAVES BEHIND IS SWALLOWED AT THE WINDOW, not
// here. A finger's lift raises a click at the point it lifted from, and by
// then what a hold OPENED may be lying over that point: the chooser is a
// modal `<dialog>`, and a click on its backdrop is how it is dismissed —
// so a hold that opened it was closing it again in the same gesture. The
// tail of a gesture belongs to that gesture wherever it lands, so the
// swallow is a one-shot capture listener on the window, taken down by that
// click or by the next press, whichever comes first.

/** How long a press has to last before it counts as a long press. */
export const HOLD_MS = 450;

export interface Holding {
	/**
	 * What a hold means here, given the element held. Undefined turns the
	 * gesture off — a title under a lock has no widget to change.
	 */
	run?: (on: HTMLElement) => void;
	/** Which descendants can be held. The node itself when absent. */
	within?: string;
}

/**
 * Eat the one click a gesture is about to leave behind, wherever it lands —
 * see the head of this file. It is taken down by that click or by the next
 * press, so no click a reader means is ever inside it. A root tab's drag
 * leaves one too (layout/reorder.ts).
 */
export function swallowTail() {
	const off = () => {
		removeEventListener('click', eat, true);
		removeEventListener('pointerdown', off, true);
	};
	const eat = (event: MouseEvent) => {
		event.stopPropagation();
		event.preventDefault();
		off();
	};
	addEventListener('click', eat, true);
	addEventListener('pointerdown', off, true);
	return off;
}

export function holds(node: HTMLElement, spec: Holding) {
	let held: Holding = spec;
	let timer: ReturnType<typeof setTimeout> | null = null;
	/** The element the timer is counting for, so a finger that wanders off it cancels. */
	let on: HTMLElement | null = null;
	/**
	 * What THIS PRESS's timer already fired on, which is what says that the
	 * `contextmenu` now arriving is the browser catching up with a finger's
	 * hold and not a reader asking again.
	 *
	 * The PRESS and not a stopwatch. A deadline in milliseconds would have
	 * to be wide enough to outlast a task and would then swallow a
	 * deliberate second gesture made quickly — two right clicks in a row on
	 * two different fields is one, and it is what a reader does. The next
	 * `pointerdown` is the exact end of the press this belongs to, so that
	 * is what clears it — and so is a key (da-zdv2u): a mouse's long press
	 * leaves no `contextmenu` behind to catch up with, and the menu key's
	 * own `contextmenu` always follows a keydown, which a finger's never
	 * does.
	 */
	let fired: HTMLElement | null = null;
	/** Takes the tail swallow down early — on destroy, and nowhere else. */
	let unswallow: (() => void) | null = null;

	/** The holdable element this event landed on, or null — which is also «the gesture is off». */
	const holdable = (event: Event): HTMLElement | null => {
		if (!held.run) return null;
		if (!held.within) return node;
		const el = (event.target as Element | null)?.closest(held.within);
		return el instanceof HTMLElement && node.contains(el) ? el : null;
	};

	function clear() {
		if (timer !== null) clearTimeout(timer);
		timer = null;
		on = null;
	}

	const key = () => void (fired = null);

	function down(event: PointerEvent) {
		clear();
		fired = null;
		on = holdable(event);
		if (!on) return;
		const target = on;
		timer = setTimeout(() => {
			timer = null;
			on = null;
			fired = target;
			unswallow = swallowTail();
			held.run?.(target);
		}, HOLD_MS);
	}

	/** A finger that wanders off what it pressed has not held it. */
	function move(event: PointerEvent) {
		if (timer !== null && holdable(event) !== on) clear();
	}

	/**
	 * The right button, and the menu key, arrive as `contextmenu` — the one
	 * event of the press with a default to suppress, so it is the one the
	 * gesture is taken on ($lib/inspect.ts says the same of a picture).
	 * Only within the container: a reader right-clicking a pane's CONTENT
	 * still gets the browser's own menu, which is where text is selected
	 * and copied.
	 */
	function menu(event: MouseEvent) {
		const target = holdable(event);
		if (!target) return;
		event.preventDefault();
		clear();
		// This press has already held: the browser is catching up, not the
		// reader asking twice. Left standing for the `click` that may still
		// follow on a touch platform.
		if (fired) return;
		held.run?.(target);
	}

	/**
	 * In the CAPTURE phase, before anything on the element held:
	 * shift+click — the desktop's third way in (da-8810n) — is the gesture
	 * itself and never also the press.
	 */
	function click(event: MouseEvent) {
		if (!event.shiftKey) return;
		const target = holdable(event);
		if (!target) return;
		event.stopPropagation();
		event.preventDefault();
		held.run?.(target);
	}

	node.addEventListener('pointerdown', down);
	node.addEventListener('pointermove', move);
	node.addEventListener('pointerup', clear);
	node.addEventListener('pointerleave', clear);
	node.addEventListener('pointercancel', clear);
	node.addEventListener('contextmenu', menu);
	node.addEventListener('keydown', key);
	node.addEventListener('click', click, true);

	return {
		update: (next: Holding) => void (held = next),
		destroy() {
			clear();
			unswallow?.();
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('pointermove', move);
			node.removeEventListener('pointerup', clear);
			node.removeEventListener('pointerleave', clear);
			node.removeEventListener('pointercancel', clear);
			node.removeEventListener('contextmenu', menu);
			node.removeEventListener('keydown', key);
			node.removeEventListener('click', click, true);
		}
	};
}
