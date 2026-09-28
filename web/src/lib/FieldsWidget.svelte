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
	field's own colour and cleared like any other pill, by its × — or by
	holding the same field again.

	HOLD HAS EXACTLY THAT ONE MEANING (vividn, 2026-09-22: «mint the pill,
	let's remove that -t/dur panel»). It had two for a day — da-8810n put
	the pill first and left the older `-t` / `dur` / `note` panel a second
	hold away — and da-e78x6 took the panel out rather than keep a gesture
	that means two things. So a hold on a field already pilled opens
	nothing: it TOGGLES, taking that pill away, locked or not (da-05oit —
	vividn: «long pressing or right clicking on a field again after a pill is
	added should remove the pill/filter»). The panel's jobs are reached
	elsewhere: a TIME or a DURATION comes from the time cursor as a `-t` /
	`dur=` pill (a click or a drag on any drawn picture, $lib/cursor.ts)
	or is typed,
	and both ride after the field pill into the very argv a press composes
	(`withPills` in $lib/selection.ts); a multi-state field's states are
	the list a PLAIN press already fills the bar with (da-g635); and `note`
	is typed — `note <field> <words>`, the CLI routing the bare words into
	the comment (da-py5w).

	A multi-state field asks WHICH, so its button reads `name:` and a press
	fills the bar with that field's states in place of the fields (da-g635)
	— RADIALLY AROUND THE TAP (da-7q3ka, `ringOf` in fields.ts): the
	spec's order is a rank, the first nearest the thumb, so a state is
	always the same place relative to where the field was, like a pie
	menu. A `false` state, `end`, takes the field's own position, with
	`< Back` left of it and `+ Add state` right; those three are hollow,
	the other states a light tint, the one the field is in solid and bold.
	Near an edge the ring is capped, not shifted: `end` stays on the tap
	and the states spread along the free sides (da-8mk9p). Nothing floats: the ring is the
	widget's own content, over the bar's own box, and the pane is
	scrolled back to where it stood when Back returns the fields. A
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
		ringOf,
		lookOf,
		pressArgv,
		stateLook,
		type Spot,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type FieldShape,
		type Standings
	} from './fields.js';
	import { act } from './act.js';
	import { holds } from './hold.js';
	import { fieldPill, setFieldPill } from './bar.js';
	import { rowsUnder, type Near } from './lanes.js';
	import { openTrack } from './specModal.js';
	import { shellJoin } from './shellSplit.js';
	import {
		declaredVerb,
		fieldsUnder,
		fieldsWithDuration,
		lanesOf,
		pressKind,
		typesKeys,
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

	const dispatch = createEventDispatcher<{ run: string[]; fill: string[] }>();

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
	/**
	 * Where the press that picked it landed: the tapped button's centre,
	 * from the bar's top left corner as drawn.
	 */
	let tap = { x: 0, y: 0 };
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
	 * What the bar draws. A picked field's states (da-g635) are ONE entry,
	 * the ring over the whole bar (da-7q3ka): `< Back`, which logs nothing,
	 * the states around the tap, and `+ Add state`, which is how a state
	 * never logged gets into the list at all. Otherwise the fields in their rows
	 * (`arrange`), with Track + starting the top one — the way into the app
	 * on an empty database — or, under a pending verb, the `+` ending the
	 * bottom one (vividn: «There should be a + field at the end of this
	 * filtered fields view for adding new fields»).
	 */
	type Entry =
		| { kind: 'track' | 'hint' }
		| { kind: 'ring' }
		| { kind: 'field'; button: FieldButton; promoted: boolean; edge: boolean };
	$: entries = ((): Entry[] => {
		if (picking) return [{ kind: 'ring' }];
		if (buttons.length > 0) return [];
		return verb === null ? [{ kind: 'track' }, { kind: 'hint' }] : [{ kind: 'hint' }, { kind: 'track' }];
	})();
	/**
	 * The fields' rows (da-xtkcp): by height top to bottom, most used first
	 * along each, as they stood at the last sort — and a press's fields at
	 * the head of them, the hairline after the last of them in each row.
	 */
	$: arrange = picking
		? (items: Entry[]): Entry[][] => [items]
		: buttons.length === 0
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
	const keyOf = (e: Entry) => (e.kind === 'field' ? `f:${e.button.field}` : e.kind);

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
	 * A TOGGLE: the field goes on the bar as a pill, in its own colour,
	 * and the page narrows to it; held again while its pill is there, the
	 * pill goes — locked or not — and the page is whole again (da-05oit).
	 * vividn: «long pressing or right clicking on a field again after a pill
	 * is added should remove the pill/filter». Only that field's pill:
	 * every other pill and the typed line stay.
	 */
	function hold(button: FieldButton) {
		setFieldPill($fieldPill === button.field ? null : button.field, button.color);
	}

	/**
	 * What a button says it does, which is the one place both gestures are
	 * named: the press, then the hold.
	 */
	function titleOf(button: FieldButton): string {
		const press =
			button.shape.kind === 'record' ? `${shellJoin(['add', button.field])} … — fills the prompt` : 'press to log';
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

	/** Put `argv` in the command line, unrun, for the rest to be typed. */
	function fill(argv: string[]) {
		back();
		dispatch('fill', argv);
	}

	/** Run `argv`, or fill it where `button` has keys to type (da-chml5). */
	function log(argv: string[], button: FieldButton) {
		if (typesKeys(argv[0], button)) fill(argv);
		else emit(argv);
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
	 * the pane keeps the height its layout gives it (da-mbz2). Where the
	 * press landed is kept, for the states to stand around (da-7q3ka).
	 */
	/** `el` is the pressed button itself — the click's `currentTarget`, never a glyph inside it. */
	function pick(button: FieldButton, el: HTMLElement) {
		const at = el.getBoundingClientRect();
		const box = bar!.getBoundingClientRect();
		tap = { x: at.left + at.width / 2 - box.left, y: at.top + at.height / 2 - box.top };
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

	/**
	 * A press does what `pressKind` says — the same answer the button's
	 * outline is drawn from, so the two cannot disagree (da-z8z40).
	 */
	function press(button: FieldButton, el: HTMLElement) {
		const kind = pressKind(button, verb);
		// With a verb pending, the press IS that verb (C1: a field tap runs
		// at once).
		const argv = (verb && verbPress(verb, button)) || pressArgv(button);
		// A multi-state field can't be logged without saying which, so a
		// plain press asks — the bar fills with its states.
		if (kind === 'show') pick(button, el);
		// A record, or a field with keys to type: the line goes to the prompt.
		else if (kind === 'fill') fill(argv);
		else emit(argv);
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
		log(pressArgv(button, state), button);
	}

	/**
	 * Stand the ring's buttons around the tap (da-7q3ka): `ringOf` gives the
	 * spots from their measured sizes and the room the box leaves on each
	 * side of the tap — capped at an edge, never shifted, so `end` stays
	 * under the thumb (da-8mk9p). The ring reaches two rows either way, and
	 * a side the box caps gives what it lost to the other; at a side edge
	 * `< Back` or `+ Add state` steps onto another row (da-u9nfv). When the
	 * centre itself cannot fit sideways, or a button runs past the right
	 * edge, the box scrolls its centre under the tap as far as it can. Two
	 * pixels short of the box, or a border on its edge is clipped. Measured
	 * in the box's own offsets, never on the screen, since the bar may still
	 * be scrolled to where the fields were; and again whenever the box or a
	 * button is resized — `+ Add state` becoming its input is one — or a
	 * button comes or goes, as a state learned while the ring is open does.
	 */
	function around(box: HTMLElement, at: { x: number; y: number }) {
		const GAP = 6;
		// Unrounded: `offsetWidth` rounds down, and a ring at the box's edge
		// then loses its last pixel.
		const size = (el: HTMLElement) => el.getBoundingClientRect();
		const place = () => {
			const spots = [...box.children] as HTMLElement[];
			const one = (role: string) => spots.find((s) => s.dataset.role === role);
			const [back, add, end] = [one('back')!, one('add')!, one('end')];
			const states = spots.filter((s) => s.dataset.role === 'state');
			const h = Math.max(...spots.map((s) => size(s).height));
			const reach = 2 * (h + GAP) + h / 2;
			const [up, down] = [at.y - 2, box.clientHeight - at.y - 2];
			const ring = ringOf(
				{
					back: size(back).width,
					add: size(add).width,
					end: end ? size(end).width : null,
					states: states.map((s) => size(s).width)
				},
				h,
				{
					up: Math.min(up, 2 * reach - Math.min(down, reach)),
					down: Math.min(down, 2 * reach - Math.min(up, reach)),
					left: at.x - 2,
					right: box.clientWidth - at.x - 2
				},
				GAP
			);
			const placed: [HTMLElement, Spot][] = [
				[back, ring.back],
				[add, ring.add],
				...(end && ring.end ? [[end, ring.end] as [HTMLElement, Spot]] : []),
				...states.map((s, i): [HTMLElement, Spot] => [s, ring.states[i]])
			];
			for (const [el, p] of placed) {
				el.style.left = `${at.x + p.x - size(el).width / 2}px`;
				el.style.top = `${at.y + p.y - size(el).height / 2}px`;
			}
			box.scrollLeft = (ring.end ?? ring.states[0])?.x ?? 0;
		};
		const observer = new ResizeObserver(place);
		const watch = () => {
			observer.observe(box);
			for (const spot of box.children) observer.observe(spot);
			// Placed now as well, not a frame later: the buttons are in the
			// DOM when an action runs, and measuring them lays them out.
			place();
		};
		const children = new MutationObserver(watch);
		children.observe(box, { childList: true });
		watch();
		return {
			update: (next: { x: number; y: number }) => ((at = next), watch()),
			destroy: () => (observer.disconnect(), children.disconnect())
		};
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
		// It marks the key taken, so the command line does not ALSO peel
		// its pills (PromptBar.svelte, da-al892).
		if (addingState) {
			addingState = false;
			newState = '';
		} else if (picked !== null) back();
		else return;
		event.preventDefault();
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
	{#if item.kind === 'ring' && picking}
		{@const chosen = picking}
		{@const states = statesOf(chosen.shape)}
		<div class="ring" use:around={tap}>
			<div class="spot" data-role="back">
				<button class="nav back" use:act={'show'} data-back {disabled} title="back to the fields" on:click={back}>
					&lt; Back
				</button>
			</div>
			{#if states.includes('false')}
				{@const color = chosen.stateColors.false ?? chosen.color}
				<!-- `false` is off: `end`, on the tap itself, and hollow like the
				     two ends — the one state a thumb reaches without moving. -->
				<div class="spot" data-role="end">
					<button
						class="widget states end"
						use:act={'run'}
						style={vars({ fill: null, line: color, text: color, label: 'end' })}
						{disabled}
						data-state="false"
						title={shellJoin(['end', chosen.field])}
						on:click={() => emit(['end', chosen.field])}
					>
						end
					</button>
				</div>
			{/if}
			{#each states.filter((s) => s !== 'false') as state (state)}
				{@const look = stateLook(chosen, state)}
				<div class="spot" data-role="state">
					<!-- A state RUNS at once, drawn as acting (da-z8z40) — unless the field has keys to type. -->
					<button
						class="widget states"
						use:act={typesKeys('switch', chosen) ? 'fill' : 'run'}
						class:on={chosen.current === state}
						style={vars(look)}
						{disabled}
						data-state={state}
						title={shellJoin(pressArgv(chosen, state))}
						on:click={() => log(pressArgv(chosen, state), chosen)}
					>
						{state}
					</button>
				</div>
			{/each}
			<div class="spot" data-role="add">
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
						use:act={'show'}
						data-add-state
						{disabled}
						title="add a state to {chosen.field}"
						on:click={() => (addingState = true)}
					>
						+ Add state
					</button>
				{/if}
			</div>
		</div>
	{:else if item.kind === 'track'}
		<div class="slot">
			{#if verb === null}
				<button
					class="track add-button"
					use:act={'open'}
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
					use:act={'open'}
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
					use:act={pressKind(button, verb)}
					class:ignored={button.ignored}
					class:pilled={$fieldPill === button.field}
					role="link"
					data-field={button.field}
					data-ignored={button.ignored ? '' : undefined}
					data-pilled={$fieldPill === button.field ? '' : undefined}
					style={vars(look)}
					{disabled}
					title={titleOf(button)}
					on:click={(e) => press(button, e.currentTarget)}>{button.field}</button
				>
			{:else}
				<button
					class="widget {button.shape.kind}"
					use:act={pressKind(button, verb)}
					class:on={look.fill !== null}
					class:ignored={button.ignored}
					class:pilled={$fieldPill === button.field}
					style={vars(look)}
					{disabled}
					data-field={button.field}
					data-ignored={button.ignored ? '' : undefined}
					data-pilled={$fieldPill === button.field ? '' : undefined}
					title={titleOf(button)}
					on:click={(e) => press(button, e.currentTarget)}
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

	/* A button a tap RUNS (`pressKind`, da-z8z40) is drawn in a DOUBLE
	   outline, as the commands widget's are ($lib/act.ts) — in the
	   field's own colour, since that is already the button's line. The
	   padding gives back what the wider border takes. The fill stops at
	   the border (da-0lmb7): a double's gap shows the background, and a
	   button FILLED in its own line's colour would paint the two lines and
	   the gap as one edge. Clipped, the gap shows the bar behind it, as it
	   does on a button that is not filled. */
	.widget:global([data-act='run']):not(.ignored) {
		border: 3px double var(--line);
		background-clip: padding-box;
		padding: calc(0.35rem - 1.5px) calc(0.85rem - 1.5px);
	}

	.record:global([data-act='run']):not(.ignored) {
		border: 3px double var(--line);
		border-radius: 3px;
		padding: calc(0.35rem - 3px) calc(0.3rem - 3px);
	}

	/* An IGNORED field that runs keeps its dashes, which are what tell it
	   from a tracked one (da-xhio), and draws them DOUBLE (da-nmna0): CSS
	   has no double-dashed border, so the second line is a dashed outline
	   a border's gap inside the dashed border, following its radius. It
	   gives way to the focus ring, which is an outline too. */
	.widget.ignored:global([data-act='run']):not(:focus-visible),
	.record.ignored:global([data-act='run']):not(:focus-visible) {
		outline: 1.5px dashed var(--line);
		outline-offset: -4.5px;
	}

	.record.ignored:global([data-act='run']) {
		border: 1.5px dashed var(--line);
		border-radius: 3px;
		padding: calc(0.35rem - 1.5px) calc(0.3rem - 1.5px);
	}

	.widget:disabled,
	.track:disabled,
	.record:disabled,
	.nav:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* `< Back` and `+ Add state`: the two sides of the ring, and neither of
	   them a state — so they are drawn hollow in the app's own ink rather
	   than filled in any field's colour, and cannot be mistaken for
	   something to log (da-7q3ka). */
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

	/* A field's states over the whole bar (da-7q3ka), each button placed
	   by `around` — nothing here decides where. Out of the bar's flex
	   layout and over its box (`inset: 0` against the bar's
	   `position: relative`), which is all the bar holds while it is open. */
	.ring {
		position: absolute;
		inset: 0;
		overflow-x: auto;
		overflow-y: hidden;
	}

	.spot {
		position: absolute;
		/* Near the box's right edge a spot would otherwise shrink to what
		   is left of it, and `+ Add state` wrap. */
		white-space: nowrap;
	}

	/* The state the field is in now: solid, as the field's own chip draws
	   it (`stateLook`), and bold — the rest are a tint. */
	.states.on {
		font-weight: 700;
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
