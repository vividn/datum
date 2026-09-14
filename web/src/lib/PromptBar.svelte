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

	At the left, spanning both, the brand with `beta` and the ⓘ that opens
	what the beta banner used to say — that this is early software, and
	where the entries are (the storage sentence, read off the database that
	actually opened) — and UNDER it the clock, bright, in the input row's
	40% so it lines up with the prompt (vividn: "Move the clock underneath
	the logo, and make it nice and bright, takes up 40% to align with
	command"). Which build this is went to the root bar in da-wjob.

	Centred on the bar, a `^` that expands upward into the whole REPL. It
	and the result area's «(tap to expand)» are ONE expansion with two
	entry points. What it opens is the REPL COMPONENT — the same widget a
	layout can hold, reading the same scrollback store — not a second
	rendering of it. The expansion's REPL does not register with the engine
	($lib/layout/engine.ts): a press still runs wherever it would have.

	THE BAR'S HEIGHT IS FIXED, and that is the point of it: 6rem (96px at
	the browser's default), never changing with what the last command
	answered, so the dashboard above it never reflows. The two tiers are
	57.6px and 38.4px, and 6rem is the SMALLEST number that holds all four
	cases of the result area, which is what fixes it:

	  · 4 raw lines at 0.68rem/1.25 — 54.4px, the deepest case, from a
	    `tail` too long to show whole;
	  · 3 document lines at 0.72rem/1.3 — 44.9px, an entry with data;
	  · 2 at 1rem/1.35 — 43.2px, one without, at the larger size vividn asked
	    for ("the font size could be bigger though when there are only 2
	    lines");
	  · and 38.4px below for the 1.15rem prompt, whose line box is ~22px.

	The «(tap to expand)» costs no line: it sits in the area's corner
	(BarResult.svelte), which is what keeps the deepest case at four.

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
	stays and slides back leftward as the ghost goes. A line the fields
	widget prefills (`prefill`) lands in the field, focused, unrun.

	While storage is degraded the bar's border is the panes' alarm orange
	and its title is the storage sentence: the warning the banner carried
	is not dismissible, because a warning you can switch off is not a
	warning (measured 2026-09-08 — see the old BetaBanner's note).
-->
<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import BarResult from '$lib/BarResult.svelte';
	import {
		GHOST_MS,
		ghost as ghostStore,
		last,
		pills,
		prefill,
		removePill,
		spent,
		toggleLockPill,
		type Ghost
	} from '$lib/bar.js';
	import { clockSecondsLabel, now } from '$lib/clock.js';
	import { pressWithPills, ready } from '$lib/layout/engine.js';
	import Prompt from '$lib/Prompt.svelte';
	import Repl from '$lib/Repl.svelte';
	import { shellSplit } from '$lib/shellSplit.js';
	import { isDegraded, persisted, storage } from '$lib/storageState.js';

	let input = '';
	let field: HTMLElement;
	let hint = false;
	let expanded = false;
	let ghost: Ghost | null = null;
	let fade: ReturnType<typeof setTimeout> | undefined;
	onDestroy(() => clearTimeout(fade));

	$: if ($ghostStore) haunt($ghostStore);
	$: if ($prefill !== null) take($prefill);

	async function take(line: string): Promise<void> {
		prefill.set(null);
		input = line;
		await tick();
		field.querySelector('input')?.focus();
	}

	/**
	 * Show what ran, and take it away again. Nothing here touches `input`:
	 * whatever the reader is halfway through typing is theirs, however slow
	 * the command that landed was.
	 */
	function haunt(next: Ghost): void {
		// A one-shot signal, as `prefill` is: taken, then put back to null,
		// so a bar mounted later does not open onto an old command's ghost.
		ghostStore.set(null);
		clearTimeout(fade);
		ghost = next;
		fade = setTimeout(() => (ghost = null), GHOST_MS);
	}

	async function submit({ detail: typed }: CustomEvent<string>): Promise<void> {
		await pressWithPills(shellSplit(typed), typed);
	}

	$: degraded = isDegraded($storage);

	$: sentence =
		$storage === 'failed'
			? 'The database could not be opened, so nothing you log here will be saved.'
			: $storage === null
				? 'Checking where your entries are stored…'
				: $storage === 'memory'
					? 'Entries are held in memory only — reloading this page discards them.'
					: 'Your entries are stored in this browser only: clearing site data deletes them, there is no sync yet, and ' +
						($persisted
							? 'the browser has promised to keep them.'
							: 'the browser may clear them if space runs short — install the app to keep them.');

	function onKey(event: KeyboardEvent): void {
		if (event.key !== 'Escape') return;
		if (hint) {
			hint = false;
			event.preventDefault();
		} else if (expanded) {
			expanded = false;
			event.preventDefault();
		}
	}
</script>

<svelte:window on:keydown={onKey} />

<footer class="prompt-bar" class:alarm={degraded} title={degraded ? sentence : undefined}>
	{#if expanded}
		<!-- Upward, over the page: the bar is the bottom edge. -->
		<div class="expansion">
			<Repl />
		</div>
	{/if}

	<div class="brand-block">
		<div class="brand-top">
			<a class="brand" href="/">datum<span class="tld">.day</span></a>
			<span class="beta-row">
				<span class="tag">beta</span>
				<button
					type="button"
					class="hint-button"
					aria-label="about datum.day"
					aria-expanded={hint}
					title="about datum.day"
					on:click={() => (hint = !hint)}>ⓘ</button
				>
			</span>
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
		{#if hint}
			<aside class="beta-hint" role="dialog" aria-label="about datum.day">
				<p>datum.day is early software. {sentence}</p>
			</aside>
		{/if}
	</div>

	<div class="tiers">
		<div class="tier top">
			<BarResult result={$last?.result ?? null} on:expand={() => (expanded = true)} />
		</div>
		<div class="tier bottom">
			<div class="prompt" bind:this={field}>
				<Prompt bind:input bind:ghost disabled={!$ready} on:submit={submit}>
					<svelte:fragment slot="pills">
						{#each $pills as pill (pill.id)}
							<span class="pill {pill.kind}" class:locked={pill.locked}
								>{pill.text}<button
									type="button"
									class="lock"
									aria-label="{pill.locked ? 'unlock' : 'lock'} {pill.text}"
									aria-pressed={!!pill.locked}
									on:click={() => toggleLockPill(pill.id)}>{pill.locked ? '🔒' : '🔓'}</button
								><button
									type="button"
									class="remove"
									aria-label="remove {pill.text}"
									on:click={() => removePill(pill.id)}>×</button
								></span
							>
						{/each}
						<!-- Spent, not live: no × and no lock, out of the argv and out
						     of the time cursor, fading with the ghost of the command it
						     went into. A different class for a different thing, so a
						     reader of `.pill` — a test included — is never handed one. -->
						{#each $spent as pill (pill.id)}
							<span class="spent {pill.kind}">{pill.text}</span>
						{/each}
					</svelte:fragment>
				</Prompt>
			</div>
		</div>
	</div>

	<!-- vividn: "in the center of this input bar, let's put a little ^, that
	     if clicked expands upperwards to show the whole REPL". Its bottom
	     edge sits on the line between the tiers, so it is never over the
	     box a finger is aiming at.

	     Two boxes on purpose: the BUTTON is the hit area, which reaches a
	     fingertip's 24px upward under a coarse pointer (conventions #12),
	     and the TAB inside it is the paint, pinned to its foot and the same
	     small thing at every pointer. Growing the paint instead would put
	     an opaque 24px block over the middle of the result area. -->
	<button
		type="button"
		class="caret"
		class:open={expanded}
		aria-label={expanded ? 'close the REPL' : 'open the REPL'}
		aria-expanded={expanded}
		title={expanded ? 'close the REPL' : 'open the REPL'}
		on:click={() => (expanded = !expanded)}
		><span class="tab"><span class="chevron">^</span></span></button
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

	.brand-block {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		min-width: 0;
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

	.brand {
		color: var(--ink-bright);
		text-decoration: none;
		font-size: 1.25rem;
	}

	.tld {
		color: var(--muted);
	}

	.beta-row {
		display: flex;
		align-items: center;
		gap: 0.3rem;
	}

	/* The beta mark, the ⓘ beside it and the note it opens are an amber of
	   their own — #f0b429 and the four shades under it — and not in the
	   palette: they say "this software is early", which is a thing the app
	   says about ITSELF, said once, in a corner. None of them is `--warn`,
	   which means unsaved work. */
	.tag {
		text-transform: uppercase;
		letter-spacing: 0.08em;
		font-size: 0.75rem;
		color: var(--page);
		background: #f0b429;
		border-radius: 3px;
		padding: 0 0.3rem;
	}

	.hint-button {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 0.95rem;
		line-height: 1;
		color: #c0a060;
		cursor: pointer;
	}

	.hint-button:hover,
	.hint-button:focus-visible,
	.hint-button[aria-expanded='true'] {
		color: #f0b429;
	}

	/* Above the bar, over the page: the bar is the bottom edge. */
	.beta-hint {
		position: absolute;
		bottom: calc(100% + 0.6rem);
		left: 0;
		z-index: 20;
		width: max-content;
		max-width: min(28rem, calc(100vw - 2rem));
		padding: 0.5rem 0.75rem;
		background: #2a2010;
		border: 1px solid #5a4418;
		border-radius: 6px;
		box-shadow: 0 4px 16px #00000088;
		font-size: 0.8rem;
		line-height: 1.35;
	}

	.beta-hint p {
		margin: 0;
		color: #e8d5a8;
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

	/* At least half its tier: the prompt is what the bar is for. */
	.prompt {
		flex: 1 0 60%;
		min-width: 0;
	}

	/* The prompt line draws its own rule above itself; here the tier's
	   border is the rule. */
	.prompt :global(.line) {
		border-top: none;
		padding: 0 0.2rem;
	}

	.prompt :global(input),
	.prompt :global(.ghost),
	.prompt :global(.mirror) {
		font-size: 1.15rem;
	}

	.pill,
	.spent {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0 0.2rem 0 0.5rem;
		border: 1px solid var(--accent);
		border-radius: 999px;
		color: var(--accent-bright);
		background: var(--ground);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.95rem;
		white-space: nowrap;
	}

	.spent {
		padding-right: 0.5rem;
		pointer-events: none;
		animation: ghost-fade 1s ease-out forwards;
	}

	@keyframes ghost-fade {
		from {
			opacity: 1;
		}
		to {
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.spent {
			animation: none;
		}
	}

	.pill button {
		background: none;
		border: none;
		padding: 0 0.2rem;
		font: inherit;
		font-size: 0.8rem;
		color: var(--accent);
		cursor: pointer;
	}

	/* A locked pill stands out: it will not clear on its own after a press. */
	.pill.locked {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}

	/* The time cursor's own blue, as its dotted lines are drawn — not a
	   token, because the lines are drawn from $lib/hover.ts as an SVG
	   attribute, where `var()` does not reach, and one colour written in
	   two places is better than a token only one of them can use. */
	.pill.time,
	.spent.time {
		border-color: #7c9cff;
		color: #7c9cff;
	}

	.pill button:hover {
		color: var(--ink-bright);
	}

	/* On the line between the tiers, centred on the whole bar. */
	.caret {
		position: absolute;
		left: 50%;
		bottom: var(--input-band);
		transform: translateX(-50%);
		z-index: 2;
		display: flex;
		align-items: flex-end;
		justify-content: center;
		width: 2.2rem;
		height: 0.9rem;
		padding: 0;
		background: none;
		border: none;
		font: inherit;
		color: var(--quiet);
		cursor: pointer;
	}

	@media (pointer: coarse) {
		.caret {
			height: 1.5rem;
		}
	}

	.tab {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 0.9rem;
		background: var(--ground);
		border: 1px solid var(--tint);
		border-bottom: none;
		border-radius: 4px 4px 0 0;
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

	/* The REPL, opened upward from the bar and over the page. */
	.expansion {
		position: absolute;
		bottom: 100%;
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

	/* A phone: the prompt keeps the room. The brand is `datum` alone with
	   `beta` under it, and the clock keeps its minutes and drops its
	   seconds. */
	@media (max-width: 480px) {
		.tld,
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
