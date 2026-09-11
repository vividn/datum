<!--
	The web's `db purge` guard (da-podv). The CLI asks for the database's
	name on a TTY and refuses without one; the browser has none, so this
	asks instead. `close` says whether the purge button was pressed with
	the name typed exactly — Escape, cancel or a wrong name are all no.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';

	export let name: string;

	const dispatch = createEventDispatcher<{ close: boolean }>();
	let dialog: HTMLDialogElement;
	let typed = '';

	onMount(() => dialog.showModal());
</script>

<dialog
	class="confirm"
	bind:this={dialog}
	on:close={() => dispatch('close', dialog.returnValue === 'purge' && typed === name)}
>
	<form method="dialog">
		<p>This will destroy ALL data in <code>{name}</code>.</p>
		<label>Type <code>{name}</code> to confirm purge: <input bind:value={typed} spellcheck="false" autocapitalize="none" autocomplete="off" /></label>
		<div class="buttons">
			<button value="purge">purge</button>
			<button type="button" on:click={() => dialog.close()}>cancel</button>
		</div>
	</form>
</dialog>

<style>
	.confirm {
		max-width: min(26rem, calc(100vw - 2rem));
		background: #0d0d1a;
		color: #e0e0ff;
		border: 1px solid #f4645a;
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9rem;
	}

	.confirm::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	p {
		margin: 0 0 0.75rem;
	}

	input {
		display: block;
		width: 100%;
		box-sizing: border-box;
		margin-top: 0.4rem;
		background: #1a1a30;
		border: 1px solid #2a2a4a;
		color: inherit;
		font: inherit;
	}

	.buttons {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.75rem;
	}

	button {
		background: #1a1a30;
		border: 1px solid #2a2a4a;
		border-radius: 4px;
		color: inherit;
		font: inherit;
		padding: 0.2rem 0.8rem;
	}

	button[value='purge'] {
		border-color: #f4645a;
		color: #f4645a;
	}
</style>
