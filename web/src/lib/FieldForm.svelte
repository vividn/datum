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
	database — with a field of that shape, its colour, symbol, keys and
	states, to be read, changed and saved or not. Neither a shape nor an
	example takes the focus (da-zz9rn), vividn, 2026-10-19: «after selecting
	an example don't enter text edit mode and open the keyboard, that is
	jarring». The name line reads `field:` with «fields» beside it, the
	widget's (`pick` slot), which goes to another field, and on a field
	«rename». The box keeps its placeholder whole: the buttons wrap under it
	first (da-qimwi).

	Every field, a new one too, has the three segments — Basics · Data ·
	Aliases — none of them required (da-i9upr), their bar under the tabbed
	region and over Cancel and Save (da-nmhf2), and a swipe left or right
	across the region goes to the next or the one before (da-kmr29). Basics shows a switch's states,
	off first and disabled (`never_false`) only once there is another, and
	where its lane sits in the day (`Placement.svelte`), its entries drawn
	there as the form holds them (da-srrqd); a line that goes to
	Data, or on a card the Data tab's rows themselves. Its colour and the
	SYMBOL its dots draw (da-ogyxa) stand on one line, the symbol in the
	colour, each a popup (da-rpfws); the symbol is a character or an .svg
	(the spec's `icon`); a state is told apart by its colour alone, its own
	symbol kept under its `…` as advanced (da-puzij). A
	`glyph_by` (a symbol by a data key's value) is not edited here and a
	save leaves it as it was. A field with no name yet has no colour to
	show: its swatch is empty, not the pink an empty name hashes to. The data keys
	are `DataKeys.svelte`'s rows, with each key's type, on Data (da-zqnt3).
	Aliases are made by datum from
	the verb and the states until that is switched off (`no_autoalias`).
	The rows are `setup F --json`'s own `keys`, parsed by the CLI, so what is
	saved is what it reads back.

	What it writes is decided by what it was opened on. A new field gets
	ONE command, which one Undo takes back whole (da-2pbvd): `setup F
	--from-json` with the whole spec and `--track`, which writes the TRACK
	document with it (`declareArgv`). A field with a spec gets the whole
	edited spec back in ONE `setup F --from-json` — the list entire, never a
	stream of one-state changes. It is all handed up as `close`'s `Save` for
	the widget to run; null runs nothing. At the foot of a field's Basics,
	tracked or ignored, whose button runs `ignore F` or `track F` at once,
	handed up as `run` and leaving the form open.
-->
<script lang="ts">
	import { createEventDispatcher, tick } from 'svelte';
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
		draftSpec,
		draggedTo,
		editedSpec,
		EXAMPLES,
		keyRows,
		moved,
		placeOf,
		specArgv,
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

	const dispatch = createEventDispatcher<{ close: Save | null; run: Save }>();
	const TABS = ['basics', 'data', 'aliases'] as const;

	let tab: (typeof TABS)[number] = 'basics';
	let color = spec?.color ?? '';
	let description = spec?.description ?? '';
	let states: StateRow[] = spec ? stateRows(spec) : [];
	let glyph = spec?.glyph ?? '';
	let keys: KeyRow[] = spec ? keyRows(spec) : [];
	let autoalias = !spec?.no_autoalias;
	let neverFalse = !!spec?.never_false;
	let aliases: DerivedAlias[] = (spec?.derived_aliases ?? []).map((a) => ({ ...a }));
	let place: Place = spec ? placeOf(spec) : {};
	let icon = spec?.icon ?? '';
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
		place,
		icon,
		lookup
	};
	/** The spec a save would store, which the placement draws. */
	$: drafted = spec ? editedSpec(spec, edits) : draftSpec(field.trim(), edits);
	$: save = { argv: spec ? specArgv(editedSpec(spec, edits)) : declareArgv(field.trim(), edits) };
	/**
	 * What the placement draws, held while Basics is behind (da-nmhf2): it
	 * stays mounted there, every redraw is a CLI run (`dayview --sample`),
	 * and no one sees it. Basics in front again, it draws once.
	 */
	let sampled = { field: '', verb, spec: null as Spec | null };
	$: if (tab === 'basics') sampled = { field: field.trim(), verb, spec: field.trim() ? drafted : null };
	let opened: string | undefined;
	$: {
		const now = JSON.stringify([field, verb, edits]);
		opened ??= now;
		dirty = now !== opened;
	}
	// What `auto` paints: the name's colour, and none while there is no name.
	$: auto = field.trim() ? defaultColor(field.trim()) : 'transparent';
	$: named = states.filter((s) => s.name.trim());
	$: keyed = keys.map((k) => k.name.trim()).filter(Boolean);
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
		icon = '';
		[autoalias, aliases, place] = [true, [], {}];
		states = Object.entries(example.states ?? {}).map(([name, color]) => ({ name, color, description: '', glyph: '' }));
		keys = (example.keys ?? []).map((k) => ({ ...k, value: '', value_type: 'any' }));
		lookup = example.lookup ?? '';
		neverFalse = !!example.neverFalse;
		catalog = null;
	}

	// ---- the states' order (da-puzij), as the Data tab's keys are moved (DataKeys.svelte) ----
	let stateList: HTMLUListElement;
	/** The state a handle is dragging, and the one whose options are open. */
	let held: StateRow | null = null;
	let shown: StateRow | null = null;

	// A drag follows the pointer on the window: the row moves in the DOM as it goes, which would end a capture on the handle.
	function grab(e: PointerEvent, state: StateRow) {
		e.preventDefault();
		held = state;
		const drop = () => {
			held = null;
			removeEventListener('pointermove', drag);
			removeEventListener('pointerup', drop);
			removeEventListener('pointercancel', drop);
		};
		addEventListener('pointermove', drag);
		addEventListener('pointerup', drop);
		addEventListener('pointercancel', drop);
	}

	function drag(e: PointerEvent) {
		if (!held) return;
		const from = states.indexOf(held);
		const to = draggedTo([...stateList.querySelectorAll(':scope > li.state')], from, e.clientY);
		if (to !== from) states = moved(states, from, to);
	}

	/** The arrow keys move a row as a drag does, and the handle keeps the focus. */
	async function nudge(e: KeyboardEvent, i: number) {
		const to = i + ({ ArrowUp: -1, ArrowDown: 1 }[e.key] ?? 0);
		if (to === i || to < 0 || to >= states.length) return;
		e.preventDefault();
		states = moved(states, i, to);
		await tick();
		stateList.querySelectorAll<HTMLElement>(':scope > li.state .handle')[to]?.focus();
	}

	/** Hand `next` up to run, or null to run nothing. */
	const finish = (next: Save | null) => dispatch('close', next);

	// ---- swipe (da-kmr29, da-nmhf2) -----------------------------------------
	// vividn, 2026-10-19: «you should be able to switch between tabs in the setup widget by swiping left and right»;
	// 2026-10-22: «If you swipe on the tab bar itself, or in the margins, it should go to the surrounding container».
	// So it is the tabbed region's alone: begun on the head, the tab bar, the buttons or a margin, a swipe is the
	// pane's (PaneFrame.svelte) from any tab. In the region it is the pane's frame rule one level in (da-kcmp): the
	// first move past COMMIT_PX decides whose the gesture is. A horizontal one with a tab that way is the form's and
	// stops there; on the last tab it goes on to the frame. A vertical one stays a scroll, and one begun on a box, a
	// popup, the placement (which drags itself) or a row's drag handle is never the form's. Every tab is mounted side by side in the track,
	// inert but the one in front, so the finger drags the tab in front and its neighbor comes in with it. Released
	// past SWIPE_PX, or flicked, the track goes on to the neighbor; short of that it springs back. A tab pressed in
	// the bar slides there the same way.
	const COMMIT_PX = 10;
	const SWIPE_PX = 40;
	/** A swipe faster than this, in px a ms, and at least FLICK_PX long, is a flick: it goes on to the next tab. */
	const FLICK = 0.5;
	const FLICK_PX = 20;
	const KEEPS = 'input, textarea, select, dialog, [role="listbox"], [role="slider"], .placement, [data-pans], .handle';
	let swipe: { x: number; y: number; at: number; state: 'pending' | 'ours' | 'theirs' } | null = null;
	/** How far the finger has dragged the track from the tab in front, in px. */
	let offset = 0;
	/** The tab a swipe by `dx` brings on, if there is one that way. */
	const toward = (dx: number) => TABS[TABS.indexOf(tab) + (dx < 0 ? 1 : -1)];
	/** The reader asked for no motion: the tab changes at once, as the pane's do. */
	const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

	function swipeStart(event: TouchEvent) {
		const t = event.touches[0];
		const kept = (event.target as Element).closest(KEEPS);
		const theirs = event.touches.length > 1 || (kept && (event.currentTarget as Element).contains(kept));
		swipe = { x: t.clientX, y: t.clientY, at: event.timeStamp, state: theirs ? 'theirs' : 'pending' };
	}

	function swipeMove(event: TouchEvent) {
		if (!swipe || swipe.state === 'theirs') return;
		if (event.touches.length > 1) return drop();
		const dx = event.touches[0].clientX - swipe.x;
		const dy = event.touches[0].clientY - swipe.y;
		if (swipe.state === 'pending') {
			if (Math.max(Math.abs(dx), Math.abs(dy)) < COMMIT_PX) return;
			swipe.state = Math.abs(dx) > Math.abs(dy) && toward(dx) ? 'ours' : 'theirs';
			if (swipe.state === 'theirs') return;
		}
		event.stopPropagation();
		event.preventDefault();
		// Never toward a side with no tab on it.
		if (!still()) offset = toward(dx) ? dx : 0;
	}

	function swipeEnd(event: TouchEvent) {
		const was = swipe;
		drop();
		if (was?.state !== 'ours') return;
		event.stopPropagation();
		const dx = event.changedTouches[0].clientX - was.x;
		const far = Math.abs(dx);
		if (far >= SWIPE_PX || (far >= FLICK_PX && far > FLICK * (event.timeStamp - was.at))) tab = toward(dx) ?? tab;
	}

	/** The gesture is over: the track glides to the tab in front, the same one or the next. */
	function drop() {
		swipe = null;
		offset = 0;
	}
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

		<!-- The tabbed region (da-nmhf2): a swipe begun in it is the form's, one begun anywhere else the pane's. -->
		<div
			class="tabbed"
			on:touchstart={swipeStart}
			on:touchmove|nonpassive={swipeMove}
			on:touchend={swipeEnd}
			on:touchcancel={drop}
		>
			<div class="track" class:dragging={swipe?.state === 'ours'} style:transform="translateX(calc({-TABS.indexOf(tab) * 100}% + {offset}px))">
				{#each TABS as name (name)}
					<div class="panel" role="tabpanel" id="spec-panel-{name}" aria-labelledby="spec-tab-{name}" data-panel={name} inert={name !== tab}>
						{#if name === 'basics'}
							<!-- vividn, 2026-10-19: «move description to the very top of the basics tab». -->
							<label class="row">
								<span class="label">description</span>
								<input class="description" bind:value={description} />
							</label>
							<!-- vividn, 2026-10-19: «move color and symbol (not glyph) to one line, next to each other. make
							     the symbol take on the color of color». -->
							<div class="row">
								<span class="label">color</span>
								<Swatch bind:value={color} fallback={auto} label="the field's color" />
								<span class="label near">symbol</span>
								<Glyph bind:value={glyph} bind:icon color={color || (field.trim() && auto)} label="the field's symbol" />
							</div>
							{#if verb === 'switch'}
								<!-- A switch starts with one state, off (da-i9upr): vividn, «that can be disabled as soon as you have
								     more states». Off disabled is `never_false` (da-cwieh); a field that lists `false` keeps its `end`.
								     Each row is handle · color · name · … · ✕ (da-puzij); a state is told apart by its color, and
								     what it means and its symbol are under its …. Off is a row like the others, whose ✕ strikes it. -->
								{@const listsFalse = states.some((s) => s.name.trim() === 'false')}
								<ul class="states" bind:this={stateList}>
									<li class="off" class:disabled={neverFalse}>
										<span class="swatch" aria-hidden="true"></span>
										<input class="name" value="off" disabled aria-label="off" />
										<button
											type="button"
											class="remove off-state"
											aria-label="disable off"
											aria-pressed={neverFalse}
											title={listsFalse
												? 'false is one of its states, so the ring keeps end'
												: !neverFalse && named.length === 0
													? 'a switch needs another state before off can go'
													: 'off disabled, the fields widget leaves a gap where end would stand'}
											disabled={listsFalse || (!neverFalse && named.length === 0)}
											on:click={() => (neverFalse = !neverFalse)}>✕</button
										>
									</li>
									{#each states as state, i (state)}
										{@const which = state.name || 'this state'}
										<li class="state" class:held={held === state}>
											<!-- The order is where each state stands around the tap in the
											     fields widget (da-7q3ka): the first nearest the thumb. -->
											<button
												type="button"
												class="handle"
												aria-label="move {which}: drag, or the arrow keys"
												on:pointerdown={(e) => grab(e, state)}
												on:keydown={(e) => nudge(e, i)}>⠿</button
											>
											<Swatch bind:value={state.color} fallback={defaultColor(state.name.trim())} label="the color of {which}" />
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
												class="remove more"
												aria-label="the options of {which}"
												aria-expanded={shown === state}
												on:click={() => (shown = shown === state ? null : state)}>…</button
											>
											<button
												type="button"
												class="remove"
												aria-label="remove {state.name}"
												on:click={() => (states = states.filter((_, j) => j !== i))}>✕</button
											>
											{#if shown === state}
												<div class="options">
													<input class="meaning" bind:value={state.description} placeholder="what it means" aria-label="what {which} means" />
													<span class="advanced">advanced</span>
													<span class="row">
														<span class="label">symbol</span>
														<Glyph bind:value={state.glyph} color={state.color || color} label="the symbol of {which}" />
													</span>
												</div>
											{/if}
										</li>
									{/each}
								</ul>
								<button type="button" class="add" on:click={() => (states = [...states, { name: '', color: '', description: '', glyph: '' }])}
									>+ state</button
								>
								<label class="row" title="a bare state names the entry FIELD:<state>, as switch reading dune names book:dune">
									<span class="label">each state is a</span>
									<input class="lookup" bind:value={lookup} placeholder="field, e.g. book" spellcheck="false" autocapitalize="none" />
								</label>
							{/if}
							{#if sampled.verb && sampled.verb !== 'add'}
								{#key sampled.verb}<Placement field={sampled.field} occur={sampled.verb === 'occur'} bind:place spec={sampled.spec} />{/key}
							{/if}
							{#if verb === 'add'}
								<!-- vividn: «for card, embed the data widget directly on the basics». -->
								<span class="label">data</span>
								<DataKeys bind:keys card detail />
							{:else}
								<!-- vividn: «want to record extra data with each entry? [switch to data tab]». -->
								<p class="row data-line">
									<span class="label">data</span>
									<span class="ask"
										>{keyed.length ? `each entry records ${keyed.join(', ')}` : 'want to record extra data with each entry?'}</span
									>
									<button type="button" class="to-data" data-to-tab="data" on:click={() => (tab = 'data')}>data tab</button>
								</p>
							{/if}
							{#if spec}
								<!-- vividn, 2026-10-19: «put them back at the very bottom of the basics tab». Run at once, as the
								     ignored list's buttons are; a new field saves with `--track`. -->
								{@const off = ignored.includes(field)}
								<p class="row tracking" data-tracking={off ? 'ignored' : 'tracked'}>
									<span class="label">{off ? 'ignored' : 'tracked'}</span>
									<span class="ask">{off ? 'no button and not in now; its entries stay' : 'a button in the fields widget'}</span>
									<button
										type="button"
										use:act={'run'}
										title={shellJoin([off ? 'track' : 'ignore', field])}
										on:click={() => dispatch('run', { argv: [off ? 'track' : 'ignore', field] })}>{off ? 'track' : 'ignore'}</button
									>
								</p>
							{/if}
						{:else if name === 'data'}
							<DataKeys bind:keys card={verb === 'add'} detail />
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
				{/each}
			</div>
		</div>

		<!-- Under the tabs, over Cancel and Save (da-nmhf2): a swipe on it is the pane's. -->
		<div class="tabs" role="tablist">
			{#each TABS as name (name)}
				<button
					type="button"
					role="tab"
					id="spec-tab-{name}"
					aria-controls="spec-panel-{name}"
					data-tab={name}
					aria-selected={tab === name}
					on:click={() => (tab = name)}>{name}</button
				>
			{/each}
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

	/* The shapes, name, tabs and buttons stay put; a long tab scrolls in its
	   panel, and the region is never shorter than its first few rows. Its
	   own ground and border say where a swipe changes the tab (da-nmhf2).
	   Clipped, never scrolled: `hidden` lets a box brought into view while
	   a tab glides in scroll the region, and the tab lands off its place. */
	.tabbed {
		display: flex;
		flex: 1 1 8rem;
		min-block-size: 0;
		overflow: clip;
		background: var(--page);
		border: 1px solid var(--rule);
		border-radius: 6px;
	}

	/* Every tab side by side, the one in front in the box: a finger drags
	   it with no transition, and the release, a tab pressed in the bar and
	   the spring back of one too short glide. */
	.track {
		display: flex;
		flex: 1;
		min-inline-size: 0;
		transition: transform 150ms ease-out;
	}

	.track.dragging {
		transition: none;
	}

	.panel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 0 0 100%;
		box-sizing: border-box;
		min-inline-size: 0;
		padding: 0.5rem;
		overflow: auto;
	}

	/* A tab not in front is out of sight once the track has glided off it,
	   and in sight while a finger brings it on. */
	.panel[inert] {
		visibility: hidden;
		transition: visibility 0s 150ms;
	}

	.dragging > .panel {
		visibility: visible;
		transition: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.track,
		.panel[inert] {
			transition: none;
		}
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

	/* One control of three segments: the chosen one carries the accent
	   along its top edge (below). Each is as wide as its word and shares
	   out the rest, with next to no padding of its own, so the words fit
	   the place's modal on a 320px screen (tests/track.test.mjs). */
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
		border-block-start: 2px solid transparent;
		color: var(--muted);
		font: inherit;
		font-size: 0.7rem;
		cursor: pointer;
	}

	.tabs button:first-child {
		border-inline-start: 0;
	}

	/* The bar is under the region it names: the chosen one's accent on its top edge, toward it. */
	.tabs button[aria-selected='true'] {
		color: var(--ink-bright);
		border-block-start-color: var(--accent);
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

	/* `field:`, the name, and the widget's buttons at its end; over the shapes on a field that is. */
	.name-line .label {
		inline-size: auto;
	}

	/* The second label of a line: as wide as its word, a gap before it. */
	.label.near {
		inline-size: auto;
		margin-inline-start: 1rem;
	}

	.name-line {
		flex-wrap: wrap;
	}

	/* As wide as «what do you want to track?», or the buttons beside it wrap under it. */
	.name-line .field {
		flex: 1 1 27ch;
	}

	.name-line h2 {
		flex: 1;
		min-inline-size: 0;
		overflow-wrap: anywhere;
	}

	.name-line.over {
		order: -1;
	}

	/* Handle, swatch, name, …, ✕ on one line, off's too; a state's options
	   the width of the row under it. */
	.states {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.state,
	.off {
		position: relative;
		display: grid;
		grid-template-columns: 1.4rem 28px minmax(0, 1fr) auto auto;
		align-items: center;
		gap: 0.4rem;
	}

	.held {
		outline: 1px dashed var(--accent);
	}

	/* Off has no handle and no …: its swatch, name and ✕ stay in their columns. */
	.off .swatch {
		grid-column: 2;
		box-sizing: border-box;
		block-size: 28px;
		border: 1px solid var(--rule);
		border-radius: 4px;
	}

	.off .remove {
		grid-column: 5;
	}

	/* Disabled, the whole line struck through. */
	.off.disabled {
		opacity: 0.5;
	}

	.off.disabled::after {
		content: '';
		position: absolute;
		inset-inline: 0;
		inset-block-start: 50%;
		border-block-start: 1px solid var(--ink-bright);
		pointer-events: none;
	}

	.state .name,
	.off .name {
		min-inline-size: 0;
	}

	/* The handle takes the touch: the panel would scroll under it. */
	.handle {
		box-sizing: border-box;
		min-block-size: 24px;
		border: 0;
		background: none;
		padding: 0;
		color: var(--muted);
		font: inherit;
		cursor: grab;
		touch-action: none;
	}

	.remove[aria-expanded='true'] {
		border-color: var(--accent);
	}

	.options {
		grid-column: 2 / -1;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.advanced {
		color: var(--muted);
		font-size: 0.7rem;
		letter-spacing: 0.06em;
		border-block-end: 1px solid var(--rule);
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

	p.row {
		margin: 0;
	}

	.ask {
		flex: 1;
		color: var(--muted);
		font-size: 0.75rem;
	}

	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.buttons button,
	.ignored button,
	.tracking button,
	.to-data,
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

	/* An ignored field's button runs `track <field>` at once (da-juku7), and so does tracked / ignored. */
	.tracking button:global([data-act='run']),
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
