<!--
	The spec widget (da-hbrfg): one field's spec, edited where it stands —
	the form `SpecModal.svelte` asks with when a field is declared, drawn
	in a pane. Its place is `spec:<field>` (`specPlace`, defaults.ts), a
	tab per field; dropped into a pane with no field, it asks for one.
	Done runs the one `spec set` and leaves the place, Cancel leaves it
	running nothing; in a pane that is no place, the form is read afresh.
	A save the CLI refuses leaves the form as it was typed, to be put right.

	A field with no spec yet is declared right here, in the same form: a
	widget someone put in a pane is where they chose to work, and no
	question is being asked of them. `openSpec` is what keeps «which verb?»
	a question — it asks with the Track dialog instead of opening a place.
-->
<script lang="ts">
	import { browser } from '$app/environment';
	import { createEventDispatcher } from 'svelte';
	import Pane from '$lib/Pane.svelte';
	import SpecModal from '$lib/SpecModal.svelte';
	import { answerSpec } from '$lib/specModal.js';
	import { run } from '$lib/run.js';
	import { specShowArgv } from '$lib/calls.js';
	import type { Spec } from '$lib/track.js';
	import { fields, press } from './engine.js';
	import { leavePlace } from './shell.js';

	export let field = '';

	const dispatch = createEventDispatcher<{ change: string }>();

	/** The spec as read: undefined while reading, null for a field without one (declared here instead). */
	let spec: Spec | null | undefined;
	/** Bumped by every read, so the form is drawn afresh from it. */
	let read = 0;

	$: if (browser && field) void load(field);

	async function load(name: string): Promise<void> {
		const shown = await run(specShowArgv(name));
		if (name !== field) return;
		try {
			spec = shown.code === 0 ? JSON.parse(shown.stdout) : null;
		} catch {
			spec = null;
		}
		read++;
	}

	async function close(argv: string[] | null): Promise<void> {
		const result = argv && (await press(argv));
		answerSpec(field, result);
		// Refused, the form stays as it was, to be put right.
		if (result && result.code !== 0) return;
		leavePlace();
		void load(field);
	}
</script>

<Pane title="spec">
	{#if !field}
		<p class="quiet">which field?</p>
		<div class="fields">
			{#each $fields.filter((f) => !f.ignored) as f (f.field)}
				<button type="button" data-field={f.field} style:color={f.color} on:click={() => dispatch('change', f.field)}
					>{f.field}</button
				>
			{/each}
		</div>
	{:else if spec !== undefined}
		{#key read}
			<SpecModal inline {field} {spec} on:close={(e) => close(e.detail)} />
		{/key}
	{/if}
</Pane>

<style>
	.quiet {
		color: var(--muted);
		margin: 0 0 0.5rem;
	}

	.fields {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.fields button {
		min-block-size: 28px;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		font: inherit;
		cursor: pointer;
	}
</style>
