<!--
	A guide in the Help place (da-oefd0, #lib/guides.ts): a widget's, from its
	registry entry, or the page's, the general help. vividn, 2026-11-05: «Each
	widget should carry with it in the code how to use it, and document all of
	its features». Drawn from its Markdown by #lib/markdown.ts, as the docs
	widget draws a doc.

	The page's leads with its first paragraph and then `datum help`, one line
	down, for the commands. A kind naming `commands` folds that CLI page at the
	end, closed, for a terminal (the sync guide's, da-oyvsz).

	A link to a place is a button (`guideHtml`), which a press opens
	(`openPlace`, da-7s7xc).
-->
<script lang="ts">
	import { act } from '#lib/act.ts';
	import { openPlace } from './shell.js';
	import { ansiSpans } from '#lib/ansi.ts';
	import AnsiText from '#lib/AnsiText.svelte';
	import { helpArgv } from '#lib/calls.ts';
	import { PAGE, guideHtml, guideOf } from '#lib/guides.ts';
	import { explainCommand } from '#lib/help.ts';
	import { run } from '#lib/run.ts';

	let { name }: { name: string } = $props();

	const guide = $derived(guideOf(name));
	const blocks = $derived((guide?.text ?? '').split(/\n\s*\n/));
	const lead = $derived(guideHtml(blocks[0] ?? ''));
	const rest = $derived(guideHtml(blocks.slice(1).join('\n\n')));

	let body: HTMLElement;
	$effect(() => {
		void [lead, rest];
		for (const button of body.querySelectorAll<HTMLButtonElement>('button[data-place]')) {
			act(button, 'open');
			button.onclick = () => void openPlace(button.dataset.place!);
		}
	});

	let reference = $state('');
	$effect(() => {
		const commands = guide?.commands;
		reference = '';
		if (commands) void run(helpArgv(commands)).then(({ stdout, stderr }) => (reference = stdout || stderr));
	});
</script>

<div class="guide" data-guide={name} bind:this={body}>
	{@html lead}
	{#if name === PAGE}
		<p class="commands">
			The commands, for typing in the bar:
			<button type="button" class="cli-help" use:act={'show'} onclick={() => void explainCommand()}>datum help</button>
		</p>
	{/if}
	{@html rest}
	{#if guide?.commands}
		<details class="reference">
			<summary>The {guide.commands} commands, for a terminal</summary>
			<pre><AnsiText spans={ansiSpans(reference)} /></pre>
		</details>
	{/if}
</div>

<style>
	/* Read like the welcome, whose words are the Help place's front page. */
	.guide {
		max-inline-size: 34rem;
		color: var(--ink);
		line-height: 1.45;
	}
	.guide :global(h2),
	.guide :global(h3) {
		margin: 1rem 0 0.4rem;
		color: var(--ink-bright);
		font-size: 0.85rem;
	}
	.guide :global(p),
	.guide :global(ul),
	.guide :global(ol) {
		margin: 0 0 0.5rem;
	}
	.guide :global(ul),
	.guide :global(ol) {
		padding-inline-start: 1.4rem;
	}
	.guide :global(strong) {
		color: var(--ink-bright);
	}
	.guide :global(code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9em;
		color: var(--ink-bright);
	}
	.cli-help {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-bright);
		font: inherit;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		cursor: pointer;
	}
	summary {
		color: var(--accent-bright);
		cursor: pointer;
	}
	pre {
		margin: 0.4rem 0 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-size: 0.8rem;
		color: var(--ink-bright);
	}
</style>
