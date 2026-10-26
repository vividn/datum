<!--
	The setup widget (da-fu29p): where a field is made and where it is
	changed — the Track dialog and the spec widget (da-hbrfg), which were the
	same form at two doors, as ONE widget. vividn, 2026-10-08, after watching
	a newcomer: «let's fully combine track/spec widgets into just 1, and
	make the appearance better when no fields exist … make it so that you
	can switch the field you're editing inside of it more easy and
	intuitive … It should also be clearer what "field" actually is».

	So, top to bottom: what a field IS, in one sentence, always; and the form
	($lib/FieldForm.svelte) for whichever is chosen. Beside its `field:` line,
	once there is a field, «fields» opens a sheet of every tracked field in
	its own colour and glyph, the one being edited marked, narrowed by typing
	once there are more than a glance takes in. One word, vividn on #944:
	«"edit existing field" takes up an enormous amount of space. maybe just
	have it be "fields"». There is no «+ new field» line (da-zz9rn), vividn, 2026-10-19: «The (+ new field)
	button at the top is confusing, especially when this is the very first
	field being added»: on a field, the sheet's first entry is the way back
	to a new one, since a setup tab has no (+) of its own. Beside it on a
	field with a spec, «rename» (da-qimwi), vividn: «there should probably be an
	explicit rename button though»: it asks for the new name, shows `rename
	OLD NEW --dry-run`'s counts (a refusal asks again, saying why), runs
	`rename OLD NEW` and reopens the form on NEW.

	Which field is `editing` ($lib/fieldWidget.ts), one store for every copy
	of the widget, so anything that opens it on a field has one place to
	say so. A field pill minted while this is on the page says so too: a
	hold on a field in the fields widget brings that field here. A new field
	picked away from asks first where the form has edits nobody saved.

	Save on a new field runs its one command (da-2pbvd) and STAYS, on the
	field it made — but for the person's first, which goes on to the tour of
	where it is logged ($lib/tour.ts); Cancel leaves the place running nothing.
	A save the CLI refuses leaves the form as it was typed, to be put right.

	A field's spec saves when it is left (da-hda4j), vividn, 2026-10-26: «if
	you navigate away from a spec, maybe we should autosave it? But we change
	the bottom to have [Revert] and [Save] buttons, clicking revert goes back
	to the unsaved settings, or if already at a save checkpoint undoes the
	spec to the previous save (maybe we also then need a [redo] button for
	undoing that revert??)», and how far back: «whole saved history». So going
	to another field, renaming, or the widget closing runs the edits' one
	`setup F`; Save runs it and stays, at a save point. Revert with edits drops
	them; at a save point it is `undo SPEC --to-version N`, the version before
	the one it is at as `datum history` numbers them, every copy's and kit's
	included, and Redo the one after, back up to where the walk began. The
	walk is this widget's: `redo SPEC` steps forward once, and a save drops it.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import FieldForm from '$lib/FieldForm.svelte';
	import Pane from '$lib/Pane.svelte';
	import { act } from '$lib/act.js';
	import { fieldPill } from '$lib/bar.js';
	import { renameFieldArgv, specHistoryArgv, specShowArgv, undoToVersionArgv, type SpecVersion } from '$lib/calls.js';
	import { stripAnsi } from '$lib/ansi.js';
	import { ask, choose, confirm } from '$lib/confirm.js';
	import type { FieldButton } from '$lib/fields.js';
	import { answerField, editing } from '$lib/fieldWidget.js';
	import { run } from '$lib/run.js';
	import Sheet from '$lib/Sheet.svelte';
	import { begin } from '$lib/tour.js';
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
	/** Whether the sheet of fields is open, and what it is narrowed to. */
	let picking = false;
	let find = '';
	/** Whether the form holds edits nobody saved (`dirty`, FieldForm.svelte), and a spec's save of them. */
	let dirty = false;
	let pending: Save | null = null;
	/** The spec's walk along its history: its entry, the version the walk began at, and the one it is at. */
	let walk: { hid: string; top: number; at: number } | null = null;

	$: tracked = $fields.filter((f) => !f.ignored);
	$: if (browser) void load($editing);

	// Only a pill minted while this is here: one that was there already is not an ask.
	let pilled = $fieldPill;
	$: if ($fieldPill !== pilled) {
		pilled = $fieldPill;
		if (pilled !== null) $editing = { field: pilled };
	}

	/** Read `asked`'s spec afresh, the edits left behind saved first; `at` keeps a walk going, else it begins at the newest version. */
	async function load(asked: typeof $editing, at: typeof walk = null): Promise<void> {
		await flush();
		const shown = asked.field ? await run(specShowArgv(asked.field)) : null;
		const trail = at ?? (shown?.code === 0 ? await versions(asked.field) : null);
		if (asked !== $editing) return;
		try {
			spec = shown?.code === 0 ? JSON.parse(shown.stdout) : null;
		} catch {
			spec = null;
		}
		walk = trail;
		read++;
	}

	/** The spec's history as a walk at its newest version, or none (a spec only a template gives). */
	async function versions(field: string): Promise<typeof walk> {
		const shown = await run(specHistoryArgv(field));
		try {
			const [newest]: SpecVersion[] = shown.code === 0 ? JSON.parse(shown.stdout) : [];
			return newest ? { hid: newest.hid, top: newest.version, at: newest.version } : null;
		} catch {
			return null;
		}
	}

	/** Run the save of what the form holds unsaved, if it is a spec's: what leaving it does. */
	async function flush(): Promise<void> {
		const save = pending;
		pending = null;
		if (save) await press(save.argv);
	}

	onDestroy(() => void flush());

	/** Revert: the edits dropped, their save with them, or with none, the spec a version back. */
	function revert(): void {
		if (dirty) {
			pending = null;
			read++;
		} else if (walk && walk.at > 1) void step(walk.at - 1);
	}

	/** The spec put back as version `to` of its history, the walk at it; refused (the bar says why), read afresh, the walk begun again. */
	async function step(to: number): Promise<void> {
		const [asked, from] = [$editing, walk];
		if (!from) return;
		const result = await press(undoToVersionArgv(from.hid, to));
		if (asked === $editing) void load(asked, result.code === 0 ? { ...from, at: to } : null);
	}

	/** A field picked: to `field`, the spec's edits saved, a new field's once the reader agrees to lose them; the sheet stays open on a no. */
	async function go(field: string): Promise<void> {
		if (dirty && !spec && !(await confirm(`Leave ${$editing.field || 'the new field'} without saving what you changed?`, 'leave'))) return;
		picking = false;
		$editing = { field };
	}

	/** The field renamed: asked, its dry run shown, run, and the form on the new name. */
	async function rename(): Promise<void> {
		const old = $editing.field;
		let [to, said]: [string | null, string] = [old, `rename ${old} to:`];
		for (;;) {
			to = await ask(said, 'rename', to ?? old);
			if (to === null || to === old) return;
			const dry = await run(renameFieldArgv(old, to, true));
			if (dry.code === 0) {
				if ((await choose(dry.stdout.trim(), ['rename'], 'cancel', 'run')) === null) return;
				break;
			}
			said = `${stripAnsi(dry.stderr).trim()} — rename ${old} to:`;
		}
		await flush();
		const result = await press(renameFieldArgv(old, to));
		if (result.code === 0 && $editing.field === old) $editing = { field: to };
	}

	async function close(save: Save | null): Promise<void> {
		// The field a new one's `track F` or `setup F` names.
		const [asked, made] = [$editing, spec === null && save && save.argv[1]];
		// The person's first field, tracked: the tour follows its save (da-ii8lo).
		const first = made && tracked.length === 0 && !save?.argv.includes('--ignore');
		// The form's own save of its edits is cleared, so the read after it does not run it again;
		// another command (an ignored field's `track`) leaves them pending, for that read to save.
		if (save === pending) pending = null;
		const result = save && (await press(save.argv));
		answerField(result);
		// Refused, the form stays as it was, to be put right; and a reader who
		// went to another field while the save ran stays where they went.
		if ((result && result.code !== 0) || asked !== $editing) return;
		if (made) {
			$editing = { field: made };
			if (first) void begin(made);
			return;
		}
		// Cancel leaves; a save stays, at a save point.
		if (!save) leavePlace();
		void load($editing);
	}
</script>

<Pane title="setup">
	<div class="field">
		<p class="what">A field is one thing you track: mood, coffee, sleep. Each time you log it, that's an entry.</p>
		{#if spec !== undefined}
			{#key read}
				<FieldForm
					field={$editing.field}
					verb={$editing.verb ?? null}
					{spec}
					ignored={$fields.filter((f) => f.ignored).map((f) => f.field)}
					taken={$fields.map((f) => f.field)}
					bind:dirty
					bind:pending
					back={!!walk && walk.at > 1}
					ahead={!!walk && walk.at < walk.top}
					on:close={(e) => close(e.detail)}
					on:revert={revert}
					on:redo={() => walk && void step(walk.at + 1)}
					on:run={(e) => press(e.detail.argv)}
					on:open={(e) => go(e.detail)}
				>
					<svelte:fragment slot="pick">
						{#if $editing.field !== '' || tracked.length > 0}
							<button type="button" class="pick" use:act={'open'} aria-haspopup="dialog" on:click={() => ((find = ''), (picking = true))}
								>fields</button
							>
						{/if}
						{#if spec}
							<button type="button" class="rename" use:act={'open'} aria-haspopup="dialog" on:click={rename}>rename</button>
						{/if}
					</svelte:fragment>
				</FieldForm>
			{/key}
		{/if}
	</div>
	<Sheet title="fields" open={picking} onclose={() => (picking = false)} data-picker>
		{#if tracked.length > 6}
			<input class="find" bind:value={find} placeholder="find" aria-label="find a field" spellcheck="false" autocapitalize="none" />
		{/if}
		<div class="chips" role="group" aria-label="fields">
			{#if $editing.field !== ''}
				<button type="button" class="new" on:click={() => go('')}>+ new field</button>
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
	</Sheet>
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

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}

	.pick,
	.rename,
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

	.pick,
	.rename,
	.chips button {
		cursor: pointer;
	}

	.pick,
	.rename {
		flex: none;
		border-color: var(--rule);
		font-size: 0.75rem;
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
		inline-size: 100%;
		margin-block-end: 0.5rem;
		border-radius: 4px;
	}
</style>
