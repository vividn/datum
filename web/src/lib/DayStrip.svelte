<!--
	The day strip: today pinned at the bottom, earlier days stacked above.

	The comparison a person actually makes is *is today like yesterday,
	like last Tuesday* — which needs the earlier days on screen at the same
	scale, under the same x-axis, with today always in view (da-cgyh). So
	the strip is two regions in one column: a scroll container holding the
	K most recent days before today, newest at the bottom, and today's
	band under it, which never scrolls away.

	The past is ONE `dayview` of K days through the `run(argv)` seam (da-hnyb)
	— see `bandArgv` in $lib/panes.ts — so its hour gridlines run through
	every day and its ground is one picture, not K boxes with the page
	showing between them. Today is its own band, with the hour axis the
	past suppresses (`--time-axis-height 0`). The two align because both
	are drawn at the same `--width` and the same `--day-height`, which is
	what pins `label_width` and so the x of every gridline.

	A fortnight to a picture, at most: sixty days in one `dayview` took
	a second or more in the browser, so a deep past is drawn in pictures of
	CHUNK days, the newest against today, each shown as it lands. Between
	two of them the stack leaves the space datum leaves between two days,
	so a day costs the same wherever it falls.

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
		dayPitch,
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

	/** The margin between the past and today, in pixels, matching the CSS below. */
	const GAP = 5;
	/** How far back the strip will reach, however tall its box. */
	const MAX_DEPTH = 60;
	/** The most days one picture of the past holds — see the note above. */
	const CHUNK = 14;
	/** What one press of an arrow key moves the day-height handle. */
	const HEIGHT_STEP = 5;

	/** Each picture on screen, by how many days back its newest day is. 0 is today's. */
	let shown: Record<number, RunResult> = {};

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

	/**
	 * The days the strip is showing: as many as the room above today
	 * holds, counting a day that is only partly revealed — a day half on
	 * screen is a day that has to be drawn, and it is the one the stack
	 * scrolls up to. The first costs its box; each one more, its pitch.
	 */
	$: room = Math.max(0, height - todayBox(dayHeight) - GAP);
	$: depth =
		room > 0
			? Math.min(MAX_DEPTH, Math.ceil((room - bandBox(dayHeight)) / dayPitch(dayHeight)) + 1)
			: 0;

	/** The height a drag is previewing, or the committed one at rest. */
	$: shownDayHeight = sizing ? sizing.preview : dayHeight;

	/**
	 * The past's pictures, oldest first — top to bottom, as they are drawn:
	 * a fortnight each counting back from yesterday, the oldest what is left.
	 */
	$: chunks = Array.from({ length: Math.ceil(depth / CHUNK) }, (_, i) => {
		const end = 1 + i * CHUNK;
		return { end, days: Math.min(CHUNK, depth - end + 1) };
	}).reverse();
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

	/** How many draws have been started, and which one each picture on screen came from. */
	let started = 0;
	const from: Record<number, number> = {};

	/**
	 * Every picture asked for at the current width, day height and epoch,
	 * by its argv — held as the promise, so two draws in flight for the
	 * same picture share one run. A taller box asks for one more day and
	 * re-draws the oldest picture only: today's, and every full fortnight,
	 * are the same argv as before. Dropped whole when any of the three
	 * changes.
	 */
	let cache = { key: '', runs: new Map<string, Promise<RunResult>>() };

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	$: request = { width, dayHeight, epoch };
	$: if (browser && request.width > 0) draw(request, chunks);

	function draw(
		at: { width: number; dayHeight: number; epoch: number },
		chunks: { end: number; days: number }[]
	): void {
		const key = `${at.width}/${at.dayHeight}/${at.epoch}`;
		if (cache.key !== key) cache = { key, runs: new Map() };
		const token = ++started;
		// Today first, then the past newest first: the worker takes them in
		// turn, and the picture nearest today is the one being compared.
		for (const { end, days } of [{ end: 0, days: 1 }, ...[...chunks].reverse()]) {
			const argv = bandArgv(at.width, at.dayHeight, end, days);
			const id = argv.join(' ');
			let pending = cache.runs.get(id);
			if (!pending) cache.runs.set(id, (pending = run(argv)));
			void pending.then((result) => {
				// A resize while a draw is in flight and two draws overlap;
				// they can finish out of order, and the loser would paint the
				// older answer over the newer one. Last started wins, picture
				// by picture — and until it lands, the one on screen stays.
				if (token < (from[end] ?? 0)) return;
				from[end] = token;
				shown = { ...shown, [end]: result };
				void anchor();
			});
		}
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
	 * The jagged line's teeth: a vertex every 2 units across a 400-wide
	 * viewBox, stretched to the strip's width — about 5px a stroke at
	 * 1000px and 2px on a 390px phone, still a tear rather than a blur.
	 */
	const ZIGZAG = Array.from({ length: 201 }, (_, i) => `${i * 2},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<div
		class="strip"
		style:--band="{bandBox(shownDayHeight)}px"
		style:--unit="{dayPitch(shownDayHeight)}px"
		style:--today="{todayBox(shownDayHeight)}px"
		style:--gap="{GAP}px"
	>
		{#if depth > 0}
			<div class="stack" bind:this={stackEl} on:scroll={onScroll}>
				{#each chunks as { end, days } (end)}
					<div class="band past" data-days-ago={end} data-days={days} style:--days={days}>
						{#if shown[end]}<CliOutput {...picture(shown[end])} />{/if}
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
			{#if shown[0]}<CliOutput {...picture(shown[0])} />{:else}<p class="quiet">…</p>{/if}
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
	   body, and `depth` is read off that height. A box shorter than one
	   day (da-zh2o) cuts today's band from the TOP — the overflow goes
	   past the start edge — so the hour axis along its bottom is the last
	   thing to go, and the body never scrolls. */
	.strip {
		position: relative;
		height: 100%;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: var(--gap);
		overflow: hidden;
	}

	/* The room above today, whatever is left of the box once today's band
	   has its floor. The past in it may run past it by up to one day (see
	   `depth`), and then it scrolls. Between two of its pictures, the
	   space datum leaves between two days: a day costs one pitch anywhere. */
	.stack {
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		gap: calc(var(--unit) - var(--band));
		overflow-y: auto;
		/* No scrollbar: it would take layout width from the stack and not
		   from today's band below it, and the two would stop lining up. */
		scrollbar-width: none;
	}
	.stack::-webkit-scrollbar {
		display: none;
	}
	/* Yesterday against today when the stack has room to spare: the
	   oldest picture takes the slack above it. An auto margin rather than
	   `justify-content: flex-end`, which in a scroll box puts the overflow
	   past the start edge where no scroll reaches it. */
	.stack .band:first-child {
		margin-top: auto;
	}

	.band {
		flex: none;
		overflow: hidden;
	}
	/* Reserved before the picture decodes — see bandBox and dayPitch. A
	   picture of the previous depth or day height, in the frame before its
	   re-render lands, sits against today and is clipped at the top rather
	   than allowed to shove the stack: here the overflow is meant to be
	   out of reach. */
	.band.past {
		height: calc(var(--band) + (var(--days) - 1) * var(--unit));
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
	}
	/* Today's band carries the hour axis. A floor, not a height: in a
	   shorter strip it is clipped whole from the top, never squeezed. */
	.band.today {
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
	   plus half the margin above it — clamped into a strip shorter than
	   that, so the day height can still be reached, at the top edge. */
	.day-grip {
		position: absolute;
		left: 0;
		bottom: min(calc(var(--today) + var(--gap) / 2), calc(100% - 6.5px));
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
