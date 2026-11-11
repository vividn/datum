<!--
	The collect widget (da-w6437), the fields widget until then (da-nz29,
	da-c7u8): every field `datum fields` lists, and the primary way anything
	gets logged without typing. vividn, 2026-11-03: «the primary interaction
	widget on the collect pane … this widget really adapts to whatever
	you're doing».

	Every field that is not ignored (`ignored` in `datum fields --json`,
	da-d08c), spec or no spec, each shaped by its kind and painted by what it
	is now (`lookOf` in fields.ts): a point is an empty pill with its occur
	mark beside the name; a block is a rectangle, filled while it is on; a
	switch is a rectangle filled with its current state's color; a keyed
	record is its name, underlined, and a click prefills `add <field> ` in
	the bottom bar rather than running anything.

	AT REST (da-lw7ue) the fields stand in ROWS locked to the pictures'
	height, each in the row its band's middle falls in (`atRest`): on the
	left a STRIP, the last half second of today's data on each field's band, now at
	its right edge and sliding with the clock, so a change sweeps in from the
	right and a dot crosses it in half a second (NowStrip.svelte, da-uuzu6); a tap
	adds its own mark there before the database answers (`nowPending`). At a
	selected time the strip is centered on it, still, zoomed out to hold a
	length or the entries selected in its inner 80% (`frame`, da-xqae2), and the fields stand as
	they stood then, a moment there in the active section and a tap taking it
	away (`nowAt`, `here`, da-0qdsp). And a
	tap on an active field's band puts that field's pill on the bar; then the ACTIVE SECTION, every field in an active state
	inside a dotted outline that steps with each row; then each row's other
	fields right against its own part of the section (da-il6ni), the most
	recently used first (`fieldRows`, the order taken on
	load and on coming back to the foreground). A drag pans those sideways,
	gliding on after a swipe, and up and down scrolls the page (da-tsomc);
	the strip and the active section stay, until the section is wider than
	half the widget and pans with them. A field
	that starts glides into the section, and one that ends out of it into the
	first slot of its row (`bumped`). While two fingers are down, and until
	the rows have settled after the last is up, no tap logs or opens anything
	(`untapped`).

	IN MODIFY MODE (`modifying`, da-xi4up) the rows are still under a cover
	that holds the widget's own controls, each written into the layout
	(`onmodify`): a handle on the strip's line sets the now section's share
	of the width (`nowWidth`), ⇄ the `side`, + at the line's top and − at its
	foot a row more or fewer (`rows`, the zoom left to fit them), and a PINCH
	— two fingers, or ctrl+wheel, a trackpad's — zooms the rows (da-l1z6h):
	each field's button, its font and its height together, so smaller fits
	more rows into the widget and larger fewer, between `ZOOM`'s bounds.
	Out of the mode a pinch zooms nothing.

	A TAP OPENS a field (vividn's frames, 2026-11-03), a switch alone in
	1-click mode and every field in 2-click (below): its pill goes on the
	bar and its view takes the widget beside the now strip, which stays where
	it stands at rest, so a state tapped is seen sweeping in at now's edge
	(da-u7tij, vividn, 2026-11-05: «still show the now section so you can see
	the animation right after changing states»), in the rows the fields stand
	in at rest and at their zoom (da-r48y9, vividn, 2026-11-07: «looks and
	behaves much more like the field view»): the first row in a dotted outline,
	the field's pill with its × (back, da-dswuc), the state it is in painted as the CLI paints one, and off
	where the spec allows it, a yellow `!` before it while the field is off; then
	a change field's STATES, its spec's in their order and every other it has
	used, the last used first (`datum states`), each to the row whose end is
	furthest left (`opened`), the one it is in outlined with the `!` too. Its KEYS are a drawer on the far edge,
	opposite the strip (CollectDrawer.svelte, da-igqs2), closed unless the
	field has a required key or it is dragged open: a tap putting `key=` on
	the line, with `+ data` taking the caret there. Its time and
	length are the TIME AND DURATION SECTION's (CollectWhen.svelte, da-us1n1),
	a column between the now strip and the rows, at rest and beside a field's
	view alike, a clock alone on the strip's edge until it is tapped open
	(da-iunoo, da-s2hht, the session's
	`timeOpen`): `time −1m −5m · dur 1m 5m`, summing into the page's selection
	(#lib/cursor.ts), so they are the nowview's own pills. A state tapped LOGS AT ONCE with them (vividn, 16:07: one
	click); any other field runs from the commands, which take the pills.
	In 1-click mode, the default, a field that needs nothing more is not
	opened at all (da-gra0z, `pressKind`): a tap on a moment logs it, on a
	block starts or ends it, with the pills there are.
	In 2-click mode (`clicks`, da-u017i) every field opens, and a tap only
	CHOOSES the state: its pill is placed and its chip ringed, and the command
	pane's ready «switch → state» logs it. A long press, on a field or a
	state, does the other mode's tap (da-z45a6), except that in 1-click mode
	a held state begins MULTI-SELECT (da-laibn): each tap after it adds a
	state or takes a chosen one out, the bar's state pill the array as it
	will be logged (`happy,sad`), for the bar's run. Whatever puts a field pill
	on the bar opens that field: a hold, the now strip, a script.
	A run spends them, the field's with them, and collect is back at rest.
	Back, a press on that pill in the view, takes it and every pill made since the tap, and puts
	back the time and `-m` there were before it; Escape is Back.

	Ignored fields get no button: the setup widget's «track something» lists
	them, and its start tracking runs `track <field>` (da-4ta1k). `showIgnored` (da-xhio) is the way to SEE them
	without the setup widget — a switch in the menu the pane's NAME
	opens (da-vhwq), this session's and off by default. It adds them to the
	list; it does not change what they are, so each is drawn faint and
	dashed and says «ignored» when a pointer rests on it.

	ALIGNED TO THE PICTURE (da-omw4n) is the other layout, a choice in the
	same menu, rows the default: the widget becomes a legend on the
	pictures' own y axis. Each field's button stands centred on its band,
	`y` and `height` from `datum fields --json` — never computed here —
	and as tall as the band where that is taller than a button; fields
	whose bands overlap stand side by side, last used first by the last
	sort, and a band too crowded for the width runs on to the right, where
	the bar scrolls (`aligned` in fields.ts). A pinch — two fingers, or ctrl+wheel, a trackpad's pinch — zooms
	the y axis around itself: the buttons keep their size and glide apart.
	While zoomed a vertical drag or the wheel pans, a press on a picture
	pans to the field it landed nearest, and a slim marker down the right
	edge shows which part of the pictures' height is in view; a tap on it
	shows the whole again. The zoom is this session's alone.

	A PENDING COMMAND (`pending`, da-ltux) is the commands widget talking to this
	one through the command line they share: the list becomes the fields
	that command can be spent on, and a press MEANS that command and runs at once.
	Which fields, and what a press then runs, are `fieldsUnder` and `pendingPress` in commands.ts.

	A SELECTION talks to it too (da-qu16). A length pill (`25m`) on the bar leaves
	the fields a length can be spent on — on/off and change — and composes with a
	pending command, both filters applying. A field open stays open whatever
	filters the bar: the length made in its view would.

	The widget shows the fields and nothing else: a new field is made in
	the setup widget or at the prompt (`setup <field> …`), never here — its
	bare (+) is gone (vividn, 2026-10-22: «let's get rid of + button, it's
	confusing and looks bad», da-myet1).

	HOLD — or right-click, or shift+click, or the menu key — puts the field
	on the bar as a PILL alone (da-8810n), which narrows the rest of the
	page to it, and held again takes it away (da-05oit).

	Under a pending command a press composes an argv and dispatches it, and the page runs it through `run()` and echoes the
	result's own argv into the CLI widget. That is the teach loop, and it only
	teaches the truth because the echoed argv IS the executed argv — there
	is no second string built for display that could drift from it.
-->
<script lang="ts" module>
	import type { StateUse } from './calls.js';
	/**
	 * `datum states <field>` as last read, by database and field (da-2ylmj),
	 * which a field opened again draws at once, reading nothing. Forgotten
	 * as a write names the field, and all of them by a write that names none.
	 */
	const listings = new Map<string, StateUse[]>();
	const forget = (fields?: string[]) => listings.forEach((_, key) => (!fields || fields.includes(key.split('\0')[1])) && listings.delete(key));
</script>

<script lang="ts">
	import { flushSync, onDestroy, onMount, untrack } from 'svelte';
	import ButtonRows from './ButtonRows.svelte';
	import CollectDrawer from './CollectDrawer.svelte';
	import CollectWhen, { GLIDE_MS, MOMENT_ID, setMoment } from './CollectWhen.svelte';
	import { CLICK_SLOP } from './inspect.js';
	import NowStrip from './NowStrip.svelte';
	import PillChip from './Pill.svelte';
	import { framed, isoMs, type Mark } from './nowStrip.js';
	import { TAU } from './detent.js';
	import { speedOver, tracked, type Sample } from './grip.js';
	import {
		FULL,
		aligned,
		atRest,
		fieldRows,
		panned,
		opened,
		listedStates,
		inkOver,
		lookOf,
		zoomed,
		pressArgv,
		stateLook,
		offLook,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type Standings,
		type Align,
		type View
	} from './fields.js';
	import { on } from 'svelte/events';
	import { get } from 'svelte/store';
	import { act } from './act.js';
	import { holds } from './hold.js';
	import { FIELD_ID, fieldPill, focusLine, pills, prefill, refs, selected, setFieldPill, text, type Pill } from './bar.js';
	import { onAnyChange } from './changes.js';
	import { currentDb } from './database.js';
	import { run } from './run.js';
	import { entryArgv, statesArgv } from './calls.js';
	import { cursor, pointAt, pointed, type Cursor } from './cursor.js';
	import { grammar } from '#lib/layout/engine.ts';
	import { previewOf } from './preview.js';
	import { labelOf, withPills } from './selection.js';
	import { shellJoin } from './shellSplit.js';
	import {
		fieldsUnder,
		fieldsWithDuration,
		lanesOf,
		pressKind,
		typesKeys,
		pendingPress,
		type PressKind,
		type LogCommand
	} from './commands.js';

	let {
		fields = [],
		/**
		 * Where the buttons stand (da-omw4n): `rows`, today's, or `picture` —
		 * each at the height its field is drawn, see «ALIGNED TO THE PICTURE»
		 * above.
		 */
		align = 'rows',
		disabled = false,
		/** How a switch in a state is labelled; see `SwitchLabel`. */
		switchLabel = 'field',
		/**
		 * The command the line is pending (da-ltux), or null.
		 *
		 * It filters the list to the fields that command can be spent on and
		 * makes a press MEAN that command — so `on` on a change field with
		 * `true` among its states runs `on <field>`, not the state list a
		 * plain press would open.
		 */
		pending = null,
		/**
		 * Whether the bar holds a DURATION — a pill of kind `dur` (da-qu16).
		 *
		 * It leaves the fields a length can be spent on: on/off and change.
		 * It COMPOSES with `pending` — both filters apply.
		 */
		duration = false,
		/** The fields that stand ahead of the rest, marked so: a day pill's all-day ones (da-x3l9n). */
		first = [],
		/**
		 * Whether the IGNORED fields get a button too (da-xhio).
		 *
		 * Off is today's bar: a field whose latest status is IGNORED is not
		 * drawn, and the setup widget is where it is offered back. On adds it to the
		 * list, drawn as an ignored field and not as a tracked one — a second
		 * way to see it, never a way to stop it being ignored.
		 *
		 * This session's, not the layout's: the option is declared `session`
		 * in the registry and held in #lib/layout/session.ts.
		 */
		showIgnored = false,
		/** Where each field stood at the last sort (`standings` in #lib/layout/engine.ts, da-xtkcp). */
		standings = new Map(),
		/**
		 * What a tap does (da-u017i, da-gra0z): `one` logs a moment or a block
		 * on its field's tap and a state on its own, `two` opens every field
		 * and only chooses a state, for the command pane's ready command to
		 * log. A long press, on a field or a state, does what the other does.
		 */
		clicks = 'one',
		/**
		 * Which edge the strip and the active section stand at (da-lw7ue):
		 * `right` is the mirror, recency running leftward and panning left.
		 */
		side = 'left',
		/** Today's data layer, which the now strip draws the last half second of (da-uuzu6). */
		nowDoc = null,
		/** What the taps added to the strip that the layer does not hold yet. */
		nowPending = [],
		/** The selected instant the strip is centered on, still (da-0qdsp); null slides with now. */
		nowAt = null,
		/** The selected range's far edge from `nowAt`, in ms: 0 for none, negative for a length ending at it, positive for one ahead. */
		nowSpan = 0,
		/** The instant the strip frames, at its center (da-xqae2): for the pane to draw that day's layer. */
		onframe,
		/** How large the rows at rest are drawn, a button's font and height together (da-l1z6h): unset, `fitted`. */
		zoom,
		/** The rows an unset zoom fits the box to (da-je8ju); none, zoom 1. */
		rows,
		/** The now section's share of the widget's width (da-xi4up); unset, a fifth. */
		nowWidth,
		/** Modify mode is on this widget (da-xi4up): its controls drawn over it, the rest still. */
		modifying = false,
		/** A control of modify mode moved: the settings it left, for the layout. */
		onmodify,
		/** The time and duration section open, or its clock alone (da-iunoo); the session's, as the zoom is. */
		timeOpen = false,
		/** The section's clock tapped: whether it is open now, for the session to keep. */
		ontime,
		/** Run a press's line: settled once it has run, where it runs. */
		onrun,
		/** Put a press's line in the command line, unrun, for the rest to be typed. */
		onfill
	}: {
		fields?: FieldButton[];
		align?: Align;
		disabled?: boolean;
		switchLabel?: SwitchLabel;
		pending?: LogCommand | null;
		duration?: boolean;
		first?: string[];
		showIgnored?: boolean;
		standings?: Standings;
		clicks?: 'one' | 'two';
		side?: 'left' | 'right';
		nowDoc?: Document | null;
		nowPending?: Mark[];
		nowAt?: number | null;
		nowSpan?: number;
		onframe?: (at: number | null) => void;
		zoom?: number;
		rows?: number;
		nowWidth?: number;
		modifying?: boolean;
		onmodify?: (moved: { zoom?: number; rows?: number; side?: 'left' | 'right'; nowWidth?: number }) => void;
		timeOpen?: boolean;
		ontime?: (open: boolean) => void;
		onrun: (argv: string[]) => Promise<unknown> | void;
		onfill: (argv: string[]) => void;
	} = $props();

	/** Does a bare tap log what needs nothing more (`pressKind`)? */
	const oneTap = $derived(clicks === 'one');

	let bar: HTMLElement | undefined = $state.raw();
	// The hold's action (#lib/hold.ts) on the bar ButtonRows draws — an
	// action cannot be handed through a component, so it is attached here.
	// So are the picture layout's pinch, pan and wheel, and, after the hold,
	// what keeps two fingers from tapping.
	onMount(() => {
		const held = holds(bar!, { within: 'button[data-field], button[data-state], button[data-off]', run: holdField });
		const unzoom = zooming(bar!);
		const untap = untapped(bar!);
		const unscroll = on(bar!, 'touchmove', noScroll, { passive: false });
		const box = new ResizeObserver(() => (tall = bar!.clientHeight));
		box.observe(bar!);
		return () => (held.destroy(), unzoom(), untap(), unscroll(), box.disconnect());
	});

	/**
	 * The page scrolls under a finger going up or down (`touch-action`), but
	 * not under two, a pinch, nor once a pan of ours has moved: the browser
	 * would claim a turn of it and cancel the gesture mid-way (da-tsomc).
	 * On the bar, which holds the rows' room as well as the picture, so it
	 * hears both: `drag` is the rows' pan, where they overrun the room
	 * (`pans`; a drag on rows that fit is the pane's tab swipe, which a
	 * cancelled move is not), and `live` the zoomed picture's (`zooming`).
	 */
	function noScroll(e: TouchEvent) {
		if (e.touches.length > 1 || (drag?.moving && pans.includes('x')) || live) e.preventDefault();
	}

	/**
	 * No tap while two fingers are down (da-gh1ep, vividn, 2026-11-05: «If two
	 * fingers are on the collect view or you're in the middle of the zoom
	 * tapping action should not be registered»): from a second finger landing
	 * until every finger is up, and `GLIDE_MS` on, the rows' glide to the new
	 * zoom, no click on the bar is a press, wherever it is, and a hold one of
	 * the fingers started is called off. Registered after `holds`, on the same
	 * node, so its cancel comes after the hold's own pointerdown has started a
	 * timer for the finger.
	 */
	function untapped(node: HTMLElement) {
		const down = new Set<number>();
		let [pinched, calm] = [false, -Infinity];
		const land = (e: PointerEvent) => {
			if (e.pointerType !== 'touch' || down.add(e.pointerId).size < 2) return;
			pinched = true;
			node.dispatchEvent(new PointerEvent('pointercancel'));
		};
		const lift = (e: PointerEvent) => {
			if (down.delete(e.pointerId) && !down.size && pinched) [pinched, calm] = [false, e.timeStamp + GLIDE_MS];
		};
		const eat = (e: MouseEvent) => {
			if (pinched || e.timeStamp < calm) (e.preventDefault(), e.stopPropagation());
		};
		node.addEventListener('pointerdown', land);
		addEventListener('pointerup', lift);
		addEventListener('pointercancel', lift);
		node.addEventListener('click', eat, true);
		return () => {
			node.removeEventListener('pointerdown', land);
			removeEventListener('pointerup', lift);
			removeEventListener('pointercancel', lift);
			node.removeEventListener('click', eat, true);
		};
	}

	/**
	 * The field tapped open, by name (da-w6437).
	 *
	 * By name and not by the button, because the entries are rebuilt on
	 * every beat: held by name, the view redraws from the field as it is
	 * now — the state just logged marked — rather than from a copy that
	 * went stale the moment it was picked.
	 */
	let picked: string | null = $state(null);
	/** The field a state tap left, its write still running: its pill on the bar opens it only once that has failed. */
	let leaving: string | null = $state(null);
	let leavingRun: Promise<unknown> | null = null;
	/**
	 * What the bar held as the field was tapped: its selection and whether
	 * it said `-m`. Back puts them back, so a time pill made before the tap
	 * stays and one made since goes (vividn, 14:49).
	 */
	let entered: { cursor: Cursor | null; moment: boolean } | null = null;
	/** How far a drag has panned the rows sideways, kept so they cannot leave the room; none at rest. */
	let pan: { x: number; y: number } | null = $state.raw(null);
	/** The fields' pan while a field is open, for its view to close back to. */
	let restPan: { x: number; y: number } | null = null;
	/** The size of the room the cluster stands in, and of the cluster's own box. */
	let paneW = $state(0);
	let paneH = $state(0);
	/** The widget's own size, which the strip is measured by: the room narrows beside it while a field is open. */
	let boxW = $state(0);
	let boxH = $state(0);
	const buttons = $derived(
		fieldsUnder(
			fieldsWithDuration(
				fields.filter((w) => showIgnored || !w.ignored),
				duration
			),
			pending
		)
	);
	// Among every field, not the bar's: a length made in its own view must
	// not filter the field it was made for away (`fieldsWithDuration`).
	const picking = $derived(picked === null ? null : (fields.find((b) => b.field === picked) ?? null));

	/**
	 * What the bar draws: in rows only the hint, on a bar with none, and the
	 * fields aligned to the picture where that is asked; otherwise the fields
	 * stand in the cluster (`clustered`), and a picked field's view floats
	 * over the bar.
	 */
	type Entry = { kind: 'hint' } | { kind: 'field'; button: FieldButton; edge: boolean };
	const entries = $derived.by((): Entry[] => {
		if (buttons.length > 0) return [];
		return [{ kind: 'hint' }];
	});
	const arrange = $derived.by((): ((items: Entry[], rows: number) => Entry[][]) => {
		// Read here and not only in the functions: a new one lays the bar out again.
		const placed = placing;
		return buttons.length === 0 ? lanesOf : picture ? () => [placed] : () => [];
	});
	const keyOf = (e: Entry) => (e.kind === 'field' ? `f:${e.button.field}` : e.kind);

	/** Whether the buttons stand where their fields are drawn: the option, on a bar of fields. */
	const picture = $derived(align === 'picture' && buttons.length > 0);
	/**
	 * The fields at rest, last used first, `first` ahead of them: the order
	 * they take their slots in collect's rows (`atRest`, da-lw7ue).
	 */
	const clustered = $derived.by(() => {
		if (buttons.length === 0 || picture) return [];
		const [row] = fieldRows(buttons, 1, standings);
		return [...row.filter((b) => first.includes(b.field)), ...row.filter((b) => !first.includes(b.field))];
	});
	/** Whether the fields stand in collect's rows at rest (`atRest`). */
	const resting = $derived(clustered.length > 0);
	/** The fields in the order they claim their places: most recently used at the last sort first. */
	const placing = $derived(
		[...buttons]
			.sort((a, b) => (standings.get(b.field)?.last ?? -1) - (standings.get(a.field)?.last ?? -1))
			.map((button): Entry => ({ kind: 'field', button, edge: false }))
	);
	/** The bar's height, and each placed slot's size as drawn, in px. */
	let tall = $state(0);
	let sizes = $state.raw(new Map<string, { w: number; h: number }>());
	/** A button's height at zoom 1, the hidden probe's, which no zoom reaches: what `rows` fits the box by. */
	let unit = $state(0);
	/** The part of the pictures shown; this session's, never the layout's. */
	let view: View = $state.raw(FULL);
	/** Where the cluster is drawn from the center: the pan, or none at rest. */
	const shift = $derived(pan ?? { x: 0, y: 0 });
	const zoomedIn = $derived(view.to - view.from < 1);
	const GAP = 6;
	/** Where each slot stands. */
	const stands = $derived.by((): Map<string, { x: number; top: number }> => {
		if (!picture) return new Map();
		const keys = placing.map(keyOf);
		const at = aligned(
			placing.map((e, i) => ({
				...(e.kind === 'field' ? e.button : { y: 1, height: 0 }),
				...(sizes.get(keys[i]) ?? { w: 0, h: 0 })
			})),
			tall,
			view,
			GAP
		);
		return new Map(at.map((stand, i) => [keys[i], stand]));
	});
	/** Whether a move glides: not on the first placing, nor under a finger that is moving it. */
	let glide = $state(false);
	let live = $state(false);
	$effect.pre(() => {
		const on = picture || resting;
		if (on) requestAnimationFrame(() => requestAnimationFrame(() => (glide = picture || resting)));
		else glide = false;
	});

	/** The now section's share of the widget, inside bounds whatever was stored; its handle's while it is dragged. */
	const NOW_WIDTH = { min: 0.1, max: 0.5 };
	const shareOf = (w: number) => Math.min(NOW_WIDTH.max, Math.max(NOW_WIDTH.min, w));
	let share = $derived(shareOf(nowWidth ?? 0.2));
	/** The strip's width at rest: about a fifth of the widget (vividn: «maybe 20%»), or what modify mode's handle made it. */
	const strip = $derived(Math.round(boxW * share));
	/**
	 * The picture stands this far short of the strip's edge, the line there, as
	 * far as the widget's body pads it from the pane's other edges (0.6rem,
	 * CollectPane.svelte), so it has even margins all round (da-c93wz, vividn,
	 * 2026-11-09: «space it out from the rendered now svg a little more, so
	 * that the now section has consistent margins around it»).
	 */
	const PICTURE_GAP = 10;
	/** The time and duration section's width past the strip (da-us1n1), its clock's far half while closed (da-s2hht); the rows stand past both. */
	let whenW = $state(0);
	const edge = $derived(strip + whenW);
	/** Where each field stands at rest, and the active section's outline. */
	/** A row's height at rest: its tallest button's. */
	const rowH = $derived.by(() => {
		const hs = [...sizes].filter(([key]) => key.startsWith('f:')).map(([, s]) => s.h);
		return hs.length ? Math.max(...hs) : 30;
	});
	/** The rows the room holds at rest, as `atRest` counts them: what modify mode's + and − step from. */
	const rowCount = $derived(Math.max(1, Math.floor((paneH + GAP) / (rowH + GAP))));
	const shelves = $derived(
		atRest(
			clustered.map((b) => ({ ...b, w: sizes.get(`f:${b.field}`)?.w ?? 0 })),
			{ height: paneH, rowH, gap: GAP, strip: edge }
		)
	);
	/** The strip and the active section stay while the section is under half the widget, the time section beside the strip not counted; past it the section pans too. */
	const frozen = $derived(shelves.section - whenW <= paneW / 2);
	/** Each active field's band on the strip, a tap's target over its picture. */
	const nowBands = $derived(fields.filter((b) => b.active && !b.ignored));
	// The other side pans the other way: a pan kept from the last starts again at the strip.
	$effect.pre(() => {
		void side;
		untrack(() => (pan = null));
	});
	/** Where a field stands across the room at rest, `x` from the strip's edge: mirrored on the right. */
	const across = (x: number, field: string) => (side === 'right' ? paneW - x - (sizes.get(`f:${field}`)?.w ?? 0) : x);

	/**
	 * Keep `sizes` to the slot's size as drawn: each item's key, by its node,
	 * for the one observer that measures them all. One map is written a
	 * delivery, so fifty buttons drawn at once lay the rows out once, and not
	 * fifty times, a flush after each observer's callback (da-2ylmj).
	 */
	const keyed = new Map<Element, string>();
	const measure = new ResizeObserver((seen) => {
		let next: typeof sizes | null = null;
		for (const { target } of seen) {
			const key = keyed.get(target);
			if (key === undefined) continue;
			const { width: w, height: h } = target.getBoundingClientRect();
			const was = (next ?? sizes).get(key);
			if (was?.w !== w || was?.h !== h) (next ??= new Map(sizes)).set(key, { w, h });
		}
		if (next) sizes = next;
	});
	onDestroy(() => measure.disconnect());
	function sized(node: HTMLElement, key: string) {
		keyed.set(node, key);
		measure.observe(node);
		// A key let go of takes its size with it, so a stale one is never read.
		const drop = () => sizes.delete(key) && (sizes = new Map(sizes));
		return {
			update: (next: string) => void (next !== key && (drop(), keyed.set(node, (key = next)))),
			destroy: () => (measure.unobserve(node), keyed.delete(node), drop())
		};
	}

	/** How tall a field's button is drawn: its band, and never less than a button. */
	const tallOf = (button: FieldButton) => (picture ? `; min-height: ${Math.max(24, button.height * tall)}px` : '');

	/**
	 * The picture layout's gestures, on the bar: a pinch — two pointers, or
	 * ctrl+wheel, which is what a trackpad's pinch arrives as — zooms around
	 * where it is; while zoomed one pointer dragging up or down, or the
	 * wheel, pans. A drag that moved the view eats the click it leaves, and
	 * calls off a hold the press had started.
	 */
	function zooming(node: HTMLElement) {
		const pointers = new Map<number, { x: number; y: number }>();
		let from = { view, at: 0, d: 0, y: 0 };
		// Taken again whenever a finger lands or lifts: a second finger turns a
		// pan into a pinch around the two, a lifted one a pinch back into a
		// pan under the one left.
		const begin = () => {
			const ps = [...pointers.values()];
			const box = node.getBoundingClientRect();
			const y = ps.reduce((sum, p) => sum + p.y, 0) / ps.length;
			const d = ps.length > 1 ? Math.hypot(ps[0].x - ps[1].x, ps[0].y - ps[1].y) : 0;
			from = { view, at: (y - box.top) / box.height, d, y };
		};
		const moving = () => {
			if (!live) node.dispatchEvent(new PointerEvent('pointercancel'));
			live = true;
		};
		const down = (e: PointerEvent) => {
			if (!picture) return;
			pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
			begin();
		};
		const move = (e: PointerEvent) => {
			if (!picture || !pointers.has(e.pointerId)) return;
			pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
			const ps = [...pointers.values()];
			if (ps.length > 1 && from.d > 0) {
				moving();
				view = zoomed(from.view, from.at, Math.hypot(ps[0].x - ps[1].x, ps[0].y - ps[1].y) / from.d);
			} else if (ps.length === 1 && zoomedIn && (live || Math.abs(ps[0].y - from.y) > 8)) {
				moving();
				view = panned(from.view, (from.y - ps[0].y) / tall);
			}
		};
		const up = (e: PointerEvent) => {
			if (!pointers.delete(e.pointerId)) return;
			if (pointers.size > 0) return begin();
			if (live && e.type === 'pointerup') {
				const off = () => (removeEventListener('click', eat, true), removeEventListener('pointerdown', off, true));
				const eat = (click: MouseEvent) => (click.stopPropagation(), click.preventDefault(), off());
				addEventListener('click', eat, true);
				addEventListener('pointerdown', off, true);
			}
			live = false;
		};
		const wheel = (e: WheelEvent) => {
			if (!picture) return;
			const box = node.getBoundingClientRect();
			if (e.ctrlKey) view = zoomed(view, (e.clientY - box.top) / box.height, Math.exp(-e.deltaY / 200));
			else if (zoomedIn && Math.abs(e.deltaY) > Math.abs(e.deltaX)) view = panned(view, e.deltaY / tall);
			else return;
			e.preventDefault();
		};
		node.addEventListener('pointerdown', down);
		addEventListener('pointermove', move);
		addEventListener('pointerup', up);
		addEventListener('pointercancel', up);
		node.addEventListener('wheel', wheel, { passive: false });
		return () => {
			node.removeEventListener('pointerdown', down);
			removeEventListener('pointermove', move);
			removeEventListener('pointerup', up);
			removeEventListener('pointercancel', up);
			node.removeEventListener('wheel', wheel);
		};
	}

	/** A look as the custom properties every button below is painted from. */
	function vars(look: Look) {
		return `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`;
	}

	/**
	 * What the HOLD gesture does — the long press, the right click, the
	 * shift+click and the menu key, which are the four vividn named
	 * (da-8810n): the other mode's tap (da-z45a6). vividn, 2026-11-04:
	 * «1-click mode: tap -> logs immediately (if no extra data needed), long
	 * press/right click -> field pill and open the next menu / 2-click mode:
	 * reversed». So in 1-click it opens the field, its pill on the bar, and
	 * in 2-click it logs what needs nothing more, as `press` does with the
	 * other `oneTap`. Under a pending command, which a tap runs in either mode,
	 * it opens the field. With the view open the field's button is not there
	 * to hold again: Back or Escape takes the pill away, as for a tapped one.
	 */
	function hold(button: FieldButton) {
		press(button, heldKind(button));
	}

	/** What a hold does: the other mode's tap, or under a pending command, which a tap runs, the field opened. */
	const heldKind = (button: FieldButton): PressKind =>
		pending && pendingPress(pending, button) ? 'show' : pressKind(button, pending, !oneTap);

	/**
	 * What a button says it does, which is the one place both gestures are
	 * named: the press, then the hold. The press is `pressKind`'s answer, as
	 * the outline is (da-gra0z). The field's whole name leads where the press
	 * does not name it, since the button may cut it short.
	 */
	function titleOf(button: FieldButton): string {
		const record = button.shape.kind === 'record';
		const kind = pressKind(button, pending, oneTap);
		const does = (kind: PressKind) => (kind === 'run' ? 'log it' : kind === 'fill' ? 'fill the prompt' : 'collect it');
		const press = record ? `${shellJoin(['record', button.field])} … — fills the prompt` : `press to ${does(kind)}`;
		const held = does(heldKind(button));
		return [...(record ? [] : [button.field]), ...(button.ignored ? ['ignored'] : []), press, `hold to ${held}`].join(' · ');
	}

	// ---- the PILLS a field's view makes (da-w6437) ----------------------
	//
	// Its time and length are the section's (CollectWhen.svelte); a state is
	// a pill of its own.

	const STATE_ID = 'state';
	/** The state pill's state, and whether `-m` is on the bar. */
	const chosen = $derived($pills.find((p) => p.id === STATE_ID)?.text ?? null);
	const moment = $derived($pills.some((p) => p.id === MOMENT_ID));
	/**
	 * MULTI-SELECT (da-laibn, vividn, 2026-11-09: «pressing and holding on a
	 * state … should also enter into multi select mode, so if you tap on
	 * another state afterwords it starts forming an array state»): the states
	 * a hold began, in the order chosen, and null outside it. A tap then adds
	 * a state or takes a chosen one out, nothing runs, and the bar's run logs
	 * the array; closing the view or the last one out ends it. Declared
	 * here, above the effects that may close a view as the widget mounts.
	 */
	let together: string[] | null = $state.raw(null);

	// What the open field would log, drawn on the nowview before it is
	// (da-axrdv): its line as a state's tap or the bar would run it, the
	// CLI's dry run of it. Asked again as a pill moves, and as the minute
	// moves a relative time on.
	$effect(() => {
		const command = picking && picking.shape.kind !== 'record' ? pressArgv(picking)[0] : null;
		void $pointed;
		previewOf(command ? withPills($grammar, [command], $pills).argv : null);
	});
	$effect(() => () => void previewOf(null));

	// A picked field gone — deleted, the database emptied or restored,
	// another tab's write — closes, not a view held in waiting to take the
	// bar over again the moment a field of that name returns (da-mv8ng). So
	// does one whose pill went: a run spent it, or its × was pressed. A pill
	// still on the bar is no view held in waiting: it is in sight, its ×
	// takes it, and collect follows it (da-z45a6, below), so its field opens
	// again when it returns, as on a reload, whose pill comes before its field.
	$effect.pre(() => {
		if (picked !== null && (picking === null || $fieldPill !== picked)) untrack(close);
	});

	// ---- an ENTRY selected (da-xdz73) ------------------------------------
	//
	// vividn, 2026-11-03: «if you select an hid pill, it could show
	// information about this individual datum … no need to show by metadata
	// here, just the data / put a -A button which toggles showing all the
	// data». The bar's `hid` pill, each of its entries, while no field is open
	// whatever surface it was tapped on (da-g5y5o). A field pill on
	// the bar keeps the fields too, its hold being theirs (da-05oit):
	// its field and time, then its data; `−A` adds every other column, as
	// `get -A` prints them. Its actions are the command pane's (da-fg4w4).

	/**
	 * The entries shown: every one the hid pill selects, oldest first
	 * (da-ug56t), read again on every write. vividn, 2026-11-07: «if there
	 * are multiple entries selected show all of them (in chronological order)».
	 */
	const hids = $derived($selected.join(','));
	let cards: { hid: string; entry: Record<string, unknown> }[] = $state.raw([]);
	/** `−A`, each entry's own: every column, metadata included. */
	let everything: string[] = $state([]);
	/** Where an entry falls in time: its `at`, or with none its create time. */
	const when = (e: Record<string, unknown>) => Date.parse(((e.at as { utc: string } | null)?.utc ?? e.create_time) as string) || 0;
	$effect(() => {
		if (!hids) return void ((cards = []), (everything = []));
		// One read at a time, and one more if a write came meanwhile: a burst
		// of writes is one read, not one each (`onAnyChange` coalesces nothing).
		let live = true;
		let reading = false;
		let again = false;
		const read = async () => {
			if (reading) return void (again = true);
			reading = true;
			do {
				again = false;
				// One `get` of them all, which points `_LAST` at the same set every read,
				// as the one card's did: a `get` each moved it between them, and each move
				// set off another read, forever. A hid gone fails it after the others print.
				const got = await run(entryArgv(hids));
				const next: typeof cards = [];
				for (const doc of got.stdout.split(/\n(?=\{)/)) {
					try {
						const entry = JSON.parse(doc);
						next.push({ hid: entry.hid, entry });
					} catch {
						// Not a document: no card.
					}
				}
				if (live) cards = next.sort((a, b) => when(a.entry) - when(b.entry));
			} while (again && live);
			reading = false;
		};
		void read();
		const unhear = onAnyChange(() => void read(), currentDb());
		return () => ((live = false), unhear());
	});
	/** Whether the entries' cards are up, in place of the fields. */
	const card = $derived(cards.length > 0 && !!hids && !$fieldPill);
	/** The bar's hid pill, drawn over the cards it opened (da-dswuc). */
	const hidOpener = $derived($pills.find((p) => p.kind === 'hid'));
	/** What the now strip frames (da-xqae2): the entries the cards show, each `at − dur` to `at`, else the selected time and its length. */
	const frame = $derived(
		framed(
			card
				? cards.flatMap(({ entry }): [number, number][] => {
						const end = Date.parse((entry.at as { utc: string } | null)?.utc ?? '');
						return Number.isFinite(end) ? [[end - isoMs(entry.dur), end]] : [];
					})
				: []
		) ?? (nowAt === null ? null : framed([nowSpan < 0 ? [nowAt + nowSpan, nowAt] : [nowAt, nowAt + nowSpan]]))
	);
	$effect(() => onframe?.(frame?.at ?? null));
	/** The entry's own clock, `13:23`, at the offset it was logged at. */
	function clockOf(e: Record<string, unknown>): string {
		const at = e.at as { utc: string; offset_minutes: number } | null;
		return at ? new Date(Date.parse(at.utc) + at.offset_minutes * 60_000).toISOString().slice(11, 16) : '';
	}
	/** A value as it reads: a string bare, anything else its JSON. */
	const said = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));
	/** An entry's rows: its data's keys, and with `−A` every other column too. */
	function rowsOf(hid: string, entry: Record<string, unknown>): [string, unknown][] {
		const data = Object.entries((entry.data as Record<string, unknown> | null) ?? {});
		return everything.includes(hid) ? [...data, ...Object.entries(entry).filter(([k]) => k !== 'data')] : data;
	}
	/** `−A` on one entry's card. */
	const toggleAll = (hid: string) => (everything = everything.includes(hid) ? everything.filter((h) => h !== hid) : [...everything, hid]);
	/** Back from an entry: its pill goes. */
	function unselect() {
		pills.update((all) => all.filter((p) => p.kind !== 'hid'));
	}
	/** The entry's field tapped: its pill in the entry's place, which opens the field (below). */
	function toField(field: string, color?: string) {
		unselect();
		setFieldPill(field, color);
	}
	/**
	 * The entry's time tapped: the cursor pointed at its `at`, to the second,
	 * so the bar holds its absolute time pill. A period's `at` is its END.
	 */
	function toTime(e: Record<string, unknown>) {
		pointAt({ at: new Date((e.at as { utc: string }).utc), step: 1 });
	}

	// ---- a field OPEN, in collect's rows (da-r48y9) ------------------------
	//
	// vividn, 2026-11-07: «so that it looks and behaves much more like the field
	// view»: its first row in a dotted outline, the field's pill (back), the state it
	// is in as the CLI paints one, and off where the spec allows it; then the
	// spec's states in its order, and every other it has used, most recent
	// first, placed as the fields are (`opened`).

	/** `datum states <field>` for the open change field: which states its spec declares, and when each was last used; null where it gave no listing. */
	let uses: { field: string; states: StateUse[] | null } | null = $state.raw(null);
	const asked = $derived(picking?.shape.kind === 'change' ? picking.field : null);
	// A write made while no collect heard it may have changed any.
	forget();
	onMount(() => onAnyChange((_, fields) => forget(fields), currentDb()));
	$effect(() => {
		const field = asked;
		if (field === null) return void (uses = null);
		const key = `${currentDb()}\0${field}`;
		const kept = listings.get(key);
		uses = kept === undefined ? null : { field, states: kept };
		let [live, n] = [true, 0];
		const read = async () => {
			const me = ++n;
			const got = await run(statesArgv(field));
			let states: StateUse[] | null = null;
			try {
				states = JSON.parse(got.stdout).states;
			} catch {
				// No listing: the field's own states alone.
			}
			// No listing is not kept: the next open reads again, so a read that failed heals.
			if (me === n && states !== null) listings.set(key, states);
			if (live && me === n) uses = { field, states };
		};
		if (kept === undefined) void read();
		const unhear = onAnyChange((_, fields) => void ((!fields || fields.includes(field)) && read()), currentDb());
		return () => ((live = false), unhear());
	});
	/** Off is no state of `listed`: it heads `stated`, as setup lists it. */
	const isOff = (s: unknown) => s === false || s === 'false';
	/** The open field's multi-states, by the label they are tapped as, `good,bad` as the tail shows them (da-a2cxa). */
	const sets = $derived(new Map(picking?.shape.kind === 'change' ? (picking.shape.sets ?? []).map((p) => [p.join(','), p]) : []));
	/** Whether the open field is in `state` now, a multi-state by its label, `false` off and `true` on. */
	const isIn = (b: FieldButton, state: string) =>
		state === 'false' ? !b.active : b.active && (b.current === state || (state === 'true' && b.current === true) || b.set?.join(',') === state);
	/** A state's word: `true` is on, as the CLI says it. */
	const named = (state: string) => (state === 'true' ? 'on' : state);
	/** The open field's states: the spec's in its order, then every other it has used, a multi-state among them, the last used first, none twice. */
	const listed = $derived.by((): string[] => {
		if (picking?.shape.kind !== 'change') return [];
		const { field, shape } = picking;
		const rows = uses?.field === field ? uses.states : undefined;
		// The spec's from the field's own row, drawn before `states` answers, so what it adds comes after them and
		// nothing drawn moves; a field whose spec lists none has no spec order. With no listing, the field's own.
		const spec = shape.declared || rows === null ? shape.states.filter((s) => !isOff(s)) : [];
		return listedStates(spec, rows ?? [], [...sets.keys()]);
	});
	/** Whether off is offered: a change field its spec lets be off, or an on/off field. */
	const offable = $derived(picking?.shape.kind === 'on/off' || (picking?.shape.kind === 'change' && !picking.neverFalse));
	/** The open view's states (da-qryu8, vividn, 2026-11-11): off first where offered, then on where the field takes `true`, then `listed`. */
	const stated = $derived([...(offable ? ['false'] : []), ...(listed.includes('true') ? ['true'] : []), ...listed.filter((s) => s !== 'true')]);
	/** Where the open view stands: its first row, then each state, by `sized`'s keys. */
	const laid = $derived.by(() => {
		// The first row's key is `open`, with no colon, so no state's `o:` key can be it.
		const keys = picking ? ['open', ...stated.map((s) => `o:${s}`)] : [];
		const rowH = Math.max(24, ...keys.map((k) => sizes.get(k)?.h ?? 0));
		const on = (i: number) => i === 0 || (!!picking && isIn(picking, stated[i - 1]));
		// The items measured, from the first: each stands where those before it put it, and one not yet where a width of 0 does, hidden.
		const unmeasured = keys.findIndex((k) => !sizes.has(k));
		const ready = unmeasured < 0 ? keys.length : unmeasured;
		return { keys, ready, ...opened(keys.map((k, i) => ({ w: sizes.get(k)?.w ?? 0, outlined: on(i) })), { height: paneH, rowH, gap: GAP }) };
	});
	/** Where an open item stands across the room: mirrored on the right. */
	const openAt = (x: number, key: string) => (side === 'right' ? paneW - x - (sizes.get(key)?.w ?? 0) : x) + shift.x;
	/**
	 * How far an open field's rows overrun the room's left, which the room is
	 * kept scrolled by and the rows drawn that much further in (da-u846n): a
	 * reveal only scrolls toward a box's end, so a state left of the room is in
	 * reach of one only with the room scrolled. None at rest; whole pixels, as
	 * a scroll is.
	 */
	let roomEl: HTMLElement | undefined = $state();
	const overrun = (a: number) => Math.max(0, Math.ceil(a));
	const lead = $derived(picking ? { x: overrun(side === 'left' ? -shift.x : laid.end + GAP - paneW - shift.x), y: 0 } : { x: 0, y: 0 });
	/** Where the room was last scrolled to: a write, even of the same place, is a forced style in the flush (da-rh5oa). */
	let scrolled: { room?: HTMLElement; x: number; y: number } = { x: 0, y: 0 };
	$effect(() => {
		const { x, y } = lead;
		if (!roomEl) return;
		// A room just made is at its start.
		if (scrolled.room !== roomEl) scrolled = { room: roomEl, x: 0, y: 0 };
		if (x !== scrolled.x || y !== scrolled.y) [roomEl.scrollLeft, roomEl.scrollTop, scrolled.x, scrolled.y] = [x, y, x, y];
	});
	/** Off pressed: a change field switched off as a state is, an on/off field ended. */
	function off(button: FieldButton) {
		if (button.shape.kind === 'change') choose(button, 'false');
		else leave(button, ['off']);
	}

	/**
	 * A state tapped LOGS AT ONCE (vividn, 2026-11-03 16:07: one click), with
	 * the time pills set: its pill goes beside the field's and `switch` takes
	 * them all, so a success spends them and collect is back at rest. A field
	 * with keys to type has its line filled instead, the field's view closed.
	 * Not `logs` (2-click, or 1-click's long press, da-u017i), the pill is
	 * placed and nothing runs: the command pane's ready command logs it.
	 */
	function choose(button: FieldButton, state: string, logs = clicks === 'one') {
		if (logs && typesKeys('change', button)) {
			// Closed, not Back: the time pills made here stay for the line filled.
			setFieldPill(null);
			close();
			return onfill(pressArgv(button, state));
		}
		place(button, [state]);
		if (logs) leave(button, ['change']);
	}

	/** `states` as datum reads them: one state, or an array glued with commas (`happy,sad`), or the bracket form where a state holds a comma. */
	const asOne = (states: string[]) => (states.length === 1 ? states[0] : states.some((s) => s.includes(',')) ? JSON.stringify(states) : states.join(','));

	/** The state pill: `states` as datum reads them, one state or an array, beside the field's pill; none takes it off. */
	function place(button: FieldButton, states: string[]) {
		pills.update((all) => {
			const rest = all.filter((p) => p.id !== STATE_ID);
			if (!states.length) return rest;
			const pill: Pill = { id: STATE_ID, kind: 'state', text: shellJoin([asOne(states)]), color: button.stateColors[states[0]] ?? button.color };
			const at = rest.findIndex((p) => p.kind === 'field') + 1;
			return [...rest.slice(0, at), pill, ...rest.slice(at)];
		});
	}

	/**
	 * THE TRANSITION (da-rou2i, vividn, 2026-11-10: «If you long press a
	 * state, then replace the end button with a button that shows that new
	 * state and the transition arrow between them. Clicking this, then
	 * activates enters the data. / if there is an after state (like when
	 * there is a duration), then display the after state with and arrow
	 * afterwards»): while a hold's states are on the bar, the first row holds
	 * the field's state now → them, in place of the state now (da-qryu8), and with a length on the bar → the state
	 * it returns to, which is the one it is in now. Null otherwise.
	 */
	const moving = $derived(together && chosen !== null && chosen !== 'false' ? together : null);
	/** The open field's state now, as the transition draws it: its states, `on`, or null for off; undefined where none is known. */
	const fromOf = (b: FieldButton): string[] | null | undefined =>
		!b.active ? null : (b.set ?? (b.current === true ? ['on'] : b.current === undefined ? undefined : [b.current]));
	/** The transition tapped: the states gathered logged, as a state's tap logs one. */
	const transit = (button: FieldButton, states: string[]) => (typesKeys('change', button) ? choose(button, asOne(states), true) : leave(button, ['change']));

	function gather(button: FieldButton, state: string) {
		const was = together ?? [];
		const next = was.includes(state) ? was.filter((s) => s !== state) : [...was, state];
		together = next.length ? next : null;
		place(button, next);
	}

	/**
	 * Run `argv` from the open view, which closes at the tap and not once the
	 * write is in (da-2ylmj): the field's pill, which the write spends, opens
	 * nothing until it has run, and a failure, keeping the pills, opens the
	 * field again.
	 */
	function leave(button: FieldButton, argv: string[]) {
		leaving = button.field;
		const run = (leavingRun = Promise.resolve(onrun(argv)));
		// Only the last run lets go: a state tapped in the field opened again holds it until its own write has run.
		void run.finally(() => leavingRun === run && (leaving = null));
		close(true);
	}

	/** A key's chip: `key=` on the line, the caret after it. */
	function putKey(key: string) {
		const line = get(text);
		prefill.set({ line: `${line}${line && !line.endsWith(' ') ? ' ' : ''}${key}=`, keyboard: true });
	}

	/** Open `button`: its pill on the bar, and its view over the fields. */
	function open(button: FieldButton) {
		// A glide still going stops where it is, for Back to return to.
		cancelAnimationFrame(coast);
		coast = 0;
		entered = { cursor: get(cursor), moment };
		// Its pills a write still running is spending: a fresh pill, which that write does not take with it.
		if (leaving === button.field) setFieldPill(null);
		if ($fieldPill !== button.field) setFieldPill(button.field, button.color);
		restPan = pan;
		pan = null;
		picked = button.field;
	}

	/** Close the field's view: the fields under it are as they were, and the state chosen goes unless a run is `spending` it. */
	function close(spending = false) {
		picked = null;
		pan = restPan;
		restPan = null;
		entered = null;
		together = null;
		if (chosen !== null && !spending) pills.update((all) => all.filter((p) => p.id !== STATE_ID));
	}

	/** Back: the field's pill and the ones made since it was tapped go, and a time pill already there stays. */
	function back() {
		const was = entered;
		setFieldPill(null);
		close();
		if (!was) return;
		if (get(cursor) !== was.cursor) cursor.set(was.cursor);
		setMoment(was.moment);
	}

	function emit(argv: string[]) {
		onrun(argv);
	}

	/** Put `argv` in the command line, unrun, for the rest to be typed. */
	function fill(argv: string[]) {
		onfill(argv);
	}

	/**
	 * A press does what `pressKind` says — the same answer the button's
	 * outline is drawn from, so the two cannot disagree (da-z8z40).
	 */
	function press(button: FieldButton, kind = pressKind(button, pending, oneTap)) {
		// With a command pending, the press IS that command (C1: a field tap runs
		// at once).
		const argv = (pending && pendingPress(pending, button)) || pressArgv(button);
		// A plain press opens the field, to collect it (da-w6437).
		if (kind === 'show') open(button);
		// Its moment at the selected time is taken away: a second tap logs it again (da-0qdsp).
		else if (button.here && !pending) emit(['delete', button.here]);
		// A record, or a field with keys to type: the line goes to the prompt.
		else if (kind === 'fill') fill(argv);
		else emit(argv);
	}

	/**
	 * `pan` kept to what the rows need, sideways alone: at rest or open, as far
	 * as brings the furthest field or state into the room and no further, like
	 * a scroll.
	 */
	function kept(x: number, _y: number) {
		const most = Math.max(0, (picked === null ? shelves.end : laid.end) + GAP - paneW);
		return { x: side === 'right' ? Math.min(most, Math.max(x, 0)) : Math.min(0, Math.max(x, -most)), y: 0 };
	}

	/**
	 * The axes a drag pans the cluster along, those it overruns (`kept`),
	 * declared to PaneFrame as `data-pans` so a drag along one is the room's
	 * and never also the pane's tab swipe (da-ut8e5); along an axis it fits
	 * the swipe is the pane's, as before.
	 */
	const pans = $derived.by(() => {
		const [low, high] = [kept(-Infinity, -Infinity), kept(Infinity, Infinity)];
		return (low.x < high.x ? 'x' : '') + (low.y < high.y ? 'y' : '');
	});

	/**
	 * A drag on the cluster pans it: a press that travels past the slop,
	 * even one that starts on a button, and its click is swallowed; one that
	 * does not is a press of whatever it is on.
	 */
	let drag: {
		id: number;
		x: number;
		y: number;
		from: { x: number; y: number };
		moving: boolean;
		xs: Sample[];
		ys: Sample[];
	} | null = null;
	/** The rows' zoom from small, legible still, to large (da-l1z6h). */
	const ZOOM = { min: 0.65, max: 2.5 };
	const bounded = (z: number) => Math.min(ZOOM.max, Math.max(ZOOM.min, z));
	/**
	 * The zoom no pinch has set: 1, or with `rows` the one at which the box fits that many rows at rest, half a row
	 * spare against `atRest`'s count (da-je8ju, vividn, 2026-11-10: «change the default zoom level of all the
	 * collect widgets in the tutorial to be 3 rows»). From the probe, not the rows as drawn: a row's height is not
	 * quite in proportion to its zoom, and read from the rows the fit swung between two hundredths and never rested.
	 */
	const fitted = $derived(rows && unit && paneH ? ((paneH + GAP) / (rows + 0.5) - GAP) / unit : 1);
	/** The rows' zoom as drawn: `zoom`, inside the bounds whatever was stored, or a pinch's while it moves. */
	let scale = $derived(bounded(zoom ?? fitted));
	/**
	 * The fingers down on modify mode's cover, and a pinch of two of them: how far apart they began, and the
	 * zoom then (da-l1z6h). Only there (da-xi4up, vividn, 2026-11-10: «We can allow pinch zooming when in
	 * modify mode though in addition to the +/-»): out of the mode a pinch zooms nothing.
	 */
	const fingers = new Map<number, { x: number; y: number }>();
	let pinch: { d: number; from: number } | null = null;
	const apart = () => {
		const [a, b] = [...fingers.values()];
		return Math.hypot(a.x - b.x, a.y - b.y);
	};
	/** A drag just ended: the click it ends in is not a press. */
	let dragged = false;
	/** The glide a swipe let go of (da-lw7ue), a frame's request; 0 when none. */
	let coast = 0;
	// Whatever sets a field pill opens that field's view (da-z45a6, vividn:
	// «if anything elese opens a field pill, it hsould also advance to this
	// secondary menu as well»): a hold, the now strip, a reload, a script.
	// Here, below what `open` reads, as it runs at init (da-1oi8w).
	$effect.pre(() => {
		const pilled = picked === $fieldPill || leaving === $fieldPill ? null : fields.find((b) => b.field === $fieldPill);
		if (pilled) untrack(() => open(pilled));
	});
	function down(e: PointerEvent) {
		// Any press ends a swallow still pending. A drag's click follows its
		// pointerup at once, and a touch that travelled makes none, so only a
		// second pen or mouse pressed in between could let one through.
		dragged = false;
		// A finger put down stops a glide where it is.
		cancelAnimationFrame(coast);
		coast = 0;
		if (e.button === 0)
			drag = {
				id: e.pointerId,
				x: e.clientX,
				y: e.clientY,
				from: shift,
				moving: false,
				xs: [{ at: e.clientX, t: e.timeStamp }],
				ys: [{ at: e.clientY, t: e.timeStamp }]
			};
	}
	function move(e: PointerEvent) {
		if (drag?.id !== e.pointerId) return;
		const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
		drag.xs = tracked(drag.xs, e.clientX, e.timeStamp);
		drag.ys = tracked(drag.ys, e.clientY, e.timeStamp);
		if (!drag.moving && Math.hypot(dx, dy) <= CLICK_SLOP) return;
		// A finger going up or down is the page's scroll (`touch-action`), not a pan: no drag of ours (da-tsomc).
		if (!drag.moving && e.pointerType === 'touch' && Math.abs(dy) > Math.abs(dx)) return void (drag = null);
		if (!drag.moving) (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		drag.moving = true;
		pan = kept(drag.from.x + dx, drag.from.y + dy);
	}
	function up(e: PointerEvent) {
		if (drag?.id !== e.pointerId) return;
		dragged = drag.moving;
		// A swipe carries on (vividn, 2026-11-04: «scrolling should have some
		// momentum. Right now there is no inertia with swipes»), by the bar's own
		// reading of its speed (#lib/grip.ts) and its own decay: `speed * TAU`
		// further, slowing to a stop. A finger's or a pen's: a mouse's drag
		// stops where the mouse lets go. A cancel carries nothing on.
		const v = e.type === 'pointerup' && drag.moving && e.pointerType !== 'mouse' ? { x: speedOver(drag.xs, e.clientX, e.timeStamp), y: speedOver(drag.ys, e.clientY, e.timeStamp) } : null;
		drag = null;
		if (v && (v.x || v.y)) glideOn(shift, v);
	}
	/** Pan on from `from` at `v` px/ms, decaying by `TAU`, and as far as `kept` allows. */
	function glideOn(from: { x: number; y: number }, v: { x: number; y: number }) {
		const t0 = performance.now();
		const step = (now: number) => {
			const fade = Math.exp(-(now - t0) / TAU);
			pan = kept(from.x + v.x * TAU * (1 - fade), from.y + v.y * TAU * (1 - fade));
			coast = Math.hypot(v.x, v.y) * fade > 0.01 ? requestAnimationFrame(step) : 0;
		};
		coast = requestAnimationFrame(step);
	}
	$effect(() => () => cancelAnimationFrame(coast));

	// ---- MODIFY MODE (da-xi4up) ---------------------------------------------
	// vividn, 2026-11-10: «in the collect widget for example some things that
	// you could change in the modify mode would be the now section width, have
	// a button to flip left and right, but a + button at the top of the line
	// separating the fields section from the time/now section and a - button
	// on the bottom which changes the number of rows in the widget». A cover
	// over the widget, the rows under it still (`inert`), holds them: a handle
	// on the strip's line, + and − at its ends, ⇄, and the pinch.
	$effect(() => {
		if (bar) bar.inert = modifying;
	});
	/** The zoom at which the room fits `n` rows, as `fitted` reads `rows`. */
	const fitsFor = (n: number) => (unit && paneH ? ((paneH + GAP) / (n + 0.5) - GAP) / unit : 1);
	const moreRows = $derived(fitsFor(rowCount + 1) >= ZOOM.min);
	const fewerRows = $derived(rowCount > 1 && fitsFor(rowCount - 1) <= ZOOM.max);
	/** + and −: a row more or fewer, the zoom left to fit them. */
	const stepRows = (by: 1 | -1) => onmodify?.({ rows: rowCount + by, zoom: undefined });
	/** The handle in hand: the pointer, and the cover it is read against, its box read at each move. */
	let sizing: { id: number; cover: HTMLElement } | null = null;
	function grab(e: PointerEvent) {
		if (e.button !== 0) return;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		sizing = { id: e.pointerId, cover: (e.currentTarget as HTMLElement).parentElement!.parentElement! };
	}
	function slide(e: PointerEvent) {
		const box = sizing?.id === e.pointerId ? sizing.cover.getBoundingClientRect() : null;
		if (!box?.width) return;
		const x = (e.clientX - box.left) / box.width;
		share = shareOf(side === 'right' ? 1 - x : x);
	}
	function drop(e: PointerEvent) {
		if (sizing?.id !== e.pointerId) return;
		sizing = null;
		onmodify?.({ nowWidth: Math.round(share * 100) / 100 });
	}
	function nudgeWidth(e: KeyboardEvent) {
		const by = { ArrowLeft: -0.02, ArrowRight: 0.02 }[e.key];
		if (by === undefined) return;
		e.preventDefault();
		share = shareOf(share + (side === 'right' ? -by : by));
		onmodify?.({ nowWidth: Math.round(share * 100) / 100 });
	}
	/** A pinch on the cover (da-l1z6h): two fingers, their spread the zoom; let go, the layout's. */
	function pinchDown(e: PointerEvent) {
		// A control's own press stays its own: captured here, its click would land on the cover.
		if (e.pointerType !== 'touch' || (e.target as Element).closest('button, .grip')) return;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (fingers.size === 2) pinch = { d: apart(), from: scale };
	}
	function pinchMove(e: PointerEvent) {
		if (!fingers.has(e.pointerId)) return;
		fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (pinch && fingers.size === 2 && pinch.d > 0) scale = bounded((pinch.from * apart()) / pinch.d);
	}
	function pinchUp(e: PointerEvent) {
		if (!fingers.delete(e.pointerId) || !pinch || fingers.size >= 2) return;
		pinch = null;
		onmodify?.({ zoom: scale });
	}
	/**
	 * Ctrl and the wheel, which is what a trackpad's pinch arrives as: a
	 * stream, written once it goes quiet, or as the pointer leaves the cover
	 * for ✓. A write still waiting when the mode ends is dropped, the zoom
	 * drawn back to the layout's: late, it would land in the session, under ↺.
	 */
	let wheeling: ReturnType<typeof setTimeout> | undefined;
	function pinchWheel(e: WheelEvent) {
		if (!e.ctrlKey) return;
		e.preventDefault();
		scale = bounded(scale * Math.exp(-e.deltaY / 200));
		clearTimeout(wheeling);
		wheeling = setTimeout(wheelDone, 150);
	}
	function wheelDone() {
		if (wheeling === undefined) return;
		clearTimeout(wheeling);
		wheeling = undefined;
		onmodify?.({ zoom: scale });
	}
	$effect(() => {
		if (modifying || wheeling === undefined) return;
		clearTimeout(wheeling);
		wheeling = undefined;
		scale = bounded(untrack(() => zoom ?? fitted));
	});
	$effect(() => () => clearTimeout(wheeling));
	/**
	 * A reveal — a focus, a scroll into view — scrolls the room itself (da-71p3u):
	 * taken as the pan that shows the same, so the room stays scrolled by its
	 * `lead` alone and a drag starts from where the cluster is drawn.
	 */
	function fold(e: Event) {
		const room = e.currentTarget as HTMLElement;
		const [dx, dy] = [lead.x - room.scrollLeft, lead.y - room.scrollTop];
		if (!(dx || dy)) return;
		flushSync(() => (pan = kept(shift.x + dx, shift.y + dy)));
		[room.scrollLeft, room.scrollTop] = [lead.x, lead.y];
	}
	/**
	 * A tapped button lets go of the focus before its tap redraws the view
	 * (da-rh5oa): removed while focused, as a field opening, back and a state
	 * all remove theirs, it forces a layout in the middle of the flush. A key's
	 * click (`detail` 0) keeps it.
	 */
	function letGo(e: MouseEvent) {
		const held = document.activeElement;
		if (e.detail && held instanceof HTMLElement && (e.currentTarget as Node).contains(held)) held.blur();
	}
	function swallow(e: MouseEvent) {
		if (!dragged) return;
		dragged = false;
		e.preventDefault();
		e.stopPropagation();
	}

	/** Escape is Back. It marks the key taken, so the command line does not ALSO peel its pills (da-al892). */
	function onWindowKeyDown(e: KeyboardEvent) {
		if (e.key !== 'Escape' || e.defaultPrevented || picked === null) return;
		e.preventDefault();
		back();
	}

	/**
	 * The hold, from the app's ONE implementation of it (#lib/hold.ts): the
	 * timer, the right click, the menu key and shift+click are all in the
	 * action, on the BAR rather than on each button, and `data-field` is
	 * what says which of its children can be held. A press that turns out
	 * to be a hold never reaches the buttons' own `on:click` at all, so
	 * there is nothing left here to swallow it with.
	 */
	const holdField = (el: HTMLElement) => {
		// A state held does the other mode's tap (da-u017i).
		// In 1-click mode, or once gathering, it chooses among several: multi-select (da-laibn).
		if (el.dataset.state !== undefined && picking)
			return clicks === 'one' || together ? gather(picking, el.dataset.state) : choose(picking, el.dataset.state, true);
		// An on/off field's off runs in either mode, so the other mode's tap is its own.
		if (el.dataset.off !== undefined && picking) return picking.shape.kind === 'change' ? choose(picking, 'false', clicks !== 'one') : off(picking);
		const button = buttons.find((b) => b.field === el.dataset.field);
		if (button) hold(button);
	};
</script>

<svelte:window onkeydown={onWindowKeyDown} onpointerup={up} onpointercancel={up} />

<!-- One leg of the transition: a state, or several, each in its color as the state buttons are, off empty (null). -->
{#snippet leg(button: FieldButton, states: string[] | null, current = false)}
	{#if states === null}<span class="leg" data-leg="off">off</span>{:else}{@const color = button.stateColors[states[0]] ?? button.color}<span
			class="leg"
			class:current
			data-current={current ? '' : undefined}
			class:multi={states.length > 1}
			data-leg={states.join(',')}
			style:background={color}
			style:color={inkOver(color)}
			>{#each states as part, j}{@const color = button.stateColors[part] ?? button.color}{#if j}<span class="comma" style:--next={color}>,</span
				>{/if}<span style:color={inkOver(color)}>{named(part)}</span>{/each}</span
		>{/if}
{/snippet}

<!-- One field's button, wherever it stands: in the cluster, or aligned to the picture. -->
{#snippet fieldButton(button: FieldButton)}
	{@const look = lookOf(button, switchLabel)}
	{#if button.shape.kind === 'record'}
		<!-- A PRESS runs nothing: the line goes to the bar's prompt. The
		     HOLD is every field's (da-8810n) — a record is a field like
		     any other, and only what a press MEANS is its own. -->
		<button
			class="record"
			use:act={pressKind(button, pending, oneTap)}
			class:ignored={button.ignored}
			class:pilled={$fieldPill === button.field}
			role="link"
			data-field={button.field}
			data-ignored={button.ignored ? '' : undefined}
			data-pilled={$fieldPill === button.field ? '' : undefined}
			style={vars(look) + tallOf(button)}
			{disabled}
			title={titleOf(button)}
			onclick={() => press(button)}>{button.field}</button
		>
	{:else}
		<button
			class="widget {button.shape.kind.replace('/', '')}"
			use:act={pressKind(button, pending, oneTap)}
			class:on={look.fill !== null}
			class:ignored={button.ignored}
			class:pilled={$fieldPill === button.field}
			style={vars(look) + tallOf(button)}
			{disabled}
			data-field={button.field}
			data-ignored={button.ignored ? '' : undefined}
			data-pilled={$fieldPill === button.field ? '' : undefined}
			title={titleOf(button)}
			onclick={() => press(button)}
		>{#if look.glyph}<span class="glyph" aria-hidden="true">{look.glyph}</span>{/if}{look.label}{button.shape.kind === 'change' ? ':' : ''}</button>
	{/if}
{/snippet}

<!-- The now strip (da-lw7ue), the widget's height (da-u7tij): ONE for the fields at rest, a field's view and the
     entries', so a tap between them does not draw it again (da-rh5oa). At rest a sideways drag on it pans the rows, as the room's, and a vertical one scrolls the page. -->
{#snippet present()}
{@const pans = !card && !picking}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="present"
	class:right={side === 'right'}
	class:pans
	style:width="{strip}px"
	data-strip
	onpointerdown={pans ? down : undefined}
	onpointermove={pans ? move : undefined}
	onclickcapture={pans ? swallow : undefined}
>
	<NowStrip doc={nowDoc} pending={nowPending} at={frame?.at ?? null} span={frame?.span} width={strip - PICTURE_GAP} height={boxH} mirrored={side === 'right'} />
	{#each nowBands as band (band.field)}
		<button
			class="now-state"
			data-now-field={band.field}
			tabindex="-1"
			style:top="{band.y * boxH}px"
			style:height="{Math.max(3, band.height * boxH)}px"
			{disabled}
			title="{band.field}{typeof band.current === 'string' ? ` · ${band.current}` : ''} · tap to select the field"
			aria-label="select {band.field}"
			onclick={() => setFieldPill(band.field, band.color)}
		></button>
	{/each}
</div>
{/snippet}

<div class="fields" class:modifying class:picture class:zoomed={picture && zoomedIn} data-align={picture ? 'picture' : 'rows'} bind:clientWidth={boxW} bind:clientHeight={boxH} onclickcapture={letGo}>
<ButtonRows
	items={entries}
	{arrange}
	maxRows={Infinity}
	center={!picture && buttons.length > 0}
	key={keyOf}
	class={picking || card ? 'picking' : clustered.length ? 'clustered' : ''}
	bind:bar
>
	<!-- Given a line colour, or `var(--line)` voids its border and the rows
	     are counted two pixels short of the buttons they hold. -->
	{#snippet probe()}
		<button class="widget" style="--line: transparent" tabindex="-1" bind:offsetHeight={unit}>probe</button>
	{/snippet}
	{#snippet children({ item })}
		{#if item.kind === 'hint'}
			<!-- No direction word here on purpose: this hint said "below" while the
			     bar was at the top of the page (da-nz29), and da-1x72 moved the
			     bar to the bottom without the copy following. Name the thing, not
			     where it is. -->
			<p class="hint empty">nothing tracked yet — set a field up in the setup widget, or type a command to log something</p>
		{:else if item.kind === 'field'}
			{@const stand = stands.get(keyOf(item))}
			<div
				class="slot"
				class:stand={picture}
				class:glide={glide && !live}
				style:left={stand ? `${stand.x}px` : null}
				style:top={stand ? `${stand.top}px` : null}
				use:sized={keyOf(item)}
			>
				{@render fieldButton(item.button)}
			</div>
		{/if}
	{/snippet}
	{#snippet overlay()}
	{#if card || resting}{@render present()}{/if}
	{#if card}
		<!-- The entries selected (da-xdz73), oldest first (da-ug56t), beside the now strip: each its field and time, then its data. -->
		<div class="collect open entry" data-entry={hids} style:left={side === 'left' ? `${strip}px` : null} style:right={side === 'right' ? `${strip}px` : null}>
			<div class="head">
				<!-- The hid pill that opened the cards, with its ×, in back's place (da-dswuc): a press anywhere on it closes them. -->
				{#if hidOpener}<span class="opener" data-back data-open-hid={hids}><PillChip pill={hidOpener} closes={unselect} {disabled} /></span>{/if}
			</div>
			{#each cards as { hid, entry } (hid)}
				{@const button = fields.find((b) => b.field === entry.field)}
				{@const state = entry.state}
				<div class="card" data-card={hid}>
					<div class="title">
						<!-- A field with no button (`timezone`, one deleted) has no view to open: its name is text. -->
						{#if button}<button class="name link" style:color={button.color} data-entry-field title="collect {entry.field}" onclick={() => toField(button.field, button.color)}>{entry.field}</button>{:else}<span class="name">{entry.field}</span>{/if}
						{#if state !== null && state !== undefined}<span style:color={button?.stateColors[said(state)]}>{said(state)}</span>{/if}
						{#if entry.at}<button class="clock link" data-entry-time title="put this time on the bar" onclick={() => toTime(entry)}>{clockOf(entry)}</button>{/if}
						<span class="ref">{labelOf(hid, $refs)}</span>
						<button class="nav round all" class:on={everything.includes(hid)} data-all aria-pressed={everything.includes(hid)} title="show all of it, as get -A" onclick={() => toggleAll(hid)}>−A</button>
					</div>
					<dl>
						<!-- Unkeyed: under −A a data key may share a column's name (an import's `data.at`). -->
						{#each rowsOf(hid, entry) as [key, value]}
							<div data-row={key}><dt>{key}</dt><dd>{said(value)}</dd></div>
						{/each}
					</dl>
				</div>
			{/each}
		</div>
	{:else if picking || clustered.length}
		{@const here = picking}
		<!-- The fields' place (da-w6437): at rest the fields in their rows
		     (da-lw7ue); a field tapped, its name at the top, its states in the
		     middle, its keys in the drawer, the fields hidden under it. -->
		<div class="collect" class:open={!!here} style:left={here && resting && side === 'left' ? `${edge}px` : null} style:right={here && resting && side === 'right' ? `${edge}px` : null}>
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="room"
				bind:this={roomEl}
				bind:clientWidth={paneW}
				bind:clientHeight={paneH}
				data-pans={pans || undefined}
				style:--zoom={scale}
				onpointerdown={down}
				onpointermove={move}
				onscroll={fold}
				onclickcapture={swallow}
			>
				{#if here}
				<!-- The room's box, `lead` into its scroll: the rows are drawn where they were, and nothing of them before the scroll's start. -->
				<div class="lead" style:left="{lead.x}px" style:width="{paneW}px" style:height="{paneH}px">
					<svg class="dotted" aria-hidden="true">
						<path
							d={laid.stands.flatMap((s) => s.outline ?? []).join('')}
							transform={side === 'right' ? `matrix(-1 0 0 1 ${paneW + shift.x} 0)` : `translate(${shift.x} 0)`}
						/>
					</svg>
					{#each laid.keys as key, i (key)}
						{@const stand = laid.stands[i]}
						{@const state = stated[i - 1]}
						<!-- A state is not shown, nor tapped, before it stands where it stays; the first row's place depends on no other's, only its own width, mirrored on the right. -->
						<div
							class="spot open"
							data-role={i ? 'state' : 'head'}
							data-row={stand?.row}
							style:left="{openAt(stand?.x ?? 0, key)}px"
							style:top="{stand?.top ?? 0}px"
							style:visibility={i < laid.ready ? null : 'hidden'}
							use:sized={key}
						>
							{#if i === 0}
								<!-- The first row (vividn, 2026-11-07): the field's pill, which is back and the field at once (da-dswuc, vividn, 2026-11-10: «just the field pill. 2 birds»), and the state it is in drawn as the CLI paints one, or the transition, which starts with it (da-qryu8). -->
								{@const opener = $pills.find((p) => p.kind === 'field' && p.text === here.field) ?? { id: FIELD_ID, kind: 'field', text: here.field, color: here.color }}
								<span class="opener" data-back data-open-field={here.field}><PillChip pill={opener} closes={back} {disabled} /></span>
								<!-- One state or several (da-o5jfh), painted as the transition's legs are. -->
								{@const now = fromOf(here)}
								{#if now && !moving}{@render leg(here, now, true)}{/if}
								{#if moving}
									<!-- The transition, while a hold's states are on the bar (da-rou2i): now → them (→ back, with a length). -->
									{@const from = fromOf(here)}
									<button
										class="widget states transition"
										use:act={typesKeys('change', here) ? 'fill' : 'run'}
										style={vars({ fill: null, line: here.color, text: 'var(--ink-bright)', label: '' })}
										{disabled}
										data-transition
										title={shellJoin(['change', here.field, asOne(moving)])}
										onclick={() => transit(here, moving)}
										>{#if from !== undefined}{@render leg(here, from)}<span class="arrow">→</span>{/if}{@render leg(here, moving)}{#if duration && from !== undefined}<span
												class="arrow">→</span
											>{@render leg(here, from)}{/if}</button
									>
								{/if}
								{#if !moving && chosen !== null && chosen !== 'false'}<span class="to">→ <span style:color={here.stateColors[chosen] ?? here.color}>{named(chosen)}</span></span>{/if}
							{:else if state === 'false'}
								<!-- Off, the first state where it is offered (da-qryu8): empty, an outline alone. -->
								<button
									class="widget states"
									use:act={clicks === 'two' && here.shape.kind === 'change' ? 'arm' : 'run'}
									class:on={!here.active}
									class:chosen={chosen === 'false'}
									style={vars(offLook(here))}
									{disabled}
									data-off
									title={shellJoin(['off', here.field])}
									onclick={() => ((together = null), off(here))}
								>{#if !here.active}<span class="repeat" data-repeat>!</span>{/if}off</button>
							{:else}
								{@const parts = sets.get(state)}
								{@const look = stateLook(here, state, parts)}
								{@const on = isIn(here, state)}
								<!-- A state RUNS at once, drawn as acting (da-z8z40) — unless the field has keys to type, or a tap only chooses it (2-click, da-u017i). -->
								<button
									class="widget states"
									use:act={clicks === 'two' || together ? 'arm' : typesKeys('change', here) ? 'fill' : 'run'}
									class:on
									class:chosen={together ? together.includes(state) : chosen === shellJoin([state])}
									style={vars(look)}
									{disabled}
									data-state={state}
									title={shellJoin(['change', here.field, state])}
									onclick={() => (together ? gather(here, state) : choose(here, state))}
									class:multi={!!parts}
								>{#if on}<span class="repeat" data-repeat>!</span>{/if}{#if parts}{#each parts as part, j}{@const color = here.stateColors[part] ?? here.color}{#if j}<span
												class="comma"
												style:--next={color}>,</span
											>{/if}<span data-part={part} style:color={inkOver(color)}>{part}</span>{/each}{:else}{named(state)}{/if}</button>
							{/if}
						</div>
					{/each}
				</div>
				{:else if resting}
					<!-- At rest (da-lw7ue): the strip, the active section, then each row by recency. -->
					<svg class="backing" aria-hidden="true">
						<path d={frozen ? shelves.backing : `M0 0H${edge}V${paneH}H0Z`} transform={side === 'right' ? `matrix(-1 0 0 1 ${paneW} 0)` : undefined} />
					</svg>
					{#if shelves.outline}
						<svg class="dotted" class:frozen style:transform={frozen ? null : `translateX(${shift.x}px)`} aria-hidden="true">
							<path d={shelves.outline} transform={side === 'right' ? `matrix(-1 0 0 1 ${paneW} 0)` : undefined} />
						</svg>
					{/if}
					{#each clustered as button (button.field)}
						{@const stand = shelves.stands.get(button.field)}
						{@const pinned = button.active && frozen}
						<div
							class="spot shelf"
							class:glide
							class:pinned
							style:--glide="{GLIDE_MS}ms"
							data-role="field"
							data-row={shelves.rows.get(button.field)}
							data-section={button.active ? 'active' : 'recent'}
							data-promoted={first.includes(button.field) ? '' : undefined}
							style:left="{across(stand?.x ?? 0, button.field)}px"
							style:top="{stand?.top ?? 0}px"
							style:transform={pinned ? null : `translateX(${shift.x}px)`}
							use:sized={`f:${button.field}`}
						>
							{@render fieldButton(button)}
						</div>
					{/each}
				{/if}
			</div>
		</div>
		<!-- Its data, a drawer on the far edge (da-igqs2): one per field, so it opens again for a field with a required key. -->
		{#if here}
			{#key here.field}<CollectDrawer keys={here.keys} {disabled} edge={side === 'right' ? 'left' : 'right'} onkey={putKey} onmore={() => focusLine(true)} />{/key}
		{/if}
		<!-- After the rows and the view, so a field comes first in the Tab order; it stands between them and the strip. -->
		{#if resting}<CollectWhen {disabled} {side} offset={strip} noLength={here?.shape.kind === 'moment'} shown={timeOpen} onshown={ontime} bind:width={whenW} />{/if}
	{/if}
	{/snippet}
</ButtonRows>
{#if modifying}
	<!-- Modify mode's cover (da-xi4up): the rows under it still, the pinch its own. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="modify"
		class:right={side === 'right'}
		data-modify
		data-rows={rowCount}
		data-now-width={share}
		onpointerdown={pinchDown}
		onpointermove={pinchMove}
		onpointerup={pinchUp}
		onpointercancel={pinchUp}
		onwheel={pinchWheel}
		onpointerleave={wheelDone}
	>
		<button type="button" class="flip" style:width="{strip}px" title="put the now section on the {side === 'left' ? 'right' : 'left'}" aria-label="flip" onclick={() => onmodify?.({ side: side === 'left' ? 'right' : 'left' })}>⇄</button>
		<div class="seam" style:left={side === 'left' ? `${strip}px` : null} style:right={side === 'right' ? `${strip}px` : null}>
			<button type="button" class="step" title="a row more" aria-label="more rows" disabled={!moreRows} onclick={() => stepRows(1)}>+</button>
			<!-- Its touch moves consumed: unconsumed, Chrome flings a drag let go at speed, `touch-action: none` or not, and a tap within ~150 ms only cancels the fling, with no click (da-rw31h). -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
			<div
				class="grip"
				role="separator"
				tabindex="0"
				aria-orientation="vertical"
				aria-label="now section width"
				aria-valuenow={Math.round(share * 100)}
				aria-valuemin={NOW_WIDTH.min * 100}
				aria-valuemax={NOW_WIDTH.max * 100}
				onpointerdown={grab}
				onpointermove={slide}
				onpointerup={drop}
				onpointercancel={drop}
				onkeydown={nudgeWidth}
				{@attach (el) => on(el, 'touchmove', (e) => e.preventDefault(), { passive: false })}
			></div>
			<button type="button" class="step" title="a row fewer" aria-label="fewer rows" disabled={!fewerRows} onclick={() => stepRows(-1)}>−</button>
		</div>
	</div>
{/if}
{#if picture && zoomedIn}
	<!-- Which part of the pictures' height is in view; a tap shows it all again. -->
	<button
		class="range"
		data-from={view.from}
		data-to={view.to}
		title="showing {Math.round(view.from * 100)}–{Math.round(view.to * 100)}% of the pictures' height · tap to zoom out"
		aria-label="zoom out"
		onclick={() => (view = FULL)}
	>
		<span style:top="{view.from * 100}%" style:height="{(view.to - view.from) * 100}%"></span>
	</button>
{/if}
</div>

<style>
	/* Modify mode (da-xi4up): the rows dimmed and still under the cover, which a pinch is the widget's on. */
	.modifying > :global(.bar) {
		opacity: 0.45;
	}
	.modify {
		position: absolute;
		inset: 0;
		z-index: 5;
		touch-action: none;
	}
	.modify .flip {
		position: absolute;
		top: 50%;
		left: 0;
		transform: translateY(-50%);
		min-height: 2rem;
		border: 1px solid var(--accent);
		border-radius: 3px;
		background: var(--ground);
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}
	.modify.right .flip {
		left: auto;
		right: 0;
	}
	/* The handle is the whole line, however short the widget; + and − lie over its ends. */
	.seam {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 24px;
		transform: translateX(-50%);
	}
	.modify.right .seam {
		transform: translateX(50%);
	}
	.seam .step {
		position: absolute;
		left: 0;
		z-index: 1;
		width: 24px;
		height: 24px;
		padding: 0;
		border: 1px solid var(--accent);
		border-radius: 50%;
		background: var(--ground);
		color: var(--accent);
		font: inherit;
		line-height: 1;
		cursor: pointer;
	}
	.seam .step:first-child {
		top: 0;
	}
	.seam .step:last-child {
		bottom: 0;
	}
	.seam .step:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.grip {
		position: absolute;
		top: 0;
		bottom: 0;
		left: 7px;
		width: 10px;
		border-radius: 5px;
		background: var(--accent);
		cursor: ew-resize;
		touch-action: none;
	}

	.fields {
		position: relative;
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		/* The empty hint's measure (`100cqw` below). */
		container-type: inline-size;
	}

	/* A pinch on the bar is the widget's, not the page's; its sideways
	   scroll is still the browser's, and so is a scroll up or down, the
	   page's, until a zoom makes that the view's pan (da-tsomc). */
	.picture :global(.bar) {
		touch-action: pan-x pan-y;
	}

	.picture.zoomed :global(.bar) {
		touch-action: pan-x;
	}

	/* ALIGNED TO THE PICTURE (da-omw4n): each field's slot where `aligned`
	   stood it, over the bar's own box — whose sideways scroll is what a
	   band too crowded for the width runs into. */
	.slot.stand {
		position: absolute;
		white-space: nowrap;
	}

	.slot.glide {
		transition:
			left 0.25s ease-out,
			top 0.25s ease-out;
	}

	@media (prefers-reduced-motion: reduce) {
		.slot.glide {
			transition: none;
		}
	}

	/* The zoom's range: a slim track down the widget's right edge, the part
	   of the pictures in view lit on it. */
	.range {
		position: absolute;
		top: 0;
		right: 0;
		bottom: 0;
		width: 10px;
		padding: 0;
		border: none;
		border-left: 1px solid var(--rule);
		background: var(--ground);
		cursor: zoom-out;
	}

	.range span {
		position: absolute;
		left: 2px;
		right: 2px;
		border-radius: 2px;
		background: var(--accent);
		transition:
			top 0.25s ease-out,
			height 0.25s ease-out;
	}

	/* Never wider than the pane (da-hygvc): a longer name is cut, and the
	   rest of it is not left under the pane beside, which a tap there
	   reaches instead. */
	.widget,
	.record {
		max-width: 100cqw;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.widget,
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

	.widget.moment {
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
	   outline, as the commands widget's are (#lib/act.ts) — in the
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
	.record:disabled,
	.nav:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* `−A` is no state, so it is drawn hollow in the app's own ink rather
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

	/* While the cluster or a field's view is up, the bar's rows are hidden
	   under it, and the bar is no containing block, so it fills the widget
	   whatever the bar is scrolled to — and the scroll is kept for Back. */
	.fields :global(.bar.picking),
	.fields :global(.bar.clustered) {
		position: static;
	}

	.fields :global(.bar.picking > .section),
	.fields :global(.bar.clustered > .section) {
		visibility: hidden;
	}

	/* The fields' place (da-w6437): a field's view top to bottom — its
	   pill, its states in the room — or at rest the room alone. */
	.collect {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		container: collect / size;
	}

	/* The entries' cards scroll where they do not fit; a field's view is the
	   room alone, its rows the room's height as at rest (da-r48y9). */
	.collect.entry {
		padding: 0.25rem;
		overflow: hidden auto;
	}

	/* A pane a few rows tall, a phone's main tab: `−A` a little smaller. */
	@container collect (max-height: 140px) {
		.nav.round {
			min-width: 2rem;
			min-height: 2rem;
		}
	}

	.head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.card .name {
		font-weight: 600;
	}

	/* The entries selected (da-xdz73): their cards one under another, in the middle of the widget. */
	.entry .card {
		margin: 0 auto;
		flex: none;
		max-width: 100%;
		padding: 0.6rem 0.8rem;
		border: 1px solid var(--rule);
		border-radius: 6px;
		font-size: 0.85rem;
	}

	.head + .card {
		margin-top: auto;
	}

	.entry .card:last-child {
		margin-bottom: auto;
	}

	.card .title {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}

	/* The field and the time are links (da-ffwmw): buttons drawn as the words. */
	.card .link {
		background: none;
		border: none;
		padding: 0;
		font-family: inherit;
		font-size: inherit;
		line-height: inherit;
		color: inherit;
		text-decoration: underline;
		cursor: pointer;
	}

	/* The browser's own ring is near-black, no ring on this page. */
	.card .link:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.card .all {
		margin-left: auto;
	}

	.card .all.on {
		border-color: var(--accent);
		color: var(--ink-bright);
	}

	.entry .ref,
	.card .clock,
	.card dt {
		color: var(--muted);
	}

	.card dl {
		margin: 0.4rem 0 0;
	}

	.card dl div {
		display: flex;
		gap: 0.6rem;
	}

	.card dd {
		margin: 0;
		overflow-wrap: anywhere;
	}

	.spot.open .to {
		color: var(--muted);
	}

	/* The cluster's room: placed by `around` and panned by a drag. */
	.room {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		/* A finger's sideways drag pans the cluster and two fingers zoom it;
		   up and down, which the rows never move, the page scrolls (da-tsomc).
		   `noScroll` keeps the page off a gesture once it is ours. */
		touch-action: pan-y;
	}

	.lead {
		position: absolute;
		top: 0;
	}

	/* At rest (da-lw7ue): the strip and, while it is frozen, the active
	   section stand over the rows that pan under them, each row under its
	   own part of the section (da-il6ni). A field's place glides (`glide`);
	   one leaving the section keeps above the backing until it has left it,
	   and one entering rises above it at once. */
	.backing {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		z-index: 1;
		pointer-events: none;
		fill: var(--ground);
	}

	.backing path {
		pointer-events: auto;
	}

	.present {
		position: absolute;
		inset: 0 auto 0 0;
		z-index: 2;
		border-right: 1px solid var(--rule);
	}

	/* A band's hit area over the strip's picture (NowStrip.svelte): a tap selects the field. */
	.now-state {
		position: absolute;
		left: 0;
		right: 0;
		padding: 0;
		border: none;
		background: none;
		cursor: pointer;
	}

	/* At rest a finger's sideways drag on the strip pans the rows, and up and
	   down the page scrolls, as on the room (da-90nlf). */
	.present.pans {
		touch-action: pan-y;
	}

	/* The mirror (`side`): the strip and the section at the right edge. */
	.present.right {
		left: auto;
		right: 0;
	}

	.present.right {
		border-right: none;
		border-left: 1px solid var(--rule);
	}

	/* Mirrored, the picture keeps to the pane's edge, its gap toward the line. */
	.present.right :global(.now-strip) {
		left: auto;
	}

	.dotted {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		overflow: visible;
		/* Not the ground the app's pictures stand on: the rows show through. */
		background: none;
		pointer-events: none;
		fill: none;
		stroke: var(--muted);
		stroke-width: 1.5;
		stroke-dasharray: 1.5 4;
		stroke-linecap: round;
	}

	.dotted.frozen {
		z-index: 2;
	}

	.spot.shelf {
		transition: z-index 0s var(--glide);
	}

	/* The rows' zoom (da-l1z6h): the button and all in it, so its slot,
	   which `atRest` counts the rows from, is measured at the zoom. */
	.spot.shelf > button {
		zoom: var(--zoom);
	}

	.spot.shelf.glide {
		transition:
			left var(--glide) ease-out,
			top var(--glide) ease-out,
			z-index 0s var(--glide);
	}

	.spot.shelf.pinned {
		z-index: 2;
		transition: z-index 0s;
	}

	.spot.shelf.pinned.glide {
		transition:
			left var(--glide) ease-out,
			top var(--glide) ease-out,
			z-index 0s;
	}

	@media (prefers-reduced-motion: reduce) {
		.spot.shelf.glide {
			transition: none;
		}
	}

	.nav {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		line-height: 1;
	}

	/* The pill that opened a view (da-dswuc), at the size back was. */
	.opener {
		display: inline-flex;
		--input-font: 0.85rem;
	}

	/* `−A`: round. */
	.nav.round {
		justify-content: center;
		min-width: 2.4rem;
		min-height: 2.4rem;
		border-radius: 999px;
		padding: 0;
	}

	/* The state whose run is pending or failed: ringed. */
	.states.chosen {
		box-shadow: 0 0 0 2px var(--ink-bright);
	}

	/* The state the field is in already, and off while it is off: a switch to
	   it is the CLI's yellow ! of a repeat, in front (vividn, 2026-11-07). */
	.repeat {
		margin-right: 0.3em;
		color: var(--warn);
		font-weight: 700;
	}

	/* An open field's first row (da-r48y9), at the rows' zoom as each state is. */
	.spot.open {
		align-items: center;
		gap: 0.5rem;
	}

	.spot.open > * {
		zoom: var(--zoom);
	}

	/* The state it is in, painted as the CLI paints a state: its colour, the CLI's ink over it. */
	.current {
		padding: 0 0.3em;
		font-size: 0.85rem;
	}

	/* A flex box, so it is exactly its button's size and nothing of a line box. */
	.spot {
		position: absolute;
		display: flex;
		white-space: nowrap;
	}

	/* The state the field is in now: bold. Every state is filled (`stateLook`). */
	.states.on,
	.widget.change.on {
		font-weight: 700;
	}

	/* A multi-state (da-a2cxa): each state's color behind its own words,
	   the boundary through the middle of each comma, slanted right at the
	   45° of the stripes dayview draws a multi-state with. Each comma paints
	   the next state's color from its middle on, under the words. */
	.states.multi {
		position: relative;
		isolation: isolate;
		overflow: hidden;
	}

	.comma {
		position: relative;
	}

	/* The transition (da-rou2i): an outline, each leg filled in its state's color, the arrows between them bare. */
	.transition {
		display: inline-flex;
		align-items: center;
		gap: 0.3em;
	}

	.leg {
		padding: 0 0.3em;
		border-radius: 2px;
	}

	.leg.multi {
		position: relative;
		isolation: isolate;
		overflow: hidden;
	}

	.comma::before {
		content: '';
		position: absolute;
		z-index: -1;
		left: 50%;
		right: -100vw;
		top: -2em;
		bottom: -2em;
		background: var(--next);
		transform: skewX(-45deg);
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

	.hint {
		margin: 0;
		color: var(--quiet);
		font-size: 0.8rem;
		font-style: italic;
		/* A sentence, wrapped to the widget's width: the rows scroll
		   sideways under it, and on a phone's half-screen pane cut it at
		   «nothing tracked yet -» (da-dq19n). */
		max-width: 100cqw;
	}
</style>
