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
	say so. The bar's field pill is that field while this is on the page
	(da-upig4): a hold on a field in the collect widget brings that field
	here, either × clears both, and a field opened here is the bar's pill. A new field
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
	Reached by their own pick in its list of fields, a spec's save stays,
	and the output section says what it wrote. Brought from another pane
	(`away`: a hold on a field, then the setup command), it has a cancel too:
	«a "cancel" button, that always goes back to where you came from. And
	then revert always just reverts to the last state without going back».
	Cancel drops the edits and goes back, save saves and goes back; revert
	and redo never leave. They are all commands in the bar's palette
	(da-r674m, FieldForm.svelte), and its undo, with no edit to take back,
	is the CLI's of the save this widget made while that is the last command
	run (`saved`, cleared by any press).
-->
<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { browser } from '$app/env';
	import FieldForm from '#lib/FieldForm.svelte';
	import Pane from '#lib/Pane.svelte';
	import { act } from '#lib/act.ts';
	import { commandsOpen, fieldPill, setFieldPill } from '#lib/bar.ts';
	import { renameFieldArgv, specHistoryArgv, specShowArgv, revertToVersionArgv, touchedArgv, type SpecVersion } from '#lib/calls.ts';
	import { stripAnsi } from '#lib/ansi.ts';
	import { ask, choose, confirm } from '#lib/confirm.ts';
	import { answerField, away, editing } from '#lib/fieldWidget.ts';
	import { run, type RunResult } from '#lib/run.ts';
	import { begin } from '#lib/tour.ts';
	import type { TailListing } from '#lib/tail.ts';
	import { VERBS, type Save, type Spec } from '#lib/track.ts';
	import { fields, onPress, press } from './engine.js';
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
	/** The field whose save this widget ran, while it is the last command run: what the form's undo takes back. */
	let saved: string | null = $state(null);
	onDestroy(onPress(() => (saved = null)));

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
	$effect.pre(() => {
		const asked = $editing;
		if (browser) untrack(() => void load(asked));
	});

	// The bar's field pill is this widget's field (da-upig4), vividn, 2026-11-11: «if you dismiss the field
	// pill in the input bar, it should also be dismissed in the setup widget and vice versa». One there as it
	// opens is its field, and one minted or taken away while it is here moves it. Synchronous, so a door
	// (`openField`) clearing a pill that disagrees with it is heard before it sets its own field.
	let opening = true;
	onDestroy(
		fieldPill.subscribe((pill) => {
			if ((pill !== null || !opening) && pill !== ($editing.field || null)) $editing = { field: pill ?? '' };
			opening = false;
		})
	);
	// And back: the field read is the bar's pill, in its colour — none on a new one. Only once the read is
	// of the field asked for, so a pill that moved the field is not taken back by the read before. Not a
	// field this widget just made (`fresh`): its pill would open collect's view on it, over the chip the
	// tour points at. And it leaves the command widget as it was, which a pill added opens (da-lnirx):
	// on a phone that covers the form's own buttons.
	let fresh: typeof $editing | null = null;
	$effect(() => {
		const [field, color] = [readFor.field, $fields.find((f) => f.field === readFor.field)?.color];
		untrack(() => {
			if (readFor !== $editing || readFor === fresh || $fieldPill === (field || null)) return;
			if (!field) return setFieldPill(null);
			if (color === undefined) return;
			const open = $commandsOpen;
			setFieldPill(field, color);
			$commandsOpen = open;
		});
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

	onDestroy(() => void flush());

	/** The save taken back by the CLI's undo, and the spec read again. */
	async function unsave(): Promise<void> {
		const asked = $editing;
		await press(['undo']);
		if (asked === $editing) await load(asked);
	}

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

	/** A write on the saved spec from the form (da-sbgff): its edits saved first, as leaving does, so they are kept, then the spec read again. */
	async function write(argv: string[]): Promise<RunResult> {
		const asked = $editing;
		await flush();
		const result = await press(argv);
		if (result.code === 0 && asked === $editing) await load(asked);
		return result;
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
		if (result) saved = made || asked.field;
		if (made) {
			$editing = fresh = { field: made };
			if (first) void begin(made);
			return;
		}
		// Cancel leaves, and so does a spec's Save for a person brought here; else a save stays, at a save point, and says so.
		const leaving = !save || (saving && $away);
		if (leaving) leavePlace();
		await load($editing);
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
					unsave={saved === readFor.field ? unsave : null}
					back={!!walk && walk.at > 1}
					ahead={!!walk && walk.at < walk.top}
					onclose={close}
					onrevert={revert}
					onredo={() => walk && void step(walk.at + 1)}
					onrun={(save) => press(save.argv)}
					onact={write}
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
