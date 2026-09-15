<!--
	A modal sheet, as the `+N` chips open one (da-f4mw).

	A real `<dialog>`, as $lib/Confirm.svelte uses: the browser's own modal
	brings Escape, the backdrop and the focus trap with it, and none of
	them is a keyboard handler written here a second time.

	Two chips open one — the documents' `+N`, which lists every document
	($lib/Docs.svelte), and a document's own, which lists every key of its
	data ($lib/Doc.svelte) — and the second reads as the first because it
	is the same sheet and not a second one built to look like it.
-->
<script lang="ts">
	export let title: string;
	export let open = false;

	let sheet: HTMLDialogElement | undefined;

	$: if (sheet && open && !sheet.open) sheet.showModal();
</script>

{#if open}
	<dialog class="sheet" bind:this={sheet} on:close={() => (open = false)}>
		<div class="sheet-head">
			<span>{title}</span>
			<button type="button" class="close" aria-label="close" on:click={() => sheet?.close()}
				>×</button
			>
		</div>
		<div class="sheet-body">
			<slot />
		</div>
	</dialog>
{/if}

<style>
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
