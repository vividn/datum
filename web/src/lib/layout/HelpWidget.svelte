<!--
	The Help place (da-z955f): what the Help button was last asked about,
	explained. It was a sheet of its own (da-35f23); it is a PLACE now, so
	$lib/help.ts lands the answer here: in a pane showing this widget if the
	page has one in view, else wherever `openPlace('help')` hosts it — a tab
	on a phone, a modal on a desktop (da-jozgv).

	The explanation is TEXT, painted as the CLI painted it: a widget's or a
	pill's is plain prose, the general help `datum help`'s stdout — but a
	GUIDE ($lib/help.ts `GUIDES`), which is a page of its own. A command's
	or a topic's is its PAGE, `datum help <name> --json` (da-efruz), drawn
	intro first: its usage forms and flag tables fold into <details>, closed
	until opened, and an opened one stays open on every page after, on this
	device. vividn, 2026-10-15: «better intros without getting so bogged down
	by options … the command line options can be in expandable menus for the
	curious or the keyboarders out there».

	With nothing explained it shows the welcome's words as its front page,
	and «← front page» goes back to them (vividn, 2026-10-14: «this welcome
	screen could just be the front page of the help widget in general»).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { ansiSpans } from '$lib/ansi.js';
	import AnsiText from '$lib/AnsiText.svelte';
	import { explained, fitHelp } from '$lib/help.js';
	import { charactersIn } from '$lib/panes.js';
	import Pane from '$lib/Pane.svelte';
	import SyncGuide from './SyncGuide.svelte';
	import WelcomeText from './WelcomeText.svelte';

	/** The headings of the folds this device keeps open: a per-viewer convenience. */
	const KEY = 'datum-help-open';
	let opened: string[] = $state([]);
	try {
		opened = JSON.parse(localStorage.getItem(KEY) ?? '[]');
	} catch {
		opened = [];
	}

	function toggled(heading: string, open: boolean): void {
		opened = open ? [...new Set([...opened, heading])] : opened.filter((h) => h !== heading);
		try {
			localStorage.setItem(KEY, JSON.stringify(opened));
		} catch {
			// No storage: every page starts folded.
		}
	}

	/** The explanation, whose width in characters a help page is asked for at (da-nrjze). */
	let box: HTMLDivElement | undefined = $state();
	let width = $state(0);
	$effect.pre(() => {
		const characters = box && width ? charactersIn(box.querySelector('pre') ?? box, width) : null;
		untrack(() => fitHelp(characters));
	});

	/** `GLOBAL OPTIONS` → `Global options`. */
	const label = (heading: string) => heading.charAt(0) + heading.slice(1).toLowerCase();
</script>

<Pane title="help">
	{#if $explained}
		<div bind:this={box} bind:clientWidth={width}>
			<button type="button" class="front" onclick={() => explained.set(null)}>← front page</button>
			<h3 class="asked">{$explained.title}</h3>
			{#if $explained.guide === 'sync'}
				<SyncGuide widget={$explained.text} />
			{:else if $explained.page}
				{@const page = $explained.page}
				<pre class="help">{page.title}{page.intro ? `\n\n${page.intro}` : ''}</pre>
				{#each page.sections as section, i (i)}
					{#if section.reference}
						<details
							data-section={section.heading}
							open={opened.includes(section.heading)}
							ontoggle={(e) => toggled(section.heading, e.currentTarget.open)}
						>
							<summary>{label(section.heading)}</summary>
							<pre class="section">{section.text}</pre>
						</details>
					{:else}
						<pre class="section">{section.heading ? `${section.heading}:\n` : ''}{section.text}</pre>
					{/if}
				{/each}
			{:else}
				<pre class="help"><AnsiText spans={ansiSpans($explained.text)} /></pre>
			{/if}
		</div>
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
	.help,
	.section {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		color: var(--ink-bright);
	}
	.section {
		margin-top: 0.6rem;
	}
	details {
		margin-top: 0.6rem;
	}
	details .section {
		margin-top: 0.3rem;
	}
	summary {
		color: var(--accent-bright);
		font-size: 0.8rem;
		cursor: pointer;
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
