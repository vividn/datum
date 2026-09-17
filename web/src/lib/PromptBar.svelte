<!--
	The bottom bar (da-0a5o, reshaped by da-xfa4): the one fixed thing on
	the screen, and the app's status line as well as its command line.

	vividn: "The bottom 40% of the input bar will be for input, the top 60%
	should show the result of the last command". So the bar is two tiers:

	- the RESULT area on top ($lib/BarResult.svelte) — the last command's
	  documents as sections, one each, or its own output when it echoes
	  none;
	- the INPUT row below — the prompt line every command on the page goes
	  through (Prompt.svelte), with the pills riding after what is typed.

	At the left, spanning both, the brand — the NAME of the database the
	page is on, in Gemunu Libre and in that database's colour, which is also the
	menu of databases (da-y992, Brand.svelte). The `beta`
	mark and its ⓘ went up to the root bar beside the gear (vividn on PR #25,
	Beta.svelte). UNDER the brand is the clock, bright, in the input row's
	40% so it lines up with the prompt (vividn: "Move the clock underneath
	the logo, and make it nice and bright, takes up 40% to align with
	command"). Which build this is went to the root bar in da-wjob.

	At the bar's TOP EDGE, one `^` that expands upward into the whole REPL
	and closes it again (da-bhvy). vividn: "The (tap to expand) should be at
	the top — it can overlay the top line of the entire input bar. And it
	should toggle, hitting it again should close the repl." So the result
	area's «(tap to expand)» is gone: one control, one place, both ways.

	What it opens is the REPL COMPONENT — the same widget a layout can
	hold, reading the same scrollback store — not a second rendering of
	it. The expansion's REPL does not register with the engine
	($lib/layout/engine.ts): a press still runs wherever it would have.

	The open REPL's foot sits on the INPUT ROW, not on the bar: it covers
	the result tier exactly, so its last entry IS the very output the tier
	was showing — and whole, where the tier had cut it with an ellipsis or
	a `+N`. The prompt below it stays put and keeps the focus, so Enter
	runs and the new command lands in the REPL and in the covered tier
	alike.

	THE BAR'S HEIGHT IS FIXED, and that is the point of it: 6rem (96px at
	the browser's default), never changing with what the last command
	answered, so the dashboard above it never reflows. The two tiers are
	57.6px and 38.4px, and 6rem is the SMALLEST number that holds all four
	cases of the result area, which is what fixes it:

	  · 4 raw lines at 0.68rem/1.25 — 54.4px, the deepest case, from a
	    `tail` too long to show whole;
	  · 2 document lines at 1rem/1.35 — 43.2px, at the larger size vividn
	    asked for ("the font size could be bigger though when there are
	    only 2 lines"), and it is two whatever the entry carries: its data
	    stands BESIDE them in a brace, four 0.6rem/1.1 rows deep, which is
	    42.2px and so inside the same 43.2px (da-f4mw, Doc.svelte);
	  · and 38.4px below for the 1.15rem prompt, whose line box is ~22px.

	It grew from 64px, which takes 32px from the dashboard. The floors that
	room is measured against are the panes' own (`legibleOfKind` in
	layout/registry.ts, through `rootFloorOf`), and the root scrolls to them
	when the page is shorter than they are — so the cost of the extra tier
	is 32px of scroll on a short screen and nothing on a tall one.

	Every command run anywhere shows here (`last` in $lib/bar.ts, told by
	`record`): the result stays until the next command. That is the teach
	loop, from wherever the command came. A command NOBODY TYPED — a
	fields-widget press, the console — also says what it ran, as a ghost in
	the field that fades within a second (`ghost`, da-jprg). A line the
	reader typed does not come back: it is cleared on Enter and stays
	cleared, because a box that refills itself reads as text the reader
	typed and may edit.

	The pills — `-t 14:30`, x-able — which the time cursor fills
	($lib/cursor.ts, da-wnt2) ride inside the prompt just after the typed
	text; a submitted line is what was typed and then the pills' text, as a
	fields-widget press is (`pressWithPills`). A press SPENDS its unlocked
	pills: they leave the live set at once and fade beside the ghost of the
	command they went into (`spent` in $lib/bar.ts), while a locked one
	stays and slides back leftward as the ghost goes. All of that is the
	PROMPT's, not the bar's, since da-3zon: an input widget draws the same
	command line in a pane and the two are one buffer, one set of pills and
	one Enter. A line the fields widget prefills (`prefill`) lands in it,
	focused, unrun — in the widget while one is on screen, else here
	(`focusLine`).

	While storage is degraded the bar's border is the panes' alarm orange
	and its title is the storage sentence: the warning the banner carried
	is not dismissible, because a warning you can switch off is not a
	warning (measured 2026-09-08 — see the old BetaBanner's note).
-->
<script lang="ts">
	import { tick } from 'svelte';
	import BarResult from '$lib/BarResult.svelte';
	import Brand from '$lib/Brand.svelte';
	import { focusLine, last, prefill, text } from '$lib/bar.js';
	import { clockSecondsLabel, now } from '$lib/clock.js';
	import Prompt from '$lib/Prompt.svelte';
	import Repl from '$lib/Repl.svelte';
	import { isDegraded, storage, storageSentence } from '$lib/storageState.js';

	let field: HTMLElement;
	let expanded = false;

	$: if ($prefill !== null) take($prefill);

	/**
	 * The one-shot `prefill` signal is taken HERE because the bar is the one
	 * view always on screen — but where the caret then goes is `focusLine`'s
	 * choice, which is an input widget when the reader has put one on the
	 * page (da-3zon).
	 */
	async function take(line: string): Promise<void> {
		prefill.set(null);
		text.set(line);
		await tick();
		focusLine();
	}

	const focusInput = () => field?.querySelector('input')?.focus();

	/**
	 * Open the REPL over the result tier, or close it again.
	 *
	 * The focus goes back to the prompt either way: the control is a lid,
	 * not a place to be — the line under it stays the page's command line
	 * while the scrollback is open.
	 */
	async function toggle(): Promise<void> {
		expanded = !expanded;
		await tick();
		focusInput();
	}

	$: degraded = isDegraded($storage);

	function onKey(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !expanded) return;
		expanded = false;
		event.preventDefault();
	}
</script>

<svelte:window on:keydown={onKey} />

<footer class="prompt-bar" class:alarm={degraded} title={degraded ? $storageSentence : undefined}>
	{#if expanded}
		<!-- Upward, over the page: the INPUT ROW is the bottom edge, so the
		     result tier is under the scrollback's last entry. -->
		<div class="expansion">
			<Repl />
		</div>
	{/if}

	<div class="brand-block">
		<div class="brand-top">
			<Brand />
		</div>
		<!-- Live, not restored: a command's stamp is history, this one is the
		     clock. On a narrow screen the seconds go and the minutes stay.
		     The whole label is ONE flex item: a `:SS` that were an item of its
		     own would be a block box beside a block box, and `innerText` — and
		     a reader copying the clock — would find a line break in the middle
		     of a time. -->
		<span class="stamp"
			><span class="clock"
				>{clockSecondsLabel($now).slice(0, 5)}<span class="seconds"
					>{clockSecondsLabel($now).slice(5)}</span
				></span
			></span
		>
	</div>

	<div class="tiers">
		<div class="tier top">
			<BarResult result={$last?.result ?? null} />
		</div>
		<div class="tier bottom">
			<div class="prompt" bind:this={field}>
				<Prompt />
			</div>
		</div>
	</div>

	<!-- vividn: "in the center of this input bar, let's put a little ^, that
	     if clicked expands upperwards to show the whole REPL" — at the
	     bar's TOP EDGE (da-bhvy), where it stays whether the REPL is open
	     or shut, and over it, so the way out is where the way in was.

	     Centred, not the full width: the brand's ⓘ and the clock are on
	     the left of this same line and a lid across all of it would take
	     their taps. Two boxes on purpose: the BUTTON is the hit area, a
	     fingertip's 24px (conventions #12) reaching down into the result
	     tier, and the TAB inside it is the paint, pinned to its head, so
	     what hangs over the reader's four lines is a small thing. -->
	<button
		type="button"
		class="caret"
		class:open={expanded}
		aria-label={expanded ? 'close the REPL' : 'open the REPL'}
		aria-expanded={expanded}
		title={expanded ? 'close the REPL' : 'open the REPL'}
		on:click={toggle}><span class="tab"><span class="chevron">^</span></span></button
	>
</footer>

<style>
	.prompt-bar {
		/* See the note above: fixed, and every tier inside it derived from
		   this one number. `tier`, not `band`: the day strip's stacked days
		   are `.band` and a test asking the page for one must not be handed
		   half the bottom bar. */
		--bar: 6rem;
		--input-band: 40%;
		position: relative;
		display: flex;
		align-items: stretch;
		gap: 0.6rem;
		flex-shrink: 0;
		height: var(--bar);
		min-width: 0;
		padding: 0 0.9rem;
		background: var(--ground);
		border-top: 1px solid var(--rule);
	}

	/* Orange rather than red: "your data is not safe yet" is a warning
	   about the future, not a report of a failure. Visible from across a
	   desk (PaneFrame.svelte's alarm). */
	.prompt-bar.alarm {
		border-top-color: var(--alarm);
		box-shadow: 0 -1px 0 var(--alarm) inset;
	}

	/* At least a short name's worth of room even where the clock is only
	   `HH:MM` (a phone), and the same room whatever the name is: fixed, so
	   a database's name never moves the prompt. */
	.brand-block {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		min-width: 4.5rem;
		line-height: 1.1;
	}

	/* The two tiers, on the left too: the brand over the result area's
	   60%, the clock over the input row's 40%, so the clock and the prompt
	   are on one line. */
	.brand-top {
		display: flex;
		flex-direction: column;
		justify-content: center;
		flex: 1 1 auto;
		min-height: 0;
		/* A size container: what is in it — the database's name, which can
		   be any length — does not widen the brand block, and so never takes
		   room from the prompt. The block is as wide as the clock under it
		   makes it, and the name's type is fitted to that (Brand.svelte). */
		container-type: inline-size;
	}

	.stamp {
		display: flex;
		align-items: center;
		flex: 0 0 var(--input-band);
		color: var(--ink-bright);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 1.15rem;
		font-variant-numeric: tabular-nums;
	}

	/* The bar's own two tiers, and everything that is not the brand.
	   `tiers`, not `stack`: the day strip's scroller is `.stack`. */
	.tiers {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.tier {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
	}

	.tier.top {
		flex: 1 1 auto;
		min-height: 0;
		border-bottom: 1px solid var(--tint);
	}

	.tier.top :global(.result) {
		flex: 1;
		height: 100%;
	}

	.tier.bottom {
		flex: 0 0 var(--input-band);
	}

	/* The whole of its tier: nothing else stands on this row since the
	   build stamp went to the root bar (da-wjob). */
	.prompt {
		flex: 1;
		min-width: 0;
	}

	/* The prompt line draws its own rule above itself; here the tier's
	   border is the rule. */
	.prompt :global(.line) {
		border-top: none;
		padding: 0 0.2rem;
	}

	/* On the bar's top line, centred on the whole bar — and above the
	   expansion (z-index 19), which covers everything else on this line
	   while it is open. */
	.caret {
		position: absolute;
		left: 50%;
		top: 0;
		transform: translateX(-50%);
		z-index: 21;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		width: 2.2rem;
		height: 1.5rem;
		padding: 0;
		background: none;
		border: none;
		font: inherit;
		color: var(--quiet);
		cursor: pointer;
	}

	.tab {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 0.9rem;
		background: var(--ground);
		border: 1px solid var(--tint);
		border-top: none;
		border-radius: 0 0 4px 4px;
		font-size: 0.75rem;
		line-height: 1;
	}

	.caret:hover .tab,
	.caret:focus-visible .tab {
		color: var(--accent-bright);
		border-color: var(--accent);
	}

	/* The glyph turns over, not the tab: a rotated tab would put its
	   rounded corners and its open edge at the bottom. */
	.chevron {
		display: inline-block;
		line-height: 1;
	}

	.caret.open .tab {
		color: var(--accent-bright);
	}

	.caret.open .chevron {
		transform: rotate(180deg);
	}

	/* The REPL, opened upward over the page — and over the result tier,
	   whose bottom edge is this one: `--input-band` of the bar's height is
	   the input row, and the tier ends where it begins. So the scrollback's
	   last entry stands exactly where the tier's answer stood. */
	.expansion {
		position: absolute;
		bottom: var(--input-band);
		left: 0;
		right: 0;
		z-index: 19;
		display: flex;
		height: min(60vh, 32rem);
		background: var(--ground);
		border-top: 1px solid var(--rule);
		box-shadow: 0 -8px 24px #00000088;
	}

	.expansion :global(section.pane) {
		flex: 1;
		min-width: 0;
	}

	/* A phone: the prompt keeps the room, and the clock keeps its minutes
	   and drops its seconds. */
	@media (max-width: 480px) {
		.stamp .seconds {
			display: none;
		}
		.prompt-bar {
			gap: 0.35rem;
			padding-inline: 0.4rem;
		}
		.prompt :global(.line) {
			gap: 0.3rem;
			padding-inline: 0;
		}
	}
</style>
