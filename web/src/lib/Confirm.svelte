<!--
	The web's answer to a CLI prompt that wants a TTY the browser has not
	got: `db purge`'s database name (da-podv), `kit install`'s trust
	(da-axrk), a stale `_LAST`'s question (da-f5pg2) — the CLI's own text,
	painted and spaced as it printed it. `onclose` hears whether the `action`
	button was pressed — with `name` typed exactly, when there is one to
	type. Escape, cancel or a wrong name are all no. A question with more
	answers than one, a chained purge's leave, rejoin or chain (da-0hay9), puts
	the rest in `also`, and `onchose` hears which was pressed, or null.
	`danger` names the button that destroys, drawn in `--error`: the action
	for a confirm, the cancel for a choice whose way out throws work away
	(layout mode's Discard changes), or none, and every action is the accent.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { act, type ActKind } from './act.js';
	import { ansiSpans } from './ansi.js';
	import AnsiText from './AnsiText.svelte';

	interface Props {
		text: string;
		action: string;
		also?: string[];
		name?: string;
		cancel?: string;
		kind?: ActKind;
		danger?: 'action' | 'cancel' | null;
		onclose?: (yes: boolean) => void;
		onchose?: (pressed: string | null) => void;
	}

	let {
		text,
		action,
		also = [],
		name = '',
		cancel = 'cancel',
		kind = 'run',
		danger = 'action',
		onclose,
		onchose
	}: Props = $props();

	let dialog: HTMLDialogElement;
	let typed = $state('');

	onMount(() => dialog.showModal());
</script>

<dialog
	class="confirm"
	class:danger
	style:--act={danger === 'action' ? 'var(--error)' : 'var(--accent)'}
	bind:this={dialog}
	onclose={() => {
		const pressed = typed === name && [action, ...also].includes(dialog.returnValue) ? dialog.returnValue : null;
		onchose?.(pressed);
		onclose?.(pressed === action);
	}}
>
	<form method="dialog">
		<p><AnsiText spans={ansiSpans(text)} /></p>
		{#if name}
			<label>Type <code>{name}</code> to confirm {action}: <input bind:value={typed} spellcheck="false" autocapitalize="none" autocomplete="off" /></label>
		{/if}
		<div class="buttons">
			{#each [action, ...also] as a}
				<button value={a} use:act={kind}>{a}</button>
			{/each}
			<button type="button" class:danger={danger === 'cancel'} onclick={() => dialog.close()}>{cancel}</button>
		</div>
	</form>
</dialog>

<style>
	.confirm {
		max-width: min(26rem, calc(100vw - 2rem));
		background: var(--ground);
		color: var(--ink-bright);
		border: 1px solid var(--accent);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.9rem;
	}

	.confirm.danger {
		border-color: var(--error);
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
		background: var(--raised);
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
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: inherit;
		font: inherit;
		padding: 0.2rem 0.8rem;
	}

	/* The action runs at once, so the double outline (#lib/act.ts). */
	button:global([data-act='run']) {
		border: 3px double var(--act);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
	}

	button[value] {
		border-color: var(--act);
		color: var(--act);
	}

	button.danger {
		/* `--error` on `--raised` is under 4.5:1 on a yellow database. */
		background: var(--ground);
		border-color: var(--error);
		color: var(--error);
	}
</style>
