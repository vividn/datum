<!--
	The form a field is declared and edited in (da-94q1, da-ci85o), the body
	of the field widget ($lib/layout/FieldWidget.svelte, da-fu29p): vividn,
	2026-10-08 — «let's fully combine track/spec widgets into just 1».
	There is no dialog: a field is made and edited in a place.

	A field cannot be born without a verb (da-c8mh), and the four pictograms
	are the whole of that question. The glyphs are the CLI's own (`VERBS` in
	$lib/track.ts, `track_action_label` in datum-core): the row this form
	is about to write will print `TRACK●` in every action column there is,
	so what a person presses here is the shape they will meet again. Each is
	named by what a field of that kind DOES — happens, lasts, switches, a
	note — with the CLI's word under it.

	A NEW field starts as its name box and the examples (`EXAMPLES`, less
	the ones there is already a field for): one tap fills the form — never
	the database — with a field of that kind, to be read, changed and saved
	or not. Named, it shows Basics and nothing
	else: the kind, the colour, the keys its entries carry (da-h119q: a name
	and a KIND — required, optional, default or assign, da-p3hxn), the key
	the rest of a line goes to, and a switch's states. `track` carries all
	of that in one command, so that is all a new field is asked.

	A field with a spec has the five segments — Basics · States · Keys ·
	Aliases · Placement — and picks the GLYPH its dots draw (da-ogyxa): one
	for the field, or one by a data key's value, and one per state beside
	the state's colour. Keys are grouped required over the rest, and each
	row says whether the key is IN THE SLUG. Aliases are made by datum from
	the verb and the states until that is switched off (`no_autoalias`).
	The rows are `spec show`'s own `keys`, parsed by the CLI, so what is
	saved is what it reads back.

	What it writes is decided by what it was opened on. A new field gets
	`track`, the one command that writes the TRACK document. A field with a
	spec gets the whole edited spec back in ONE `spec set --from-json` — the
	`set` shape, the list entire, never a stream of `add-state`s. Either is
	handed up as `close`'s argv for the widget to run; null runs nothing.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { act } from './act.js';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import { defaultColor } from './fields.js';
	import Glyph from './Glyph.svelte';
	import Placement from './Placement.svelte';
	import { shellJoin } from './shellSplit.js';
	import Swatch from './Swatch.svelte';
	import {
		editedSpec,
		EXAMPLES,
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
		type Example,
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
	/** The fields there are already: an example of that name would re-declare one, so it is not offered. */
	export let taken: string[] = [];
	/** Whether it differs from what it opened as: what leaving it now would lose. */
	export let dirty = false;

	const dispatch = createEventDispatcher<{ close: string[] | null }>();
	const TABS = ['basics', 'states', 'keys', 'aliases', 'placement'] as const;

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
	/** The file the icon came from, while this form is open; a stored icon has none. */
	let iconName = '';
	/** The key a line's leftover words go to (`-R KEY`), or none. */
	let remainder = spec?.remainder ?? '';
	if (spec) verb = spec.verb;

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
					icon,
					remainder
				})
			)
		: trackArgv(field.trim() || '<field>', verb ?? 'occur', states, color, keys, remainder);
	let opened: string | undefined;
	$: {
		const now = JSON.stringify([verb, argv]);
		opened ??= now;
		dirty = now !== opened;
	}
	// A new field with no name and no kind yet is its name box and the examples.
	$: named = spec !== null || field.trim() !== '' || verb !== null;
	$: examples = EXAMPLES.filter((e) => !taken.includes(e.field));

	/** Fill the form with `example`: nothing is written until it is saved. */
	function fill(example: Example) {
		field = example.field;
		verb = example.verb;
		states = (example.states ?? []).map((name) => ({ name, color: '', description: '', glyph: '' }));
		keys = (example.keys ?? []).map((k) => ({ ...k, value: '', value_type: 'any', in_slug: false }));
		remainder = example.remainder ?? '';
	}

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

	/** Hand `next` up to run, or null to run nothing. */
	const finish = (next: string[] | null) => dispatch('close', next);
</script>

<div class="field-form" role="region" aria-label={spec ? `the spec of ${field}` : 'track a field'}>
	<form on:submit|preventDefault={() => ready && finish(argv)}>
		{#if spec}
			<h2>{field}</h2>
		{:else}
			<!-- svelte-ignore a11y-autofocus -->
			<input
				class="field"
				bind:value={field}
				placeholder="name a field"
				aria-label="the field's name"
				spellcheck="false"
				autocapitalize="none"
				autocomplete="off"
				autofocus
			/>
		{/if}
		{#if hint}
			<p class="hint">
				{hint}{#if fixed}
					<button type="button" class="fix" on:click={() => (field = fixed ?? '')}>use {fixed}</button
					>{/if}
			</p>
		{/if}

		{#if spec}
			<div class="tabs" role="tablist">
				{#each TABS as name (name)}
					<button
						type="button"
						role="tab"
						id="spec-tab-{name}"
						aria-controls="spec-panel"
						data-tab={name}
						aria-selected={tab === name}
						on:click={() => (tab = name)}>{name}</button
					>
				{/each}
			</div>
		{/if}

		<div class="panel" role="tabpanel" id="spec-panel" aria-labelledby={spec ? `spec-tab-${tab}` : undefined} data-panel={tab}>
			{#if !named && examples.length}
				<!-- Each fills the form, not the database; «+ new field» is the way back to them. -->
				<div class="examples" role="group" aria-label="examples">
					<span class="label">or start from an example</span>
					{#each examples as example (example.field)}
						<button type="button" data-example={example.field} on:click={() => fill(example)}>
							<span class="name"
								><span class="mark">{VERBS.find((v) => v.verb === example.verb)?.glyph}</span> {example.field}</span
							>
							<span class="what">{example.shows}</span>
						</button>
					{/each}
				</div>
			{/if}
			{#if named && tab === 'basics'}
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
							<span class="name">{choice.what}</span>
							<span class="what">{choice.verb}</span>
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
				<label class="row" title="the words of a line that no key takes are kept under this key, as -R KEY keeps them">
					<span class="label">rest of line</span>
					<input class="remainder" bind:value={remainder} placeholder="key" spellcheck="false" autocapitalize="none" />
				</label>
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
			{/if}
			<!-- A new field's states are a switch's, asked under its kind: `track` carries them for no other verb. -->
			{#if !spec && verb === 'switch'}<span class="label">states</span>{/if}
			{#if tab === 'states' || (!spec && verb === 'switch')}
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
			{:else if tab === 'aliases'}
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
			<button class="commit" use:act={'run'} type="submit" disabled={!ready}>{spec ? 'Done' : 'Save'}</button>
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
</div>

<style>
	/* The room under the widget's chips, and never less than its name, tabs,
	   a few rows and Done: a pane shorter than that scrolls the widget. A
	   long tab scrolls in the panel. */
	.field-form {
		display: flex;
		flex: 1 1 0;
		min-block-size: 14rem;
		color: var(--ink);
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.85rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1;
		min-inline-size: 0;
	}

	/* The name, hint, tabs and buttons stay put; a long tab scrolls here. */
	.panel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1;
		min-block-size: 0;
		overflow: auto;
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
	   and shares out the rest, with next to no padding of its own, so five
	   words fit the place's modal on a 320px screen (tests/track.test.mjs). */
	.tabs {
		display: flex;
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.tabs button {
		flex: 1 1 auto;
		min-block-size: 28px;
		padding-inline: 0.1rem;
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

	/* Two across at every width the form has: four fingertips, not a list
	   of words with a picture beside each — and the six examples the same. */
	.verbs,
	.examples {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.4rem;
	}

	.examples .label {
		grid-column: 1 / -1;
		inline-size: auto;
	}

	.examples button {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		background: #1a1a30;
		border: 1px solid var(--rule);
		border-radius: 6px;
		padding: 0.35rem 0.5rem;
		color: var(--ink-bright);
		font: inherit;
		text-align: start;
		cursor: pointer;
	}

	.examples button:hover,
	.examples button:focus-visible {
		border-color: var(--accent);
	}

	.mark {
		color: var(--accent);
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

	/* The pictogram is the point of the question: big enough to be a picture
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
