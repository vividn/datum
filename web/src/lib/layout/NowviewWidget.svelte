<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a tiny − / + beside the axis's far label (da-ndzl) stepping how far
	back the picture reaches — down to 0, the snapshot of each field's state.
	The window's size is written nowhere: each button's title names the step
	it goes to (da-rex1). The step is the widget's setting, handed out with
	`change`, so the tree keeps it with the layout.

	The snapshot draws no axis, so it has no steps: nothing lies over its
	chips, and the way out of it is the zoom below.

	THE BAND IS A PLACE YOU POINT INTO (da-rskp). Four gestures live on it,
	over a pane that swipes between tabs and a page that scrolls, so the
	state machine is written down in `$lib/nowpoint.ts` and this component
	only runs it:

	  tap                     → a SELECTION where it fell; the window does not move
	  drag sideways           → PAN in time; the selection does not move
	  press 400 ms, then drag → a RANGE, its bar dropped the moment the press
	                            is long enough so the finger knows it is not panning
	  shift+drag (desktop)    → the same range; a long press with a mouse is
	                            nobody's habit (MY reading, not vividn's words)
	  drag left from the NOW  → a duration ending NOW — the sentence «I have been
	    region                  doing this for the last N minutes», which is most
	                            of the blocks a person logs, so it needs no long press
	  two fingers / ctrl+wheel→ the window's LENGTH (da-5goc), untouched here

	What a tap MEANS depends on where the window is. On now the axis is
	RELATIVE, so the selection is too: the −5 / −10 / −15 / −20 gridlines
	spell themselves `-q` … `-qqqq` and everywhere else is the relative
	`-t -7`, and the white line stays on its gridline as the minutes pass
	(`$lib/cursor.ts`). Panned away the axis is absolute, and so is the
	pill: `-t HH:MM`, the one the dayview has always made (vividn, 17:01:
	«off-now, yes use absolute time»).

	A click on the DAYVIEW still centres this window on the instant, which
	is what it has always done; a click in a nowview does not, and says so
	by marking the selection `from: 'nowview'`. `data-points="own"` is the
	other half of that: it tells the `picture` action to leave the band's
	clicks alone, the same idiom as `data-pans`.

	The pills the selection makes are drawn TWICE — in the bottom bar, as
	every pill is, and here in the band's MARGIN beside the instant each one
	names (vividn: «let's show the pills in the margin of the nowview»). One
	store, two views, `Pill.svelte` for both. The margin is real room: a
	swipe that starts in it changes tabs, because `data-pans` is the BAND's
	and not the widget's.

	`now →` sits at the band's right edge whenever the window is not on now,
	and takes the window back AND the time-based pills away with it. Its
	mirror `← selection` appears when a selection exists that the window
	does not contain (vividn's second «maybe»).

	The band takes sideways motion of its own, so it says so with
	`data-pans="x"` (da-kcmp): a one-finger drag across it is never a swipe
	between the pane's tabs, whatever the pane it is in.

	The steps, the buttons and the margin pills are the WIDGET's, not the
	picture's: a redraw replaces the picture, never them — it only re-places
	them against the new picture's geometry.

	The band carries its own `data-mode` — `press` between gestures, then
	`pan`, `range` or `now` — so someone driving it by hand can see in
	devtools which gesture the machine took.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy, onMount } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import Pill from '$lib/Pill.svelte';
	import { pills } from '$lib/bar.js';
	import { cursor, DUR_ID, pointAt, pointed, type Cursor } from '$lib/cursor.js';
	import { instantOn, msPerPx, timeAxis } from '$lib/hover.js';
	import { readLanes } from '$lib/lanes.js';
	import { advance, LONG_MS, SLOP_PX, snapTo, type Mode, type Press } from '$lib/nowpoint.js';
	import { NOWVIEW_HISTORY, NOWVIEW_STEPS, nowviewSpec, zoomHistory } from '$lib/panes.js';

	/** How far back the window reaches: one of NOWVIEW_STEPS. */
	export let history = NOWVIEW_HISTORY;
	/** The instant the window is centred on (`-t`); none is now. */
	export let at: string | undefined = undefined;
	export let epoch = 0;

	const dispatch = createEventDispatcher<{ change: string }>();

	/**
	 * Where the window is centred, and WHY — `null` is on now.
	 *
	 * The why is the difference between the two ways a window leaves now.
	 * `sel` means it is following the page's selection: a click on the
	 * dayview put it there, and clearing that selection brings it home. `pan`
	 * means a hand moved it, and then nothing brings it home but `now →` —
	 * the minute tick and a write re-draw it in place (da-rskp).
	 *
	 * It carries the `-t` VALUE the CLI is handed rather than an instant,
	 * because a hand-written tree's `at` may be any time flag the CLI reads
	 * — `09:00` — and not an ISO stamp. Where the window IS in time is read
	 * off the picture (`axis`), never off this.
	 *
	 * Seeded from that `at`, as a pan: nothing in the app writes the prop,
	 * and a tree that names an instant means it.
	 */
	let centre: { arg: string; by: 'sel' | 'pan' } | null = at ? { arg: at, by: 'pan' } : null;

	// A history off the list (a hand-written tree) steps from the default.
	$: i = NOWVIEW_STEPS.indexOf(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	$: up = NOWVIEW_STEPS[i + 1];
	$: down = NOWVIEW_STEPS[i - 1];
	$: instant = centre?.arg;
	$: spec = nowviewSpec(history, instant);
	/** Whether the window has left now — and so whether a tap makes an absolute pill. */
	$: offNow = !!instant;

	// A selection made somewhere ELSE points this window; one made here does
	// not, and neither does a RELATIVE one — that is measured from now, and
	// the window it was dropped on is the one on now. A selection CLEARED
	// takes back only the window it was pointing.
	//
	// A subscription rather than a `$:`, because this ASSIGNS the window and
	// a reactive statement that both reads and writes one variable re-runs
	// itself for ever.
	//
	// And of `cursor`, NOT of `pointed` — plus a guard on the instant it
	// last acted on. `pointed` is derived over the minute clock, and a
	// Svelte store calls its subscribers for every value it is given, the
	// same object included, so this ran once a minute and re-centred a
	// window a hand had panned away: asked at 12:00, panned to 11:45, back
	// at 12:00 a minute later (vremya on PR #48, da-rskp). The header above
	// says only `now →` brings a hand-moved window home, and now it is
	// true. The guard is for the other door into the same bug: the pills
	// reconciler re-points the cursor at the SAME instant when a `dur=`
	// pill is dropped, and that is not a new selection either.
	/**
	 * The instant `follow` last pointed the window at; `null` after a clear.
	 *
	 * Declared BEFORE the subscription below, which calls `follow` at once
	 * with the value the store holds — a `let` under that line is still in
	 * its dead zone when that first call reads it, and the whole page throws.
	 */
	let followed: number | null = null;

	onDestroy(cursor.subscribe(follow));

	function follow(c: Cursor | null): void {
		if (!c) {
			followed = null;
			centre = centre?.by === 'pan' ? centre : null;
			return;
		}
		if (c.rel !== undefined || c.from === 'nowview') return;
		if (+c.at === followed) return;
		followed = +c.at;
		centre = { arg: c.at.toISOString(), by: 'sel' };
	}

	/** Back to now, and the time-based pills — and so the selection — with it. */
	function toNow(): void {
		centre = null;
		pointAt(null);
	}

	/**
	 * The window onto the selection the reader cannot see.
	 *
	 * `sel`, not `pan`: the window is there BECAUSE of the selection, so
	 * clearing the selection brings it home — the same bargain a click on
	 * the dayview strikes.
	 */
	function toSelection(): void {
		const p = $pointed;
		if (p) centre = { arg: p.at.toISOString(), by: 'sel' };
	}

	// ── the picture's geometry ──────────────────────────────────────────

	let el: HTMLDivElement | undefined;

	const svgEl = () => el?.querySelector<SVGSVGElement>('svg.svg') ?? null;

	/**
	 * The picture's time axis on screen, as the last redraw left it.
	 *
	 * Cached rather than read per event: a gesture asks for it on every
	 * move, and it changes only when the picture is replaced or the box
	 * resized — which is exactly when `place` runs.
	 */
	let axis: ReturnType<typeof timeAxis> = null;

	/** Along the axis, whichever way it runs. */
	const along = (x: number, y: number) => (axis?.y ? y : x);

	/** The instant a point on screen is, by the picture's own geometry. */
	const instantAt = (x: number, y: number) => (axis ? instantOn(axis, along(x, y)) : NaN);

	/** The window's own idea of NOW: where its span ends while it is on now. */
	const nowMs = () => (offNow || !axis ? Date.now() : axis.to);

	/**
	 * The gridlines' positions along the axis, as the CLI ruled them.
	 *
	 * Which minutes a picture marks is the picture's to say — a 30-minute
	 * window rules five-minute steps, a 12-hour one rules hours — so a
	 * press snaps to what is DRAWN and never to a number computed here.
	 */
	function gridlines(): number[] {
		const svg = svgEl();
		return [...(svg?.querySelectorAll('.gridline') ?? [])].map((l) => {
			const r = l.getBoundingClientRect();
			return axis?.y ? r.top + r.height / 2 : r.left + r.width / 2;
		});
	}

	/**
	 * How wide the NOW region is for a finger: the strip at and beyond the
	 * now-line, plus this much of the band before it.
	 *
	 * 24px is a fingertip's half-width. The strip the CLI draws right of the
	 * now-line is the current states' (`.now-chunk`) and is only as wide as
	 * the clock label under it — around 30px in a wide pane and less in a
	 * narrow one — so aiming at the strip alone would be aiming at a target
	 * too small to hit. A window not on now has no now region at all.
	 */
	const NOW_PX = 24;

	function inNowRegion(x: number, y: number): boolean {
		const svg = svgEl();
		const line = svg?.querySelector('.current-time-line');
		const r = line?.getBoundingClientRect();
		if (!r) return false;
		return along(x, y) >= along(r.left + r.width / 2, r.top + r.height / 2) - NOW_PX;
	}

	// ── the one-pointer gestures ───────────────────────────────────────

	/** The press in hand, and what it has become. */
	let press: Press | null = null;
	let mode: Mode = 'press';
	/** Where the pointer is, for the timer that has no event of its own. */
	let last = { x: 0, y: 0 };
	/** Whether the press has lasted long enough to be a range. */
	let long = false;
	/** How far the band is dragged while a pan is in hand, in px. */
	let panned = 0;
	let held: ReturnType<typeof setTimeout> | undefined;

	function begin(event: PointerEvent): void {
		if (event.button > 0) return;
		press = {
			x: event.clientX,
			y: event.clientY,
			inNow: inNowRegion(event.clientX, event.clientY),
			shift: event.shiftKey
		};
		mode = 'press';
		long = false;
		last = { x: event.clientX, y: event.clientY };
		clearTimeout(held);
		held = setTimeout(() => {
			long = true;
			step(last.x, last.y);
		}, LONG_MS);
	}

	/** A press the pinch took over, or a cancel: nothing of it happens. */
	function drop(): void {
		clearTimeout(held);
		press = null;
		mode = 'press';
		long = false;
		panned = 0;
	}

	/** Run the machine to where the pointer is now, and do what the mode does. */
	function step(x: number, y: number): void {
		if (!press || !axis) return;
		const was = mode;
		mode = advance(mode, press, x, y, long);
		if (mode === 'range' && was !== 'range') pointAt(selection(instantAt(press.x, press.y)));
		if (mode === 'pan') panned = along(x, y) - along(press.x, press.y);
		if (mode === 'range') range(instantAt(press.x, press.y), instantAt(x, y));
		if (mode === 'now') pointAt({ rel: 0, span: (nowMs() - instantAt(x, y)) / 60_000, from: 'nowview' });
	}

	/**
	 * The selection an instant in this window is: RELATIVE while the window
	 * is on now — the axis is, so a `-q` stays on its gridline and the entry
	 * is logged that long before the Enter — and absolute once it is not.
	 */
	const selection = (ms: number, span?: number) =>
		offNow
			? { at: new Date(ms), span, from: 'nowview' as const }
			: { rel: (nowMs() - ms) / 60_000, span, from: 'nowview' as const };

	/** A range between two instants: it ENDS at the later, because a duration runs back. */
	function range(a: number, b: number): void {
		const [from, to] = a <= b ? [a, b] : [b, a];
		pointAt(selection(to, (to - from) / 60_000));
	}

	/** A tap: the selection lands where it fell, and the window is left alone. */
	function tap(x: number, y: number): void {
		const at = axis ? instantOn(axis, snapTo(along(x, y), gridlines())) : null;
		if (at !== null) pointAt(selection(at));
		// The lanes the press crossed, for the fields widget to answer
		// (da-qu16, $lib/lanes.ts): the band takes the pointer itself, so
		// nothing else on the page would hear the press.
		//
		// AFTER the selection, and not before it: a selection cleared
		// clears the lanes, and a tap at now IS a cleared selection
		// ($lib/cursor.ts) — yet it still re-orders the fields by its
		// height. The snapshot has no axis and so selects no time at all;
		// there the height is the whole of what a press says (vividn, 17:01).
		const svg = svgEl();
		if (svg) readLanes(svg, { x, y, t: at });
	}

	/** The window the pan has dragged it to: the drag moves the content, so the span moves the other way. */
	function commitPan(): void {
		if (!axis) return drop();
		const middle = (axis.from + axis.to) / 2 - panned * msPerPx(axis);
		centre = { arg: new Date(middle).toISOString(), by: 'pan' };
		panned = 0;
	}

	function finish(event: PointerEvent): void {
		if (!press) return;
		const still = Math.hypot(event.clientX - press.x, event.clientY - press.y) <= SLOP_PX;
		if (mode === 'pan') commitPan();
		// A press that never committed and never travelled is a tap; one
		// that travelled is a scroll the browser took, and points nowhere.
		else if (mode === 'press' && still) tap(press.x, press.y);
		drop();
	}

	// ── the zoom ───────────────────────────────────────────────────────

	/** The gesture in hand: what the band is scaled by, about which x. */
	let zoom: { origin: number; scale: number } | null = null;

	/**
	 * Where the anchor is drawn, in px across the picture's own box.
	 *
	 * A window with `-t` is centred on its instant (da-5flt: the yellow
	 * line there is now, wherever now falls, not the instant). One without
	 * ends at now, which datum draws as the now-line (`current-time-line`,
	 * dayview_svg.rs) wherever its geometry puts it; the snapshot draws
	 * none, and its anchor is the right edge.
	 */
	function anchorX(svg: Element): number {
		const box = svg.getBoundingClientRect();
		const line = svg.querySelector('.current-time-line');
		if (instant || !line) return instant ? box.width / 2 : box.width;
		const marker = line.getBoundingClientRect();
		return marker.x + marker.width / 2 - box.x;
	}

	/** Begin a gesture over the picture, if there is one to scale. */
	function start(): boolean {
		const svg = svgEl();
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
	// stop hearing it. A one-finger drag or a mouse wants the same, so the
	// listeners are armed by the first pointer down of either kind.
	let listening = false;

	function listen(on: boolean): void {
		if (on === listening) return;
		listening = on;
		for (const [type, handler] of [
			['pointermove', onPointerMove],
			['pointerup', onPointerUp],
			['pointercancel', onPointerCancel]
		] as [string, EventListener][]) {
			if (on) window.addEventListener(type, handler);
			else window.removeEventListener(type, handler);
		}
	}

	function onPointerDown(event: PointerEvent): void {
		listen(true);
		if (event.pointerType === 'mouse') return begin(event);
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (fingers.size === 1) return begin(event);
		// A second finger is a pinch, and the first finger's gesture was
		// never one: whatever it had become is dropped before the scale
		// starts, so nothing of it is committed on the way out.
		drop();
		if (fingers.size !== 2) return;
		apart = spread();
		if (apart > 0) start();
	}

	function onPointerMove(event: PointerEvent): void {
		if (fingers.has(event.pointerId)) fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (zoom && fingers.size === 2) zoom = { ...zoom, scale: spread() / apart };
		if (!press) return;
		last = { x: event.clientX, y: event.clientY };
		step(event.clientX, event.clientY);
	}

	function onPointerUp(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (zoom && fingers.size < 2) commit();
		else finish(event);
		if (fingers.size === 0 && !press) listen(false);
	}

	function onPointerCancel(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (zoom && fingers.size < 2) commit();
		else drop();
		if (fingers.size === 0) listen(false);
	}

	/**
	 * `touch-action: pan-y` keeps the browser off a pinch, but a two-finger
	 * drag is still a scroll it may claim — and claiming it sends
	 * `pointercancel` mid-gesture. A gesture of ours along the band is the
	 * same: once it has committed, the move is ours.
	 */
	function onTouchMove(event: TouchEvent): void {
		if (fingers.size >= 2 || mode !== 'press') event.preventDefault();
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

	// ── the steps, the margin and the buttons ──────────────────────────

	/** The steps' row: 2px gap, then two 22px targets. See the style. */
	const STEPS_WIDTH = 2 + 22 + 22;

	/**
	 * Below this width the margin strip, its pills and `now →` do not fit,
	 * and the widget shows the LINES alone.
	 *
	 * The tiny nowview in `mobile` is ~100px wide (measured on the built
	 * app): `now →` is 46px there and the shortest time pill — `-qq`, two
	 * sections of nothing — is 54px, so the two together already overflow
	 * it, and a pill near the instant it names has no room to be near
	 * anything. 200px fits `now →` beside a `-t 14:30` pill with a third of
	 * the band still legible. vividn agreed, 17:01.
	 */
	const TINY_PX = 200;

	/** Whether this nowview is wide enough for the margin and the buttons. */
	let roomy = true;

	/**
	 * Where the far label ends, in px across and down the widget's box:
	 * the leftmost tick label, which is how far back the window reaches
	 * (`-30m`, or `-15m` with the window centred on the cursor). Read off
	 * the picture as `anchorX` reads the now-line; none when it draws no
	 * axis — the snapshot. A nearer label the steps would lie over is not
	 * drawn: the far one says the window, and the steps take its
	 * neighbour's place.
	 */
	let far: { x: number; y: number } | null = null;

	/**
	 * Where each margin pill sits, in px across the widget's box: over the
	 * mark the selection was drawn as.
	 *
	 * Read off the DRAWN marks (`$lib/hover.ts` puts the arrow at the
	 * selection and the band over the range), so a pill in the margin and
	 * the line under it cannot disagree by a pixel — including for a
	 * relative selection, which is placed against the picture's own
	 * now-line and not against the clock.
	 */
	let spots: { time: number | null; dur: number | null } = { time: null, dur: null };

	function place(): void {
		if (!el) return;
		const home = el.getBoundingClientRect();
		roomy = home.width >= TINY_PX;
		const svg = svgEl();
		axis = svg && timeAxis(svg);
		const centreOf = (selector: string) => {
			const r = el?.querySelector(selector)?.getBoundingClientRect();
			return r ? r.left + r.width / 2 - home.x : null;
		};
		// Kept inside the strip: a pill centred on a mark at either end of
		// the band would otherwise hang half of itself off the widget. The
		// pill's own width is read off the one on screen, so a two-section
		// `-t 2026-09-09T14:30` is held in as firmly as a `-qq`; a pill wider
		// than the strip is simply centred.
		const held = (x: number | null, selector: string) => {
			if (x === null) return null;
			const half = (el?.querySelector(selector)?.getBoundingClientRect().width ?? 0) / 2;
			const lo = Math.min(half, home.width / 2);
			return Math.max(lo, Math.min(Math.max(home.width - half, home.width / 2), x));
		};
		spots = {
			time: held(centreOf('svg.svg .cursor-head'), '.nowview-margin .pill.time'),
			dur: held(centreOf('svg.svg .cursor-span'), '.nowview-margin .pill.dur')
		};
		const labels = [...(svg?.querySelectorAll<SVGElement>('.tick-label') ?? [])];
		if (!labels.length) return void (far = null);
		const box = (l: SVGElement) => l.getBoundingClientRect();
		const label = box(labels.reduce((a, b) => (box(b).x < box(a).x ? b : a)));
		for (const l of labels) {
			const b = box(l);
			const under = b.x > label.x && b.left < label.right + STEPS_WIDTH;
			l.style.visibility = under ? 'hidden' : '';
		}
		far = { x: label.right - home.x, y: label.y + label.height / 2 - home.y };
	}

	/** The pills the selection made: the ones the margin draws beside their instants. */
	$: marginPills = $pills.filter((p) => p.id === 'cursor' || p.id === DUR_ID);

	/**
	 * Which way a selection the window does not contain lies.
	 *
	 * Off the cached axis rather than off the drawn marks: a selection
	 * outside the window draws nothing at all, so there would be no mark
	 * to read, and the button has to appear the moment the window leaves it.
	 */
	// A second of slack either side: a range that ends AT now sits on the
	// window's own edge, and rounding must not read that as «off it».
	const EDGE_MS = 1000;

	$: away =
		!axis || !$pointed
			? null
			: +$pointed.at < axis.from - EDGE_MS
				? 'left'
				: +$pointed.at > axis.to + EDGE_MS
					? 'right'
					: null;

	onMount(() => {
		if (!el) return;
		const redrawn = new MutationObserver(place);
		redrawn.observe(el, { childList: true, subtree: true });
		const resized = new ResizeObserver(place);
		resized.observe(el);
		return () => {
			redrawn.disconnect();
			resized.disconnect();
		};
	});

	onDestroy(() => {
		clearTimeout(settle);
		clearTimeout(held);
		listen(false);
	});
</script>

<div class="nowview" bind:this={el}>
	<!-- The margin: the pills the selection made, each over the mark it
	     names, and ✕-able here as in the bar. NOT `data-pans`, so a swipe
	     that starts in it still changes the pane's tab (da-um7y). -->
	{#if roomy}
		<div class="nowview-margin" aria-label="selection">
			{#each marginPills as pill (pill.id)}
				{@const x = pill.id === DUR_ID ? spots.dur : spots.time}
				{#if x !== null}
					<span class="margin-pill" style:left="{x}px"><Pill {pill} /></span>
				{/if}
			{/each}
		</div>
	{/if}

	<div
		class="nowview-band"
		class:busy={!!zoom || mode === 'pan'}
		style:--zoom={zoom?.scale ?? 1}
		style:--zoom-x="{zoom?.origin ?? 0}px"
		style:--pan="{panned}px"
		data-pans="x"
		data-points="own"
		data-mode={mode}
		on:pointerdown={onPointerDown}
		on:touchmove|nonpassive={onTouchMove}
		on:wheel|nonpassive={onWheel}
	>
		<CliPane {spec} {epoch} />
	</div>

	{#if far && !zoom}
		<div class="nowview-steps" data-pans="x" style:left="{far.x}px" style:top="{far.y}px">
			<button
				type="button"
				class="step"
				aria-label="shorter window"
				title={down ? `shorter window (${down === '0' ? 'now' : down})` : 'shorter window'}
				disabled={!down}
				on:click={() => dispatch('change', down)}>−</button
			>
			<button
				type="button"
				class="step"
				aria-label="longer window"
				title={up ? `longer window (${up})` : 'longer window'}
				disabled={!up}
				on:click={() => dispatch('change', up)}>+</button
			>
		</div>
	{/if}

	<!-- Outside the band on purpose: a press on a button is not a press on
	     the picture, and keeping them siblings is what makes that true
	     without a single stopPropagation. They still declare the band's
	     axis, so a drag that starts on one is not a tab swipe either
	     (da-ndzl's test says so of the steps). -->
	{#if roomy && offNow && !zoom}
		<button
			type="button"
			class="nowview-jump right"
			data-pans="x"
			title="back to now, and the time pills away"
			on:click={toNow}>now →</button
		>
	{/if}
	{#if roomy && away && !zoom}
		<button
			type="button"
			class="nowview-jump {away} selection"
			data-pans="x"
			title="the window onto the selection"
			on:click={toSelection}>{away === 'left' ? '← sel' : 'sel →'}</button
		>
	{/if}
</div>

<style>
	/* The frame's content box, for the pane inside (`display: contents` in a
	   frame) and for the steps, the margin pills and the buttons over it. */
	.nowview {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		/* A finger still scrolls whatever this pane sits in, and two fingers
		   are the zoom's, not the browser's page zoom. On the WHOLE widget,
		   margin included: sideways is the page's to claim otherwise, and a
		   swipe from the margin is the pane's tab swipe — which the browser
		   would take first. */
		touch-action: pan-y;
	}

	/* The band is what takes the pointer, and so what declares the axis:
	   a finger still scrolls whatever this pane sits in, and two fingers
	   are the zoom's, not the browser's page zoom. */
	.nowview-band {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	/* The gesture's preview: the band scaled about the anchor, which is
	   therefore the one column of it that does not move, and slid by the
	   pan. Width only — the window is a span of time, and the field rows
	   are not part of it. */
	.nowview-band.busy {
		overflow: clip;
	}
	.nowview-band.busy :global(svg.svg) {
		transform: translateX(var(--pan)) scaleX(var(--zoom));
		transform-origin: var(--zoom-x) center;
	}

	/* A strip above the band: one pill tall, plus the half-row the frame's
	   own titles hang into it (PR #43) — measured at 8px on both screens,
	   which is why the pill sits at the strip's BOTTOM and not its top. A
	   pill drawn up there is a pill with a tab strip over it, which is what
	   the phone showed. Its own room besides: a swipe here is the pane's,
	   not the band's, because nothing in it declares `data-pans`. */
	.nowview-margin {
		position: relative;
		flex: 0 0 auto;
		height: 1.75rem;
		/* The pill reads at the command line's size where the bar draws it;
		   here it is a label over a band and takes a smaller one. */
		--input-font: 0.7rem;
	}

	/* Centred on the mark it names, and held inside the strip (see `held`). */
	.margin-pill {
		position: absolute;
		bottom: 0;
		transform: translateX(-50%);
		max-width: 100%;
	}

	/* Just right of the far label, on its middle. The glyph is tiny; the
	   target is a fingertip's, and the two sit edge to edge. */
	.nowview-steps {
		position: absolute;
		display: flex;
		margin-left: 2px;
		transform: translateY(-50%);
	}

	.step {
		width: 22px;
		height: 24px;
		padding: 0;
		border: 0;
		background: none;
		color: var(--muted);
		font: inherit;
		font-size: 13px;
		line-height: 1;
		cursor: pointer;
	}

	.step:hover:not(:disabled) {
		color: inherit;
	}

	.step:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* Against the band's edge, on its middle: small, in the page's own ink
	   over its own ground, so it reads as a control and not as a mark of
	   the picture. */
	.nowview-jump {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		padding: 2px 6px;
		border: 1px solid var(--rule);
		border-radius: 6px;
		background: var(--ground);
		color: var(--ink);
		font: inherit;
		font-size: 0.7rem;
		line-height: 1.2;
		white-space: nowrap;
		cursor: pointer;
		z-index: 2;
	}

	.nowview-jump.right {
		right: 4px;
	}

	.nowview-jump.left {
		left: 4px;
	}

	/* The selection's button is white-edged, as the selection is; on the
	   right it stands inside `now →`, which owns that corner. */
	.nowview-jump.selection {
		border-color: #ffffff;
	}

	.nowview-jump.selection.right {
		right: 62px;
	}
</style>
