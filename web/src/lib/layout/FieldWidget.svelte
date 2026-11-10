<!--
	The setup widget (da-fu29p): where a field is made and where it is
	changed — the Track dialog and the spec widget (da-hbrfg), which were the
	same form at two doors, as ONE widget. vividn, 2026-10-08, after watching
	a newcomer: «let's fully combine track/spec widgets into just 1, and
	make the appearance better when no fields exist … make it so that you
	can switch the field you're editing inside of it more easy and
	intuitive … It should also be clearer what "field" actually is».

	So it is the form (#lib/FieldForm.svelte) for whichever is chosen. With
	none, «manual setup», whose modal says what a field is, takes a new one's
	name and lists every field there is to edit (da-4ta1k, vividn,
	2026-11-05), the three fields touched last beside it, and under them the
	premade examples (./SetupHome.svelte); with one, its pill, whose × starts
	over. vividn, 2026-11-09 (da-asgtm): «combine the premade and setup tabs
	back into one tab, which makes it easy for new people to get going, but
	also experienced users to add new fields, edit existing fields». The
	recent fields are the database's (vividn: «just take it from the database
	for now»): a field's newest entry or spec version, `touchedArgv`.
	Beside the pill on a field with a spec, «rename» (da-qimwi), vividn: «there should probably be an
	explicit rename button though»: it asks for the new name, shows `rename
	OLD NEW --dry-run`'s counts (a refusal asks again, saying why), runs
	`rename OLD NEW` and reopens the form on NEW.

	Which field is `editing` (#lib/fieldWidget.ts), one store for every copy
	of the widget, so anything that opens it on a field has one place to
	say so. A field pill minted while this is on the page says so too: a
	hold on a field in the collect widget brings that field here. A new field
	picked away from asks first where the form has edits nobody saved.

	Save on a new field runs its one command (da-2pbvd) and STAYS, on the
	field it made — but for the person's first, which goes on to the tour of
	where it is logged (#lib/tour.ts); Cancel leaves the place running nothing.
	A save the CLI refuses leaves the form as it was typed, to be put right.

	A field's spec saves when it is left (da-hda4j), vividn, 2026-10-26: «if
	you navigate away from a spec, maybe we should autosave it? But we change
	the bottom to have [Revert] and [Save] buttons, clicking revert goes back
	to the unsaved settings, or if already at a save checkpoint undoes the
	spec to the previous save (maybe we also then need a [redo] button for
	undoing that revert??)», and how far back: «whole saved history». So going
	to another field, renaming, or the widget closing runs the edits' one
	`setup F`; Save runs it and stays, at a save point. Revert with edits drops
	them; at a save point it is `revert SPEC --to-version N`, the version before
	the one it is at as `datum history` numbers them, every copy's and kit's
	included, and Redo the one after, back up to where the walk began. The
	walk is this widget's, and a save drops it.

	What Save and Cancel do depends on how the person came (da-wz84y),
	vividn, 2026-10-26: «maybe it should depend on how you got there?».
	Reached by their own pick in its list of fields, a spec's row is Revert ·
	Save · Redo, and Save stays, a brief «saved» under it. Brought from
	another pane (`away`: a hold on a field, then the setup command), it is
	Cancel › · Revert · Save › · Redo: «a "cancel" button, that always goes
	back to where you came from. And then revert always just reverts to the
	last state without going back … if hitting save or cancel takes you
	somewhere else it should have a little chevron on it». Cancel drops the
	edits and goes back, Save saves and goes back; Revert and Redo never leave.
-->
<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { browser } from '$app/env';
	import FieldForm from '#lib/FieldForm.svelte';
	import Pane from '#lib/Pane.svelte';
	import { act } from '#lib/act.ts';
	import { fieldPill } from '#lib/bar.ts';
	import { renameFieldArgv, specHistoryArgv, specShowArgv, revertToVersionArgv, touchedArgv, type SpecVersion } from '#lib/calls.ts';
	import { stripAnsi } from '#lib/ansi.ts';
	import { ask, choose, confirm } from '#lib/confirm.ts';
	import { answerField, away, editing } from '#lib/fieldWidget.ts';
	import { run } from '#lib/run.ts';
	import { begin } from '#lib/tour.ts';
	import type { TailListing } from '#lib/tail.ts';
	import { VERBS, type Save, type Spec } from '#lib/track.ts';
	import { fields, press } from './engine.js';
	import { leavePlace } from './shell.js';
	import SetupHome from './SetupHome.svelte';

	/** The spec as read: undefined while reading, null for a field without one (made here instead). */
	let spec: Spec | null | undefined = $state.raw();
	/** Bumped by every read, so the form is drawn afresh from it. */
	let read = $state(0);
	/** What the spec was read for: the form's field and verb change with its spec, not before it. */
	let readFor = $state.raw($editing);
	/** Whether the form holds edits nobody saved (`dirty`, FieldForm.svelte), and a spec's save of them. */
	let dirty = $state(false);
	let pending: Save | null = $state.raw(null);
	/** Whether the form holds no field yet (`empty`, FieldForm.svelte): a name typed in manual setup is one, read before it is saved. */
	let empty = $state(true);
	/** The spec's walk along its history: its entry, the version the walk began at, and the one it is at. */
	let walk: { hid: string; top: number; at: number } | null = $state.raw(null);
	/** A save that stayed, said under Save for a moment (da-wz84y). */
	let saved = $state(false);
	let unsay: ReturnType<typeof setTimeout> | undefined;

	const tracked = $derived($fields.filter((f) => !f.ignored));
	/** With no field: none chosen and none typed, the form its one line over the catalog (da-gnba1). */
	const home = $derived(spec !== undefined && readFor.field === '' && empty);

	/** The layout's epoch: a tab kept behind the one in front is drawn at it once the page is quiet (da-tajsm). */
	let { epoch = 0 }: { epoch?: number } = $props();

	/** The fields touched last, newest first, read again at each epoch while there is none chosen; the latest asked is kept. */
	let touched: string[] = $state([]);
	let asking = 0;
	$effect(() => {
		void epoch;
		if (!home) return;
		const at = ++asking;
		untrack(() => void lastTouched().then((names) => at === asking && (touched = names)));
	});
	const recent = $derived(touched.flatMap((name) => $fields.filter((f) => f.field === name)).slice(0, 3));

	/** Each field by its newest entry or spec version, newest first: a spec's row names its field in its slug. */
	async function lastTouched(): Promise<string[]> {
		const read = await run(touchedArgv());
		try {
			const rows: TailListing['rows'] = read.code === 0 ? JSON.parse(read.stdout).rows : [];
			return [...new Set(rows.map((r) => (r.field === 'spec' ? r.slug.slice('spec:'.length) : r.field)).reverse())];
		} catch {
			return [];
		}
	}
	// Another field: what was said of the last one's save is not said of it.
	$effect.pre(() => {
		const asked = $editing;
		if (browser)
			untrack(() => {
				saved = false;
				void load(asked);
			});
	});

	// Only a pill minted while this is here: one that was there already is not an ask.
	let pilled = $fieldPill;
	$effect.pre(() => {
		if ($fieldPill !== pilled) {
			pilled = $fieldPill;
			if (pilled !== null) $editing = { field: pilled };
		}
	});

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
		readFor = asked;
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

	onDestroy(() => {
		clearTimeout(unsay);
		void flush();
	});

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
		const result = await press(revertToVersionArgv(from.hid, to));
		if (asked === $editing) void load(asked, result.code === 0 ? { ...from, at: to } : null);
	}

	/** A field picked, `''` a new one: the spec's edits saved, a new field's once the reader agrees to lose them. */
	async function go(field: string): Promise<void> {
		if (dirty && !spec && !(await confirm(`Leave ${$editing.field || 'the new field'} without saving?`, 'leave'))) return;
		$away = false;
		$editing = { field };
	}

	/** The field renamed: asked, its dry run shown, run, and the form on the new name. A name that exists says so, and offers the merge with its dry run (da-icna7). */
	async function rename(): Promise<void> {
		const old = $editing.field;
		let [to, said, merge]: [string | null, string, boolean] = [old, `rename ${old} to:`, false];
		for (;;) {
			to = await ask(said, 'rename', to ?? old);
			if (to === null || to === old) return;
			const dry = await run(renameFieldArgv(old, to, true));
			if (dry.code === 0) {
				if ((await choose(dry.stdout.trim(), ['rename'], 'cancel', 'run')) === null) return;
				break;
			}
			// Refused as it stands and not as a merge: the name exists. Refused either way, the name is what is wrong.
			const merging = await run(renameFieldArgv(old, to, true, true));
			if (merging.code === 0) {
				const offer = `${to} already exists. Merge ${old} into it?\n\n${stripAnsi(merging.stdout).trim()}`;
				if ((await choose(offer, ['merge'], 'cancel', 'run')) === null) return;
				merge = true;
				break;
			}
			said = `${stripAnsi(merging.stderr).trim()} — rename ${old} to:`;
		}
		await flush();
		const result = await press(renameFieldArgv(old, to, false, merge));
		if (result.code === 0 && $editing.field === old) $editing = { field: to };
	}

	async function close(save: Save | null): Promise<void> {
		// The field a new one's `track F` or `setup F` names.
		const [asked, made] = [$editing, spec === null && save && save.argv[1]];
		// The person's first field, tracked: the tour follows its save (da-ii8lo).
		const first = made && tracked.length === 0 && !save?.argv.includes('--ignore');
		// A spec's own Save, as against another command (an ignored field's `track`).
		const saving = save !== null && save === pending;
		// The form's own save of its edits is cleared, so the read after it does not run it again,
		// and Cancel drops them; another command leaves them pending, for that read to save.
		if (!save || saving) pending = null;
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
		// Cancel leaves, and so does a spec's Save for a person brought here; else a save stays, at a save point, and says so.
		const leaving = !save || (saving && $away);
		if (leaving) leavePlace();
		await load($editing);
		// Said once the read has drawn the form afresh, so its fade and its end start together.
		if (saving && !leaving && asked === $editing) {
			saved = true;
			clearTimeout(unsay);
			unsay = setTimeout(() => (saved = false), 2000);
		}
	}
</script>

<Pane title="setup">
	<div class="field">
		{#if spec !== undefined}
			{#key read}
				<FieldForm
					field={readFor.field}
					verb={readFor.verb ?? null}
					{spec}
					fields={$fields}
					example={readFor.example ?? null}
					bind:dirty
					bind:pending
					bind:empty
					away={$away}
					{saved}
					back={!!walk && walk.at > 1}
					ahead={!!walk && walk.at < walk.top}
					onclose={close}
					onrevert={revert}
					onredo={() => walk && void step(walk.at + 1)}
					onrun={(save) => press(save.argv)}
					onopen={go}
				>
					{#snippet pick()}
						{#if spec}
							<button type="button" class="rename" use:act={'open'} aria-haspopup="dialog" onclick={rename}>rename</button>
						{:else if home && recent.length}
							<span class="recent" role="group" aria-label="recent">
								<span title="the three fields you touched last: logged, edited or opened">recent:</span>
								{#each recent as f (f.field)}
									<button type="button" use:act={'show'} data-recent={f.field} style:--c={f.color} onclick={() => go(f.field)}
										>{f.glyph ?? VERBS.find((v) => v.what === f.shape.kind)?.glyph} {f.field}</button
									>
								{/each}
							</span>
						{/if}
					{/snippet}
				</FieldForm>
			{/key}
			{#if home}
				<SetupHome />
			{/if}
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

	.recent {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		color: var(--muted);
	}

	.recent button {
		min-block-size: 32px;
		background: none;
		border: 1px solid var(--rule);
		border-radius: 999px;
		padding: 0.1rem 0.6rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.recent button:hover {
		border-color: var(--c);
	}

	.rename {
		flex: none;
		box-sizing: border-box;
		min-block-size: 28px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 999px;
		padding: 0.1rem 0.6rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.75rem;
		cursor: pointer;
	}
</style>
