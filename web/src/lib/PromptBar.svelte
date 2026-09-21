<!--
	THE SEAM (da-ckw3, reshaping da-xfa4's bar).

	The bar is no longer a box with a REPL that opens upward over the
	dashboard. It is the seam between the dashboard above it and a
	newest-first log below it:

	    dashboard (the layout)
	    ────────────────────────────────────────────────────
	    INPUT    datum  16:09:21  ❯ occur coffee▏ [pills]  ⌄
	    OUTPUT   the newest result  ← this IS the log's first entry
	    ──────────────────── below the fold, reached only through the bar
	    older entry
	    older entry …
	                                       [ close history ]

	Input, then its newest result, then older ones: one reading order.

	TWO SECTIONS, INPUT OVER OUTPUT. vividn, 2026-09-17: "Put dbname, clock, >
	and input all on the input section. The output section is reserved just
	for output. Put the input section on top of the output section". They
	were equal at first ("so 50/50"); on seeing that build he asked for the
	input to come back down — "Decrease the height of the input bar. There
	is a lot of wasted padding" — and for its type to go up a step, so the
	input is 2.9rem against the output's 3.6rem. So:

	- the INPUT section is ONE ROW — the mark (Brand.svelte: the database's
	  name, its colour, its menu), the clock WITH SECONDS, the `❯` and the
	  box with its pills (Prompt.svelte), and nothing else. The clock no
	  longer hides its seconds on a narrow screen: vividn, "On mobile too
	  please show seconds on the clock". The mark is a step smaller than it
	  was ("The size of the db/logo can be reduced some to save some
	  space") and still ellipsizes rather than taking the box's room.
	- the OUTPUT section is ROW ONE OF THE HISTORY and the full width of the
	  bar: the same `ReplEntry.svelte` the log below draws its rows with, so
	  the newest command looks the same whether the log is open or shut and
	  dragging the log up never meets it twice (vividn, 2026-09-21).

	Both are `--input-height` / `--output-height`, declared together on the
	shell (routes/+layout.svelte) because the dashboard's own height is the
	other side of that sum. vividn: "we may have to play around with the
	heights some" — they are two numbers in one place.

	THE OUTPUT FOLDS AWAY, and the GESTURE is what folds it. A phone starts
	folded and anything wider starts open, and after that it is whatever this
	device last chose ($lib/log.ts). Each state's height is fixed, so the
	dashboard reflows when a PERSON moves the bar and never because a command
	answered something long.

	There was a `^` BUTTON at the top edge for two rounds. vividn, 2026-09-18:
	"The ^ button obscures the text. Let's just get rid of this button and
	instead just put a little carrat above the input bar to suggest that it
	can be pulled up." So the chevron is a HINT now — a mark on the bar's top
	rule that takes no pointer and is in no Tab order — and a drag or a wheel
	is the whole of how the bar moves.

	FOLDED, A COMMAND STILL ANSWERS: its ACTION words — one per document,
	in the CLI's order and in the colour a document draws its action in —
	stand at the right of the input row and fade over `ACTION_MS`
	($lib/bar.ts). A failed command puts one red `ERROR` there instead,
	for `ERROR_MS`, and a tap on it unfolds the output onto the error text.
	Nothing unfolds by itself.

	THE LOG IS REACHED THROUGH THE BAR AND NOWHERE ELSE. How far open the
	bar is, is ONE number of pixels ($lib/log.ts has the arithmetic and the
	notch); the page itself never scrolls. This file owns the two gestures
	that move that number, and both begin on the bar:

	- a DRAG, by finger or pointer, from anywhere on the bar. The whole bar
	  is the handle — a phone's bottom 30px belong to the system's own
	  home gesture, so nothing may require starting there. A drag becomes a
	  drag only past `DRAG_START`, so a tap on the box, the mark, a pill or
	  the caret is still a tap, and the click that would follow a real drag
	  is swallowed.
	- a WHEEL while the pointer is over the bar. One notch down goes to the
	  log, one notch up comes back. A wheel that started over a pane and
	  drifted here on its inertia is still the pane's (`INERTIA_MS`).

	Escape comes back too. Entering the log from the keyboard is not built:
	vividn wants the whole app keyboard-navigable "once the shape starts to
	take form a little more", and half a shortcut now is a shortcut to
	unpick later.

	Everything the bar already did it still does: the pills, the ghost and
	`spent` are the PROMPT's (Prompt.svelte, one buffer shared with every
	input widget); `prefill` is taken here because the bar is the one view
	always on screen, and `focusLine` decides which box the caret lands in;
	while storage is degraded the border is the panes' alarm orange and the
	title is the storage sentence.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import Brand from '$lib/Brand.svelte';
	import ReplEntry from '$lib/ReplEntry.svelte';
	import { repl } from '$lib/replStore.js';
	import { ACTION_MS, ERROR_MS, focusLine, marks, prefill, text } from '$lib/bar.js';
	import { clockSecondsLabel, now } from '$lib/clock.js';
	import { arm, at, closeLog, inLog, moveTo, moving, notch, openOutput, output, resume, settle, shown, stop, toggleOutput } from '$lib/log.js';
	import Prompt from '$lib/Prompt.svelte';
	import { isDegraded, storage, storageSentence } from '$lib/storageState.js';

	/**
	 * How far a pointer travels before it is a drag and not a tap.
	 *
	 * This and `SWALLOW_MS` are asserted with SYNTHETIC pointer events
	 * (barlog.test.mjs), which is the right instrument for them: a synthetic
	 * event carries the exact coordinates and the exact order a threshold is
	 * about. What it cannot carry is TIME — every move in a dispatched loop
	 * shares one stamp — nor can the browser cancel it. So the two things that
	 * need either, `touch-action` and the fling, are driven by a real CDP
	 * touch instead, in the same file. Neither instrument replaces the other.
	 */
	const DRAG_START = 8;
	/** A wheel this soon after one elsewhere is that gesture's inertia. */
	const INERTIA_MS = 150;
	/**
	 * How long after a drag a click is still that drag's tail.
	 *
	 * A browser dispatches the click straight after the pointerup it came
	 * from, so this only has to outlast one task; it is wide because being
	 * wide costs nothing (no click is expected in that window anyway) and
	 * being narrow would let a drag press the caret it ended on.
	 *
	 * The pointerup's stamp and the click's are comparable: both are
	 * `DOMHighResTimeStamp` against the document's one time origin, which is
	 * what `event.timeStamp` is for every event a page receives.
	 */
	const SWALLOW_MS = 300;

	let bar: HTMLElement;
	let field: HTMLElement;

	$: if ($prefill !== null) take($prefill);

	/**
	 * The one-shot `prefill` signal is taken HERE because the bar is the one
	 * view always on screen — but where the caret then goes is `focusLine`'s
	 * choice, which is an input widget when the reader has put one on the
	 * page (da-3zon).
	 */
	async function take({ line, keyboard }: { line: string; keyboard: boolean }): Promise<void> {
		prefill.set(null);
		text.set(line);
		await tick();
		focusLine(keyboard);
	}


	/**
	 * The arrow's press: the fold, and NOT the focus.
	 *
	 * vividn, 2026-09-18: "Hitting ^ should not focus on the input bar or open
	 * the keyboard. That is jarring." On a phone a focus IS the keyboard,
	 * taking half the screen for a press that was about the other half. So
	 * this moves the bar and nothing else — the browser leaves the focus on
	 * the button it was pressed on, which is where a press belongs.
	 */
	function toggle(): void {
		toggleOutput();
	}

	/** The newest command, which is row one — see the output section below. */
	$: newest = $repl.scrollback.at(-1) ?? null;

	$: degraded = isDegraded($storage);

	function onKey(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !$inLog) return;
		closeLog();
		event.preventDefault();
	}

	/** When a wheel last turned somewhere that is not the bar. */
	let foreignAt = -Infinity;

	onMount(() => {
		const elsewhere = (event: WheelEvent): void => {
			if (!bar?.contains(event.target as Node)) foreignAt = event.timeStamp;
		};
		addEventListener('wheel', elsewhere, { capture: true, passive: true });
		return () => removeEventListener('wheel', elsewhere, { capture: true });
	});

	/**
	 * A wheel over the bar opens it further, or less.
	 *
	 * The same one number a drag moves, so a trackpad and a finger agree —
	 * and settled the same way, which is what puts the notch under a wheel
	 * too. A notch's worth per event, so one flick of a mouse's wheel opens
	 * the output exactly as the `^` does.
	 */
	// A DIAGONAL wheel is vertical: only `deltaY` is read, so a trackpad
	// gesture carrying both moves the bar by its vertical part and a
	// horizontal-only one is left alone — which is the prompt's own sideways
	// scroll, and not the bar's business (canary on PR #46).
	function onWheel(event: WheelEvent): void {
		if (event.timeStamp - foreignAt < INERTIA_MS || !event.deltaY) return;
		event.preventDefault();
		arm();
		moveTo(at() + (event.deltaY > 0 ? notch() : -notch()));
		settle(event.deltaY > 0);
	}

	/** The pointer that is down on the bar, and where the page was under it. */
	let from: { y: number; open: number; id: number } | null = null;
	/**
	 * The moves of the last `WINDOW_MS`, so a release knows how fast the
	 * finger was going at the END of the gesture.
	 *
	 * The end and not the whole drag: a finger that raced and then stopped to
	 * place the bar means the place, not the race. The stamps are
	 * `DOMHighResTimeStamp` against the document's one time origin, so the
	 * difference is milliseconds.
	 */
	let track: { y: number; t: number }[] = [];

	/** How far back a release looks for its speed. */
	const WINDOW_MS = 80;
	/**
	 * And the least it will measure over.
	 *
	 * A real finger reports a move a frame at a time, so any gesture a person
	 * made spans tens of milliseconds. Synthetic pointer events dispatched in
	 * a loop all carry the SAME stamp, which over 200px would read as
	 * hundreds of px/ms — a fling nobody threw. Under this floor there is no
	 * reading, so a script that sends a drag without time in it gets the
	 * let-go rules, which is what it is describing.
	 */
	const MIN_MS = 8;
	/** Faster than this is a slip of the sensor, not a thumb. */
	const MAX_SPEED = 4;

	/** px/ms over the window, positive when the bar is opening. */
	function speed(y: number, t: number): number {
		const first = track[0];
		if (!first) return 0;
		const ms = t - first.t;
		if (ms < MIN_MS) return 0;
		const px = (first.y - y) / ms;
		return Math.max(-MAX_SPEED, Math.min(MAX_SPEED, px));
	}
	let dragging = false;
	/** Whether the press interrupted a settle that has to be finished for it. */
	let settling = false;
	/** A click this instant or sooner is the tail of a drag, and is not a tap. */
	let swallowUntil = -Infinity;

	/** Put the bar back on its way, after a press that turned out to be a tap. */
	function unpause(): void {
		if (!settling) return;
		settling = false;
		// Where the glide WAS GOING, not "shut" ($lib/log.ts's `aim`): a tap
		// that lands inside a settle is not a request to undo what started it.
		resume();
	}

	function onPointerDown(event: PointerEvent): void {
		if (event.pointerType === 'mouse' && event.button !== 0) return;
		// A FINGER ALWAYS WINS OVER AN ANIMATION, so a press stops whatever
		// glide is in flight — and then it owes the bar a resting place, which
		// is what `settling` is for: a press that turns out to be a TAP puts
		// the bar back where the glide was taking it. Without that, a tap on
		// the box or the mark during a settle left the bar frozen part-way,
		// and a fling's settle is up to 520ms long, so the window is wide
		// enough to hit (canary on PR #46).
		settling = moving();
		stop();
		from = { y: event.clientY, open: at(), id: event.pointerId };
		track = [{ y: event.clientY, t: event.timeStamp }];
		dragging = false;
	}

	function onPointerMove(event: PointerEvent): void {
		if (!from || event.pointerId !== from.id) return;
		const moved = from.y - event.clientY;

		// STILL A TAP — nothing below this line runs, and in particular
		// nothing is prevented: the box keeps its caret, its selection and
		// its keyboard until the finger has said it means a drag.
		//
		// A guard at the top rather than an early `return` nested inside the
		// block below, which is the same thing and read as the opposite by
		// two reviewers in a row (PR #46). The shape was the problem, not the
		// order.
		if (!dragging && Math.abs(moved) < DRAG_START) return;

		// The first move past it, once.
		if (!dragging) {
			dragging = true;
			// Not on pointerdown — see `armed` in $lib/log.ts.
			arm();
			// So a finger that wanders off the bar mid-pull keeps pulling.
			// It can refuse — a synthetic pointer has no capture to give —
			// and the drag is fine without it.
			try {
				bar.setPointerCapture(event.pointerId);
			} catch {
				// no capture; the listeners on the bar are enough
			}
		}

		// And EVERY move from here on, which is why it is not inside the
		// block above: the browser would otherwise select text under the
		// finger for the whole rest of the pull.
		event.preventDefault();
		track = [...track, { y: event.clientY, t: event.timeStamp }].filter(
			(sample) => event.timeStamp - sample.t <= WINDOW_MS
		);
		moveTo(from.open + moved);
	}

	function onPointerUp(event: PointerEvent): void {
		if (!from || event.pointerId !== from.id) return;
		const from0 = from;
		from = null;
		if (!dragging) return unpause();
		dragging = false;
		settling = false;
		swallowUntil = event.timeStamp + SWALLOW_MS;
		// Where it comes to rest is `settle`'s to say, and how fast the finger
		// was going when it let go is half of that answer ($lib/log.ts): a
		// fling carries on, a let-go falls where it lies.
		settle(from0.y - event.clientY > 0, speed(event.clientY, event.timeStamp));
	}

	function onPointerCancel(): void {
		if (!from) return;
		from = null;
		if (!dragging) return unpause();
		dragging = false;
		settling = false;
		// A gesture the system took away (a call, a notification) still has to
		// leave the bar somewhere it is ALLOWED to be, and it decided nothing —
		// so it settles as a release with no speed in it. Below the notch that
		// is back the way it came; above it, where the reader had already
		// pulled it to, which is a height they chose and the system interrupted
		// (a canary on PR #46 read the old comment as promising the first of
		// those in every case).
		settle(false);
	}

	function swallow(event: MouseEvent): void {
		if (event.timeStamp > swallowUntil) return;
		swallowUntil = -Infinity;
		event.stopPropagation();
		event.preventDefault();
	}
</script>

<svelte:window on:keydown={onKey} />

<!-- The footer's own listeners are the DRAG, and a drag has no keyboard
     equivalent to offer: everything it reaches — the box, the mark, `return
     to top`, Escape — is a control or a key of its own.
     `swallow` is not an act; it is how a drag declines to be a click. -->
<!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
<footer
	class="prompt-bar"
	class:alarm={degraded}
		title={degraded ? $storageSentence : undefined}
	bind:this={bar}
	on:wheel={onWheel}
	on:pointerdown={onPointerDown}
	on:pointermove={onPointerMove}
	on:pointerup={onPointerUp}
	on:pointercancel={onPointerCancel}
	on:click|capture={swallow}
>
	<div class="section input">
		<!-- A size container: the database's name, whatever its length, does
		     not widen this block and so never takes room from the box. -->
		<div class="brand-block">
			<Brand />
		</div>
		<!-- Live, not restored: a command's stamp is history, this one is the
		     clock. Seconds at every width (vividn, on the phone). The whole
		     label is ONE flex item: a `:SS` that were an item of its own
		     would be a block box beside a block box, and `innerText` — and a
		     reader copying the clock — would find a line break in the middle
		     of a time. -->
		<span class="stamp"
			><span class="clock"
				>{clockSecondsLabel($now).slice(0, 5)}<span class="seconds"
					>{clockSecondsLabel($now).slice(5)}</span
				></span
			></span
		>
		<!-- The ACTION strip lives INSIDE this block, so it begins exactly at
		     the `❯` — vividn, 2026-09-18: "put them still in bottom part like
		     this, but over by the >. Over on the right side they are hidden."
		     Absolute positioning cannot find the `❯` from the section, because
		     where the row's items fall depends on the mark's and the clock's
		     widths; inside the block that starts there, `left: 0` IS there. -->
		<div class="prompt" bind:this={field}>
			<Prompt />

			<!-- Folded, this is the whole of what a command says. Keyed on the
			     object so a second command restarts the fade rather than joining
			     the first one's; the store's own timer takes it away at the
			     moment the fade ends ($lib/bar.ts). -->
			{#if !$output && $marks}
				{#key $marks}
					{#if $marks.failed}
						<!-- Its own element, not one of the words: a failure has to
						     be TAPPABLE, and a fingertip does not fit in the strip.
						     The button is 24px (conventions #12) and overflows the
						     strip by a few pixels either way; the chip inside it is
						     the paint, so the hit area can be bigger than the ink. -->
						<button
							type="button"
							class="failed"
							style="--fade: {ERROR_MS}ms"
							on:click={openOutput}><span class="chip">ERROR</span></button
						>
					{:else}
						<div class="marks" style="--fade: {ACTION_MS}ms">
							{#each $marks.words as word, i (i)}
								<span class="action" style:color={word.color}>{word.text}</span>
							{/each}
						</div>
					{/if}
				{/key}
			{/if}
		</div>
	</div>

	{#if $shown > 0}
		<!-- It OPENS: its height is the first slice of how far the bar is
		     pulled ($lib/log.ts), so a drag grows it smoothly and the notch
		     is where it has finished. Clipped while it is part-way. -->
		<div class="section output">
			<!-- ROW ONE OF THE HISTORY, not a second rendering of it. vividn,
			     2026-09-21: "the output section … should actually *be* the
			     first line of the repl. That is, it shouldn't be repeated
			     when you drag it up. It's just the last entry of the last
			     entries … It should also have the same format as the rest of
			     the repl." So it mounts the same `ReplEntry` the log does —
			     stamp, echo line and all — and the log starts at row two
			     (`skip={1}` in BarLog.svelte).

			     Straight from the scrollback rather than from `last`: both
			     say the same thing (bar.ts's `noteRun` IS what `record`
			     calls), and one list with one source cannot drift from
			     itself. `all` is left off, so a multi-document command draws
			     what fits here and `+N` for the rest — the history has the
			     room and passes it. -->
			{#if newest}
				<ReplEntry entry={newest} cap={4} />
			{/if}
		</div>
	{/if}

	<!-- THE ARROW, and it is a control. It was a button standing IN the row,
	     then a mark that took no pointer, and it is a button again on the
	     rule. The three asks, in his order:

	     - 2026-09-18: "Hitting ^ should not focus on the input bar or open
	       the keyboard. That is jarring." Still true, and `toggle` below is
	       where that is kept.
	     - 2026-09-18: "The ^ button obscures the text. Let's just get rid of
	       this button and instead just put a little carrat above the input
	       bar to suggest that it can be pulled up." The SHAPE he asked for
	       is what it keeps: a small chevron on the bar's top rule, drawn
	       mostly above it, not a tab standing in the row over the text.
	     - 2026-09-21: "tapping the arrow should open/close the output
	       section." So the press is back.

	     Removing the press to answer the first of those took more than he
	     asked for: it cost a tap target and left the fold with no accessible
	     or keyboard way in at all. It is a real `<button>` with a label that
	     says what it does, and its 24px hit area (conventions #12) hangs
	     above the rule so that the ink and the target both stay off the
	     row's text. -->
	<button
		type="button"
		class="caret"
		class:open={$output}
		aria-label={$output ? 'close the output section' : 'open the output section'}
		aria-expanded={$output}
		title={$output ? 'close the output section' : 'open the output section'}
		on:click={toggle}><span class="chevron">^</span></button
	>
</footer>

<style>
	.prompt-bar {
		/* The strip under the input that the ACTION words live in. */
		--marks-strip: 1.1rem;
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		/* The two sections, summed on the shell — which is also where the
		   dashboard's own height is taken from that sum
		   (routes/+layout.svelte). */
		height: var(--bar-height);
		min-width: 0;
		background: var(--ground);
		border-top: var(--bar-rule) solid var(--rule);
		/* The bar is a vertical handle: the browser may pan it sideways (a
		   long line in the box) but never up and down, which is ours. */
		touch-action: pan-x;
	}

	/* AND AGAIN ON THE BOX, because the box is a scroll container now.
	   `touch-action` is resolved from the touch's target up to the NEAREST
	   SCROLL CONTAINER and no further — so once `.field` became one (the
	   pills ride its run, Prompt.svelte), the bar's own `pan-x` stopped
	   being consulted for a touch that lands on the box, `.field`'s `auto`
	   let the browser claim a vertical pan, and the drag died on the
	   `pointercancel` that followed the first move. vremya measured it on
	   PR #46 with a real finger: seven drags from the box, `pointermove 1,
	   pointercancel 1`, the bar back at 0 every time, while the same drag
	   from the clock or the mark opened the log.

	   `pan-x` is what this box wants anyway — sideways is exactly the scroll
	   the pills are reached by, and up and down is the bar's. */
	.section.input :global(.field) {
		touch-action: pan-x;
	}

	/* Orange rather than red: "your data is not safe yet" is a warning
	   about the future, not a report of a failure. Visible from across a
	   desk (PaneFrame.svelte's alarm). */
	.prompt-bar.alarm {
		border-top-color: var(--alarm);
		box-shadow: 0 -1px 0 var(--alarm) inset;
	}

	/* vividn, 2026-09-18: "decrease margin between db/logo and clock, they are
	   different colors so they can be almost touching". So the row's gap is
	   the SMALL one and the things that do want air ask for it themselves
	   (the box, below) — rather than one generous gap paid for four times. */
	.section {
		display: flex;
		align-items: center;
		/* Between the row's items. It went to almost nothing — vividn, 2026-09-18:
		   "they can be almost touching" — and came back on 2026-09-21, when
		   almost-touching turned out to read as one string on a phone and to
		   put the mark's hover arrow over the clock. */
		gap: 0.5rem;
		min-width: 0;
		padding: 0 0.9rem 0 0.5rem;
	}

	/* The one seam that still wants air: what is typed, away from the clock. */
	.section.input .prompt {
		margin-left: 0.35rem;
	}

	/* The row's items are CENTRED IN THE WHOLE SECTION. vividn, 2026-09-18:
	   "Right now in the input section, all the text is at the very top of the
	   bar and there is a huge margin on the bottom. Center the text vertically
	   in the input section." They used to centre in what was left above a
	   `padding-bottom` the width of the ACTION strip, which put them a strip's
	   half-height too high and left the strip's whole height empty when no
	   command had answered.

	   So there is no padding, and the strip OVERLAYS the bottom of the row
	   instead — it is 0.7rem type on the last few px of a 2.9rem row, under
	   the line's own descender space and beside nothing, because the words
	   begin at the `❯` and the box's text begins after it. */
	.section.input {
		position: relative;
		flex: none;
		height: var(--input-height);
	}

	/* Output only, and the full width of the bar. */
	/* As tall as the bar is open, not a fixed 3.6rem: the first `--output-height`
	   px of the pull ARE this section growing (`--shown`, set on the shell).
	   Clipped, so a half-open one shows the top of what it says rather than
	   squashing it. */
	.section.output {
		flex: none;
		/* A COLUMN, because what is in it is a row of the history now
		   (ReplEntry.svelte): its head — the time and the command — sits
		   ABOVE what the command answered, exactly as it does in the log.
		   `.section` is a flex row for the input's four things, and an entry
		   laid out that way put its head beside its answer instead of over
		   it: at 320px the head measured 540px wide and the document had
		   nowhere left to be. */
		flex-direction: column;
		align-items: stretch;
		justify-content: center;
		height: var(--shown, 0px);
		overflow: hidden;
		border-top: 1px solid var(--tint);
	}

	.section.output :global(.result) {
		flex: 1;
		height: 100%;
	}

	/* A FIXED width, and the same width whatever the name is: a database's
	   name never moves the box beside it, and never takes a pixel from it.
	   The number is the width the mark is fitted INTO — the block is a size
	   container and the name's type is a share of it (Brand.svelte) — and
	   it is smaller than the column the mark used to have, which is the
	   space vividn asked the mark to give back now that it stands beside the
	   clock rather than over it. */
	.brand-block {
		display: flex;
		align-items: center;
		/* The name sits at the block's LEFT, near the screen's edge, and the
		   block's leftover falls between it and the clock.
		   
		   This is the third setting of one number and the reason is worth
		   keeping. The block is a FIXED width so a long database name can
		   never take a pixel from the box; a short name does not fill it, so
		   the leftover has to go somewhere. Left-aligned it goes into the gap
		   (where it started); right-aligned it goes to the outside, which is
		   what vividn asked for on 2026-09-18 — "you can sacrifice the margin in
		   between it and the clock more" — and what made the mark 16px in from
		   the edge with 1.6px before the clock. Seeing that on a phone he
		   amended it, 2026-09-21: "reduce left margin of db/logo, and put a
		   little more space between it and the clock. Right now the clock and
		   db select arrow overlap." So it is left-aligned again, and the gap
		   is the leftover plus `.section`'s own.
		   
		   1.6px was not only an overlap. The arrow is invisible until hover
		   and a phone has no pointer, so what a reader saw at rest was
		   `datum09:35:58` reading as one string. */
		justify-content: flex-start;
		flex: 0 0 4.6rem;
		min-width: 0;
		line-height: 1.1;
		container-type: inline-size;
	}

	.stamp {
		display: flex;
		align-items: center;
		flex: none;
		color: var(--ink-bright);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 1.05rem;
		font-variant-numeric: tabular-nums;
	}

	/* The rest of the row, and never less than this: a command line you
	   cannot read what you typed in is not a command line (the bead's trap,
	   and fit.test.mjs's claim at rest). 9rem is about thirteen characters
	   at the prompt's own size. */
	/* Stretched and RELATIVE, because the ACTION strip hangs off its bottom
	   edge and the `❯` is its left edge. The line inside still centres. */
	.prompt {
		position: relative;
		display: flex;
		align-items: center;
		align-self: stretch;
		flex: 1;
		min-width: 9rem;
	}

	/* The row's own padding. There is no `border-top: none` here any more:
	   the line no longer draws a rule for this row to take away
	   (Prompt.svelte). An override was the wrong shape — Svelte bumps a
	   scoped selector's specificity, so it lost, and a hairline the code
	   said was gone was drawn from the `❯` to the right edge for as long as
	   something else happened to cover it (vremya on PR #46). It is
	   asserted computed in bar.test.mjs. */
	.prompt :global(.line) {
		flex: 1;
		min-width: 0;
		padding: 0 0.2rem;
	}

	/* The action words, in the strip under the input and STARTING AT THE `❯`.
	   vividn asked for the right side first ("show all the ACTION words on the
	   right side") and then, on seeing it on a phone: "put them still in
	   bottom part like this, but over by the >. Over on the right side they
	   are hidden." A phone's right edge is where a case, a curve or the
	   system's own furniture takes the last few millimetres; the `❯` is in
	   the middle of the screen and in the middle of the reader's attention.

	   `left: 0` of the PROMPT block, which is the element that begins at the
	   `❯` — see the markup. */
	.marks {
		position: absolute;
		left: 0;
		bottom: 0;
		display: flex;
		align-items: flex-end;
		gap: 0.4rem;
		max-width: 100%;
		height: var(--marks-strip);
		/* SIDEWAYS ONLY. The words may not push the row wider than the block
		   they begin at, so the strip clips — but it sits with its bottom ON
		   the seam, and clipping the OTHER axis there is clipping letters
		   (da-qdm7, vividn: "the ACTION verb is slightly clipped under the
		   first repl input history line"). `overflow: hidden` takes both
		   axes and cannot be asked for one; `clip` can, and `visible` beside
		   it stays visible rather than becoming a scrollport the way it
		   would beside `hidden`. */
		overflow-x: clip;
		overflow-y: visible;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.7rem;
		/* AND THE LINE BOX HOLDS THE LETTERS. At `1` it is the font's size
		   and the font's own box is taller than that — 13px of ascent and
		   descent inside an 11.2px line — so `flex-end`, which puts the LINE
		   box on the strip's bottom edge, hung the last of every glyph PAST
		   it, where the clip took it.

		   Measured at 390x664, `TRACK●`: the ink reached 664.78 against a
		   strip floor of 663.98, so it hung over by 0.80px; at 1.2 it reaches
		   663.55 and clears by 0.44. THAT IS THE WHOLE MARGIN — less than half
		   a pixel, and it is a font metric, not a chosen number. There is no
		   room here to spend. The `overflow-y: visible` above is the second
		   guarantee for exactly that reason: a box whose monospace resolves
		   differently may overhang anyway, and it must be seen overhanging
		   rather than cut. */
		line-height: 1.2;
		animation: mark-fade var(--fade) ease-out forwards;
	}

	/* datum's own colour for the tag, inline from `action_color` (da-ckw3);
	   this is the fallback for a module too old to publish one. */
	.action {
		color: var(--accent-bright);
		white-space: nowrap;
	}

	/* Prominent, because it is the only thing a folded bar says about a
	   command that did not land — and a button, because it opens the output
	   that says what went wrong. */
	/* The hit area — a fingertip's 24px, centred on the strip so it reaches a
	   little above it. The bar does NOT grow for it: a command that fails may
	   not reflow the dashboard any more than one that succeeds. */
	.failed {
		position: absolute;
		left: 0;
		bottom: calc((var(--marks-strip) - 24px) / 2);
		display: flex;
		align-items: center;
		height: 24px;
		padding: 0;
		background: none;
		border: none;
		cursor: pointer;
		animation: mark-fade var(--fade) ease-out forwards;
	}

	/* The paint, which stays the size of the strip. */
	.chip {
		padding: 0 0.45rem;
		background: var(--error);
		border-radius: 4px;
		color: var(--page);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.7rem;
		font-weight: 700;
		line-height: 1.6;
		letter-spacing: 0.06em;
	}

	/* Held bright for most of its life and then gone: a word that fades
	   evenly from the first second reads as unimportant for fourteen of
	   them. */
	@keyframes mark-fade {
		0%,
		80% {
			opacity: 1;
		}
		100% {
			opacity: 0;
		}
	}

	/* On the bar's top line, centred on the whole bar. */
	/* The arrow: a small chevron on the bar's top rule, centred, and a 24px
	   target around it. The TARGET hangs above the rule — `bottom: 100%` less
	   a few pixels — so that neither the ink nor the hit area reaches the
	   row's text, which is what the old in-row tab did and what vividn asked to
	   be rid of. What it overlaps instead is the dashboard's own bottom
	   margin, which has nothing in it. */
	.caret {
		position: absolute;
		left: 50%;
		bottom: calc(100% - 7px);
		transform: translateX(-50%);
		z-index: 21;
		display: flex;
		align-items: flex-end;
		justify-content: center;
		width: 2.4rem;
		height: 24px;
		padding: 0 0 2px;
		background: none;
		border: none;
		color: var(--quiet);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		line-height: 1;
		cursor: pointer;
	}

	.caret:hover,
	.caret:focus-visible {
		color: var(--accent-bright);
	}

	/* The glyph turns over, not the button: the same mark says which way the
	   bar will go. */
	.chevron {
		display: inline-block;
		padding: 0 0.35rem;
		background: var(--ground);
		line-height: 1;
	}

	.caret.open {
		color: var(--accent);
	}

	.caret.open .chevron {
		transform: rotate(180deg);
	}

	/* Shown, then gone at the same moment, for a reader who asked for no
	   animation — the store's timer is what actually takes it away. */
	@media (prefers-reduced-motion: reduce) {
		.marks {
			animation: none;
		}
	}

	/* A phone: the row keeps its four things and gives the box the rest. */
	@media (max-width: 480px) {
		.section {
			gap: 0.4rem;
			/* "reduce left margin of db/logo" (vividn, 2026-09-21): the bar's own
			   left padding is that margin, and the mark now sits against it. */
			padding-inline: 0.25rem 0.4rem;
		}
		.brand-block {
			flex-basis: 4rem;
		}
		.stamp {
			font-size: 0.9rem;
		}
		.prompt :global(.line) {
			gap: 0.3rem;
			padding-inline: 0;
		}
	}
</style>
