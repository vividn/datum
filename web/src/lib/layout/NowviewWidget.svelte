<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a + / − over its top-left corner stepping how far back the picture
	reaches — down to 0, the snapshot of each field's state. The window's size
	is written nowhere: each button's title names the step it goes to
	(da-rex1). The step is the widget's setting, handed out with `change`, so
	the tree keeps it with the layout.

	The band takes sideways motion of its own, so it says so with
	`data-pans="x"` (da-kcmp): a one-finger drag across it is never a swipe
	between the pane's tabs, whatever the pane it is in.

	The same window is also ZOOMED (da-5goc): two fingers pinching over the
	picture, or ctrl+wheel on a desktop, scale it about the anchor — now, or
	the `--at` instant when one is set — which stays where it is on screen.
	The gesture scales the band as a transform and asks for nothing; the
	release rounds the scale to a step of the same ladder the buttons walk,
	so the two controls always agree about where the window can be.

	While the time cursor is set ($lib/cursor.ts) the window is centred on
	it, over the `at` a hand-written tree may carry; its dotted line is the
	picture's own (the `picture` action draws it wherever the cursor falls).

	The controls sit beside the pane, not inside its picture: a redraw
	replaces the picture, never them.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import { cursor } from '$lib/cursor.js';
	import { NOWVIEW_HISTORY, NOWVIEW_STEPS, nowviewSpec, zoomHistory } from '$lib/panes.js';

	/** How far back the window reaches: one of NOWVIEW_STEPS. */
	export let history = NOWVIEW_HISTORY;
	/** The instant the window is centred on (`--at`); none is now. */
	export let at: string | undefined = undefined;
	export let epoch = 0;

	const dispatch = createEventDispatcher<{ change: string }>();

	// A history off the list (a hand-written tree) steps from the default.
	$: i = NOWVIEW_STEPS.indexOf(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	$: up = NOWVIEW_STEPS[i + 1];
	$: down = NOWVIEW_STEPS[i - 1];
	$: instant = $cursor?.at.toISOString() ?? at;
	$: spec = nowviewSpec(history, instant);

	// ── the zoom ───────────────────────────────────────────────────────

	let el: HTMLDivElement | undefined;
	/** The gesture in hand: what the band is scaled by, about which x. */
	let zoom: { origin: number; scale: number } | null = null;

	/**
	 * Where the anchor is drawn, in px across the picture's own box.
	 *
	 * Off the picture, not off the clock: datum draws the anchor as the
	 * now-line (`current-time-line`, dayview_svg.rs) wherever its geometry
	 * puts it, and the label column on the left means that is not a
	 * fraction of the box anyone here should be deriving. Without a line to
	 * read — the snapshot draws none — the widget's own argv says which
	 * edge it would be: a window with `--at` is centred on the instant, and
	 * one without ends at now.
	 */
	function anchorX(svg: Element): number {
		const box = svg.getBoundingClientRect();
		const line = svg.querySelector('.current-time-line');
		if (!line) return instant ? box.width / 2 : box.width;
		const marker = line.getBoundingClientRect();
		return marker.x + marker.width / 2 - box.x;
	}

	/** Begin a gesture over the picture, if there is one to scale. */
	function start(): boolean {
		const svg = el?.querySelector('svg.svg');
		if (!svg) return false;
		zoom = { origin: anchorX(svg), scale: 1 };
		return true;
	}

	/**
	 * End the gesture: round the scale to a step and let go of the band.
	 *
	 * The scale is dropped before the answer arrives, so the picture on
	 * screen is datum's own from here on — the transform is the gesture's
	 * preview of a window, not a substitute for one.
	 */
	function commit(): void {
		const scale = zoom?.scale ?? 1;
		zoom = null;
		const next = zoomHistory(history, scale);
		if (next !== history) dispatch('change', next);
	}

	// ── two fingers ────────────────────────────────────────────────────

	/** The fingers down on the picture, by pointer id. */
	const fingers = new Map<number, { x: number; y: number }>();
	/** How far apart they were when the pinch began. */
	let apart = 0;

	const spread = () => {
		const [a, b] = [...fingers.values()];
		return Math.hypot(a.x - b.x, a.y - b.y);
	};

	// The window listens, not the element: a finger that wanders off the
	// picture mid-pinch is still part of the pinch, and the element would
	// stop hearing it. Armed by the first finger, so the last one off is
	// always heard and no finger is left behind on the map.
	function listen(on: boolean): void {
		if (on) {
			window.addEventListener('pointermove', onPointerMove);
			window.addEventListener('pointerup', onPointerUp);
			window.addEventListener('pointercancel', onPointerUp);
		} else {
			window.removeEventListener('pointermove', onPointerMove);
			window.removeEventListener('pointerup', onPointerUp);
			window.removeEventListener('pointercancel', onPointerUp);
		}
	}

	function onPointerDown(event: PointerEvent): void {
		if (event.pointerType === 'mouse') return;
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (fingers.size === 1) listen(true);
		if (fingers.size !== 2) return;
		apart = spread();
		if (apart > 0) start();
	}

	function onPointerMove(event: PointerEvent): void {
		if (!fingers.has(event.pointerId)) return;
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (zoom && fingers.size === 2) zoom = { ...zoom, scale: spread() / apart };
	}

	function onPointerUp(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (zoom && fingers.size < 2) commit();
		if (fingers.size === 0) listen(false);
	}

	/**
	 * `touch-action: pan-y` keeps the browser off a pinch, but a two-finger
	 * drag is still a scroll it may claim — and claiming it sends
	 * `pointercancel` mid-gesture. While two fingers are down the move is
	 * ours.
	 */
	function onTouchMove(event: TouchEvent): void {
		if (fingers.size >= 2) event.preventDefault();
	}

	// ── ctrl+wheel ─────────────────────────────────────────────────────

	/**
	 * A notch of the wheel is half a doubling, so one lands on the next
	 * step of the ladder and the wheel steps as the buttons do. A trackpad
	 * pinch arrives here too, as a shower of small deltas: they accumulate
	 * into one gesture, which the pause at the end of it commits.
	 */
	const NOTCH = 200;
	let settle: ReturnType<typeof setTimeout> | undefined;

	function onWheel(event: WheelEvent): void {
		if (!event.ctrlKey) return;
		event.preventDefault();
		if (!zoom) start();
		const held = zoom;
		if (!held) return;
		zoom = { ...held, scale: held.scale * Math.pow(2, -event.deltaY / NOTCH) };
		clearTimeout(settle);
		settle = setTimeout(commit, 120);
	}

	onDestroy(() => {
		clearTimeout(settle);
		listen(false);
	});
</script>

<div
	class="nowview"
	class:zooming={!!zoom}
	style:--zoom={zoom?.scale ?? 1}
	style:--zoom-x="{zoom?.origin ?? 0}px"
	bind:this={el}
	data-pans="x"
	on:pointerdown={onPointerDown}
	on:touchmove|nonpassive={onTouchMove}
	on:wheel|nonpassive={onWheel}
>
	<CliPane {spec} {epoch} />
	<div class="nowview-steps">
		<button
			type="button"
			class="control badge step"
			aria-label="longer window"
			title={up ? `longer window (${up})` : 'longer window'}
			disabled={!up}
			on:click={() => dispatch('change', up)}>+</button
		>
		<button
			type="button"
			class="control badge step"
			aria-label="shorter window"
			title={down ? `shorter window (${down === '0' ? 'now' : down})` : 'shorter window'}
			disabled={!down}
			on:click={() => dispatch('change', down)}>−</button
		>
	</div>
</div>

<style>
	/* The frame's content box, for the pane inside (`display: contents` in a
	   frame) and for the steps over its top-left corner. */
	.nowview {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		/* A finger still scrolls whatever this pane sits in; two fingers are
		   the zoom's, not the browser's page zoom. */
		touch-action: pan-y;
	}

	/* The gesture's preview: the band scaled about the anchor, which is
	   therefore the one column of it that does not move. Width only — the
	   window is a span of time, and the field rows are not part of it. */
	.nowview.zooming {
		overflow: clip;
	}
	.nowview.zooming :global(svg.svg) {
		transform: scaleX(var(--zoom));
		transform-origin: var(--zoom-x) center;
	}

	/* Clear of a title lying over that corner (PaneFrame.svelte). */
	.nowview-steps {
		position: absolute;
		top: calc(4px + var(--under-top, 0px));
		left: calc(4px + var(--under-left, 0px));
		display: flex;
		gap: 2px;
		/* `.control.badge` (app.css) draws the steps, and its box is
		   `--badge` — which a pane sets to its title row and these would
		   inherit, being inside one. A step is a fingertip's 24px wherever
		   it is drawn, so the row says so once, for both of them. */
		--badge: 24px;
	}

	.step:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
