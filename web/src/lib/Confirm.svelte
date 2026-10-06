<!--
	The web's answer to a CLI prompt that wants a TTY the browser has not
	got: `db purge`'s database name (da-podv), `kit install`'s trust
	(da-axrk), a stale `_LAST`'s question (da-f5pg2) — the CLI's own text,
	painted and spaced as it printed it. `close` says whether the `action`
	button was pressed — with `name` typed exactly, when there is one to
	type. Escape, cancel or a wrong name are all no.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { act } from './act.js';
	import { ansiSpans } from './ansi.js';
	import AnsiText from './AnsiText.svelte';

	export let text: string;
	export let action: string;
	export let name = '';

	const dispatch = createEventDispatcher<{ close: boolean }>();
	let dialog: HTMLDialogElement;
	let typed = '';

	onMount(() => dialog.showModal());
</script>

<dialog
	class="confirm"
	bind:this={dialog}
	on:close={() => dispatch('close', dialog.returnValue === action && typed === name)}
>
	<form method="dialog">
		<p><AnsiText spans={ansiSpans(text)} /></p>
		{#if name}
			<label>Type <code>{name}</code> to confirm {action}: <input bind:value={typed} spellcheck="false" autocapitalize="none" autocomplete="off" /></label>
		{/if}
		<div class="buttons">
			<button value={action} use:act={'run'}>{action}</button>
			<button type="button" on:click={() => dialog.close()}>cancel</button>
		</div>
	</form>
</dialog>

<style>
	.confirm {
		max-width: min(26rem, calc(100vw - 2rem));
		background: var(--ground);
		color: var(--ink-bright);
		border: 1px solid var(--error);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9rem;
	}

	.confirm::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	p {
		margin: 0 0 0.75rem;
		white-space: pre-wrap;
	}

	input {
		display: block;
		width: 100%;
		box-sizing: border-box;
		margin-top: 0.4rem;
		/* The buttons' ground below, and for the same reason. */
		background: #1a1a30;
		border: 1px solid var(--rule);
		color: inherit;
		font: inherit;
	}

	.buttons {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.75rem;
	}

	button {
		/* Not a token: a step between `--ground` and `--tint` that only the
		   two buttons in this dialog stand on. */
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: inherit;
		font: inherit;
		padding: 0.2rem 0.8rem;
	}

	/* The action runs at once, so the double outline ($lib/act.ts). */
	button:global([data-act='run']) {
		border: 3px double var(--error);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
	}

	button[value] {
		border-color: var(--error);
		color: var(--error);
	}
</style>
