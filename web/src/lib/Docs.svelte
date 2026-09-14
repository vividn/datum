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

	The row's SIZE is one decision for the whole row: three lines and the
	smaller type when any document carries data, because sections standing
	side by side in one fixed band cannot each pick their own.
-->
<script lang="ts">
	import Doc from '$lib/Doc.svelte';
	import type { EchoDoc } from '$lib/echo.js';

	export let docs: EchoDoc[];
	/** Draw every document and wrap, rather than fitting a fixed band. */
	export let all = false;

	/** The narrowest section whose info line still reads, in px. */
	const MIN = 208;
	/** What the `+N` chip takes from the row. */
	const CHIP = 48;

	let row: HTMLDivElement | undefined;
	let sheet: HTMLDialogElement | undefined;
	let width = 0;
	let open = false;

	$: small = docs.some((doc) => doc.data_inline);
	$: capacity = Math.max(1, Math.floor(width / MIN));
	$: over = !all && width > 0 && docs.length > capacity;
	$: shown = all || !over ? docs.length : Math.max(1, Math.floor((width - CHIP) / MIN));

	function measure(node: HTMLDivElement): { destroy(): void } {
		const observer = new ResizeObserver(([entry]) => (width = entry.contentRect.width));
		observer.observe(node);
		return { destroy: () => observer.disconnect() };
	}

	// A real `<dialog>`, as $lib/Confirm.svelte uses: the browser's own
	// modal brings Escape, the backdrop and the focus trap with it, and
	// none of them is a keyboard handler written here a second time.
	$: if (sheet && open && !sheet.open) sheet.showModal();
</script>

<div class="docs" class:all bind:this={row} use:measure>
	{#each docs.slice(0, shown) as doc (doc.hid + doc.action)}
		<Doc {doc} {small} />
	{/each}
	{#if over}
		<button
			type="button"
			class="more"
			title="{docs.length - shown} more"
			on:click={() => (open = true)}>+{docs.length - shown}</button
		>
	{/if}
</div>

{#if open}
	<dialog class="sheet" bind:this={sheet} on:close={() => (open = false)}>
		<div class="sheet-head">
			<span>{docs.length} documents</span>
			<button type="button" class="close" aria-label="close" on:click={() => sheet?.close()}
				>×</button
			>
		</div>
		<div class="sheet-body">
			<svelte:self {docs} all />
		</div>
	</dialog>
{/if}

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

	.sheet {
		width: min(48rem, calc(100vw - 2rem));
		max-height: min(32rem, calc(100vh - 4rem));
		padding: 0;
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 8px;
		box-shadow: 0 8px 32px #000000aa;
	}

	.sheet[open] {
		display: flex;
		flex-direction: column;
	}

	.sheet::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	.sheet-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--tint);
		color: var(--quiet);
	}

	.sheet-body {
		padding: 0.75rem;
		overflow: auto;
	}

	.close {
		background: none;
		border: none;
		padding: 0 0.3rem;
		font: inherit;
		font-size: 1.2rem;
		line-height: 1;
		color: var(--quiet);
		cursor: pointer;
	}

	.close:hover,
	.close:focus-visible {
		color: var(--ink-bright);
	}
</style>
