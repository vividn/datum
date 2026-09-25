<!--
	The fields widget — every field `datum fields` lists, and the primary
	way anything gets logged without typing (da-nz29, da-c7u8).

	Track + comes first: the way into the whole app, so it is there on an
	empty database too. Then every field that is not ignored (`ignored` in
	`datum fields --json`, da-d08c), spec or no spec — in ROWS by how high
	up the pictures they are drawn, top to bottom, and most used first
	along each row (`fieldRows`, da-xtkcp), an order taken on load and on
	coming back to the foreground and never while in use — each shaped by
	its kind and painted by what it is
	now (`lookOf` in fields.ts): a point is an empty pill with its occur
	mark beside the name; a block is a
	rectangle, filled while it is on; a switch is a rectangle filled with
	its current state's colour; a keyed record is its name, underlined, and
	a click prefills `add <field> ` in the bottom bar rather than running
	anything. The pane keeps the height its layout gives it and the BAR is
	the scroller (da-mbz2) — `ButtonRows`, every button bar's layout: as
	many rows as fit, up to three, each packed on its own and all of them
	scrolled left and right (da-vu7pi). NOTHING HANGS OUT OF IT: the widget's only
	floating surface was the `-t` / `dur` / `note` panel, and da-e78x6
	removed it.
	Ignored fields get no button: the Track modal lists them, and a press
	runs `track <field>`. `showIgnored` (da-xhio) is the way to SEE them
	without going through Track + — a switch in the menu the pane's NAME
	opens (da-vhwq), this session's and off by default. It adds them to the
	list; it does not change what they are, so each is drawn faint and
	dashed and says «ignored» when a pointer rests on it.

	A PENDING VERB (`verb`, da-ltux) is the commands widget talking to this
	one through the command line they share: the list becomes the fields
	that verb can be spent on, a press MEANS that verb and runs at once,
	and Track + is the `+` at the end of the list instead of the button
	before its start — the one door on the modal that PRESETS a verb, which
	is what `startTracking`'s note below leaves room for. Which fields, and
	what a press then runs, are `fieldsUnder` and `verbPress` in commands.ts.

	A SELECTION talks to it too (da-qu16). A `dur=` pill on the bar leaves
	the fields a length can be spent on — the blocks — and composes with a
	pending verb, both filters applying. And a press on a PICTURE gives
	the fields drawn near that height the rows to themselves, at the head
	of them (`rowsUnder`, $lib/lanes.ts): that one only ORDERS, so
	everything is still there, with a hairline after the promoted few to
	say why the bar changed. Nothing moves under a finger: while a pointer
	is over the bar the order is held until it leaves.

	Track + opens that modal (da-94q1, $lib/track.ts) — a `<dialog>` over
	the whole page with a pictogram per verb — rather than a form hanging
	off the bar. The widget still runs nothing itself: the modal answers
	with an argv and it goes out the same `run` as a press.

	Press to log. HOLD — or right-click, or shift+click, or the menu key —
	to put the field on the bar as a PILL (da-8810n), which is how the rest
	of the page is narrowed to it: the stats widget and the tail show that
	field, the commands widget keeps the commands that take one, and the
	pictures outline that field's own marks. The pill is minted in the
	field's own colour and cleared like any other pill, by its ×.

	HOLD HAS EXACTLY THAT ONE MEANING (vividn, 2026-09-22: «mint the pill,
	let's remove that -t/dur panel»). It had two for a day — da-8810n put
	the pill first and left the older `-t` / `dur` / `note` panel a second
	hold away — and da-e78x6 took the panel out rather than keep a gesture
	that means two things. So a hold on a field already pilled mints it
	again and opens nothing, and the panel's jobs are reached elsewhere: a
	TIME or a DURATION comes from the time cursor as a `-t` / `dur=` pill
	(a click or a drag on any drawn picture, $lib/cursor.ts) or is typed,
	and both ride after the field pill into the very argv a press composes
	(`withPills` in $lib/selection.ts); a multi-state field's states are
	the list a PLAIN press already fills the bar with (da-g635); and `note`
	is typed — `note <field> <words>`, the CLI routing the bare words into
	the comment (da-py5w).

	A multi-state field asks WHICH, so its button reads `name:` and a press
	fills the bar with that field's states in place of the fields — `< Back`
	first, `+` last (da-g635). Nothing floats: the list is the widget's own
	content, in the same scroller and under the same height rules, and the
	pane is scrolled back to where it stood when Back returns the fields. A
	press on a state runs `switch <field> <state>` — plainly, the bar's own
	pills being the only thing that adds to it — and comes back; `+` is an
	input, and Enter on it runs the same command with a state the field has
	never been in, which is how one is learned. The gesture still pills the
	field.

	The widget never runs anything itself: it composes an argv and
	dispatches it, and the page runs it through `run()` and echoes the
	result's own argv into the REPL. That is the teach loop, and it only
	teaches the truth because the echoed argv IS the executed argv — there
	is no second string built for display that could drift from it.
-->
<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import ButtonRows from './ButtonRows.svelte';
	import {
		lookOf,
		pressArgv,
		stateLook,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type FieldShape,
		type Standings
	} from './fields.js';
	import { holds } from './hold.js';
	import { fieldPill, setFieldPill } from './bar.js';
	import { rowsUnder, type Near } from './lanes.js';
	import { openTrack } from './specModal.js';
	import {
		declaredVerb,
		fieldsUnder,
		fieldsWithDuration,
		lanesOf,
		verbPress,
		type Verb
	} from './commands.js';

	export let fields: FieldButton[] = [];
	export let disabled = false;
	/** How a switch in a state is labelled; see `SwitchLabel`. */
	export let switchLabel: SwitchLabel = 'field';
	/**
	 * The verb the command line is pending (da-ltux), or null.
	 *
	 * It filters the list to the fields that verb can be spent on and
	 * makes a press MEAN that verb — so `start` on a switch field with
	 * `true` among its states runs `start <field>`, not the state list a
	 * plain press would open. The `+` at the end is then the way to a
	 * field that does not exist yet, with the verb already chosen.
	 */
	export let verb: Verb | null = null;
	/**
	 * Whether the bar holds a DURATION — a `dur=` pill (da-qu16).
	 *
	 * It leaves the fields a length can be spent on: the blocks. It
	 * COMPOSES with `verb` — both filters apply.
	 */
	export let duration = false;
	/**
	 * What a press on a picture landed on or near, field by rank
	 * (`nearest` in $lib/lanes.ts), or null.
	 *
	 * Those fields come FIRST. Nothing is hidden by it: a person who
	 * clicked empty space still finds every field, in its usual order.
	 */
	export let rank: Map<string, Near> | null = null;
	/**
	 * Whether the IGNORED fields get a button too (da-xhio).
	 *
	 * Off is today's bar: a field whose latest status is IGNORED is not
	 * drawn, and `Track +` is where it is offered back. On adds it to the
	 * list, drawn as an ignored field and not as a tracked one — a second
	 * way to see it, never a way to stop it being ignored.
	 *
	 * This session's, not the layout's: the option is declared `session`
	 * in the registry and held in $lib/layout/session.ts.
	 */
	export let showIgnored = false;
	/** Where each field stood at the last sort (`standings` in $lib/layout/engine.ts, da-xtkcp). */
	export let standings: Standings = new Map();

	const dispatch = createEventDispatcher<{ run: string[]; line: string }>();

	let bar: HTMLElement | undefined;
	// The hold's action ($lib/hold.ts) on the bar ButtonRows draws — an
	// action cannot be handed through a component, so it is attached here.
	onMount(() => holds(bar!, { within: 'button[data-field]', run: holdField }).destroy);

	/**
	 * The field whose STATES have taken the bar over, by name (da-g635).
	 *
	 * By name and not by the button, because the entries are rebuilt on
	 * every beat: held by name, the list redraws from the field as it is
	 * now — the state just logged marked — rather than from a copy that
	 * went stale the moment it was picked.
	 */
	let picked: string | null = null;
	/** Whether the `+` at the end of that list has become its input. */
	let addingState = false;
	let newState = '';
	/** The pane's scroller and where it stood when the states took over. */
	let scrolled: HTMLElement | null = null;
	/**
	 * Where the scroller stood when the states took over — BOTH axes, since
	 * the scroll rule decides which one the bar is on (da-ltux) and a short
	 * pane is scrolled sideways.
	 */
	let scrolledTo = { left: 0, top: 0 };

	/** Whether a pointer — a mouse, a finger — is over the bar right now. */
	let over = false;
	/**
	 * The ranking the bar is DRAWN under.
	 *
	 * It follows `rank`, except while a pointer is over the bar: no button
	 * is moved out from under a finger (vividn asked for calm). It catches
	 * up the moment the pointer leaves, which is what this statement
	 * re-running on `over` is for.
	 */
	let shown: Map<string, Near> | null = null;
	$: if (!over) shown = rank;

	$: buttons = fieldsUnder(
		fieldsWithDuration(
			fields.filter((w) => showIgnored || !w.ignored),
			duration
		),
		verb
	);
	$: ignored = fields.filter((w) => w.ignored);
	$: picking = picked === null ? null : (buttons.find((b) => b.field === picked) ?? null);
	// A picked field no longer on the bar — deleted, the database emptied or
	// restored, another tab's write, or a verb or `dur=` pill filtering it
	// out — is Back, not a list held in waiting to take the bar over again
	// the moment a field of that name returns (da-mv8ng).
	$: if (picked !== null && picking === null) back();

	/**
	 * What the bar draws. A picked field's states (da-g635): `< Back` first
	 * — it logs nothing — then one per state in the CLI's order, and `+`
	 * last, which is how a state never logged gets into the list at all,
	 * dealt into the rows by index. Otherwise the fields in their rows
	 * (`arrange`), with Track + starting the top one — the way into the app
	 * on an empty database — or, under a pending verb, the `+` ending the
	 * bottom one (vividn: «There should be a + field at the end of this
	 * filtered fields view for adding new fields»).
	 */
	type Entry =
		| { kind: 'track' | 'hint' | 'back' | 'add-state' }
		| { kind: 'state'; state: string }
		| { kind: 'field'; button: FieldButton; promoted: boolean; edge: boolean };
	$: entries = ((): Entry[] => {
		if (picking)
			return [
				{ kind: 'back' },
				...statesOf(picking.shape).map((state): Entry => ({ kind: 'state', state })),
				{ kind: 'add-state' }
			];
		if (buttons.length > 0) return [];
		return verb === null ? [{ kind: 'track' }, { kind: 'hint' }] : [{ kind: 'hint' }, { kind: 'track' }];
	})();
	/**
	 * The fields' rows (da-xtkcp): by height top to bottom, most used first
	 * along each, as they stood at the last sort — and a press's fields at
	 * the head of them, the hairline after the last of them in each row.
	 */
	$: arrange =
		picking || buttons.length === 0
			? lanesOf
			: (_: Entry[], rows: number): Entry[][] => {
					const lanes = rowsUnder(buttons, rows, standings, shown).map(({ lead, rest }): Entry[] => [
						...lead.map(
							(button, k): Entry => ({
								kind: 'field',
								button,
								promoted: true,
								edge: k === lead.length - 1 && rest.length > 0
							})
						),
						...rest.map((button): Entry => ({ kind: 'field', button, promoted: false, edge: false }))
					]);
					if (verb === null) lanes[0].unshift({ kind: 'track' });
					else lanes[lanes.length - 1].push({ kind: 'track' });
					return lanes;
				};
	const keyOf = (e: Entry) => (e.kind === 'field' ? `f:${e.button.field}` : e.kind === 'state' ? `s:${e.state}` : e.kind);

	/** A look as the custom properties every button below is painted from. */
	function vars(look: Look) {
		return `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`;
	}

	function statesOf(shape: FieldShape): string[] {
		return shape.kind === 'states' ? shape.states : [];
	}

	/**
	 * What the HOLD gesture does — the long press, the right click, the
	 * shift+click and the menu key, which are the four vividn named
	 * (da-8810n).
	 *
	 * ONE meaning: the field goes on the bar as a pill, in its own colour,
	 * and the page narrows to it. A field already pilled is pilled again —
	 * there is no second step to disambiguate against, because the `-t` /
	 * `dur` / `note` panel that used to be it is gone (da-e78x6).
	 */
	function hold(button: FieldButton) {
		setFieldPill(button.field, button.color);
	}

	/**
	 * What a button says it does, which is the one place both gestures are
	 * named: the press, then the hold.
	 */
	function titleOf(button: FieldButton): string {
		const press =
			button.shape.kind === 'record' ? `add ${button.field} … — fills the prompt` : 'press to log';
		return [
			...(button.ignored ? ['ignored'] : []),
			press,
			'hold to filter the page to it'
		].join(' · ');
	}

	/**
	 * Leave the state list for the field list, with the pane where it was.
	 *
	 * The states are fewer than the fields, so the shorter list scrolls the
	 * pane's own scroller back up under the reader — where they were is
	 * remembered by `pick` and given back here, after the list is drawn.
	 */
	function back() {
		if (picked === null) return;
		picked = null;
		addingState = false;
		newState = '';
		const scroller = scrolled;
		const at = scrolledTo;
		scrolled = null;
		if (scroller)
			void tick().then(() => {
				scroller.scrollLeft = at.left;
				scroller.scrollTop = at.top;
			});
	}

	function emit(argv: string[]) {
		back();
		dispatch('run', argv);
	}

	/** Put a line in the command line, unrun, and leave the lists as they were. */
	function line(text: string) {
		back();
		dispatch('line', text);
	}

	/**
	 * Ask «which verb?» and run the answer (da-94q1).
	 *
	 * The one door the widget has on the Track modal, so the set-up
	 * walkthrough has a single place to take it over. Nothing is preset
	 * from here: the doors that DO preset one — a filtered list's `+`, the
	 * CLI's own refusal — call `openTrack` themselves. A dismissed modal
	 * answers null and nothing runs.
	 */
	async function startTracking() {
		const argv = await openTrack({ ignored: ignored.map((w) => w.field) });
		if (argv) emit(argv);
	}

	/**
	 * Show `button`'s states in the bar, in place of the fields (da-g635).
	 *
	 * Nothing floats: the list is REPLACED inside the widget's own box, so
	 * the pane keeps the height its layout gives it (da-mbz2) and the
	 * states are reached by the same scroll the fields are.
	 */
	function pick(button: FieldButton) {
		scrolled = scrollerOf();
		scrolledTo = { left: scrolled?.scrollLeft ?? 0, top: scrolled?.scrollTop ?? 0 };
		picked = button.field;
	}

	/** What scrolls, either way: the bar itself, or an ancestor that took the overflow. */
	function scrollerOf(): HTMLElement | null {
		for (let node: HTMLElement | null = bar ?? null; node; node = node.parentElement) {
			if (node.scrollHeight > node.clientHeight || node.scrollWidth > node.clientWidth) return node;
		}
		return null;
	}

	function press(button: FieldButton) {
		// With a verb pending, the press IS that verb (C1: a field tap runs
		// at once) — except where `<verb> <field>` is not a command, and
		// `verbPress` hands back null so the two courtesies below apply.
		const asked = verb === null ? null : verbPress(verb, button);
		if (asked) return emit(asked);
		// A record is typed, never pressed: its line goes to the prompt.
		if (button.shape.kind === 'record') return line(`add ${button.field} `);
		// A multi-state field can't be logged without saying which, so a
		// plain press asks — the bar fills with its states.
		if (button.shape.kind === 'states') pick(button);
		else emit(pressArgv(button));
	}

	/**
	 * The `+` at the end of a filtered list: the same modal, with the
	 * pending verb already chosen (da-ltux).
	 *
	 * One of the doors `startTracking` above leaves the presetting to. The
	 * verb is mapped to one a field can be DECLARED with — `end` is not, so
	 * it declares the block it would have ended (`declaredVerb`).
	 */
	async function trackUnder(pending: Verb) {
		const argv = await openTrack({
			verb: declaredVerb(pending),
			ignored: ignored.map((w) => w.field)
		});
		if (argv) emit(argv);
	}

	/**
	 * Enter in the `+`'s input. A state is learned from the log, so
	 * `switch` is the whole of adding one: it is in the list next time.
	 * Empty cancels, as Escape does.
	 */
	function addState(button: FieldButton) {
		const state = newState.trim();
		if (state === '') {
			addingState = false;
			newState = '';
			return;
		}
		emit(pressArgv(button, state));
	}

	/**
	 * The hold, from the app's ONE implementation of it ($lib/hold.ts): the
	 * timer, the right click, the menu key and shift+click are all in the
	 * action, on the BAR rather than on each button, and `data-field` is
	 * what says which of its children can be held. A press that turns out
	 * to be a hold never reaches the buttons' own `on:click` at all, so
	 * there is nothing left here to swallow it with.
	 */
	const holdField = (el: HTMLElement) => {
		const button = buttons.find((b) => b.field === el.dataset.field);
		if (button) hold(button);
	};

	function onWindowKeyDown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		// Innermost first: the input the `+` became, then the state list
		// itself — Escape is its Back. The modal has its own Escape, and
		// the dialog swallows it.
		if (addingState) {
			addingState = false;
			newState = '';
		} else back();
	}
</script>

<svelte:window on:keydown={onWindowKeyDown} />

<ButtonRows
	items={entries}
	{arrange}
	key={keyOf}
	bind:bar
	let:item
	on:pointerenter={() => (over = true)}
	on:pointerleave={() => (over = false)}
>
	<!-- Given a line colour, or `var(--line)` voids its border and the rows
	     are counted two pixels short of the buttons they hold. -->
	<button slot="probe" class="widget" style="--line: transparent" tabindex="-1">probe</button>
	{#if item.kind === 'back'}
		<div class="slot">
			<button class="nav back" data-back {disabled} title="back to the fields" on:click={back}>
				&lt; Back
			</button>
		</div>
	{:else if item.kind === 'state' && picking}
		{@const look = stateLook(picking, item.state)}
		<div class="slot">
			<button
				class="widget states"
				class:on={look.fill !== null}
				style={vars(look)}
				{disabled}
				data-state={item.state}
				title={pressArgv(picking, item.state).join(' ')}
				on:click={() => picking && emit(pressArgv(picking, item.state))}
			>
				{item.state}
			</button>
		</div>
	{:else if item.kind === 'add-state' && picking}
		{@const chosen = picking}
		<div class="slot">
			{#if addingState}
				<form on:submit|preventDefault={() => addState(chosen)}>
					<!-- svelte-ignore a11y-autofocus -->
					<input
						class="new-state"
						bind:value={newState}
						placeholder="state"
						aria-label="a new state for {chosen.field}"
						spellcheck="false"
						autofocus
					/>
				</form>
			{:else}
				<button
					class="nav add-state"
					data-add-state
					{disabled}
					title="add a state to {chosen.field}"
					on:click={() => (addingState = true)}
				>
					+
				</button>
			{/if}
		</div>
	{:else if item.kind === 'track'}
		<div class="slot">
			{#if verb === null}
				<button
					class="track add-button"
					{disabled}
					title="track a new field"
					on:click={() => void startTracking()}
				>
					Track +
				</button>
			{:else}
				{@const pending = verb}
				<button
					class="nav add-field"
					data-add-field
					{disabled}
					title="track a new {pending} field"
					on:click={() => void trackUnder(pending)}
				>
					+
				</button>
			{/if}
		</div>
	{:else if item.kind === 'hint'}
		<!-- No direction word here on purpose: this hint said "below" while the
		     bar was at the top of the page (da-nz29), and da-1x72 moved the
		     bar to the bottom without the copy following. Name the thing, not
		     where it is. -->
		<p class="hint empty">nothing tracked yet — or type a command to log something</p>
	{:else if item.kind === 'field'}
		{@const button = item.button}
		{@const look = lookOf(button, switchLabel)}
		<div class="slot" class:edge={item.edge} data-promoted={item.promoted ? '' : undefined}>
			{#if button.shape.kind === 'record'}
				<!-- A PRESS runs nothing: the line goes to the bar's prompt. The
				     HOLD is every field's (da-8810n) — a record is a field like
				     any other, and only what a press MEANS is its own. -->
				<button
					class="record"
					class:ignored={button.ignored}
					class:pilled={$fieldPill === button.field}
					role="link"
					data-field={button.field}
					data-ignored={button.ignored ? '' : undefined}
					data-pilled={$fieldPill === button.field ? '' : undefined}
					style={vars(look)}
					{disabled}
					title={titleOf(button)}
					on:click={() => press(button)}>{button.field}</button
				>
			{:else}
				<button
					class="widget {button.shape.kind}"
					class:on={look.fill !== null}
					class:ignored={button.ignored}
					class:pilled={$fieldPill === button.field}
					style={vars(look)}
					{disabled}
					data-field={button.field}
					data-ignored={button.ignored ? '' : undefined}
					data-pilled={$fieldPill === button.field ? '' : undefined}
					title={titleOf(button)}
					on:click={() => press(button)}
				>{#if look.glyph}<span class="glyph" aria-hidden="true">{look.glyph}</span>{/if}{look.label}{button.shape.kind === 'states' ? ':' : ''}</button>
			{/if}
		</div>
	{/if}
</ButtonRows>

<style>
	/* The press's own quiet mark (da-qu16): a hairline after the fields a
	   click promoted, so the reader can SEE why the bar changed. It says
	   where the promotion ends and nothing else — no fill, no animation,
	   and the buttons themselves are drawn exactly as they always are.
	   Nothing is drawn when the press promoted them ALL: a line with
	   nothing on the far side of it says nothing. */
	.slot.edge {
		padding-right: 0.4rem;
		border-right: 1px solid var(--rule);
	}

	.widget,
	.track,
	.record,
	.nav {
		/* A held press must not also select the label or raise the platform's
		   text callout, on touch least of all. */
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
		touch-action: manipulation;
		min-height: 24px;
		font: inherit;
		cursor: pointer;
	}

	/* Shape by kind, fill by state (lookOf in fields.ts): the colours
	   arrive as --fill / --line / --text. */
	.widget {
		background: var(--fill);
		border: 1.5px solid var(--line);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--text);
		font-size: 0.85rem;
	}

	.widget.point {
		border-radius: 999px;
	}

	/* A point's mark (da-h2io6): what an occur of it draws, in its colour. */
	.glyph {
		margin-right: 0.35em;
		color: var(--line);
	}

	.widget:hover:not(:disabled) {
		filter: brightness(1.2);
	}

	/* The field whose PILL is on the bar (da-8810n): a ring in the app's
	   ink, so a reader can see where the pill came from. In the ink rather
	   than in the field's colour, because the button is already drawn in
	   that and a ring of it would read as part of the shape. */
	.widget.pilled,
	.record.pilled {
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--ink-bright) 55%, transparent);
	}

	/* A record has no box of its own, so its ring needs one to sit on. */
	.record.pilled {
		border-radius: 3px;
	}

	/* An IGNORED field, drawn because `showIgnored` asked for it (da-xhio).
	   It keeps its colour and its shape — it is still that field — and is
	   faded and dashed, which is how the widget already says «not kept»
	   (`.track-this`). A tracked field is never either, so the two cannot
	   be read for one another. */
	.widget.ignored,
	.record.ignored {
		opacity: 0.45;
	}

	.widget.ignored {
		border-style: dashed;
	}

	.record.ignored {
		text-decoration-style: dashed;
	}

	.widget:disabled,
	.track:disabled,
	.record:disabled,
	.nav:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* `< Back` and `+`: the two ends of the state list, and neither of them
	   a state — so they are drawn in the app's own ink rather than in any
	   field's colour, and cannot be mistaken for something to log. */
	.nav {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 3px;
		padding: 0.35rem 0.85rem;
		color: var(--ink);
		font-size: 0.85rem;
	}

	.nav:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.new-state {
		width: 7rem;
	}

	/* The first thing a person meets: larger than the fields, and filled. */
	.track {
		background: var(--accent);
		border: none;
		border-radius: 0.5rem;
		padding: 0.45rem 1.1rem;
		color: var(--page);
		font-size: 1rem;
		font-weight: 700;
		letter-spacing: 0.02em;
		box-shadow: 0 0 12px color-mix(in srgb, var(--accent) 40%, transparent);
	}

	/* Not `--accent-bright`: this is the accent as a FILL under a pointer,
	   which lifts less than the accent as ink does. */
	.track:hover:not(:disabled) {
		background: #9a9aff;
	}

	.record {
		background: none;
		border: none;
		padding: 0.35rem 0.3rem;
		color: var(--ink);
		font-size: 0.85rem;
		text-decoration: underline 2px var(--line);
		text-underline-offset: 0.3em;
	}

	.untracked {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--muted);
		font-size: 0.8rem;
	}

	.track-this {
		background: none;
		/* Not `--rule`: a dashed outline reads lighter than a solid one, so
		   it is drawn a step up to weigh the same. */
		border: 1px dashed #3a3a6a;
		border-radius: 3px;
		padding: 0 0.35rem;
		color: var(--accent);
		font-size: 0.7rem;
	}

	.track-this:hover:not(:disabled) {
		border-color: var(--accent);
	}

	.new-state {
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.8rem;
	}

	.hint {
		margin: 0;
		color: var(--quiet);
		font-size: 0.8rem;
		font-style: italic;
		white-space: nowrap;
	}
</style>
