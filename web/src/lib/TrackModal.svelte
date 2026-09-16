<!--
	The web's «which verb?» (da-94q1).

	A field cannot be born without one (da-c8mh), so this is the question
	datum asks whenever a field is declared — and the four pictograms are
	the whole of it. The glyphs are the CLI's own (`VERBS` in
	$lib/track.ts, `track_action_label` in datum-core): the row this modal
	is about to write will print `TRACK●` in every action column there is,
	so what a person presses here is the shape they will meet again.

	A modal rather than the panel that used to hang off the widget bar:
	vividn, 2026-09-15 — «let's make it a modal, and have nice pictograms for
	each of the verbs». The `<dialog>` pattern is Confirm's and Chooser's
	— `showModal` puts focus inside, Escape and a click on the backdrop
	close it and run nothing.

	It is opened from three doors and is the same modal at each: the widget
	bar's `Track +`, a filtered list's `+` with its verb already chosen,
	and the CLI's refusal, which arrives with the field already typed.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import { splitStates, trackArgv, VERBS, type Verb } from './track.js';

	/** The field name it opens with — the CLI's refusal names one. */
	export let field = '';
	/** The verb it opens with, chosen; none unless the door that opened it preset one. */
	export let verb: Verb | null = null;
	/** The ignored fields (`fields --json`), which get no button until they are re-tracked. */
	export let ignored: string[] = [];

	const dispatch = createEventDispatcher<{ close: string[] | null }>();

	let dialog: HTMLDialogElement;
	let states = '';
	let color = '';
	/** The argv to run, set by whatever was pressed; null closes and runs nothing. */
	let chosen: string[] | null = null;

	onMount(() => dialog.showModal());

	// What the name would do if it were submitted, before the press rather
	// than after it: `my coffee` is what a person naturally types and the
	// parser refuses it (da-b049).
	$: hint = field.trim() === '' ? null : fieldNameHint(field.trim());
	$: fixed = hint === null ? null : suggestFieldName(field.trim());
	$: ready = field.trim() !== '' && hint === null && verb !== null;
	// Reactive rather than called from the markup: Svelte tracks the names
	// an expression mentions, so a preview built inside a function would sit
	// at the first argv while the form said otherwise — and a preview that
	// disagrees with the command teaches a lie.
	$: argv = trackArgv(field.trim() || '<field>', verb ?? 'occur', splitStates(states), color.trim());

	function run(next: string[]) {
		chosen = next;
		dialog.close();
	}
</script>

<!-- The backdrop is the dialog's own box outside its content, so a click on
     it lands on the dialog: Chooser's rule, and the pointer's Escape. -->
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
<dialog
	class="track"
	aria-label="track a field"
	bind:this={dialog}
	on:close={() => dispatch('close', chosen)}
	on:click={(e) => e.target === dialog && dialog.close()}
>
	<form method="dialog" on:submit|preventDefault={() => ready && run(argv)}>
		<h2>Track a field</h2>

		<!-- svelte-ignore a11y-autofocus -->
		<input
			class="field"
			bind:value={field}
			placeholder="field"
			aria-label="the field's name"
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
			autofocus
		/>
		{#if hint}
			<p class="hint">
				{hint}{#if fixed}
					<button type="button" class="fix" on:click={() => (field = fixed ?? '')}>use {fixed}</button
					>{/if}
			</p>
		{/if}

		<div class="verbs" role="radiogroup" aria-label="what a press logs">
			{#each VERBS as choice (choice.verb)}
				<button
					type="button"
					class="verb"
					class:on={verb === choice.verb}
					role="radio"
					aria-checked={verb === choice.verb}
					data-verb={choice.verb}
					on:click={() => (verb = choice.verb)}
				>
					<span class="glyph">{choice.glyph}</span>
					<span class="name">{choice.verb}</span>
					<span class="what">{choice.what}</span>
				</button>
			{/each}
		</div>

		{#if verb === 'switch'}
			<input
				class="states"
				bind:value={states}
				placeholder="states, comma-separated"
				aria-label="the states it switches between"
				spellcheck="false"
			/>
		{/if}

		<input
			class="color"
			bind:value={color}
			placeholder="colour (optional) — #rrggbb"
			aria-label="the field's colour"
			spellcheck="false"
			autocapitalize="none"
			autocomplete="off"
		/>

		<div class="buttons">
			<button class="commit" type="submit" disabled={!ready}>track it</button>
			<button type="button" class="cancel" on:click={() => dialog.close()}>cancel</button>
		</div>
		{#if verb !== null}
			<code class="preview">{argv.join(' ')}</code>
		{/if}

		{#if ignored.length > 0}
			<!-- Ignored fields get no button in the bar; this is the way back (da-d08c). -->
			<div class="ignored">
				<span class="label">ignored</span>
				{#each ignored as name (name)}
					<button type="button" data-ignored={name} title="track {name}" on:click={() => run(['track', name])}
						>{name}</button
					>
				{/each}
			</div>
		{/if}
	</form>
</dialog>

<style>
	/* Narrow enough for a 320px phone with the backdrop showing either
	   side, and never taller than the screen: the form scrolls. */
	.track {
		inline-size: min(24rem, calc(100vw - 2rem));
		max-block-size: calc(100dvh - 2rem);
		overflow: auto;
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}

	.track::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	h2 {
		margin: 0;
		font-size: 0.9rem;
		font-weight: normal;
		letter-spacing: 0.06em;
		color: var(--accent);
	}

	input {
		box-sizing: border-box;
		min-block-size: 24px;
		/* The buttons' ground below, and for the same reason. */
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
	}

	/* Two across at every width the modal has: four fingertips, not a list
	   of words with a picture beside each. */
	.verbs {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.4rem;
	}

	.verb {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.1rem;
		box-sizing: border-box;
		min-block-size: 24px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 6px;
		padding: 0.4rem 0.3rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
		text-align: center;
	}

	/* The pictogram is the point of the modal: big enough to be a picture
	   rather than a character that happens to be in front of the word. */
	.glyph {
		font-size: 1.6rem;
		line-height: 1.1;
		color: var(--accent);
	}

	.name {
		font-size: 0.8rem;
		color: var(--ink-bright);
	}

	.what {
		font-size: 0.65rem;
		line-height: 1.2;
		color: var(--muted);
	}

	.verb:hover,
	.verb:focus-visible {
		border-color: var(--accent);
	}

	.verb.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent);
	}

	.buttons {
		display: flex;
		gap: 0.5rem;
	}

	.buttons button,
	.ignored button,
	.fix {
		box-sizing: border-box;
		min-block-size: 24px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.8rem;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}

	.commit {
		border-color: var(--accent);
		color: var(--accent);
	}

	.buttons button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.ignored {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem;
	}

	.ignored button {
		border-radius: 999px;
		padding: 0.2rem 0.55rem;
		font-size: 0.8rem;
	}

	.ignored .label {
		color: var(--muted);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
	}

	.preview {
		color: var(--muted);
		font-size: 0.75rem;
		overflow-wrap: anywhere;
	}

	.hint {
		margin: 0;
		color: var(--quiet);
		font-size: 0.8rem;
		font-style: italic;
	}

	.fix {
		margin-left: 0.4rem;
		font-size: 0.75rem;
		font-style: normal;
	}
</style>
