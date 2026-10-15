<!--
	The field widget (da-fu29p): where a field is made and where it is
	changed — the Track dialog and the spec widget (da-hbrfg), which were the
	same form at two doors, as ONE widget. vividn, 2026-10-08, after watching
	a newcomer: «let's fully combine track/spec widgets into just 1, and
	make the appearance better when no fields exist … make it so that you
	can switch the field you're editing inside of it more easy and
	intuitive … It should also be clearer what "field" actually is».

	So, top to bottom: what a field IS, in one sentence, always; a row of
	chips, «+ new field» first and then every tracked field in its own
	colour and glyph, the one being edited marked, narrowed by typing once
	there are more than a glance takes in; and the form
	($lib/FieldForm.svelte) for whichever is chosen. With no fields the row
	is «+ new field» alone and the form is its name box and the examples.

	Which field is `editing` ($lib/fieldWidget.ts), one store for every copy
	of the widget, so anything that opens it on a field has one place to
	say so. A field pill minted while this is on the page says so too: a
	hold on a field in the fields widget brings that field here. A chip
	asks first where the form in hand has edits nobody saved.

	Save on a new field runs its one command (da-2pbvd) and STAYS, on the
	field it made. Done on a field runs its one `spec set` and leaves the place, Cancel leaves it running nothing; in a pane
	that is no place, the form is read afresh. A save the CLI refuses
	leaves the form as it was typed, to be put right.
-->
<script lang="ts">
	import { browser } from '$app/environment';
	import FieldForm from '$lib/FieldForm.svelte';
	import Pane from '$lib/Pane.svelte';
	import { fieldPill } from '$lib/bar.js';
	import { specShowArgv } from '$lib/calls.js';
	import { confirm } from '$lib/confirm.js';
	import type { FieldButton } from '$lib/fields.js';
	import { answerField, editing } from '$lib/fieldWidget.js';
	import { run } from '$lib/run.js';
	import { VERBS, type Save, type Spec, type Verb } from '$lib/track.js';
	import { fields, press } from './engine.js';
	import { leavePlace } from './shell.js';

	/** The verb each shape of button is logged with: whose glyph its chip wears. */
	const VERB: Record<FieldButton['shape']['kind'], Verb> = { point: 'occur', state: 'start', states: 'switch', record: 'add' };
	const glyphOf = (f: FieldButton) => f.glyph ?? VERBS.find((v) => v.verb === VERB[f.shape.kind])?.glyph;

	/** The spec as read: undefined while reading, null for a field without one (made here instead). */
	let spec: Spec | null | undefined;
	/** Bumped by every read, so the form is drawn afresh from it. */
	let read = 0;
	/** What the chips are narrowed to. */
	let find = '';
	/** Whether the form holds edits nobody saved (`dirty`, FieldForm.svelte). */
	let dirty = false;

	$: tracked = $fields.filter((f) => !f.ignored);
	$: if (browser) void load($editing);

	// Only a pill minted while this is here: one that was there already is not an ask.
	let pilled = $fieldPill;
	$: if ($fieldPill !== pilled) {
		pilled = $fieldPill;
		if (pilled !== null) $editing = { field: pilled };
	}

	async function load(asked: typeof $editing): Promise<void> {
		const shown = asked.field ? await run(specShowArgv(asked.field)) : null;
		if (asked !== $editing) return;
		try {
			spec = shown?.code === 0 ? JSON.parse(shown.stdout) : null;
		} catch {
			spec = null;
		}
		read++;
	}

	/** A chip's press: to `field`, once the reader agrees to lose what the form holds unsaved. */
	async function go(field: string): Promise<void> {
		if (dirty && !(await confirm(`Leave ${$editing.field || 'the new field'} without saving what you changed?`, 'leave'))) return;
		$editing = { field };
	}

	async function close(save: Save | null): Promise<void> {
		// The field a new one's `track F` or `spec set F` names.
		const [asked, made] = [$editing, spec === null && save && save.argv[save.argv[0] === 'spec' ? 2 : 1]];
		const result = save && (await press(save.argv));
		answerField(result);
		// Refused, the form stays as it was, to be put right; and a reader who
		// went to another field while the save ran stays where they went.
		if ((result && result.code !== 0) || asked !== $editing) return;
		if (made) return void ($editing = { field: made });
		leavePlace();
		void load($editing);
	}
</script>

<Pane title="field">
	<div class="field">
		<p class="what">A field is one thing you track: mood, coffee, sleep. Each time you log it, that's an entry.</p>
		<div class="chips" role="group" aria-label="fields">
			<button type="button" class="new" aria-current={$editing.field === ''} on:click={() => go('')}>+ new field</button>
			{#if tracked.length > 6}
				<input class="find" bind:value={find} placeholder="find" aria-label="find a field" spellcheck="false" autocapitalize="none" />
			{/if}
			{#each tracked.filter((f) => f.field.toLowerCase().includes(find.trim().toLowerCase())) as f (f.field)}
				<button
					type="button"
					data-chip={f.field}
					aria-current={f.field === $editing.field}
					style:--c={f.color}
					on:click={() => go(f.field)}><span aria-hidden="true">{glyphOf(f)}</span> {f.field}</button
				>
			{/each}
		</div>
		{#if spec !== undefined}
			{#key read}
				<FieldForm
					field={$editing.field}
					verb={$editing.verb ?? null}
					{spec}
					ignored={$fields.filter((f) => f.ignored).map((f) => f.field)}
					taken={$fields.map((f) => f.field)}
					bind:dirty
					on:close={(e) => close(e.detail)}
				/>
			{/key}
		{/if}
	</div>
</Pane>

<style>
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-block-size: 100%;
	}

	.what {
		margin: 0;
		color: var(--quiet);
	}

	/* Three rows of chips at most, then the row scrolls: the form keeps its room however many fields there are. */
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		flex: none;
		max-block-size: 6rem;
		overflow: auto;
	}

	.chips button,
	.find {
		box-sizing: border-box;
		min-block-size: 28px;
		background: var(--raised);
		border: 1px solid var(--c, var(--rule));
		border-radius: 999px;
		padding: 0.1rem 0.6rem;
		color: var(--ink);
		font: inherit;
	}

	.chips button {
		cursor: pointer;
	}

	.chips span {
		color: var(--c);
	}

	.chips [aria-current='true'] {
		background: color-mix(in srgb, var(--c, var(--accent)) 30%, var(--raised));
		color: var(--ink-bright);
		box-shadow: inset 0 0 0 1px var(--c, var(--accent));
	}

	.new {
		border-style: dashed;
	}

	.find {
		inline-size: 6rem;
	}
</style>
