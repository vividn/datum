<!--
	The app's own question with a typed answer (da-y992) — what the browser's
	`prompt()` was for, drawn by the app. vividn on PR #25: "lets use our own
	dialog" for `new database…`.

	A sibling of Confirm.svelte rather than a mode of it: Confirm is the
	dialog for an act that DESTROYS or trusts (its border and its button are
	`--error`), and a name for something new is neither. The same `<dialog>`
	underneath, so the browser does what it already does well: `showModal`
	puts focus in the field, Enter in the field submits the form, Escape
	cancels, and closing hands focus back to whatever held it before — the
	control the question was asked from.

	`close` carries the trimmed answer, or null for Escape, cancel or an
	empty field. An empty answer is refused QUIETLY: the button that would
	take it is disabled, so Enter on an empty field does nothing at all.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';

	export let text: string;
	export let action: string;

	const OK = 'ok';
	const dispatch = createEventDispatcher<{ close: string | null }>();
	let dialog: HTMLDialogElement;
	let typed = '';

	$: answer = typed.trim();

	onMount(() => dialog.showModal());
</script>

<dialog
	class="ask"
	bind:this={dialog}
	on:close={() => dispatch('close', dialog.returnValue === OK && answer ? answer : null)}
>
	<form method="dialog">
		<label>
			<span>{text}</span>
			<input bind:value={typed} spellcheck="false" autocapitalize="none" autocomplete="off" />
		</label>
		<div class="buttons">
			<button value={OK} disabled={!answer}>{action}</button>
			<button type="button" on:click={() => dialog.close()}>cancel</button>
		</div>
	</form>
</dialog>

<style>
	.ask {
		max-width: min(26rem, calc(100vw - 2rem));
		background: var(--ground);
		color: var(--ink-bright);
		border: 1px solid var(--accent);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9rem;
	}

	.ask::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	label span {
		display: block;
	}

	input,
	button {
		background: var(--tint);
		border: 1px solid var(--rule);
		color: inherit;
		font: inherit;
	}

	input {
		display: block;
		width: 100%;
		box-sizing: border-box;
		margin-top: 0.4rem;
	}

	input:focus-visible {
		outline: 1px solid var(--accent);
	}

	.buttons {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.75rem;
	}

	button {
		border-radius: 4px;
		padding: 0.2rem 0.8rem;
		cursor: pointer;
	}

	button[value] {
		border-color: var(--accent);
		color: var(--accent);
	}

	button:disabled {
		border-color: var(--rule);
		color: var(--quiet);
		cursor: default;
	}
</style>
