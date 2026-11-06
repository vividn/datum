<!--
	The form a field is declared and edited in (da-94q1, da-ci85o), the body
	of the setup widget (#lib/layout/FieldWidget.svelte, da-fu29p): vividn,
	2026-10-08 — «let's fully combine track/spec widgets into just 1».
	There is no dialog: a field is made and edited in a place.

	A field cannot be born without a kind (da-c8mh), and the five pictograms
	are the whole of that question. The glyphs are the CLI's own (`VERBS` in
	#lib/track.ts, `track_action_label` in datum-core): the row this form
	is about to write will print `TRACK●` in every action column there is,
	so what a person presses here is the shape they will meet again. Each is
	named as the tutorial's games name it (`VERBS`, docs/basics.md) —
	moment, on/off, change, record, all day — the field kinds of the glossary (AGENTS.md) — with a line of what it is beside it.

	It opens on the SHAPES (da-uipyt), vividn, 2026-10-14: «rather than the
	name, start with the shape buttons. I think that ends up being the most
	important question to answer». Each is named as the tutorial's games
	name it. The examples are the premade widget's (da-36whg,
	#lib/layout/PremadeWidget.svelte), by theme: a tap there opens this form
	on its `example`, which fills it — never the database — with a field of
	that shape, its color, symbol, keys, aliases and states, to be read,
	changed and saved or not. Neither a shape nor an
	example takes the focus (da-zz9rn), vividn, 2026-10-19: «after selecting
	an example don't enter text edit mode and open the keyboard, that is
	jarring».

	The `field:` line is first, over the shapes (da-4ta1k, vividn,
	2026-11-05). With no field it holds «track something» and nothing else is
	drawn: a press opens a modal that says what a field is, takes a name,
	pilled in its hash color as it is typed, and lists the fields there are,
	narrowed by it, to edit one instead. A field entered or chosen is a pill
	in its color on the line, whose × starts over, beside the widget's
	«rename» on a spec (`pick` slot). Under it the status line: NOT SET UP
	until a new field is saved, TRACKED with its current state where it has
	one and «stop tracking» (`ignore F`, at once), IGNORED with «start
	tracking» (`track F`), each handed up as `run` and leaving the form open.

	Every field, a new one too, has the three tabs — basics · data · aliases —
	none of them required (da-i9upr), but a record's data is on basics, so it
	has no data tab (da-eh4dl), their bar under the panel and over the
	buttons (da-nmhf2), changed by a tap or the arrow keys and never by a
	swipe (da-ooflv), vividn, 2026-10-28: «bring the data and aliases tabs
	back on the setup widget, but no swipe action between them»; a swipe
	across the form is the pane's (PaneFrame.svelte), so the super tabs
	swipe as they did with no tabs (da-4t0yn). Basics shows a switch's states,
	off first and disabled (`never_false`) only once there is another, and
	where its lane sits in the day (`Placement.svelte`), its entries drawn
	there as the form holds them (da-srrqd), and on a record the data rows
	themselves. Over each tab's button but the open one's is what it holds
	(da-eh4dl, vividn, 2026-11-05): basics its kind, color and size, data its
	keys and aliases its aliases, five and «+n more», which a press opens into
	the tab, not under `prefers-reduced-motion`. Its color and the
	SYMBOL its dots draw (da-ogyxa) stand on one line, the symbol in the
	colour, each a popup (da-rpfws); the symbol is a character or an .svg
	(the spec's `icon`); a state is told apart by its colour alone, its own
	symbol kept under its `…` as advanced (da-puzij). A
	`symbol_by` (a symbol by a data key's value) is not edited here and a
	save leaves it as it was. A field with no name yet has no colour to
	show: its swatch is empty, not the pink an empty name hashes to. The data keys
	are `DataKeys.svelte`'s rows, with each key's type, on data (da-zqnt3).
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
	the widget to run; null runs nothing. A field's is `pending` too while it
	differs, which the widget runs when the form is left, and its buttons are
	Revert · Save · Redo, Revert and Redo handed up for the widget to walk the
	spec's history (da-hda4j); a new field's are cancel · save without
	tracking (`--ignore`) · start tracking (`--track`). Brought from
	another pane (`away`, da-wz84y), a spec's row starts with Cancel, and
	Cancel and a spec's Save wear the chevron, for they go back there; a Save
	that stays says «saved» under it for a moment.
-->
<script lang="ts">
	import { tick, untrack, type Snippet } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { act } from './act.js';
	import DataKeys from './DataKeys.svelte';
	import { fieldNameHint, suggestFieldName } from './fieldName.js';
	import { defaultColor, type FieldButton } from './fields.js';
	import Glyph from './Glyph.svelte';
	import Placement from './Placement.svelte';
	import Sheet from './Sheet.svelte';
	import { shellJoin } from './shellSplit.js';
	import Swatch from './Swatch.svelte';
	import {
		declareArgv,
		draftSpec,
		draggedTo,
		editedSpec,
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

	let {
		/** The field name it opens with — the CLI's refusal names one. */
		field = '',
		/** The verb it opens with, chosen; none unless the door that opened it preset one. */
		verb = null,
		/** The fields there are (`fields --json`): the modal's list, and whether this one is ignored. */
		fields = [],
		/** The spec to edit (`setup F --json`); null declares a new field instead. */
		spec = null,
		/** The example a new field's form opens filled with (the premade widget's), or none. */
		example = null,
		/** Whether it differs from what it opened as: what leaving it now would lose. */
		dirty = $bindable(false),
		/** A spec's edits as the save leaving it runs (da-hda4j), or null while there are none. */
		pending = $bindable(null),
		/** Whether Revert, at a save point, has a saved version to step back to, and Redo one to step forward to. */
		back = false,
		ahead = false,
		/** Whether the person was brought here from another pane, so Cancel and a spec's Save go back there (da-wz84y). */
		away = false,
		/** Whether a save that stayed is being said under Save. */
		saved = false,
		/** Beside the field's pill: the widget's own buttons. */
		pick,
		/** Hand a save up to run, or null to run nothing and leave. */
		onclose,
		/** Run a command and stay: tracking or ignoring the field. */
		onrun,
		onrevert,
		onredo,
		/** Go to another field, `''` a new one: the pill's ×, the modal's list, an anchor's link in the placement. */
		onopen
	}: {
		field?: string;
		verb?: Verb | null;
		fields?: FieldButton[];
		spec?: Spec | null;
		example?: Example | null;
		dirty?: boolean;
		pending?: Save | null;
		back?: boolean;
		ahead?: boolean;
		away?: boolean;
		saved?: boolean;
		pick?: Snippet;
		onclose: (save: Save | null) => void;
		onrun: (save: Save) => void;
		onrevert: () => void;
		onredo: () => void;
		onopen: (field: string) => void;
	} = $props();

	// The form is drawn from the spec it opened with; a new spec mounts a new form.
	const opening = untrack(() => spec);
	let color = $state(opening?.color ?? '');
	let description = $state(opening?.description ?? '');
	let states: StateRow[] = $state(opening ? stateRows(opening) : []);
	let symbol = $state(opening?.symbol ?? '');
	let keys: KeyRow[] = $state(opening ? keyRows(opening) : []);
	let autoalias = $state(!opening?.no_autoalias);
	let neverFalse = $state(!!opening?.never_false);
	let aliases: DerivedAlias[] = $state((opening?.derived_aliases ?? []).map((a) => ({ ...a })));
	let place: Place = $state(opening ? placeOf(opening) : {});
	let icon = $state(opening?.icon ?? '');
	/** The field a bare state names an entry of (`state_lookup_field`), or none. */
	let lookup = $state(opening?.state_lookup_field ?? '');
	type Tab = 'basics' | 'data' | 'aliases';
	let tab: Tab = $state('basics');
	let bar: HTMLElement | undefined = $state();
	if (opening) verb = opening.verb;
	// A record's data is on basics: it has no data tab (da-eh4dl).
	const TABS: Tab[] = $derived(verb === 'add' ? ['basics', 'aliases'] : ['basics', 'data', 'aliases']);
	$effect.pre(() => {
		if (!TABS.includes(tab)) tab = 'basics';
	});

	// What the name would do if it were submitted, before the press rather
	// than after it: `my coffee` is what a person naturally types and the
	// parser refuses it (da-b049).
	const hint = $derived(spec || field.trim() === '' ? null : fieldNameHint(field.trim()));
	const fixed = $derived(hint === null ? null : suggestFieldName(field.trim()));
	const ready = $derived(field.trim() !== '' && hint === null && verb !== null);
	// The form's edits as one value, which the draft, the save and `dirty` all read.
	const edits = $derived({
		verb: verb ?? spec?.verb ?? 'occur',
		color,
		description,
		states,
		keys,
		autoalias,
		aliases,
		neverFalse,
		symbol,
		place,
		icon,
		lookup
	});
	/** The spec a save would store, which the placement draws. */
	const drafted = $derived(spec ? editedSpec(spec, edits) : draftSpec(field.trim(), edits));
	const save: Save = $derived({ argv: spec ? specArgv(editedSpec(spec, edits)) : declareArgv(field.trim(), edits) });
	let opened: string | undefined;
	$effect.pre(() => {
		const now = JSON.stringify([field, verb, edits]);
		opened ??= now;
		const changed = now !== opened;
		dirty = changed;
		pending = spec && changed && ready ? save : null;
	});
	// What `auto` paints: the name's colour, and none while there is no name.
	const auto = $derived(field.trim() ? defaultColor(field.trim()) : 'transparent');
	const named = $derived(states.filter((s) => s.name.trim()));
	const keyed = $derived(keys.map((k) => k.name.trim()).filter(Boolean));

	// ---- what each tab holds, over its button while another is open (da-eh4dl) ----
	/** Five names, and how many more. */
	const five = (names: string[]) => names.slice(0, 5).join(', ') + (names.length > 5 ? ` +${names.length - 5} more` : '');
	const kind = $derived(VERBS.find((v) => v.verb === verb));
	const size = $derived(
		verb === 'add' || verb === 'allday'
			? ''
			: verb === 'occur'
				? `dot ×${(place.symbol_scale ?? 1).toFixed(2)}`
				: place.relative_to
					? `×${place.anchor_size_ratio ?? 1} of ${place.relative_to}`
					: place.height !== undefined
						? `h ${place.height.toFixed(2)}`
						: ''
	);
	const aliasNames = $derived(aliases.map((a) => a.state.trim()).filter(Boolean));
	const summary = $derived({
		data: keyed.length ? five(keyed) : 'log extra data with each entry',
		aliases: aliasNames.length ? five(aliasNames) : autoalias ? 'made from the kind and states' : 'none'
	});
	const summaries: Record<Tab, HTMLElement | undefined> = $state({ basics: undefined, data: undefined, aliases: undefined });
	/** Where the tab being opened unfolds from: its summary, as it was when pressed. */
	let unfoldFrom: DOMRect | null = null;
	const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

	/** Open a tab, its summary unfolding into it. */
	function choose(name: Tab) {
		if (name === tab) return;
		unfoldFrom = summaries[name]?.getBoundingClientRect() ?? null;
		tab = name;
	}

	/** The panel grows out of the summary pressed into its own place (FLIP), and at once under reduced motion. */
	function unfold(node: HTMLElement) {
		const from = unfoldFrom;
		unfoldFrom = null;
		if (!from || from.width === 0 || still()) return { duration: 0 };
		const to = node.getBoundingClientRect();
		const [dx, dy, sx, sy] = [from.left - to.left, from.top - to.top, from.width / to.width, from.height / (to.height || 1)];
		return {
			duration: 200,
			easing: cubicOut,
			css: (t: number, u: number) =>
				`transform-origin: 0 0; transform: translate(${u * dx}px, ${u * dy}px) scale(${t + u * sx}, ${t + u * sy}); opacity: ${0.3 + 0.7 * t}`
		};
	}

	// A new field opened on an example is that example, every tab of it; nothing is written until it is saved.
	const given = untrack(() => (opening ? null : example));
	if (given) {
		field = given.field;
		verb = given.verb;
		color = given.color;
		symbol = given.glyph;
		lookup = given.lookup ?? '';
		neverFalse = !!given.neverFalse;
		states = Object.entries(given.states ?? {}).map(([name, color]) => ({ name, color, description: '', glyph: '' }));
		keys = (given.keys ?? []).map((k) => ({ ...k, value: '', value_type: 'any' }));
		if (given.aliases) {
			autoalias = false;
			aliases = given.aliases.map((a) => ({ ...a }));
		}
	}

	// ---- the states' order (da-puzij), as the data keys are moved (DataKeys.svelte) ----
	let stateList: HTMLUListElement | undefined = $state();
	/** The state a handle is dragging, and the one whose options are open. */
	let held: StateRow | null = $state(null);
	let shown: StateRow | null = $state(null);

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
		const to = draggedTo([...stateList!.querySelectorAll(':scope > li.state')], from, e.clientY);
		if (to !== from) states = moved(states, from, to);
	}

	/** The arrow keys move a row as a drag does, and the handle keeps the focus. */
	async function nudge(e: KeyboardEvent, i: number) {
		const to = i + ({ ArrowUp: -1, ArrowDown: 1 }[e.key] ?? 0);
		if (to === i || to < 0 || to >= states.length) return;
		e.preventDefault();
		states = moved(states, i, to);
		await tick();
		stateList!.querySelectorAll<HTMLElement>(':scope > li.state .handle')[to]?.focus();
	}

	/** The arrow keys walk the tab bar, as a pane's (PaneFrame.svelte): the tab focused is the tab shown. */
	async function onTabKey(e: KeyboardEvent, i: number) {
		const to = i + ({ ArrowLeft: -1, ArrowRight: 1 }[e.key] ?? 0);
		if (to === i || to < 0 || to >= TABS.length) return;
		e.preventDefault();
		choose(TABS[to]);
		await tick();
		bar?.querySelectorAll<HTMLElement>('[role="tab"]')[to]?.focus();
	}

	/** Enter in the name or the description lets go of the box, which puts a phone's keyboard away: it neither
	 *  moves on nor saves, saving being Save's alone (vividn, 2026-10-28, da-uz2td). */
	function putDown(e: KeyboardEvent) {
		if (e.key !== 'Enter' || e.isComposing) return;
		e.preventDefault();
		(e.currentTarget as HTMLElement).blur();
	}

	/** Hand `next` up to run, or null to run nothing. */
	const finish = (next: Save | null) => onclose(next);

	// ---- the field line (da-4ta1k): «track something» with no field, its pill with one ----
	const empty = $derived(!spec && field.trim() === '');
	const here = $derived(fields.find((f) => f.field === field));
	const off = $derived(!!here?.ignored);
	/** The modal open, and the name typed in it. */
	let asking = $state(false);
	let typed = $state('');
	const draft = $derived(typed.trim());
	const draftHint = $derived(draft === '' ? null : fieldNameHint(draft));
	const draftFix = $derived(draftHint === null ? null : suggestFieldName(draft));
	const glyphOf = (f: FieldButton) => f.glyph ?? VERBS.find((v) => v.what === f.shape.kind)?.glyph;

	/** A name entered: a field there is opens, a new one is this form's, and what it is is measured from it, so its × asks nothing. */
	function enter(name = draft): void {
		if (name === '' || fieldNameHint(name) !== null) return;
		asking = false;
		if (fields.some((f) => f.field === name)) return onopen(name);
		field = name;
		opened = undefined;
	}
</script>

<div class="field-form" role="region" aria-label={spec ? `the spec of ${field}` : 'track a field'}>
	<form
		onsubmit={(e) => {
			e.preventDefault();
			if (ready) finish(save);
		}}
	>
		<!-- vividn, 2026-10-28: «don't freeze the shapes at the top of the setup widget allow them to scroll with
		     everything else» (da-uz2td): the shapes, name and tab in front scroll as one, over the tabs and buttons. -->
		<div class="scroll">
			<div class="row name-line">
				<span class="label">field:</span>
				{#if empty}
					<button type="button" class="track-something" use:act={'open'} aria-haspopup="dialog" onclick={() => ((typed = ''), (asking = true))}
						>track something</button
					>
				{:else}
					<span class="pill" style:--c={color || auto} data-field={field}>
						{#if spec}<h2>{field}</h2>{:else}<span class="named">{field}</span>{/if}
						<button type="button" class="clear" aria-label="clear {field}" onclick={() => onopen('')}>×</button>
					</span>
					{@render pick?.()}
				{/if}
			</div>
			{#if !empty}
			{#if hint}
				<p class="hint">
					{hint}{#if fixed}
						{' '}<button type="button" class="fix" onclick={() => (field = fixed ?? '')}>use {fixed}</button
						>{/if}
				</p>
			{/if}
			<p class="row status" data-status={!spec ? 'new' : off ? 'ignored' : 'tracked'}>
				<span class="word">{!spec ? 'NOT SET UP' : off ? 'IGNORED' : 'TRACKED'}</span>
				{#if spec && !off && (verb === 'switch' || verb === 'start')}
					<span class="now">current state: <b>{here?.current === true ? 'on' : (here?.current ?? 'off')}</b></span>
				{/if}
				{#if spec}
					<button
						type="button"
						use:act={'run'}
						title={shellJoin([off ? 'track' : 'ignore', field])}
						onclick={() => onrun({ argv: [off ? 'track' : 'ignore', field] })}>{off ? 'start tracking' : 'stop tracking'}</button
					>
				{/if}
			</p>
			<div class="verbs" role="radiogroup" aria-label="what a press logs">
				{#each VERBS as choice (choice.verb)}
					<button
						type="button"
						class="verb"
						class:on={verb === choice.verb}
						role="radio"
						aria-checked={verb === choice.verb}
						data-verb={choice.verb}
						onclick={() => (verb = choice.verb)}
					>
						<span class="glyph">{choice.glyph}</span>
						<span class="name">{choice.what}</span>
						<span class="line">{choice.line}</span>
					</button>
				{/each}
			</div>
			<!-- The tab in front (da-ooflv), changed by a tap or the arrow keys in the bar under it and never by a swipe:
			     vividn, 2026-10-28, «bring the data and aliases tabs back on the setup widget, but no swipe action between
			     them». A swipe across the form is the pane's (PaneFrame.svelte), so the super tabs swipe as they did. -->
			{#key tab}
			<div class="body" role="tabpanel" id="spec-panel-{tab}" aria-labelledby="spec-tab-{tab}" data-panel={tab} in:unfold>
				{#if tab === 'basics'}
					<!-- vividn, 2026-10-19: «move description to the very top of the basics tab». -->
					<label class="row">
						<span class="label">description</span>
						<input class="description" bind:value={description} enterkeyhint="done" onkeydown={putDown} />
					</label>
					<!-- vividn, 2026-10-19: «move color and symbol (not glyph) to one line, next to each other. make
					     the symbol take on the color of color». -->
					<div class="row">
						<span class="label">color</span>
						<Swatch bind:value={color} fallback={auto} label="the field's color" />
						<span class="label near">symbol</span>
						<Glyph bind:value={symbol} bind:icon color={color || (field.trim() && auto)} label="the field's symbol" />
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
										? 'false is one of its states, so the ring keeps off'
										: !neverFalse && named.length === 0
											? 'a change field needs another state before off can go'
											: 'off disabled, the collect widget leaves a gap where off would stand'}
									disabled={listsFalse || (!neverFalse && named.length === 0)}
									onclick={() => (neverFalse = !neverFalse)}>✕</button
								>
							</li>
							{#each states as state, i (state)}
								{@const which = state.name || 'this state'}
								<li class="state" class:held={held === state}>
									<!-- The order is where each state stands around the tap in the
									     collect widget (da-7q3ka): the first nearest the thumb. A drag on the
									     handle reorders, and is never the pane's tab swipe (`data-pans`). -->
									<button
										type="button"
										class="handle"
										data-pans="x"
										aria-label="move {which}: drag, or the arrow keys"
										onpointerdown={(e) => grab(e, state)}
										onkeydown={(e) => nudge(e, i)}>⠿</button
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
										onclick={() => (shown = shown === state ? null : state)}>…</button
									>
									<button
										type="button"
										class="remove"
										aria-label="remove {state.name}"
										onclick={() => (states = states.filter((_, j) => j !== i))}>✕</button
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
						<button type="button" class="add" onclick={() => (states = [...states, { name: '', color: '', description: '', glyph: '' }])}
							>+ state</button
						>
						<label class="row" title="a bare state names the entry FIELD:<state>, as change reading dune names book:dune">
							<span class="label">each state is a</span>
							<input class="lookup" bind:value={lookup} placeholder="field, e.g. book" spellcheck="false" autocapitalize="none" />
						</label>
					{/if}
					<!-- An all-day field's mark is in the day's all-day band, which `dayview --sample` does not draw (da-0cf4z).
					     Its lane keys are kept, as a record's are: the band draws the mark at the lane's height and size. -->
					{#if verb && verb !== 'add' && verb !== 'allday'}
						{#key verb}<Placement field={field.trim()} occur={verb === 'occur'} bind:place spec={drafted} {onopen} />{/key}
					{/if}
					{#if verb === 'add'}
						<!-- vividn: «for card, embed the data widget directly on the basics»; the data tab shows the same rows. -->
						<span class="label">data</span>
						<DataKeys bind:keys card detail />
					{/if}
				{:else if tab === 'data'}
					<DataKeys bind:keys detail />
				{:else}
					<label class="auto"
						><input type="checkbox" class="autoalias" bind:checked={autoalias} /> made from the kind and states</label
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
										placeholder="{(verb === 'start' ? 'on' : verb === 'allday' ? 'allday' : VERBS.find((v) => v.verb === verb)?.what) ?? 'moment'} {field}"
										aria-label="what {alias.state || 'it'} runs"
										spellcheck="false"
										autocapitalize="none"
									/>
									<button
										type="button"
										class="remove"
										aria-label="remove {alias.state}"
										onclick={() => (aliases = aliases.filter((a) => a !== alias))}>✕</button
									>
								{/if}
							</li>
						{/each}
					</ul>
					{#if autoalias}
						<p class="later">made again from the kind and states when saved</p>
					{:else}
						<button
							type="button"
							class="add"
							onclick={() => (aliases = [...aliases, { state: '', expands_to: '' }])}>+ alias</button
						>
					{/if}
				{/if}
			</div>
			{/key}
			{/if}
		</div>

		{#if !empty}
		<!-- What each tab holds, over its button while another is open (da-eh4dl). -->
		<div class="summaries" style:--n={TABS.length}>
			{#each TABS as name (name)}
				<p class="summary" id="spec-summary-{name}" data-summary={name} bind:this={summaries[name]}>
					{#if name === tab}
						<!-- The open tab says it in full above. -->
					{:else if name === 'basics'}
						<span class="mark" style:color={color || auto}>{symbol || kind?.glyph || '·'}</span>
						{kind?.what ?? 'no kind yet'}{#if size}{' · '}{size}{/if}
					{:else}
						{summary[name]}
					{/if}
				</p>
			{/each}
		</div>
		<!-- Under the panel, over the buttons (da-nmhf2). -->
		<div class="tabs" role="tablist" bind:this={bar} style:--n={TABS.length}>
			{#each TABS as name, i (name)}
				<button
					type="button"
					role="tab"
					id="spec-tab-{name}"
					aria-controls="spec-panel-{name}"
					aria-describedby={tab === name ? undefined : `spec-summary-${name}`}
					data-tab={name}
					aria-selected={tab === name}
					tabindex={tab === name ? 0 : -1}
					onclick={() => choose(name)}
					onkeydown={(e) => onTabKey(e, i)}>{name}</button
				>
			{/each}
		</div>

		<div class="buttons">
			{#if spec}
				{#if away}
					<button type="button" class="cancel" data-away onclick={() => finish(null)}>Cancel</button>
				{/if}
				<button type="button" class="revert" use:act={dirty ? undefined : 'run'} disabled={!dirty && !back} onclick={onrevert}>Revert</button>
				<span class="save">
					<button class="commit" use:act={'run'} data-away={away || undefined} type="submit" disabled={!ready || !dirty}>Save</button>
					{#if saved}<span class="saved" role="status">saved</span>{/if}
				</span>
				<button type="button" class="redo" use:act={'run'} disabled={dirty || !ahead} onclick={onredo}>Redo</button>
			{:else}
				<button type="button" class="cancel" data-away={away || undefined} onclick={() => finish(null)}>cancel</button>
				<button
					type="button"
					class="untracked"
					use:act={'run'}
					disabled={!ready}
					onclick={() => finish({ argv: declareArgv(field.trim(), edits, true) })}>save without tracking</button
				>
				<button class="commit" use:act={'run'} type="submit" disabled={!ready}>start tracking</button>
			{/if}
		</div>
		{/if}
	</form>

	<!-- What a field is, a name for a new one, or one there is to edit (da-4ta1k). -->
	<Sheet title="track something" open={asking} onclose={() => (asking = false)} data-picker>
		<p class="what">A field is one thing you track. For example mood, coffee, or sleep.</p>
		<div class="row naming">
			<span class="label">field name:</span>
			<span class="pill" style:--c={draft ? defaultColor(draft) : 'var(--rule)'}>
				<!-- svelte-ignore a11y_autofocus -->
				<input
					class="field"
					bind:value={typed}
					aria-label="field name:"
					spellcheck="false"
					autocapitalize="none"
					autocomplete="off"
					enterkeyhint="enter"
					autofocus
					onkeydown={(e) => e.key === 'Enter' && !e.isComposing && (e.preventDefault(), enter())}
				/>
				<button type="button" class="enter" aria-label="enter" disabled={draft === '' || draftHint !== null} onclick={() => enter()}>⏎</button>
			</span>
		</div>
		{#if draftHint}
			<p class="hint">
				{draftHint}{#if draftFix}
					{' '}<button type="button" class="fix" onclick={() => (typed = draftFix ?? '')}>use {draftFix}</button>{/if}
			</p>
		{/if}
		{#if fields.length > 0}
			<p class="or">or edit an existing field</p>
			<div class="chips" role="group" aria-label="fields">
				{#each fields.filter((f) => f.field.toLowerCase().includes(draft.toLowerCase())) as f (f.field)}
					<button type="button" data-chip={f.field} data-ignored={f.ignored || undefined} style:--c={f.color} onclick={() => enter(f.field)}
						><span aria-hidden="true">{glyphOf(f)}</span> {f.field}</button
					>
				{/each}
			</div>
		{/if}
	</Sheet>
</div>

<style>
	/* The room under the widget's sentence, and never less than its shapes,
	   name, a few rows of the body and Done: a pane shorter than that
	   scrolls the widget. A long body scrolls in itself. */
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

	/* The tabs and buttons stay put; the shapes, name and tab in front
	   scroll here, which is never shorter than a few rows. */
	.scroll {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1 1 8rem;
		min-block-size: 0;
		overflow: auto;
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	/* One control of three segments: the chosen one carries the accent
	   along its top edge (below). Each is as wide as its word and shares
	   out the rest, with next to no padding of its own, so the words fit
	   a 320px screen (tests/track.test.mjs). */
	.tabs {
		display: grid;
		grid-template-columns: repeat(var(--n), minmax(0, 1fr));
		border: 1px solid var(--rule);
		border-radius: 4px;
		overflow: hidden;
	}

	.tabs button {
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

	/* The bar is under the panel it names: the chosen one's accent on its top edge, toward it. */
	.tabs button[aria-selected='true'] {
		color: var(--ink-bright);
		border-block-start-color: var(--accent);
	}

	/* Each over its button, in the same columns, its last line on the bar. */
	.summaries {
		display: grid;
		grid-template-columns: repeat(var(--n), minmax(0, 1fr));
		gap: 0.3rem;
		margin-block-end: -0.3rem;
	}

	.summary {
		align-self: end;
		margin: 0;
		padding-inline: 0.2rem;
		color: var(--muted);
		font-size: 0.65rem;
		line-height: 1.25;
		overflow-wrap: anywhere;
	}

	.summary .mark {
		font-size: 0.85rem;
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

	/* Two across where two fit a shape's line, else one: five fingertips,
	   not a list of words with a picture beside each. */
	.verbs {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(9.5rem, 1fr));
		gap: 0.4rem;
	}

	/* The pictogram beside the shape's name and line, so five fit over the
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

	/* `field:`, the pill, and the widget's buttons at its end, over the shapes (da-4ta1k). */
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

	/* A field's name in its color, its × at its end; in the modal, the box typed in. */
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		min-inline-size: 0;
		box-sizing: border-box;
		min-block-size: 28px;
		border: 1px solid var(--c);
		border-radius: 999px;
		background: color-mix(in srgb, var(--c) 30%, var(--raised));
		padding-inline: 0.6rem 0.15rem;
	}

	.pill h2,
	.pill .named {
		min-inline-size: 0;
		overflow-wrap: anywhere;
		font-size: 0.9rem;
		letter-spacing: 0.06em;
		color: var(--ink-bright);
	}

	.pill input {
		flex: 1;
		min-inline-size: 0;
		border: 0;
		background: none;
		padding-inline: 0;
	}

	.naming .pill {
		flex: 1;
	}

	.pill button {
		flex: none;
		min-block-size: 24px;
		min-inline-size: 24px;
		border: 0;
		border-radius: 999px;
		background: none;
		color: var(--ink-bright);
		font: inherit;
		cursor: pointer;
	}

	.pill button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* NOT SET UP, TRACKED or IGNORED, and its button at the right. */
	.status {
		flex-wrap: wrap;
	}

	.status .word {
		color: var(--ink-bright);
		font-size: 0.75rem;
		letter-spacing: 0.06em;
	}

	.status .now {
		color: var(--muted);
		font-size: 0.75rem;
	}

	.status button {
		margin-inline-start: auto;
	}

	.what,
	.or {
		margin: 0 0 0.5rem;
		color: var(--quiet);
	}

	.or {
		margin-block-start: 0.75rem;
		font-size: 0.75rem;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}

	.chips button {
		box-sizing: border-box;
		min-block-size: 28px;
		background: var(--raised);
		border: 1px solid var(--c, var(--rule));
		border-radius: 999px;
		padding: 0.1rem 0.6rem;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.chips span {
		color: var(--c);
	}

	/* An ignored field, offered to edit and start tracking again. */
	.chips [data-ignored] {
		border-style: dashed;
		color: var(--muted);
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

	/* The handle takes the touch: the body would scroll under it. */
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

	.buttons {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.buttons button,
	.track-something,
	.status button,
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

	/* Save writes the spec at once, so the double outline (#lib/act.ts). */
	.commit:global([data-act='run']) {
		border: 3px double var(--accent);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
	}

	/* Revert and Redo at a save point step the spec along its history at once (da-hda4j). */
	.revert:global([data-act='run']),
	.redo:global([data-act='run']) {
		border: 3px double var(--rule);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
	}

	/* «saved», under Save, fading (da-wz84y): in a line kept under the row always, so the row stays still when it shows. */
	.save {
		position: relative;
		display: flex;
		padding-block-end: 0.9rem;
	}

	.saved {
		position: absolute;
		inset-block-end: 0;
		inset-inline: 0;
		text-align: center;
		font-size: 0.75rem;
		line-height: 1.2;
		color: var(--accent);
		animation: said 2s ease-in forwards;
		pointer-events: none;
	}

	@keyframes said {
		60% {
			opacity: 1;
		}
		to {
			opacity: 0;
		}
	}

	.buttons button:disabled,
	.remove:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* Stop and start tracking run `ignore F` and `track F` at once (da-juku7), and save without tracking its one `setup F`. */
	.status button:global([data-act='run']),
	.untracked:global([data-act='run']) {
		border: 3px double var(--rule);
		padding: calc(0.2rem - 2px) calc(0.8rem - 2px);
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
