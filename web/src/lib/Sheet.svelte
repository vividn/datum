<!--
	A modal sheet, as the `+N` chips open one (da-f4mw).

	A real `<dialog>`, as #lib/Confirm.svelte uses: the browser's own modal
	brings Escape, the backdrop and the focus trap with it, and none of
	them is a keyboard handler written here a second time. A tap on the
	backdrop closes it too, which the browser's modal does not: the tap
	lands on the dialog itself, since the head and body fill the rest of
	it — and only a press that STARTED there counts, so a text selection
	dragged out of the body does not close the sheet under it.

	Two chips open one — the documents' `+N`, which lists every document
	(#lib/Docs.svelte), and a document's own, which lists every key of its
	data (#lib/Doc.svelte) — and the second reads as the first because it
	is the same sheet and not a second one built to look like it.

	A PLACE on a desktop opens in one too (da-jozgv, layout/PlaceModal.svelte):
	`widget` sizes it for a widget's box, the `head` slot carries its
	`Open as tab` beside the ×, and `onclose` tells the owner when it closes
	by any road (the ×, Escape, the backdrop), since a place's history entry
	has to go with it.

	The setup place's opens `beside` the bar (da-njt0f), vividn, 2026-11-12:
	«Bar live over setup». A modal makes the rest of the page inert, the
	bar with it, and setup's commands are the bar's palette's (da-r674m);
	so that sheet is shown, not shown modal, over a veil that stops where
	the bar begins (`--bar-reach` on the shell). Escape inside it, the
	veil and the × close it as the modal's do; the bar's keys stay the
	bar's. Every other sheet stays modal.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		open?: boolean;
		/** Sized for a widget, which fills the body, rather than for a list. */
		widget?: boolean;
		/** Not modal: the command bar stays live beside it, outside its veil (da-njt0f). */
		beside?: boolean;
		/** Told when the sheet closes, however: the ×, Escape, the backdrop. */
		onclose?: (() => void) | undefined;
		head?: Snippet;
		children?: Snippet;
		[key: string]: unknown;
	}

	let {
		title,
		open = $bindable(false),
		widget = false,
		beside = false,
		onclose = undefined,
		head,
		children,
		...rest
	}: Props = $props();

	/** Escape in a modal is the browser's; beside the bar, the sheet's own, and not a dialog's inside it, nor the bar's. */
	function onkeydown(e: KeyboardEvent): void {
		const at = e.target as Element;
		if (!beside || e.key !== 'Escape' || e.defaultPrevented || (at !== document.body && at.closest('dialog') !== sheet)) return;
		e.preventDefault();
		sheet?.close();
	}

	let sheet: HTMLDialogElement | undefined = $state();
	let outside = $state(false);

	$effect.pre(() => {
		if (sheet && open && !sheet.open) beside ? sheet.show() : sheet.showModal();
	});
</script>

<svelte:window onkeydown={(e) => open && e.target === document.body && onkeydown(e)} />

{#if open}
	{#if beside}
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="veil" onclick={() => sheet?.close()}></div>
	{/if}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
	<dialog
		class="sheet"
		class:widget
		class:by-bar={beside}
		tabindex={beside ? -1 : undefined}
		{onkeydown}
		{...rest}
		bind:this={sheet}
		onclose={() => ((open = false), onclose?.())}
		onpointerdown={(e) => (outside = e.target === sheet)}
		onclick={(e) => outside && e.target === sheet && sheet?.close()}
	>
		<div class="sheet-head">
			<span>{title}</span>
			{@render head?.()}
			<button type="button" class="close" aria-label="close" onclick={() => sheet?.close()}
				>×</button
			>
		</div>
		<div class="sheet-body">
			{@render children?.()}
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

	.sheet.widget {
		height: min(40rem, calc(100vh - 4rem));
		max-height: none;
	}

	.sheet.widget .sheet-body {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}

	/* Beside the bar: placed as a modal is, over the page above the bar. */
	.sheet.by-bar {
		position: fixed;
		inset: 2rem 0 calc(var(--bar-reach, 0px) + 1rem);
		z-index: 31;
		margin: auto;
		height: auto;
		max-height: 40rem;
	}

	.sheet.by-bar:focus {
		outline: none;
	}

	.sheet::backdrop,
	.veil {
		background: rgb(0 0 0 / 0.6);
	}

	.veil {
		position: fixed;
		inset: 0 0 var(--bar-reach, 0px);
		z-index: 30;
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

	.sheet-head > span {
		margin-inline-end: auto;
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
