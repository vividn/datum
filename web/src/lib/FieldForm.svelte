<!--
	The form a field is declared and edited in (da-94q1, da-ci85o), the body
	of the setup widget ($lib/layout/FieldWidget.svelte, da-fu29p): vividn,
	2026-10-08 — «let's fully combine track/spec widgets into just 1».
	There is no dialog: a field is made and edited in a place.

	A field cannot be born without a verb (da-c8mh), and the four pictograms
	are the whole of that question. The glyphs are the CLI's own (`VERBS` in
	$lib/track.ts, `track_action_label` in datum-core): the row this form
	is about to write will print `TRACK●` in every action column there is,
	so what a person presses here is the shape they will meet again. Each is
	named as the welcome names it — occurrence, on/off, switch, card — with
	a line of what it is beside it.

	It opens on the SHAPES (da-uipyt), vividn, 2026-10-14: «rather than the
	name, start with the shape buttons. I think that ends up being the most
	important question to answer». Each is named and lined as the welcome
	names it, and on a new field has an «ex.» part to the side that opens
	its examples in a sheet (`EXAMPLES`, less the ones there is already a
	field for, da-yqx0t), each with its icon: one tap fills the form — never the
	database — with a field of that shape, its colour, glyph, keys and
	states, to be read, changed and saved or not. Neither a shape nor an
	example takes the focus (da-zz9rn), vividn, 2026-10-19: «after selecting
	an example don't enter text edit mode and open the keyboard, that is
	jarring». The name line reads `field:` with «fields» beside it, the
	widget's (`pick` slot), which goes to another field; a rename will stand
	there too.

	Every field, a new one too, has the five segments — Basics · States ·
	Data · Aliases · Placement — none of them required, and picks the
	GLYPH its dots draw (da-ogyxa): one for the field, or one by a data
	key's value, and one per state beside the state's colour. The data keys
	are `DataKeys.svelte`'s rows, on Basics and, with each key's type, on
	Data (da-zqnt3). Aliases are made by datum from
	the verb and the states until that is switched off (`no_autoalias`).
	The rows are `setup F --json`'s own `keys`, parsed by the CLI, so what is
	saved is what it reads back.

	What it writes is decided by what it was opened on. A new field gets
	ONE command, which one Undo takes back whole (da-2pbvd): `setup F
	--from-json` with the whole spec and `--track`, which writes the TRACK
	document with it (`declareArgv`). A field with a spec gets the whole
	edited spec back in ONE `setup F --from-json` — the list entire, never a
	stream of one-state changes. Tracked or ignored is not asked here
	(da-zz9rn): `ignore F` and `track F` say it. It is all handed up as
	`close`'s `Save` for the widget to run; null runs nothing.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { act } from './act.js';
	import DataKeys from './DataKeys.svelte';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import { defaultColor } from './fields.js';
	import Glyph from './Glyph.svelte';
	import Placement from './Placement.svelte';
	import { shellJoin } from './shellSplit.js';
	import Sheet from './Sheet.svelte';
	import Swatch from './Swatch.svelte';
	import {
		declareArgv,
		editedSpec,
		EXAMPLES,
		keyRows,
		moved,
		placeOf,
		specArgv,
		specKeys,
		stateRows,
		VERBS,
		type DerivedAlias,
		type Example,
		type KeyRow,
		type Place,
		type Save,
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
	/** The spec to edit (`setup F --json`); null declares a new field instead. */
	export let spec: Spec | null = null;
	/** The fields there are already: an example of that name would re-declare one, so it is not offered. */
	export let taken: string[] = [];
	/** Whether it differs from what it opened as: what leaving it now would lose. */
	export let dirty = false;

	const dispatch = createEventDispatcher<{ close: Save | null }>();
	const TABS = ['basics', 'states', 'data', 'aliases', 'placement'] as const;

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
	/** The field a bare state names an entry of (`state_lookup_field`), or none. */
	let lookup = spec?.state_lookup_field ?? '';
	/** The shape whose examples are open, if any. */
	let catalog: Verb | null = null;
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
	$: edits = {
		verb: verb ?? spec?.verb ?? 'occur',
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
		lookup
	};
	$: save = { argv: spec ? specArgv(editedSpec(spec, edits)) : declareArgv(field.trim(), edits) };
	let opened: string | undefined;
	$: {
		const now = JSON.stringify([field, verb, edits]);
		opened ??= now;
		dirty = now !== opened;
	}
	$: examples = EXAMPLES.filter((e) => e.verb === catalog && !taken.includes(e.field));
	const offers = (shape: Verb) => !spec && EXAMPLES.some((e) => e.verb === shape && !taken.includes(e.field));

	/**
	 * Fill the form with `example` — the whole of it, every tab: what was
	 * typed before it is not the example's. Nothing is written until it is saved.
	 */
	function fill(example: Example) {
		field = example.field;
		verb = example.verb;
		color = example.color;
		description = '';
		glyph = example.glyph ?? '';
		[byKey, glyphKey, glyphMap] = [false, '', []];
		[icon, iconName] = ['', ''];
		[autoalias, aliases, place] = [true, [], {}];
		states = Object.entries(example.states ?? {}).map(([name, color]) => ({ name, color, description: '', glyph: '' }));
		keys = (example.keys ?? []).map((k) => ({ ...k, value: '', value_type: 'any' }));
		lookup = example.lookup ?? '';
		neverFalse = !!example.neverFalse;
		catalog = null;
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

	/** Hand `next` up to run, or null to run nothing. */
	const finish = (next: Save | null) => dispatch('close', next);
</script>

<div class="field-form" role="region" aria-label={spec ? `the spec of ${field}` : 'track a field'}>
	<form on:submit|preventDefault={() => ready && finish(save)}>
		<div class="verbs" role="radiogroup" aria-label="what a press logs">
			{#each VERBS as choice (choice.verb)}
				<div class="shape" class:split={offers(choice.verb)}>
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
						<span class="line">{choice.line}</span>
					</button>
					{#if offers(choice.verb)}
						<button
							type="button"
							class="ex"
							use:act={'open'}
							data-examples={choice.verb}
							aria-haspopup="dialog"
							aria-label="examples of {choice.what}"
							on:click={() => (catalog = choice.verb)}>ex.</button
						>
					{/if}
				</div>
			{/each}
		</div>
		<!-- Each fills the form, not the database. -->
		<Sheet
			title="examples of {VERBS.find((v) => v.verb === catalog)?.what}"
			open={examples.length > 0}
			onclose={() => (catalog = null)}
			data-catalog={catalog}
		>
			<div class="examples" role="group" aria-label="examples">
				{#each examples as example (example.field)}
					<button type="button" data-example={example.field} on:click={() => fill(example)}>
						<span class="icon" style:color={example.color}>{example.glyph}</span>
						<span class="name">{example.field}</span>
						<span class="what">{example.shows}</span>
					</button>
				{/each}
			</div>
		</Sheet>
		<!-- A field's name is over its shapes, a new one's under them (da-uipyt): one line, moved by `order`. -->
		<div class="row name-line" class:over={spec}>
			<span class="label">field:</span>
			{#if spec}
				<h2>{field}</h2>
			{:else}
				<input
					class="field"
					bind:value={field}
					placeholder="what do you want to track?"
					aria-label="field:"
					spellcheck="false"
					autocapitalize="none"
					autocomplete="off"
				/>
			{/if}
			<slot name="pick" />
		</div>
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
					on:click={() => (tab = name)}>{name}</button
				>
			{/each}
		</div>

		<div class="panel" role="tabpanel" id="spec-panel" aria-labelledby="spec-tab-{tab}" data-panel={tab}>
			{#if tab === 'basics'}
				<!-- vividn, 2026-10-19: «move description to the very top of the basics tab». -->
				<label class="row first">
					<span class="label">description</span>
					<input class="description" bind:value={description} />
				</label>
				<div class="row top early">
					<span class="label">color</span>
					<Swatch bind:value={color} fallback={defaultColor(field.trim())} label="the field's color" />
				</div>
				<span class="label">data</span>
				<DataKeys bind:keys card={verb === 'add'} />
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
			<!-- A new field's states are a switch's, asked under its kind as well: its Save sends them for no other verb. -->
			{#if !spec && verb === 'switch' && tab === 'basics'}<span class="label late">states</span>{/if}
			{#if tab === 'states' || (!spec && verb === 'switch' && tab === 'basics')}
				<ul class="states" class:glyphed={tab === 'states'} class:late={tab === 'basics'}>
					{#each states as state, i (state)}
						<li class="state">
							<Swatch
								bind:value={state.color}
								fallback={defaultColor(state.name.trim())}
								label="the color of {state.name || 'this state'}"
							/>
							{#if tab === 'states'}
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
							{#if tab === 'states'}
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
					class:late={tab === 'basics'}
					on:click={() => (states = [...states, { name: '', color: '', description: '', glyph: '' }])}
					>+ state</button
				>
				{#if tab === 'states' && verb === 'switch'}
					<label class="row" title="a bare state names the entry FIELD:<state>, as switch reading dune names book:dune">
						<span class="label">each state is a</span>
						<input class="lookup" bind:value={lookup} placeholder="field, e.g. book" spellcheck="false" autocapitalize="none" />
					</label>
					<!-- da-cwieh — vividn: «for example environment is never false». Only a switch has a ring,
					     and a field that lists `false` as a state keeps its `end` whatever this says. -->
					{@const listsFalse = states.some((s) => s.name.trim() === 'false')}
					<label
						class="auto"
						title={listsFalse ? 'false is one of its states, so the ring keeps end' : 'the fields widget leaves a gap where end would stand'}
						><input type="checkbox" class="never-false" bind:checked={neverFalse} disabled={listsFalse} /> never false: no end</label
					>
				{/if}
			{:else if tab === 'data'}
				<DataKeys bind:keys card={verb === 'add'} detail />
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

		{#if ignored.length > 0}
			<!-- Ignored fields get no button in the bar; this is the way back (da-d08c). -->
			<div class="ignored">
				<span class="label">ignored</span>
				{#each ignored as name (name)}
					<button type="button" use:act={'run'} data-ignored={name} title={shellJoin(['track', name])} on:click={() => finish({ argv: ['track', name] })}
						>{name}</button
					>
				{/each}
			</div>
		{/if}
	</form>
</div>

<style>
	/* The room under the widget's sentence, and never less than its shapes,
	   name, tabs, a few rows of the tab and Done: a pane shorter than that
	   scrolls the widget. A long tab scrolls in the panel. */
	.field-form {
		display: flex;
		flex: 1 1 0;
		min-block-size: min-content;
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

	/* The shapes, name, tabs and buttons stay put; a long tab scrolls here,
	   and is never shorter than its first few rows. */
	.panel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1 1 8rem;
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
		background: var(--raised);
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
		background: var(--raised);
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

	/* Two across where two fit a shape's line, else one: four fingertips,
	   not a list of words with a picture beside each. */
	.verbs {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(9.5rem, 1fr));
		gap: 0.4rem;
	}

	/* A shape with examples is one card in two parts: the shape, and a
	   quarter of it to the side that reads «ex.» (vividn, 2026-10-15). */
	.shape {
		display: grid;
	}

	.shape.split {
		grid-template-columns: 3fr 1fr;
	}

	.shape.split .verb {
		border-start-end-radius: 0;
		border-end-end-radius: 0;
	}

	.ex {
		background: var(--raised);
		border: 1px solid var(--rule);
		border-inline-start: 0;
		border-radius: 0 6px 6px 0;
		color: var(--muted);
		font: inherit;
		font-size: 0.75rem;
		text-align: center;
		cursor: pointer;
	}

	.ex:hover,
	.ex:focus-visible {
		color: var(--accent);
		border-color: var(--accent);
	}

	:global(dialog.sheet[data-catalog]) {
		width: min(24rem, calc(100vw - 2rem));
	}

	.examples {
		display: grid;
		gap: 0.4rem;
	}

	/* The icon, then the name over what it shows. */
	.examples button {
		display: grid;
		grid-template-columns: 2rem 1fr;
		align-items: center;
		column-gap: 0.5rem;
		background: var(--raised);
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

	.examples .icon {
		grid-row: span 2;
		font-size: 1.5rem;
		text-align: center;
	}

	/* The pictogram beside the shape's name and line, so four fit over the
	   name box on a phone. */
	.verb {
		display: grid;
		grid-template-columns: auto 1fr;
		align-items: center;
		align-content: center;
		column-gap: 0.5rem;
		box-sizing: border-box;
		min-block-size: 24px;
		background: var(--raised);
		border: 1px solid var(--rule);
		border-radius: 6px;
		padding: 0.25rem 0.5rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
		text-align: start;
	}

	.verb .glyph {
		grid-row: span 2;
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

	.what,
	.line {
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

	/* `field:`, the name, and the widget's buttons at its end; over the shapes on a field that is. */
	.name-line .label {
		inline-size: auto;
	}

	.name-line h2 {
		flex: 1;
		min-inline-size: 0;
		overflow-wrap: anywhere;
	}

	.name-line.over {
		order: -1;
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
		background: var(--raised);
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

	.aliases {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		list-style: none;
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

	.first {
		order: -3;
	}

	/* A new switch's states under its colour on Basics, before the keys: what an example fills. */
	.early {
		order: -2;
	}

	.late {
		order: -1;
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
		background: var(--raised);
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
