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

	How many days: as many as the BOX holds (da-zb0d). The strip is a
	widget of the dashboard, and the pane it sits in is what says how tall
	it is — so the strip fills its container and `depth` is `the room
	above today ÷ one band`. It has no handle for its own height, no
	memory of one, and no cap of its own: a taller pane is more past, and
	that is the whole control.

	How tall one day is drawn is the widget's SETTING, `dayHeight`: a prop
	in, and a `change` event out whenever the reader moves it. The notch
	in the LEFT margin, on the seam above today's band — where the date
	stamps are — sets it for EVERY band at once (da-07ij); the bands are
	re-drawn at the new size rather than scaled, because a scaled band is
	a band with an illegible axis (da-m0cj). `↻` in the corner puts it
	back. Who remembers the value between visits is the owner's business
	— the dashboard keeps it in the widget's `settings` in the layout
	tree — and the strip writes nothing down: when nobody handles the
	event, the value simply holds for the session.

	The notch is a `role="slider"` button, so the keyboard reaches it.
	Dragging it UP makes a day taller and down shorter (da-hsba), and ↑ / ↓
	say the same: the pointer and the keys are one convention.
-->
<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
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
	import { settle } from './layout/resizing.js';

	/** Any change re-runs every band. The frame bumps it after each run. */
	export let epoch = 0;

	/**
	 * How tall one day is drawn, in pixels — datum's `--day-height`.
	 *
	 * The one setting the strip has. The owner passes it in and hears
	 * `change` when the reader moves it (the dashboard stores it in the
	 * widget's settings); a value the owner never sets is the default.
	 */
	export let dayHeight: number = DEFAULT_DAY_HEIGHT;

	const dispatch = createEventDispatcher<{ change: number }>();

	/** The margin between two days, in pixels, matching the CSS below. */
	const GAP = 5;
	/** How far back the strip will reach, however tall its box. */
	const MAX_DEPTH = 60;
	/** What one press of an arrow key moves the day-height handle. */
	const HEIGHT_STEP = 5;

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

	// Whatever the owner hands in — a saved setting from an older layout
	// included — is drawn at a height datum will draw legibly.
	$: dayHeight = clamp(Math.round(dayHeight) || DEFAULT_DAY_HEIGHT, MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);

	let body: HTMLDivElement | undefined;
	let stackEl: HTMLDivElement | undefined;
	/** The box the pane gives the strip, in whole pixels, once measured. */
	let width = 0;
	let height = 0;
	/** Is yesterday the bottom-most band in view? */
	let atBottom = true;

	/** The pixels one more day costs: its own box plus the margin above it. */
	$: unit = bandBox(dayHeight) + GAP;
	/**
	 * The days the strip is showing: as many as the room above today
	 * holds, counting a day that is only partly revealed — a band half on
	 * screen is a band that has to be drawn, and it is the one the stack
	 * scrolls up to.
	 */
	$: room = Math.max(0, height - todayBox(dayHeight) - GAP);
	$: depth = Math.min(MAX_DEPTH, Math.ceil(room / unit));

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

	// The content box, not the element's own size: the body is padded,
	// and a band authored wider than the box it is shown in is a band the
	// browser scales, and the text with it (da-m0cj). The strip is 100%
	// of that box, so its height is the box's height.
	//
	// A taller box is more room above today, so the stack grows: re-anchor
	// it if the reader was at the bottom, or yesterday drifts off the top
	// of today by exactly the room that was just made. From here as well
	// as from `draw`, because a resize inside one day's worth moves the
	// box without changing how many days are in it.
	//
	// Through `settle` (layout/resizing.ts): a border being dragged resizes
	// the box every frame, and a band per frame per day was the jitter
	// (da-jgrn). The size lands on the drop, or once a burst of resizes has
	// been over for a beat; the bands drawn already stay up meanwhile,
	// scaled as whole pictures so their shared axis moves with its labels.
	onMount(() => {
		const settled = settle<{ w: number; h: number }>(({ w, h }) => {
			if (w !== width) width = w;
			if (h !== height) {
				height = h;
				void anchor();
			}
		});
		const observer = new ResizeObserver(([entry]) => {
			settled.push({ w: Math.floor(entry.contentRect.width), h: Math.floor(entry.contentRect.height) });
		});
		if (body) observer.observe(body);
		return () => {
			observer.disconnect();
			settled.stop();
		};
	});

	/** How many draws have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	/**
	 * Every band drawn for the current picture, by day — held as the
	 * promise, so two draws in flight for the same day share one run.
	 *
	 * A band depends on the width, the day height and the epoch, and not
	 * on how many other bands there are: a taller box asks for one more
	 * day and must draw ONE more band, not every band again. So the cache
	 * is keyed by the picture and dropped whole when the picture changes.
	 */
	let cache = { key: '', days: new Map<number, Promise<RunResult>>() };

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	$: request = { width, dayHeight, epoch };
	$: if (browser && request.width > 0) void draw(request, depth);

	async function draw(
		at: { width: number; dayHeight: number; epoch: number },
		depth: number
	): Promise<void> {
		const key = `${at.width}/${at.dayHeight}/${at.epoch}`;
		if (cache.key !== key) cache = { key, days: new Map() };
		const drawn = cache.days;
		const token = ++started;
		const days = Array.from({ length: depth + 1 }, (_, i) => depth - i);
		const results = await Promise.all(
			days.map((n) => {
				let band = drawn.get(n);
				if (!band) drawn.set(n, (band = run(bandArgv(at.width, n, at.dayHeight))));
				return band;
			})
		);
		// A resize while a draw is in flight and two draws overlap; they can
		// finish out of order, and the loser would paint the older answer
		// over the newer one. Last started wins, not last to return.
		if (token < shown) return;
		shown = token;
		bands = Object.fromEntries(days.map((n, i) => [n, results[i]]));
		await anchor();
	}

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

	// ── the day height: what the reader asks for, told to the owner ────

	function setDayHeight(px: number): void {
		dayHeight = clamp(Math.round(px), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
		dispatch('change', dayHeight);
	}

	function reset(): void {
		setDayHeight(DEFAULT_DAY_HEIGHT);
	}

	/** Where a drag started, what the value was when it did, and its preview. */
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
	function grabHeight(event: PointerEvent): void {
		event.preventDefault();
		const handle = event.currentTarget as HTMLElement;
		handle.setPointerCapture(event.pointerId);
		handle.focus();
		sizing = { y: event.clientY, from: dayHeight, preview: dayHeight };
	}
	function sizeTo(event: PointerEvent): void {
		if (!sizing) return;
		// The box follows the finger; the bands are re-drawn on the drop.
		// One `dayview` per band per frame is a render loop, and a band
		// stretched to fit meanwhile is a band whose axis has moved.
		sizing.preview = clamp(
			Math.round(sizing.from + (sizing.y - event.clientY)),
			MIN_DAY_HEIGHT,
			MAX_DAY_HEIGHT
		);
	}
	function drop(): void {
		if (sizing) setDayHeight(sizing.preview);
		sizing = null;
	}

	function onHeightKey(event: KeyboardEvent): void {
		const by = event.key === 'ArrowUp' ? 1 : event.key === 'ArrowDown' ? -1 : 0;
		if (!by) return;
		event.preventDefault();
		setDayHeight(dayHeight + by * HEIGHT_STEP);
	}

	/**
	 * The jagged line's teeth: a vertex every 4 units across a 400-wide
	 * viewBox, stretched to the strip's width — about 10px a stroke at
	 * 1000px and 4px on a 390px phone, still a tear rather than a blur.
	 */
	const ZIGZAG = Array.from({ length: 101 }, (_, i) => `${i * 4},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<div
		class="strip"
		style:--band="{bandBox(shownDayHeight)}px"
		style:--today="{todayBox(shownDayHeight)}px"
		style:--gap="{GAP}px"
	>
		{#if depth > 0}
			<div class="stack" bind:this={stackEl} on:scroll={onScroll}>
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
				<svg viewBox="0 0 400 6" preserveAspectRatio="none" aria-hidden="true">
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

		<button class="reset" on:click={reset} title="reset the day height">
			<span aria-hidden="true">↻</span><span class="sr">reset the day height</span>
		</button>
	</div>
</Pane>

<style>
	/* The box is the control: the strip is exactly as tall as the pane's
	   body, and `depth` is read off that height. */
	.strip {
		position: relative;
		height: 100%;
		display: flex;
		flex-direction: column;
		gap: var(--gap);
	}

	/* The room above today, whatever is left of the box once today's band
	   has its floor. The bands in it may run past it by up to one day
	   (see `depth`), and then it scrolls. */
	.stack {
		flex: 1 1 auto;
		min-height: 0;
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
	/* Yesterday against today when the stack has room to spare: the
	   oldest band takes the slack above it. An auto margin rather than
	   `justify-content: flex-end`, which in a scroll box puts the overflow
	   past the start edge where no scroll reaches it. */
	.stack .band:first-child {
		margin-top: auto;
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
		flex: none;
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

	/* On the seam above today, in the left margin. Absolute, so the notch
	   sits IN the 5px margin rather than opening one of its own. Today's
	   band is the strip's bottom edge, so the seam's centre is its box
	   plus half the margin above it. */
	.day-grip {
		position: absolute;
		left: 0;
		bottom: calc(var(--today) + var(--gap) / 2);
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
