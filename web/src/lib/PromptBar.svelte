<!--
	The bottom bar (da-0a5o): the one fixed thing on the screen.

	vividn: "At the very bottom, let's put an input bar (like the one on the
	REPL). This can be fixed there and always available to click and enter
	in data or type some other command." So: at the left the brand, with
	`beta` under it and a hint that opens what the beta banner used to say
	— that this is early software, and where the entries are (the
	storage sentence, read off the database that actually opened, the
	`storage` and `persisted` stores); in the middle the prompt line every
	prompt on the page shares (Prompt.svelte: the history Up walks is the
	REPL's); at the right the receipt — the CLI's own first line, cut to
	its action word and hid — and, dim, which build this is.

	It is the page's one command line and its status line, so it stands
	out (da-rfs9, vividn: "Make the bottom input bar quite a bit bigger, it
	needs to stand out, it is the main input and also status bar for the
	whole app"): about 64px tall where it was 37, the prompt at 1.15rem.
	The REPL has no prompt of its own any more; what is typed here lands
	in its scrollback.

	Every command run anywhere shows here (`last` in $lib/bar.ts, told by
	`record`): the receipt stays until the next command. That is the teach
	loop, from wherever the command came. A command NOBODY TYPED — a
	fields-widget press, the console — also says what it ran, as a ghost in
	the field that fades within a second (`ghost`, da-jprg). A line the
	reader typed does not come back: it is cleared on Enter and stays
	cleared, because a box that refills itself reads as text the reader
	typed and may edit.

	Before the prompt is the pill slot — `-t 14:30`, x-able — which the time
	cursor fills ($lib/cursor.ts, da-wnt2) and which stays for the next
	command; a submitted line is what was typed and then the pills' text,
	as a fields-widget press is (`pressWithPills`). A line the fields
	widget prefills (`prefill`) lands in the field, focused, unrun.

	While storage is degraded the bar's border is the panes' alarm orange
	and its title is the storage sentence: the warning the banner carried
	is not dismissible, because a warning you can switch off is not a
	warning (measured 2026-09-08 — see the old BetaBanner's note).
-->
<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import {
		ghost as ghostStore,
		last,
		pills,
		prefill,
		receiptOf,
		removePill,
		toggleLockPill,
		type Ghost
	} from '$lib/bar.js';
	import { pressWithPills, ready } from '$lib/layout/engine.js';
	import Prompt from '$lib/Prompt.svelte';
	import { shellSplit } from '$lib/shellSplit.js';
	import { isDegraded, persisted, storage } from '$lib/storageState.js';

	/** How long a ghost is on screen — the fade in Prompt.svelte's own CSS. */
	const GHOST_MS = 1000;

	let input = '';
	let field: HTMLElement;
	let hint = false;
	let ghost: Ghost | null = null;
	let fade: ReturnType<typeof setTimeout> | undefined;
	onDestroy(() => clearTimeout(fade));

	$: receipt = $last ? receiptOf($last.result) : null;
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
		if (event.key === 'Escape' && hint) {
			hint = false;
			event.preventDefault();
		}
	}
</script>

<svelte:window on:keydown={onKey} />

<footer class="prompt-bar" class:alarm={degraded} title={degraded ? sentence : undefined}>
	<div class="brand-block">
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
		{#if hint}
			<aside class="beta-hint" role="dialog" aria-label="about datum.day">
				<p>datum.day is early software. {sentence}</p>
			</aside>
		{/if}
	</div>
	<div class="prompt" bind:this={field}>
		<Prompt bind:input bind:ghost disabled={!$ready} on:submit={submit} />
	</div>
	{#if $pills.length}
		<div class="pills">
			{#each $pills as pill (pill.id)}
				<span class="pill {pill.kind}" class:locked={pill.locked}
					>{pill.text}<button
						type="button"
						class="lock"
						aria-label="{pill.locked ? 'unlock' : 'lock'} {pill.text}"
						aria-pressed={!!pill.locked}
						on:click={() => toggleLockPill(pill.id)}>{pill.locked ? '🔒' : '🔓'}</button
					><button type="button" class="remove" aria-label="remove {pill.text}" on:click={() => removePill(pill.id)}
						>×</button
					></span
				>
			{/each}
		</div>
	{/if}
	<span class="receipt" class:error={receipt?.error} role="status" title={receipt?.text}
		>{receipt?.text ?? ''}</span
	>
	<!-- Which build is in front of you: a developer's sanity check, dim and last. -->
	<span class="build" title="datum.day {__DATUM_VERSION__} · commit {__DATUM_COMMIT__}"
		>{__DATUM_VERSION__} · {__DATUM_COMMIT__}</span
	>
</footer>

<style>
	.prompt-bar {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
		min-width: 0;
		padding: 0.55rem 0.9rem;
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
		line-height: 1.1;
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

	.pills {
		display: flex;
		justify-content: center;
		gap: 0.3rem;
		flex: none;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0 0.2rem 0 0.5rem;
		border: 1px solid var(--accent);
		border-radius: 999px;
		color: var(--accent-bright);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.95rem;
		white-space: nowrap;
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
	.pill.time {
		border-color: #7c9cff;
		color: #7c9cff;
	}

	.pill button:hover {
		color: var(--ink-bright);
	}

	/* At least half the bar: the receipt and the build stamp ellipsize
	   before the line typed into gives up its room. */
	.prompt {
		flex: 1 0 50%;
		min-width: 0;
	}

	/* The prompt line draws its own rule above itself for the input
	   widget; here the bar is the rule. */
	.prompt :global(.line) {
		border-top: none;
		padding: 0.7rem 0.2rem;
	}

	.prompt :global(input),
	.prompt :global(.ghost) {
		font-size: 1.15rem;
	}

	.receipt {
		flex: 0 1 auto;
		max-width: 30%;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 1rem;
		/* Not a token: the same step between `--quiet` and `--ink` the
		   echoed line uses (Input.svelte). */
		color: #8888aa;
	}

	.receipt.error {
		color: var(--error);
	}

	/* Dim on purpose: present for whoever is looking for it. */
	.build {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.7rem;
		font-variant-numeric: tabular-nums;
		color: color-mix(in srgb, currentColor 35%, transparent);
		white-space: nowrap;
	}

	/* A phone: the prompt keeps the room. The build stamp goes, the brand
	   is `datum` alone with `beta` under it, the receipt takes less. */
	@media (max-width: 480px) {
		.build,
		.tld {
			display: none;
		}
		.prompt-bar {
			gap: 0.35rem;
			padding-inline: 0.4rem;
		}
		.receipt {
			max-width: 22%;
		}
		.prompt :global(.line) {
			gap: 0.3rem;
			padding-inline: 0;
		}
	}
</style>
