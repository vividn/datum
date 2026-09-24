<!--
	The web's «which verb?» (da-94q1), and the rest of what a field is
	(da-ci85o): ONE modal for declaring a field and for editing its spec.
	vividn, 2026-09-23 — «Let's redesign the spec modal (the one that pops up
	with track as well), they should be very similar, maybe even the same.»

	A field cannot be born without a verb (da-c8mh), so this is the question
	datum asks whenever a field is declared — and the four pictograms are
	the whole of it. The glyphs are the CLI's own (`VERBS` in
	$lib/track.ts, `track_action_label` in datum-core): the row this modal
	is about to write will print `TRACK●` in every action column there is,
	so what a person presses here is the shape they will meet again.

	Segmented: Basics · States · Keys · Aliases. Track is this modal opened
	on Basics, and that is all it needs — name, pictogram, Done, the same
	taps it always took. Keys and Aliases are their own beads (da-cap5m).

	What it writes is decided by what it was opened on. A new field gets
	`track`, the one command that writes the TRACK document, and so only
	what `track` can carry is asked (no unit, no descriptions). A field with
	a spec gets the whole edited spec back in ONE `spec set --from-json` —
	the `set` shape, the list entire, never a stream of `add-state`s.

	It is opened from three doors to declare and is the same modal at each:
	the widget bar's `Track +`, a filtered list's `+` with its verb already
	chosen, and the CLI's refusal, which arrives with the field already
	typed. The `<dialog>` pattern is Confirm's and Chooser's — `showModal`
	puts focus inside, Escape and a click on the backdrop close it and run
	nothing.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import Swatch from './Swatch.svelte';
	import {
		editedSpec,
		specArgv,
		stateRows,
		trackArgv,
		VERBS,
		type Spec,
		type StateRow,
		type Verb
	} from './track.js';

	/** The field name it opens with — the CLI's refusal names one. */
	export let field = '';
	/** The verb it opens with, chosen; none unless the door that opened it preset one. */
	export let verb: Verb | null = null;
	/** The ignored fields (`fields --json`), which get no button until they are re-tracked. */
	export let ignored: string[] = [];
	/** The spec to edit (`spec show`); null declares a new field instead. */
	export let spec: Spec | null = null;

	const dispatch = createEventDispatcher<{ close: string[] | null }>();
	const TABS = ['basics', 'states', 'keys', 'aliases'] as const;

	let dialog: HTMLDialogElement;
	let tab: (typeof TABS)[number] = 'basics';
	let color = spec?.color ?? '';
	let unit = spec?.unit ?? '';
	let description = spec?.description ?? '';
	let states: StateRow[] = spec ? stateRows(spec) : [];
	if (spec) verb = spec.verb;
	/** The argv to run, set by whatever was pressed; null closes and runs nothing. */
	let chosen: string[] | null = null;

	onMount(() => dialog.showModal());

	// What the name would do if it were submitted, before the press rather
	// than after it: `my coffee` is what a person naturally types and the
	// parser refuses it (da-b049).
	$: hint = spec || field.trim() === '' ? null : fieldNameHint(field.trim());
	$: fixed = hint === null ? null : suggestFieldName(field.trim());
	$: ready = field.trim() !== '' && hint === null && verb !== null;
	// Reactive rather than called from the markup: Svelte tracks the names
	// an expression mentions, so a preview built inside a function would sit
	// at the first argv while the form said otherwise — and a preview that
	// disagrees with the command teaches a lie.
	$: argv = spec
		? specArgv(editedSpec(spec, { verb: verb ?? spec.verb, color, unit, description, states }))
		: trackArgv(field.trim() || '<field>', verb ?? 'occur', states, color);
	// A new field's states are a switch's: `track` carries them for no other verb.
	$: statesOpen = spec !== null || verb === 'switch';

	function run(next: string[]) {
		chosen = next;
		dialog.close();
	}
</script>

<!-- The backdrop is the dialog's own box outside its content, so a click on
     it lands on the dialog: Chooser's rule, and the pointer's Escape. -->
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
<dialog
	class="spec"
	aria-label={spec ? `the spec of ${field}` : 'track a field'}
	bind:this={dialog}
	on:close={() => dispatch('close', chosen)}
	on:click={(e) => e.target === dialog && dialog.close()}
>
	<form method="dialog" on:submit|preventDefault={() => ready && run(argv)}>
		<header>
			{#if spec}
				<h2>{field}</h2>
			{:else}
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
			{/if}
			<button type="button" class="close" aria-label="close" on:click={() => dialog.close()}>✕</button>
		</header>
		{#if hint}
			<p class="hint">
				{hint}{#if fixed}
					<button type="button" class="fix" on:click={() => (field = fixed ?? '')}>use {fixed}</button
					>{/if}
			</p>
		{/if}

		<div class="tabs" role="tablist">
			{#each TABS as name (name)}
				<button
					type="button"
					role="tab"
					data-tab={name}
					aria-selected={tab === name}
					disabled={name === 'states' && !statesOpen}
					on:click={() => (tab = name)}>{name}</button
				>
			{/each}
		</div>

		{#if tab === 'basics'}
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

			<div class="row top">
				<span class="label">colour</span>
				<Swatch bind:value={color} label="the field's colour" />
			</div>
			{#if spec}
				<label class="row">
					<span class="label">unit</span>
					<input class="unit" bind:value={unit} spellcheck="false" autocapitalize="none" />
				</label>
				<label class="row">
					<span class="label">description</span>
					<input class="description" bind:value={description} />
				</label>
			{/if}
		{:else if tab === 'states'}
			<ul class="states">
				{#each states as state, i (state)}
					<li class="state">
						<Swatch bind:value={state.color} label="the colour of {state.name || 'this state'}" />
						<input
							class="name"
							bind:value={state.name}
							placeholder="state"
							aria-label="the state's name"
							spellcheck="false"
							autocapitalize="none"
						/>
						<button
							type="button"
							class="remove"
							aria-label="remove {state.name}"
							on:click={() => (states = states.filter((_, j) => j !== i))}>✕</button
						>
						{#if spec}
							<input
								class="description"
								bind:value={state.description}
								placeholder="what it means"
								aria-label="what {state.name || 'this state'} means"
							/>
						{/if}
					</li>
				{/each}
			</ul>
			<button
				type="button"
				class="add"
				on:click={() => (states = [...states, { name: '', color: '', description: '' }])}
				>+ state</button
			>
		{:else}
			<p class="later">not yet</p>
		{/if}

		<div class="buttons">
			<button type="button" class="cancel" on:click={() => dialog.close()}>Cancel</button>
			<button class="commit" type="submit" disabled={!ready}>Done</button>
		</div>
		{#if verb !== null && !spec}
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
	.spec {
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

	.spec::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	header > :first-child {
		flex: 1;
		min-inline-size: 0;
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

	/* One control of four segments: the chosen one carries the accent
	   along its foot, the mockup's underline. */
	.tabs {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.tabs button {
		min-block-size: 28px;
		background: #1a1a30;
		border: 0;
		border-inline-start: 1px solid var(--rule);
		border-block-end: 2px solid transparent;
		color: var(--muted);
		font: inherit;
		cursor: pointer;
	}

	.tabs button:first-child {
		border-inline-start: 0;
	}

	.tabs button[aria-selected='true'] {
		color: var(--ink-bright);
		border-block-end-color: var(--accent);
	}

	.tabs button:disabled {
		opacity: 0.4;
		cursor: default;
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

	.verb .name {
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

	.row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	/* The swatch's set opens under it: the label stays level with the swatch. */
	.row.top {
		align-items: flex-start;
	}

	.row.top .label {
		line-height: 28px;
	}

	.row input {
		flex: 1;
		min-inline-size: 0;
	}

	.label {
		flex: none;
		inline-size: 6.5rem;
		color: var(--muted);
		font-size: 0.75rem;
		letter-spacing: 0.06em;
	}

	/* Swatch, name, ✕ on one line; the description, when there is one to
	   ask for, the whole width under them. */
	.states {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.state {
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: start;
		gap: 0.4rem;
	}

	.state .description {
		grid-column: 1 / -1;
	}

	.later {
		margin: 0;
		color: var(--muted);
		font-style: italic;
	}

	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.buttons button,
	.ignored button,
	.fix,
	.close,
	.remove,
	.add {
		box-sizing: border-box;
		min-block-size: 24px;
		min-inline-size: 24px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.2rem 0.8rem;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}

	.close,
	.remove {
		padding: 0.2rem 0.45rem;
	}

	.add {
		align-self: flex-start;
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
		inline-size: auto;
		font-size: 0.7rem;
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
