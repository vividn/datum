<!--
	A command's documents, side by side (da-xfa4).

	vividn: "if multiple docs are created, divide this upper [60]% into
	different sections to show all the different docs. If sufficiently many
	docs are created, maybe put a plus or ... or something on the end … that
	if clicked opens a modal". So: one section per document, divided, as
	many as the row's measured width holds at a legible section width — and
	a `+N` at the end when they do not all fit. The COUNT, not an `…`: how
	many more there are is the thing worth knowing, and it is the label of
	the button that opens the modal listing every one of them.

	What a section collapses to when only one fits (a 320px phone): a whole
	section, at the same size, with the `+N` beside it — never a half
	section. The minimum below is a width at which the info line still
	reads; the chip is narrow enough to leave that width at 320.

	`all` is the REPL's form: no band to fit, so every document is drawn and
	the row wraps. The modal uses it too — one rendering, three places.

	Every section is the same two lines at the same size, whatever it
	carries: a document's data sits beside its lines rather than under them
	(da-f4mw, Doc.svelte), so there is no longer a size for the row to
	decide. What this file still decides for a section is whether it is wide
	enough for those rows to read — it is the surface that measures — and a
	narrower one draws the brace as the key count alone.
-->
<script lang="ts">
	import { act } from '#lib/act.ts';
	import Doc from '#lib/Doc.svelte';
	import Docs from '#lib/Docs.svelte';
	import type { EchoDoc } from '#lib/echo.ts';
	import Sheet from '#lib/Sheet.svelte';

	interface Props {
		docs: EchoDoc[];
		/** Draw every document and wrap, rather than fitting a fixed band. */
		all?: boolean;
	}

	let { docs, all = false }: Props = $props();

	/** The narrowest section whose info line still reads, in px. */
	const MIN = 208;
	/**
	 * The narrowest section that holds the info line AND readable data rows.
	 *
	 * Measured, not chosen (Doc.svelte): the info line wants 183px and the
	 * brace's braces and chip 71px, so below this the rows are a few pixels
	 * and the count is the honest thing to draw instead.
	 */
	const ROOMY = 320;
	/** What the `+N` chip takes from the row. */
	const CHIP = 48;

	let row: HTMLDivElement | undefined = $state();
	let width = $state(0);
	let open = $state(false);

	const capacity = $derived(Math.max(1, Math.floor(width / MIN)));
	const over = $derived(!all && width > 0 && docs.length > capacity);
	const shown = $derived(all || !over ? docs.length : Math.max(1, Math.floor((width - CHIP) / MIN)));
	// Never in the REPL or the sheet: there is no band to fit there, and a
	// section is at least the 14rem the row wraps at.
	const tight = $derived(!all && width > 0 && width / shown < ROOMY);

	function measure(node: HTMLDivElement): { destroy(): void } {
		const observer = new ResizeObserver(([entry]) => (width = entry.contentRect.width));
		observer.observe(node);
		return { destroy: () => observer.disconnect() };
	}
</script>

<div class="docs" class:all bind:this={row} use:measure>
	{#each docs.slice(0, shown) as doc (doc.hid + doc.action)}
		<Doc {doc} {tight} {all} />
	{/each}
	{#if over}
		<button
			type="button"
			class="more"
			use:act={'open'}
			title="{docs.length - shown} more"
			onclick={() => (open = true)}>+{docs.length - shown}</button
		>
	{/if}
</div>

<Sheet title="{docs.length} documents" bind:open>
	<Docs {docs} all />
</Sheet>

<style>
	.docs {
		display: flex;
		align-items: stretch;
		gap: 0.6rem;
		width: 100%;
		height: 100%;
		min-width: 0;
		overflow: hidden;
	}

	/* Every section takes an equal share and none of them widens past it:
	   `min-width: 0` is what lets the text inside ellipsize instead. */
	.docs > :global(.doc) {
		flex: 1 1 0;
		min-width: 0;
	}

	/* Divided, as vividn asked — a hairline between neighbours, none in front
	   of the first. */
	.docs > :global(.doc + .doc) {
		padding-left: 0.6rem;
		border-left: 1px solid var(--tint);
	}

	.docs.all {
		height: auto;
		flex-wrap: wrap;
		overflow: visible;
	}

	.docs.all > :global(.doc) {
		flex: 1 1 14rem;
	}

	.more {
		flex: none;
		align-self: center;
		padding: 0.1rem 0.4rem;
		background: none;
		border: 1px solid var(--accent);
		border-radius: 999px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		color: var(--accent-bright);
		cursor: pointer;
	}

	.more:hover,
	.more:focus-visible {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}
</style>
