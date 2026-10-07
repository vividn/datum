// DRAGGING A ROOT TAB ALONG THE TOP BAR (da-w19ny). vividn: «Let's allow
// dragging to reorder root level tabs».
//
// A press is still a click until it moves: a mouse past `SLOP` picks the
// tab up, and a click, which switches or opens the active tab's menu, is
// what a press that never got that far stays. A FINGER is different,
// because the strip scrolls sideways under it on a phone (RootBar.svelte)
// and a sideways finger has to stay a scroll — so a finger picks the tab up
// by holding it still for the app's one hold (`HOLD_MS`, $lib/hold.ts), and
// one that moves first was scrolling. Once a tab is up, the browser's own
// scroll is refused (`touchmove`) and the strip scrolls itself instead,
// while the pointer is near either end of it.
//
// The tab rides with the pointer and the tab it would land beside is
// marked (`data-drop`). Letting go moves it; Escape and a cancelled
// pointer put it back. Either way the click the drag leaves behind is
// swallowed, so a drag never also switches or opens the menu.

import { HOLD_MS, swallowTail } from '$lib/hold.js';

/** How far a mouse moves before a press is a drag, and a finger before it is a scroll. */
const SLOP = 6;
/** How near an end of the strip a tab being dragged scrolls it, and how far a frame. */
const EDGE = 32;
const STEP = 8;

export interface Reordering {
	/** Which of the node's descendants are the tabs. */
	tabs: string;
	/** Move the tab at `from` so that it ends at `to`. */
	move: (from: number, to: number) => void;
}

export function reorders(node: HTMLElement, spec: Reordering) {
	let held = spec;
	let press: { id: number; x: number; y: number; scroll: number; tab: HTMLElement; from: number } | null = null;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let dragging = false;
	let frame = 0;
	/**
	 * The pointer's x, and the index the tab would land at in the list
	 * WITHOUT it — so `from`, until it moves, is where it already is.
	 */
	let x = 0;
	let to = -1;

	const all = () => [...node.querySelectorAll<HTMLElement>(held.tabs)];

	function down(event: PointerEvent) {
		if (press || event.button !== 0) return;
		const tab = (event.target as Element).closest<HTMLElement>(held.tabs);
		if (!tab || !node.contains(tab)) return;
		x = event.clientX;
		press = { id: event.pointerId, x, y: event.clientY, scroll: node.scrollLeft, tab, from: all().indexOf(tab) };
		to = press.from;
		if (event.pointerType === 'touch') timer = setTimeout(start, HOLD_MS);
		addEventListener('pointermove', move);
		addEventListener('pointerup', up);
		addEventListener('pointercancel', end);
		addEventListener('keydown', key, true);
	}

	function start() {
		if (!press) return;
		dragging = true;
		// Captured, so a mouse let go outside the window still says so: else
		// the next press's release would drop a drag nobody was making. On
		// the tab and not the strip, which would take the click off the tab.
		press.tab.setPointerCapture(press.id);
		press.tab.dataset.dragging = '';
		frame = requestAnimationFrame(edge);
		track();
	}

	function move(event: PointerEvent) {
		if (!press || event.pointerId !== press.id) return;
		x = event.clientX;
		if (!dragging) {
			if (Math.hypot(x - press.x, event.clientY - press.y) <= SLOP) return;
			// A finger that moves before its hold has fired is scrolling.
			if (event.pointerType === 'touch') return end();
			start();
		}
		track();
	}

	/** The tab under the pointer, and where it would land: past the middle of each other tab. */
	function track() {
		if (!press) return;
		press.tab.style.transform = `translateX(${x - press.x + node.scrollLeft - press.scroll}px)`;
		const others = all().filter((t) => t !== press?.tab);
		to = others.filter((t) => {
			const r = t.getBoundingClientRect();
			return r.left + r.width / 2 < x;
		}).length;
		const still = to === press.from;
		others.forEach((t, i) => {
			if (!still && i === to) t.dataset.drop = 'before';
			else if (!still && i === others.length - 1 && to === others.length) t.dataset.drop = 'after';
			else delete t.dataset.drop;
		});
	}

	/** Near an end of the strip, it scrolls that way, a frame at a time. */
	function edge() {
		const r = node.getBoundingClientRect();
		const by = x < r.left + EDGE ? -STEP : x > r.right - EDGE ? STEP : 0;
		if (by) {
			node.scrollLeft += by;
			track();
		}
		frame = requestAnimationFrame(edge);
	}

	function up(event: PointerEvent) {
		if (!press || event.pointerId !== press.id) return;
		const moved = dragging && to !== press.from ? [press.from, to] : null;
		end();
		if (moved) held.move(moved[0], moved[1]);
	}

	function key(event: KeyboardEvent) {
		if (!dragging || event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		end();
	}

	/** Everything put back; a drag's click swallowed. */
	function end() {
		clearTimeout(timer);
		cancelAnimationFrame(frame);
		for (const t of all()) delete t.dataset.drop;
		if (press) {
			press.tab.style.transform = '';
			delete press.tab.dataset.dragging;
		}
		if (dragging) swallowTail();
		press = null;
		dragging = false;
		removeEventListener('pointermove', move);
		removeEventListener('pointerup', up);
		removeEventListener('pointercancel', end);
		removeEventListener('keydown', key, true);
	}

	/** A tab that is up is the finger's, not the browser's to scroll or to open a menu over. */
	function refuse(event: Event) {
		if (dragging || (press && event.type === 'contextmenu')) event.preventDefault();
	}

	node.addEventListener('pointerdown', down);
	node.addEventListener('touchmove', refuse, { passive: false });
	node.addEventListener('contextmenu', refuse);

	return {
		update: (next: Reordering) => void (held = next),
		destroy() {
			end();
			node.removeEventListener('pointerdown', down);
			node.removeEventListener('touchmove', refuse);
			node.removeEventListener('contextmenu', refuse);
		}
	};
}
