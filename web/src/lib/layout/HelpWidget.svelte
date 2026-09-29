<!--
	The Help place (da-z955f): what the Help button was last asked about,
	explained. It was a sheet of its own (da-35f23); it is a PLACE now, so
	$lib/help.ts lands the answer here: in a pane showing this widget if the
	page has one in view, else wherever `openPlace('help')` hosts it — a tab
	on a phone, a modal on a desktop (da-jozgv).

	The explanation is TEXT, painted as the CLI painted it: a command's is
	`datum help <command>`'s stdout, a widget's or a pill's is plain prose.
-->
<script lang="ts">
	import { ansiSpans } from '$lib/ansi.js';
	import AnsiText from '$lib/AnsiText.svelte';
	import { explained } from '$lib/help.js';
	import Pane from '$lib/Pane.svelte';
</script>

<Pane title="help">
	{#if $explained}
		<h3 class="asked">{$explained.title}</h3>
		<pre class="help"><AnsiText spans={ansiSpans($explained.text)} /></pre>
	{:else}
		<p class="quiet">Press Help, then tap a command, a widget or a pill: it is explained here.</p>
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
</style>
