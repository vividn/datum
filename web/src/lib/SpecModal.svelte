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

	A field with a spec also picks the GLYPH its dots draw (da-ogyxa):
	one for the field, or one by a data key's value with the field's as
	the default, and one per state beside the state's colour. `track`
	carries none, so declaring asks for none.

	Segmented: Basics · States · Keys · Aliases · Placement. Track is this
	modal opened on Basics, and that is all it needs — name, pictogram,
	Done, the same taps it always took. The keys a field's entries carry
	are on Basics too (da-h119q): a name and a KIND — required, optional,
	default or assign, each with its value where it has one (da-p3hxn),
	the four ways a log line names a key — the first thing a person adds
	to a field, and `track` carries them. The rows are `spec show`'s own
	`keys`, parsed by the CLI, so what is saved is what it reads back. The
	Keys tab has the rest of a key — its type, whether it is in the slug —
	and it, Aliases and Placement are a spec's alone (da-cap5m, da-btyvj),
	so a field being declared has them shut. Placement is a fifth tab
	because it is a drag surface, not a row of inputs — cheap to move under
	Basics.

	Keys are grouped required over the rest, so which is which reads at a
	glance and not only off each row's kind, and each row says whether
	the key is IN THE SLUG — whether it identifies an entry or merely
	annotates it. Aliases are made by datum from the verb and the states
	until that is switched off (`no_autoalias`); from then on the list is
	the person's, added to and removed from here.

	What it writes is decided by what it was opened on. A new field gets
	`track`, the one command that writes the TRACK document, and so only
	what `track` can carry is asked (no descriptions, no glyphs). A field with
	a spec gets the whole edited spec back in ONE `spec set --from-json` —
	the `set` shape, the list entire, never a stream of `add-state`s.

	It is opened from three doors to declare and is the same modal at each:
	the widget bar's (+), a filtered list's `+` with its verb already
	chosen, and the CLI's refusal, which arrives with the field already
	typed. The `<dialog>` pattern is Confirm's and Chooser's — `showModal`
	puts focus inside, Escape and a click on the backdrop close it and run
	nothing.

	Editing a spec is a PLACE, not a question (da-hbrfg): «a modal asks; a
	place is a tab». `inline` draws the same form without the dialog, in
	the `spec` widget ($lib/layout/SpecWidget.svelte), which is what a
	field's `spec:<field>` tab holds. Declaring stays the dialog.
-->
<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import { act } from './act.js';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import { defaultColor } from './fields.js';
	import Glyph from './Glyph.svelte';
	import Placement from './Placement.svelte';
	import { shellJoin } from './shellSplit.js';
	import Swatch from './Swatch.svelte';
	import {
		editedSpec,
		keyRows,
		KEY_KINDS,
		KEY_TYPES,
		placeOf,
		specArgv,
		specKeys,
		stateRows,
		trackArgv,
		VERBS,
		type DerivedAlias,
		type KeyRow,
		type Place,
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
	/** Drawn in a pane rather than as a dialog: the `spec` widget's body. */
	export let inline = false;

	const dispatch = createEventDispatcher<{ close: string[] | null }>();
	const TABS = ['basics', 'states', 'keys', 'aliases', 'placement'] as const;

	let dialog: HTMLElement;
	let tab: (typeof TABS)[number] = 'basics';
	let color = spec?.color ?? '';
	let description = spec?.description ?? '';
	let states: StateRow[] = spec ? stateRows(spec) : [];
	let glyph = spec?.glyph ?? '';
	let byKey = !!spec?.glyph_by;
	let glyphKey = spec?.glyph_by?.key ?? '';
	let glyphMap = Object.entries(spec?.glyph_by?.map ?? {}).map(([value, glyph]) => ({ value, glyph }));
	/** The data keys the saved spec names, which «by a key» suggests. */
	const keyNames = spec ? specKeys(spec) : [];
	let keys: KeyRow[] = spec ? keyRows(spec) : [];
	let autoalias = !spec?.no_autoalias;
	let neverFalse = !!spec?.never_false;
	let aliases: DerivedAlias[] = (spec?.derived_aliases ?? []).map((a) => ({ ...a }));
	let place: Place = spec ? placeOf(spec) : {};
	let icon = spec?.icon ?? '';
	/** The file the icon came from, while this modal is open; a stored icon has none. */
	let iconName = '';
	if (spec) verb = spec.verb;
	/** The argv to run, set by whatever was pressed; null closes and runs nothing. */
	let chosen: string[] | null = null;

	onMount(() => {
		if (!inline) (dialog as HTMLDialogElement).showModal();
	});

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
	$: glyph_by = byKey
		? { key: glyphKey, map: Object.fromEntries(glyphMap.map((r) => [r.value, r.glyph])) }
		: null;
	$: argv = spec
		? specArgv(
				editedSpec(spec, {
					verb: verb ?? spec.verb,
					color,
					description,
					states,
					keys,
					autoalias,
					aliases,
					neverFalse,
					glyph,
					glyph_by,
					place,
					icon
				})
			)
		: trackArgv(field.trim() || '<field>', verb ?? 'occur', states, color, keys);
	// A new field's states are a switch's: `track` carries them for no other verb.
	$: statesOpen = spec !== null || verb === 'switch';

	// da-45wgz: the file is read as it is; whether it is an icon — SVG,
	// under the cap, nothing that runs — is the CLI's to say on Done, in
	// the one refusal every surface gets.
	async function pickIcon(e: Event) {
		const file = (e.currentTarget as HTMLInputElement).files?.[0];
		if (!file) return;
		icon = await file.text();
		iconName = file.name;
	}

	const addKey = () =>
		(keys = [...keys, { name: '', kind: 'optional', value: '', value_type: 'any', in_slug: false }]);

	/** Whether a key's kind carries a value: a default's, an assign's. */
	const valued = (key: KeyRow) => key.kind === 'default' || key.kind === 'assign';

	/** `list` with its `from`th item moved to `to`. */
	function moved<T>(list: T[], from: number, to: number): T[] {
		const next = [...list.slice(0, from), ...list.slice(from + 1)];
		next.splice(to, 0, list[from]);
		return next;
	}

	/** Close with `next` to run, or null to run nothing. */
	function finish(next: string[] | null) {
		chosen = next;
		if (inline) dispatch('close', next);
		else (dialog as HTMLDialogElement).close();
	}

	/** A click on the dialog's own box is on its backdrop: close, running nothing. */
	function backdrop(e: MouseEvent) {
		if (!inline && e.target === dialog) finish(null);
	}
</script>

<!-- The backdrop is the dialog's own box outside its content, so a click on
     it lands on the dialog: Chooser's rule, and the pointer's Escape. -->
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
<svelte:element
	this={inline ? 'div' : 'dialog'}
	class="spec"
	class:inline
	role={inline ? 'region' : undefined}
	aria-label={spec ? `the spec of ${field}` : 'track a field'}
	bind:this={dialog}
	on:close={() => dispatch('close', chosen)}
	on:click={backdrop}
>
	<form method="dialog" on:submit|preventDefault={() => ready && finish(argv)}>
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
			<button type="button" class="close" aria-label="close" on:click={() => finish(null)}>✕</button>
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
					id="spec-tab-{name}"
					aria-controls="spec-panel"
					data-tab={name}
					aria-selected={tab === name}
					disabled={name === 'states' ? !statesOpen : name !== 'basics' && !spec}
					on:click={() => (tab = name)}>{name}</button
				>
			{/each}
		</div>

		<div class="panel" role="tabpanel" id="spec-panel" aria-labelledby="spec-tab-{tab}" data-panel={tab}>
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
					<Swatch bind:value={color} fallback={defaultColor(field.trim())} label="the field's colour" />
				</div>
				<span class="label">keys</span>
				<ul class="basic-keys">
					{#each keys as key (key)}
						<li>
							<input
								class="name"
								bind:value={key.name}
								placeholder="key"
								aria-label="the key's name"
								spellcheck="false"
								autocapitalize="none"
							/>
							<select class="kind" bind:value={key.kind} aria-label="the kind of {key.name || 'this key'}">
								{#each KEY_KINDS as kind (kind)}<option>{kind}</option>{/each}
							</select>
							<input
								class="kind-value"
								bind:value={key.value}
								placeholder={key.kind}
								disabled={!valued(key)}
								aria-label="the {key.kind} of {key.name || 'this key'}"
							/>
							<button
								type="button"
								class="remove"
								aria-label="remove {key.name}"
								on:click={() => (keys = keys.filter((k) => k !== key))}>✕</button
							>
						</li>
					{/each}
				</ul>
				<button type="button" class="add" on:click={addKey}>+ key</button>
				{#if spec}
					<label class="row">
						<span class="label">description</span>
						<input class="description" bind:value={description} />
					</label>
					<div class="row">
						<span class="label">glyph</span>
						<div class="glyph-by" role="radiogroup" aria-label="the field's glyph">
							<button type="button" role="radio" aria-checked={!byKey} on:click={() => (byKey = false)}
								>one glyph</button
							>
							<button type="button" role="radio" aria-checked={byKey} on:click={() => (byKey = true)}
								>by a key</button
							>
						</div>
					</div>
					{#if byKey}
						<label class="row">
							<span class="label">key</span>
							<input class="glyph-key" list="spec-keys" bind:value={glyphKey} spellcheck="false" autocapitalize="none" />
							<datalist id="spec-keys">
								{#each keyNames as key (key)}<option value={key}></option>{/each}
							</datalist>
						</label>
					{/if}
					<ul class="glyphs">
						{#if byKey}
							{#each glyphMap as row, i (row)}
								<li>
									<input
										class="value"
										bind:value={row.value}
										placeholder="value"
										aria-label="a value of {glyphKey || 'the key'}"
										spellcheck="false"
										autocapitalize="none"
									/>
									<Glyph bind:value={row.glyph} {color} label="the glyph for {row.value || 'this value'}" />
									<button
										type="button"
										class="remove"
										aria-label="remove {row.value}"
										on:click={() => (glyphMap = glyphMap.filter((_, j) => j !== i))}>✕</button
									>
								</li>
							{/each}
						{/if}
						<li class="default">
							{#if byKey}<span class="value">(other)</span>{/if}
							<Glyph bind:value={glyph} {color} label="the field's glyph" />
						</li>
					</ul>
					{#if byKey}
						<button type="button" class="add" on:click={() => (glyphMap = [...glyphMap, { value: '', glyph: '' }])}
							>+ value</button
						>
					{/if}
					<!-- da-45wgz: an .svg dropped or picked; the input covers the box, so a drop lands on it. -->
					<div class="row">
						<span class="label">icon</span>
						<label class="drop">
							<input type="file" class="icon" accept=".svg,image/svg+xml" aria-label="the field's icon" on:change={pickIcon} />
							{#if icon}
								▸ {iconName || 'icon'} &nbsp;{(new Blob([icon]).size / 1024).toFixed(1)} kB
							{:else}
								drop an .svg here
							{/if}
						</label>
						{#if icon}
							<button type="button" class="remove" aria-label="remove the icon" on:click={() => ((icon = ''), (iconName = ''))}
								>✕</button
							>
						{/if}
					</div>
				{/if}
			{:else if tab === 'states'}
				<ul class="states" class:glyphed={spec}>
					{#each states as state, i (state)}
						<li class="state">
							<Swatch
								bind:value={state.color}
								fallback={defaultColor(state.name.trim())}
								label="the colour of {state.name || 'this state'}"
							/>
							{#if spec}
								<Glyph
									bind:value={state.glyph}
									color={state.color || color}
									label="the glyph of {state.name || 'this state'}"
								/>
							{/if}
							<input
								class="name"
								bind:value={state.name}
								placeholder="state"
								aria-label="the state's name"
								spellcheck="false"
								autocapitalize="none"
							/>
							<!-- The order is where each state stands around the tap in the
							     fields widget (da-7q3ka): the first nearest the thumb. -->
							<button
								type="button"
								class="remove"
								data-up
								disabled={i === 0}
								aria-label="move {state.name || 'this state'} up"
								on:click={() => (states = moved(states, i, i - 1))}>↑</button
							>
							<button
								type="button"
								class="remove"
								data-down
								disabled={i === states.length - 1}
								aria-label="move {state.name || 'this state'} down"
								on:click={() => (states = moved(states, i, i + 1))}>↓</button
							>
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
					on:click={() => (states = [...states, { name: '', color: '', description: '', glyph: '' }])}
					>+ state</button
				>
				{#if spec && verb === 'switch'}
					<!-- da-cwieh — vividn: «for example environment is never false». Only a switch has a ring,
					     and a field that lists `false` as a state keeps its `end` whatever this says. -->
					{@const listsFalse = states.some((s) => s.name.trim() === 'false')}
					<label
						class="auto"
						title={listsFalse ? 'false is one of its states, so the ring keeps end' : 'the fields widget leaves a gap where end would stand'}
						><input type="checkbox" class="never-false" bind:checked={neverFalse} disabled={listsFalse} /> never false: no end</label
					>
				{/if}
			{:else if tab === 'keys'}
				{#each [true, false] as required (required)}
					{#if keys.some((k) => (k.kind === 'required') === required)}
						<ul class="keys" data-group={required ? 'required' : 'optional'}>
							<li class="group">{required ? 'required' : 'optional'}</li>
							{#each keys.filter((k) => (k.kind === 'required') === required) as key (key)}
								<li class="key">
									<input
										class="name"
										bind:value={key.name}
										placeholder="key"
										aria-label="the key's name"
										spellcheck="false"
										autocapitalize="none"
									/>
									<button
										type="button"
										class="remove"
										aria-label="remove {key.name}"
										on:click={() => (keys = keys.filter((k) => k !== key))}>✕</button
									>
									<div class="flags">
										<select class="kind" bind:value={key.kind} aria-label="the kind of {key.name || 'this key'}">
											{#each KEY_KINDS as kind (kind)}<option>{kind}</option>{/each}
										</select>
										<label title="a key in the slug identifies an entry; one outside it annotates it"
											><input type="checkbox" class="in-slug" bind:checked={key.in_slug} /> in slug</label
										>
										<select class="type" bind:value={key.value_type} aria-label="the type of {key.name}">
											{#each KEY_TYPES as type (type)}<option>{type}</option>{/each}
										</select>
									</div>
									<input
										class="kind-value"
										bind:value={key.value}
										placeholder={key.kind}
										disabled={!valued(key)}
										aria-label="the {key.kind} of {key.name || 'this key'}"
									/>
								</li>
							{/each}
						</ul>
					{/if}
				{/each}
				<button
					type="button"
					class="add"
					on:click={addKey}>+ key</button
				>
			{:else if tab === 'placement'}
				<Placement {field} occur={verb === 'occur'} bind:place />
			{:else}
				<label class="auto"
					><input type="checkbox" class="autoalias" bind:checked={autoalias} /> made from the verb and states</label
				>
				<ul class="aliases">
					{#each aliases as alias (alias)}
						<li class="alias">
							{#if autoalias}
								<code class="name">{alias.state}</code><code class="expands">{alias.expands_to}</code>
							{:else}
								<input
									class="name"
									bind:value={alias.state}
									placeholder="alias"
									aria-label="the alias"
									spellcheck="false"
									autocapitalize="none"
								/>
								<input
									class="expands"
									bind:value={alias.expands_to}
									placeholder="{verb ?? 'occur'} {field}"
									aria-label="what {alias.state || 'it'} runs"
									spellcheck="false"
									autocapitalize="none"
								/>
								<button
									type="button"
									class="remove"
									aria-label="remove {alias.state}"
									on:click={() => (aliases = aliases.filter((a) => a !== alias))}>✕</button
								>
							{/if}
						</li>
					{/each}
				</ul>
				{#if autoalias}
					<p class="later">made again from the verb and states when saved</p>
				{:else}
					<button
						type="button"
						class="add"
						on:click={() => (aliases = [...aliases, { state: '', expands_to: '' }])}>+ alias</button
					>
				{/if}
			{/if}
		</div>

		<div class="buttons">
			<button type="button" class="cancel" on:click={() => finish(null)}>Cancel</button>
			<button class="commit" use:act={'run'} type="submit" disabled={!ready}>Done</button>
		</div>
		{#if verb !== null && !spec}
			<code class="preview">{shellJoin(argv)}</code>
		{/if}

		{#if ignored.length > 0}
			<!-- Ignored fields get no button in the bar; this is the way back (da-d08c). -->
			<div class="ignored">
				<span class="label">ignored</span>
				{#each ignored as name (name)}
					<button type="button" use:act={'run'} data-ignored={name} title={shellJoin(['track', name])} on:click={() => finish(['track', name])}
						>{name}</button
					>
				{/each}
			</div>
		{/if}
	</form>
</svelte:element>

<style>
	/* Narrow enough for a 320px phone with the backdrop showing either
	   side. One height whatever the tab (da-laxpn): a centred dialog that
	   followed its content jumped on every tab press. 38rem is the tallest
	   tab's natural height, measured — Basics, with two keys on it since
	   da-h119q; never taller than the screen. */
	.spec {
		box-sizing: border-box;
		inline-size: min(24rem, calc(100vw - 2rem));
		block-size: min(38rem, calc(100dvh - 2rem));
		overflow: auto;
		background: var(--ground);
		color: var(--ink);
		border: 1px solid var(--rule);
		border-radius: 6px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}

	/* In a pane, the pane's box: its frame is the border. */
	.spec.inline {
		inline-size: auto;
		block-size: 100%;
		border: 0;
		padding: 0.5rem 0.6rem;
	}

	.spec::backdrop {
		background: rgb(0 0 0 / 0.6);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		block-size: 100%;
	}

	/* Header, hint, tabs and buttons stay put; a long tab scrolls here. */
	.panel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1;
		min-block-size: 0;
		overflow: auto;
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

	/* One control of five segments: the chosen one carries the accent
	   along its foot, the mockup's underline. Each is as wide as its word
	   and shares out the rest, so five words fit a 320px phone. */
	.tabs {
		display: flex;
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.tabs button {
		flex: 1 1 auto;
		min-block-size: 28px;
		padding-inline: 0.2rem;
		background: #1a1a30;
		border: 0;
		border-inline-start: 1px solid var(--rule);
		border-block-end: 2px solid transparent;
		color: var(--muted);
		font: inherit;
		font-size: 0.7rem;
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

	.drop {
		position: relative;
		flex: 1;
		min-inline-size: 0;
		border: 1px dashed var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--muted);
		overflow-wrap: anywhere;
	}

	.drop input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
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
		grid-template-columns: auto 1fr auto auto auto;
		align-items: start;
		gap: 0.4rem;
	}

	/* Or the name's own width pushes ✕ out of the row. */
	.state .name {
		min-inline-size: 0;
	}

	.states.glyphed .state {
		grid-template-columns: auto auto 1fr auto auto auto;
	}

	/* After a glyph's set, which opens under the row it was tapped in. */
	.state .description {
		grid-column: 1 / -1;
		order: 2;
	}

	/* The glyph, or the value → glyph table: indented under the label. */
	.glyphs {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin: 0;
		padding: 0 0 0 7rem;
		list-style: none;
	}

	.glyphs li {
		display: grid;
		grid-template-columns: 1fr auto auto;
		align-items: center;
		gap: 0.4rem;
	}

	.glyphs .value {
		min-inline-size: 0;
	}

	.glyphs .default .value {
		color: var(--muted);
	}

	.glyph-by {
		display: flex;
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.glyph-by button {
		min-block-size: 24px;
		background: #1a1a30;
		border: 0;
		border-block-end: 2px solid transparent;
		padding: 0.2rem 0.6rem;
		color: var(--muted);
		font: inherit;
		cursor: pointer;
	}

	.glyph-by button[aria-checked='true'] {
		color: var(--ink-bright);
		border-block-end-color: var(--accent);
	}

	.glyphs + .add {
		margin-inline-start: 7rem;
	}

	/* Basics' keys: one line each, the Keys tab's first three things. */
	.basic-keys {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.basic-keys li {
		display: grid;
		grid-template-columns: 1fr auto 1fr auto;
		align-items: center;
		gap: 0.3rem;
	}

	.basic-keys input {
		min-inline-size: 0;
	}

	/* A group's name heads its rows: which keys are required reads at a
	   glance, before any toggle is. */
	.keys,
	.aliases {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.group {
		color: var(--muted);
		font-size: 0.75rem;
		letter-spacing: 0.06em;
	}

	.keys[data-group='required'] .group {
		color: var(--accent);
	}

	.key {
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 0.3rem;
	}

	.flags,
	.key .kind-value {
		grid-column: 1 / -1;
	}

	.flags {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		color: var(--muted);
		font-size: 0.75rem;
	}

	.flags select.type {
		margin-inline-start: auto;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 4px;
		color: var(--ink-bright);
		font: inherit;
	}

	.alias {
		display: grid;
		grid-template-columns: 1fr 1.6fr auto;
		align-items: center;
		gap: 0.4rem;
	}

	.alias code {
		overflow-wrap: anywhere;
	}

	.alias code.name {
		color: var(--ink-bright);
	}

	.auto {
		color: var(--muted);
		font-size: 0.8rem;
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

	/* Done writes the spec at once, so the double outline ($lib/act.ts). */
	.commit:global([data-act='run']) {
		border: 3px double var(--accent);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
	}

	.buttons button:disabled,
	.remove:disabled {
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

	/* An ignored field's button runs `track <field>` at once (da-juku7). */
	.ignored button:global([data-act='run']) {
		border: 3px double var(--rule);
		padding: calc(0.2rem - 2px) calc(0.55rem - 2px);
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
