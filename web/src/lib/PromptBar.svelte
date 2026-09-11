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

	Every command run anywhere shows here as if typed (`last` in
	$lib/bar.ts, told by `record`): the line sits in the field for a beat
	and the receipt stays until the next command. That is the teach loop,
	from wherever the command came.

	Before the prompt is the pill slot — `-t 14:30`, x-able — for the time
	cursor vividn named next. Nothing in the shell sets a pill; a submitted
	line is the pills' text and then what was typed.

	While storage is degraded the bar's border is the panes' alarm orange
	and its title is the storage sentence: the warning the banner carried
	is not dismissible, because a warning you can switch off is not a
	warning (measured 2026-09-08 — see the old BetaBanner's note).
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { last, pills, receiptOf, removePill } from '$lib/bar.js';
	import { press, ready } from '$lib/layout/engine.js';
	import Prompt from '$lib/Prompt.svelte';
	import { shellSplit } from '$lib/shellSplit.js';
	import { isDegraded, persisted, storage } from '$lib/storageState.js';

	/** How long the last command's line stays in the field. */
	const BEAT_MS = 1500;

	let input = '';
	let hint = false;
	let echoed = '';
	let beat: ReturnType<typeof setTimeout> | undefined;
	onDestroy(() => clearTimeout(beat));

	$: receipt = $last ? receiptOf($last.result) : null;
	$: if ($last) show($last.line);

	/** The line, in the field, for a beat — unless someone is typing there. */
	function show(line: string): void {
		if (input !== '' && input !== echoed) return;
		clearTimeout(beat);
		input = echoed = line;
		beat = setTimeout(() => {
			if (input === echoed) input = '';
			echoed = '';
		}, BEAT_MS);
	}

	async function submit({ detail: typed }: CustomEvent<string>): Promise<void> {
		const line = [...$pills.map((p) => p.text), typed].join(' ');
		await press(shellSplit(line));
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
	{#if $pills.length}
		<div class="pills">
			{#each $pills as pill (pill.id)}
				<span class="pill {pill.kind}"
					>{pill.text}<button type="button" aria-label="remove {pill.text}" on:click={() => removePill(pill.id)}
						>×</button
					></span
				>
			{/each}
		</div>
	{/if}
	<div class="prompt">
		<Prompt bind:input disabled={!$ready} on:submit={submit} />
	</div>
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
		padding: 0.2rem 0.6rem;
		background: #0d0d1a;
		border-top: 1px solid #2a2a4a;
	}

	/* Orange rather than red: "your data is not safe yet" is a warning
	   about the future, not a report of a failure. Visible from across a
	   desk (PaneFrame.svelte's alarm). */
	.prompt-bar.alarm {
		border-top-color: #e8820c;
		box-shadow: 0 -1px 0 #e8820c inset;
	}

	.brand-block {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		line-height: 1.1;
	}

	.brand {
		color: #e0e0ff;
		text-decoration: none;
		font-size: 0.95rem;
	}

	.tld {
		color: #55558a;
	}

	.beta-row {
		display: flex;
		align-items: center;
		gap: 0.3rem;
	}

	.tag {
		text-transform: uppercase;
		letter-spacing: 0.08em;
		font-size: 0.65rem;
		color: #0a0a1a;
		background: #f0b429;
		border-radius: 3px;
		padding: 0 0.3rem;
	}

	.hint-button {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 0.8rem;
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
		gap: 0.3rem;
		flex: none;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0 0.2rem 0 0.5rem;
		border: 1px solid #7c7cff;
		border-radius: 999px;
		color: #adadff;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		white-space: nowrap;
	}

	.pill button {
		background: none;
		border: none;
		padding: 0 0.2rem;
		font: inherit;
		color: #7c7cff;
		cursor: pointer;
	}

	.pill button:hover {
		color: #e0e0ff;
	}

	.prompt {
		flex: 1;
		min-width: 0;
	}

	/* The prompt line draws its own rule above itself for the REPL's
	   footer; here the bar is the rule. */
	.prompt :global(.line) {
		border-top: none;
		padding: 0.3rem 0.2rem;
	}

	.receipt {
		flex: 0 1 auto;
		max-width: 30%;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
		color: #8888aa;
	}

	.receipt.error {
		color: #f4645a;
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
