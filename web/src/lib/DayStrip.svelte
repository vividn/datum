<!--
	The day strip: today pinned at the bottom, earlier days stacked above.

	The comparison a person actually makes is *is today like yesterday,
	like last Tuesday* — which needs the earlier days on screen at the same
	scale, under the same x-axis, with today always in view (da-cgyh). So
	the strip is two regions in one column: a scroll container holding the
	K most recent days before today, newest at the bottom, and today's
	band under it, which never scrolls away.

	Every band is one `dayview` through the `run(argv)` seam, asked for in
	the CLI's own flags — see `bandArgv` in $lib/panes.ts. One render per
	day rather than one render of K days: reaching for one more day draws
	one more band and touches nothing already drawn, and the trap that
	comes with it — every band carrying its own copy of the hour axis — is
	answered by `--time-axis-height 0` on all but today's. The bands align
	because they are all drawn at the same `--width` and the same
	`--day-height`, which is what pins `label_width` and so the x of every
	gridline.

	How many days, and how tall: the reader says both with two handles
	(da-07ij), and the strip remembers what they said.

	  - The grab bar along the BOTTOM edge sets the strip's height, and
	    the height is what says how much past there is: `depth` is not a
	    counter any more, it is `stack ÷ one band`. Dragging down makes
	    room and the days fall into it; dragging back up to today's own
	    height leaves today alone.
	  - The notch in the LEFT margin, on the seam above today's band —
	    where the date stamps are — sets `--day-height` for EVERY band at
	    once. The bands are re-drawn at the new size rather than scaled:
	    a scaled band is a band with an illegible axis (da-m0cj).
	  - `↻` in the corner puts both back and forgets them.

	Both handles are `role="slider"` buttons, so the keyboard reaches
	them; in both, ↓ is *more* — more days, taller days — because that is
	the direction the pointer drags for more, and one widget answering the
	same key two ways is worse than one that disagrees with a convention.

	The wheel-up and finger-drag pulls this strip used to carry are gone.
	They were a second way to say the one thing the bottom handle now
	says, and a finger dragging the strip and a finger dragging the handle
	are two gestures fighting over one surface.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { run, type RunResult } from './run.js';
	import {
		DEFAULT_DAY_HEIGHT,
		MIN_DAY_HEIGHT,
		MAX_DAY_HEIGHT,
		bandArgv,
		bandBox,
		todayBox
	} from './panes.js';

	/** Any change re-runs every band. The frame bumps it after each run. */
	export let epoch = 0;

	/** The margin between two days, in pixels, matching the CSS below. */
	const GAP = 5;
	/** How tall the bottom grab bar is. A finger's worth, not a line's. */
	const GRAB = 12;
	/** How far back the strip will reach. Two months is a lot of dragging. */
	const MAX_DEPTH = 60;
	/** Where the two handles' state is kept between visits. */
	const STORE_KEY = 'datum-daystrip';
	/** What one press of an arrow key moves the day-height handle. */
	const HEIGHT_STEP = 5;

	/** How tall one day is drawn, in pixels — datum's `--day-height`. */
	let dayHeight = DEFAULT_DAY_HEIGHT;
	/** How many pixels of past the reader has asked for, above today. */
	let stack = 0;

	/** Each band's render, by how many days back it is. 0 is today's. */
	let bands: Record<number, RunResult> = {};

	/**
	 * A band's picture — or the message that came instead of one, said
	 * out loud rather than left as a box that never finishes loading.
	 */
	const picture = (result: RunResult) => ({
		text: result.stdout || result.stderr,
		error: !result.stdout
	});

	const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));
	/** The pixels one more day costs: its own box plus the margin above it. */
	const unitFor = (height: number) => bandBox(height) + GAP;

	let body: HTMLDivElement | undefined;
	let stackEl: HTMLDivElement | undefined;
	/** The band width in whole pixels, once the pane has been measured. */
	let width = 0;
	/** Is yesterday the bottom-most band in view? */
	let atBottom = true;
	/** The screen's height, which is what caps how much of it the strip takes. */
	let screenHeight = 0;

	$: unit = unitFor(dayHeight);
	/**
	 * The days the strip is showing: as many as the space asked for holds,
	 * counting a day that is only half revealed — a band half on screen is
	 * a band that has to be drawn.
	 */
	$: depth = Math.min(MAX_DEPTH, Math.ceil(stack / unit));
	/**
	 * How much of that the strip may actually occupy.
	 *
	 * Half the screen. The strip's height drives its grid track, so an
	 * uncapped strip pushes the REPL and then the widget bar — the thing a
	 * phone user logs with — off the bottom of the screen (da-hkpd). Past
	 * the cap the days keep coming and the stack scrolls to them, which is
	 * the same bargain the strip has always made with a deep stack.
	 */
	$: maxStack = Math.min(MAX_DEPTH * unit, Math.max(unit, Math.round(screenHeight / 2)));
	$: shownStack = Math.min(stack, maxStack);

	/** The height a drag is previewing, or the committed one at rest. */
	$: shownDayHeight = sizing ? sizing.preview : dayHeight;

	/** The stack's days, oldest first — top to bottom, as it is drawn. */
	$: past = Array.from({ length: depth }, (_, i) => depth - i);
	/**
	 * The jagged line shows exactly when the band sitting above today is
	 * not the day before it — which is to say, when the reader has
	 * scrolled up into older days.
	 */
	$: jagged = depth > 0 && !atBottom;

	// The content box, not the element's own width: the body is padded,
	// and a band authored wider than the box it is shown in is a band the
	// browser scales, and the text with it (da-m0cj).
	onMount(() => {
		restore();
		screenHeight = window.innerHeight;
		const onResize = () => (screenHeight = window.innerHeight);
		window.addEventListener('resize', onResize);

		const observer = new ResizeObserver(([entry]) => {
			const next = Math.floor(entry.contentRect.width);
			if (next !== width) width = next;
		});
		if (body) observer.observe(body);

		return () => {
			window.removeEventListener('resize', onResize);
			observer.disconnect();
		};
	});

	/** How many draws have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	$: request = { width, depth, dayHeight, epoch };
	$: if (browser && request.width > 0) void draw(request);

	async function draw(at: { width: number; depth: number; dayHeight: number }): Promise<void> {
		const token = ++started;
		const days = Array.from({ length: at.depth + 1 }, (_, i) => at.depth - i);
		const results = await Promise.all(days.map((n) => run(bandArgv(at.width, n, at.dayHeight))));
		// Drag while a resize is in flight and two draws overlap; they can
		// finish out of order, and the loser would paint the older answer
		// over the newer one. Last started wins, not last to return.
		if (token < shown) return;
		shown = token;
		bands = Object.fromEntries(days.map((n, i) => [n, results[i]]));
		await anchor();
	}

	// A drag adds room above, so the stack grows: re-anchor it if the
	// reader was at the bottom, or yesterday drifts off the top of today by
	// exactly the room that was just made. Watched as well as called from
	// `draw`, because a drag inside one day's worth moves the box without
	// changing how many days are in it.
	$: void shownStack, void anchor();

	async function anchor(): Promise<void> {
		if (!atBottom) return;
		await tick();
		toYesterday();
	}

	function onScroll(): void {
		atBottom = !stackEl || stackEl.scrollTop + stackEl.clientHeight >= stackEl.scrollHeight - 1;
	}

	/** Put yesterday back at the bottom of the stack. The depth is unchanged. */
	function toYesterday(): void {
		if (stackEl) stackEl.scrollTop = stackEl.scrollHeight;
		onScroll();
	}

	// ── what the reader asked for, and remembering it ──────────────────

	/**
	 * Read the stored strip back, or leave the defaults standing.
	 *
	 * Every branch is wrapped: `localStorage` is not a property that always
	 * answers — a private window, a cleared store and a browser set to
	 * block site data all reach this code, and two of them throw on the
	 * ACCESS rather than returning null. A strip that will not draw at all
	 * because it could not read a preference is a worse strip than one
	 * drawn at the default.
	 */
	function restore(): void {
		try {
			const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? 'null');
			if (typeof saved?.dayHeight === 'number')
				dayHeight = clamp(Math.round(saved.dayHeight), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
			if (typeof saved?.stack === 'number')
				stack = clamp(Math.round(saved.stack), 0, MAX_DEPTH * unitFor(dayHeight));
		} catch {
			// No store to read. The defaults are already in place.
		}
	}

	let pending: ReturnType<typeof setTimeout> | undefined;
	/** Write on the next quiet moment: a drag is hundreds of changes. */
	function remember(): void {
		clearTimeout(pending);
		pending = setTimeout(() => {
			try {
				localStorage.setItem(STORE_KEY, JSON.stringify({ stack, dayHeight }));
			} catch {
				// Nowhere to write. This session still works.
			}
		}, 150);
	}

	function reset(): void {
		stack = 0;
		dayHeight = DEFAULT_DAY_HEIGHT;
		// Before the store is cleared, or a drag's debounced write lands on
		// top of the reset a moment later and undoes it.
		clearTimeout(pending);
		try {
			localStorage.removeItem(STORE_KEY);
		} catch {
			// Nothing was stored in the first place.
		}
	}

	function setStack(px: number): void {
		stack = clamp(Math.round(px), 0, MAX_DEPTH * unit);
		remember();
	}

	function setDayHeight(px: number): void {
		dayHeight = clamp(Math.round(px), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
		remember();
	}

	/** Where a drag started, and what the value was when it did. */
	let pulling: { y: number; from: number } | null = null;
	let sizing: { y: number; from: number; preview: number } | null = null;

	/**
	 * Take the pointer, and take it away from the browser.
	 *
	 * `preventDefault` is not decoration: without it Chromium reads a
	 * press-and-drag on a control as the start of a native drag and sends
	 * `pointercancel` after the first move, which is a handle that follows
	 * the finger for one frame and then lets go. It also suppresses the
	 * focus a press would give, so the focus is taken explicitly — the
	 * handle is a slider, and a slider the keyboard cannot reach after
	 * being clicked is half a control.
	 */
	function take(event: PointerEvent): void {
		event.preventDefault();
		const handle = event.currentTarget as HTMLElement;
		handle.setPointerCapture(event.pointerId);
		handle.focus();
	}

	function grab(event: PointerEvent): void {
		take(event);
		pulling = { y: event.clientY, from: stack };
	}
	function pullTo(event: PointerEvent): void {
		if (pulling) setStack(pulling.from + (event.clientY - pulling.y));
	}

	function grabHeight(event: PointerEvent): void {
		take(event);
		sizing = { y: event.clientY, from: dayHeight, preview: dayHeight };
	}
	function sizeTo(event: PointerEvent): void {
		if (!sizing) return;
		// The box follows the finger; the bands are re-drawn on the drop.
		// One `dayview` per band per frame is a render loop, and a band
		// stretched to fit meanwhile is a band whose axis has moved.
		sizing.preview = clamp(
			Math.round(sizing.from + (event.clientY - sizing.y)),
			MIN_DAY_HEIGHT,
			MAX_DAY_HEIGHT
		);
	}
	function drop(): void {
		if (sizing) setDayHeight(sizing.preview);
		sizing = null;
		pulling = null;
	}

	function onGrabKey(event: KeyboardEvent): void {
		const by = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
		if (!by) return;
		event.preventDefault();
		setStack((depth + by) * unit);
	}

	function onHeightKey(event: KeyboardEvent): void {
		const by = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
		if (!by) return;
		event.preventDefault();
		setDayHeight(dayHeight + by * HEIGHT_STEP);
	}

	/** The jagged line's teeth, across a 100-wide viewBox. */
	const ZIGZAG = Array.from({ length: 26 }, (_, i) => `${i * 4},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<div
		class="strip"
		style:--band="{bandBox(shownDayHeight)}px"
		style:--today="{todayBox(shownDayHeight)}px"
		style:--gap="{GAP}px"
		style:--grab="{GRAB}px"
	>
		{#if depth > 0}
			<div
				class="stack"
				bind:this={stackEl}
				on:scroll={onScroll}
				style:max-height="{shownStack}px"
			>
				{#each past as daysAgo (daysAgo)}
					<div class="band" data-days-ago={daysAgo}>
						{#if bands[daysAgo]}<CliOutput {...picture(bands[daysAgo])} />{/if}
					</div>
				{/each}
			</div>
		{/if}

		{#if jagged}
			<!--
				A real element, not paint: the tear between today and a day
				that is not the day before it, and the way back. Clicking or
				pressing it scrolls yesterday down against today again.
			-->
			<button
				class="jagged"
				on:click={toYesterday}
				aria-label="the band above today is not yesterday — bring yesterday back"
			>
				<svg viewBox="0 0 100 6" preserveAspectRatio="none" aria-hidden="true">
					<polyline points={ZIGZAG} />
				</svg>
			</button>
		{/if}

		<div class="band today" data-days-ago="0">
			{#if bands[0]}<CliOutput {...picture(bands[0])} />{:else}<p class="quiet">…</p>{/if}
		</div>

		<!--
			The day-height notch, on the seam above today's band and in the
			left margin where every band's date stamp sits — so it is beside
			the thing it resizes, in the one column of the strip that holds
			no data.
		-->
		<button
			class="grip day-grip"
			role="slider"
			aria-label="day height"
			aria-orientation="vertical"
			aria-valuemin={MIN_DAY_HEIGHT}
			aria-valuemax={MAX_DAY_HEIGHT}
			aria-valuenow={shownDayHeight}
			aria-valuetext="{shownDayHeight} pixels per day"
			on:pointerdown={grabHeight}
			on:pointermove={sizeTo}
			on:pointerup={drop}
			on:pointercancel={drop}
			on:keydown={onHeightKey}
		></button>

		<!-- Drag down for more room, and the days before today fall into it. -->
		<button
			class="grip grab"
			role="slider"
			aria-label="days shown before today"
			aria-orientation="vertical"
			aria-valuemin={0}
			aria-valuemax={MAX_DEPTH}
			aria-valuenow={depth}
			aria-valuetext={depth === 1 ? '1 earlier day' : `${depth} earlier days`}
			on:pointerdown={grab}
			on:pointermove={pullTo}
			on:pointerup={drop}
			on:pointercancel={drop}
			on:keydown={onGrabKey}
		></button>

		<button class="reset" on:click={reset} title="reset to today">
			<span aria-hidden="true">↻</span><span class="sr">reset to today</span>
		</button>
	</div>
</Pane>

<style>
	.strip {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: var(--gap);
	}

	.stack {
		display: flex;
		flex-direction: column;
		gap: var(--gap);
		overflow-y: auto;
		/* No scrollbar: it would take layout width from the stack and not
		   from today's band below it, and the two would stop lining up. */
		scrollbar-width: none;
	}
	.stack::-webkit-scrollbar {
		display: none;
	}

	/* Reserved before the picture decodes — see bandBox. A band drawn at
	   the previous day height, in the frame between a drop and its
	   re-render, is clipped rather than allowed to shove the stack. */
	.band {
		height: var(--band);
		flex: none;
		overflow: hidden;
	}
	/* Today's band carries the hour axis. A floor, not a height: the axis
	   is the one thing in the strip that must never be clipped. */
	.band.today {
		height: auto;
		min-height: var(--today);
	}

	.jagged {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
		line-height: 0;
	}
	.jagged svg {
		display: block;
		width: 100%;
		height: 6px;
	}
	.jagged polyline {
		fill: none;
		stroke: #e8820c;
		stroke-width: 1.2;
		vector-effect: non-scaling-stroke;
	}
	.jagged:focus-visible {
		outline: 1px solid #7c7cff;
	}

	.grip {
		padding: 0;
		border: 0;
		background: none;
		cursor: row-resize;
		/* The browser must not read a drag on a handle as a scroll of the
		   page behind it — the pointer events below are the whole gesture. */
		touch-action: none;
	}
	.grip::before {
		content: '';
		display: block;
		border-radius: 3px;
		background: #2a2a4a;
	}
	.grip:hover::before,
	.grip:focus-visible::before {
		background: #7c7cff;
	}
	.grip:focus-visible {
		outline: 1px solid #7c7cff;
	}

	/* The bottom edge of the strip: full width, and tall enough for a
	   finger even though the line it draws is two pixels. */
	.grab {
		width: 100%;
		height: var(--grab);
		flex: none;
	}
	.grab::before {
		width: 44px;
		height: 2px;
		margin: 5px auto;
	}

	/* On the seam above today, in the left margin. Absolute, so the notch
	   sits IN the 5px margin rather than opening one of its own.
	   Measured up from the strip's own bottom edge, which is below today's
	   band by the grab bar and the margin above it — not from today's band,
	   which is what a bare `--today` would say. */
	.day-grip {
		position: absolute;
		left: 0;
		bottom: calc(var(--grab) + var(--gap) * 1.5 + var(--today));
		transform: translateY(50%);
		width: 30px;
		height: 13px;
		display: flex;
		align-items: center;
		z-index: 2;
	}
	.day-grip::before {
		width: 100%;
		height: var(--gap);
	}

	.reset {
		position: absolute;
		top: 0;
		right: 0;
		padding: 0 0.35rem;
		font: inherit;
		font-size: 0.8rem;
		line-height: 1.3;
		color: #7c7cff;
		background: #0d0d1acc;
		border: 1px solid #2a2a4a;
		border-radius: 0 0 0 4px;
		cursor: pointer;
		z-index: 2;
	}

	/* Named by what it does, for anyone who cannot see the glyph. */
	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}
</style>
