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
	day rather than one render of K days: a pull then draws one more band
	and touches nothing already drawn, and the trap that comes with it —
	every band carrying its own copy of the hour axis — is answered by
	`--time-axis-height 0` on all but today's. The bands align because
	they are all drawn at the same `--width` and the same `--day-height`,
	which is what pins `label_width` and so the x of every gridline.

	Reaching for one more day: a pull. On touch, dragging down while the
	stack is already at its top; on a pointer, a wheel up in the same
	place; and, for the keyboard and for anyone who would rather not
	guess, the labelled button in the corner. All three are one `pull()`,
	and the two gestures are rate-limited because one flick of a wheel is
	dozens of events and means one more day, not thirty.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { run, type RunResult } from './run.js';
	import { BAND_BOX, bandArgv } from './panes.js';

	/** Any change re-runs every band. The frame bumps it after each run. */
	export let epoch = 0;

	/**
	 * How many days before today the stack holds.
	 *
	 * Zero at rest — the strip is today and nothing else until someone
	 * pulls, which is the frame as it has always been. It is not
	 * persisted: a reload starts at today again, which the bead calls
	 * fine, and it keeps the strip out of the storage takeover's way.
	 */
	let depth = 0;

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

	/** The gap between bands, in pixels, matching the CSS below. */
	const GAP = 2;
	/**
	 * How tall the stack may get: two and a half bands.
	 *
	 * The half is the point — a stack cut through a band says there is
	 * more above it, which is the only cue a hidden scrollbar leaves.
	 * (Hidden because a classic scrollbar takes layout width from the
	 * stack and not from today's band, and two bands drawn at different
	 * widths are two bands whose hours do not line up.)
	 */
	const STACK_MAX = Math.round(2.5 * (BAND_BOX + GAP));
	/** How far a finger travels before a drag counts as a pull. */
	const PULL_PX = 40;
	/** The shortest gap between two gesture-driven pulls, in ms. */
	const PULL_INTERVAL = 300;

	let body: HTMLDivElement | undefined;
	let stack: HTMLDivElement | undefined;
	/** The band width in whole pixels, once the pane has been measured. */
	let width = 0;
	/** Is yesterday the bottom-most band in view? */
	let atBottom = true;

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
		if (!body) return;
		const observer = new ResizeObserver(([entry]) => {
			const next = Math.floor(entry.contentRect.width);
			if (next !== width) width = next;
		});
		observer.observe(body);
		return () => observer.disconnect();
	});

	/** How many draws have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	$: request = { width, depth, epoch };
	$: if (browser && request.width > 0) void draw(request);

	async function draw(at: { width: number; depth: number }): Promise<void> {
		const token = ++started;
		const days = Array.from({ length: at.depth + 1 }, (_, i) => at.depth - i);
		const results = await Promise.all(days.map((n) => run(bandArgv(at.width, n))));
		// Pull while a resize is in flight and two draws overlap; they can
		// finish out of order, and the loser would paint the older answer
		// over the newer one. Last started wins, not last to return.
		if (token < shown) return;
		shown = token;
		const anchored = atBottom;
		bands = Object.fromEntries(days.map((n, i) => [n, results[i]]));
		// A pull adds a band above, so the stack grows: re-anchor it if
		// the reader was at the bottom, or yesterday drifts off the top of
		// today by exactly the band that was just added.
		await tick();
		if (anchored) toYesterday();
	}

	function onScroll(): void {
		atBottom = !stack || stack.scrollTop + stack.clientHeight >= stack.scrollHeight - 1;
	}

	/** Put yesterday back at the bottom of the stack. K is unchanged. */
	function toYesterday(): void {
		if (stack) stack.scrollTop = stack.scrollHeight;
		onScroll();
	}

	function pull(): void {
		depth += 1;
	}

	/** The stack is at its top — where a pull reaches for one more day. */
	function atTop(): boolean {
		return !stack || stack.scrollTop <= 0;
	}

	let lastGesture = 0;
	function gesturePull(): void {
		const at = Date.now();
		if (at - lastGesture < PULL_INTERVAL) return;
		lastGesture = at;
		pull();
	}

	function onWheel(event: WheelEvent): void {
		if (event.deltaY < 0 && atTop()) gesturePull();
	}

	/** Where the finger went down, or null if it went down mid-scroll. */
	let touchFrom: number | null = null;
	function onTouchStart(event: TouchEvent): void {
		touchFrom = atTop() ? event.touches[0].clientY : null;
	}
	function onTouchMove(event: TouchEvent): void {
		if (touchFrom === null || !atTop()) return;
		if (event.touches[0].clientY - touchFrom < PULL_PX) return;
		touchFrom = event.touches[0].clientY;
		gesturePull();
	}

	/** The jagged line's teeth, across a 100-wide viewBox. */
	const ZIGZAG = Array.from({ length: 26 }, (_, i) => `${i * 4},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<div
		class="strip"
		style:--band="{BAND_BOX}px"
		style:--gap="{GAP}px"
		on:wheel={onWheel}
		on:touchstart={onTouchStart}
		on:touchmove={onTouchMove}
	>
		{#if depth > 0}
			<div class="stack" bind:this={stack} on:scroll={onScroll} style:max-height="{STACK_MAX}px">
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
			Named by what it reads, not by an aria-label over the top of it:
			an accessible name that does not contain the visible one is a
			button a voice user cannot ask for. The arrow is decoration.
		-->
		<button class="pull" on:click={pull} title="show one more earlier day">
			<span aria-hidden="true">↥</span> earlier
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
		/* A drag at the top of the stack is a pull, not a scroll of the
		   page behind it. */
		overscroll-behavior: contain;
		/* No scrollbar: it would take layout width from the stack and not
		   from today's band below it, and the two would stop lining up. */
		scrollbar-width: none;
	}
	.stack::-webkit-scrollbar {
		display: none;
	}

	/* Reserved before the picture decodes — see BAND_BOX. Today's band
	   carries the hour axis as well, so it is the one band whose height
	   datum, not this file, decides. */
	.band {
		height: var(--band);
		flex: none;
	}
	.band.today {
		height: auto;
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

	.pull {
		position: absolute;
		top: 0;
		right: 0;
		padding: 0 0.3rem;
		font: inherit;
		font-size: 0.7rem;
		color: #7c7cff;
		background: #0d0d1acc;
		border: 1px solid #2a2a4a;
		border-radius: 0 0 0 4px;
		cursor: pointer;
	}

	.quiet {
		margin: 0;
		color: #444466;
		font-style: italic;
	}
</style>
