<!--
	The Help place (da-z955f): what the Help button was last asked about,
	explained. It was a sheet of its own (da-35f23); it is a PLACE now, so
	$lib/help.ts lands the answer here: in a pane showing this widget if the
	page has one in view, else wherever `openPlace('help')` hosts it — a tab
	on a phone, a modal on a desktop (da-jozgv).

	The explanation is TEXT, painted as the CLI painted it: a command's is
	`datum help <command>`'s stdout, a widget's or a pill's is plain prose —
	but a GUIDE ($lib/help.ts `GUIDES`), which is a page of its own.

	With nothing explained it shows the welcome's words as its front page,
	and «← front page» goes back to them (vividn, 2026-10-14: «this welcome
	screen could just be the front page of the help widget in general»).
-->
<script lang="ts">
	import { ansiSpans } from '$lib/ansi.js';
	import AnsiText from '$lib/AnsiText.svelte';
	import { explained } from '$lib/help.js';
	import Pane from '$lib/Pane.svelte';
	import SyncGuide from './SyncGuide.svelte';
	import WelcomeText from './WelcomeText.svelte';
</script>

<Pane title="help">
	{#if $explained}
		<button type="button" class="front" on:click={() => explained.set(null)}>← front page</button>
		<h3 class="asked">{$explained.title}</h3>
		{#if $explained.guide === 'sync'}
			<SyncGuide widget={$explained.text} />
		{:else}
			<pre class="help"><AnsiText spans={ansiSpans($explained.text)} /></pre>
		{/if}
	{:else}
		<!-- The front page is the welcome's own words (vividn, 2026-10-14), its «Learn more» links leading here. -->
		<p class="quiet">Press Help, then tap a command, a widget or a pill: it is explained here.</p>
		<WelcomeText />
	{/if}
</Pane>

<style>
	h3 {
		margin: 0 0 0.4rem;
		font-size: 0.8rem;
		font-weight: normal;
		opacity: 0.7;
	}
	/* Wrapped, unlike the REPL's output: this is read, often on a phone,
	   and nothing beside it depends on its columns. */
	.help {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		color: var(--ink-bright);
	}
	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}

	.front {
		margin: 0 0 0.4rem;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-bright);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}
</style>
