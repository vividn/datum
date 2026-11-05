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
	selected time the strip is centered on it, still, and the fields stand as
	they stood then, a moment there in the active section and a tap taking it
	away (`nowAt`, `here`, da-0qdsp). And a
	tap on an active field's band puts that field's pill on the bar; then the ACTIVE SECTION, every field in an active state
	inside a dotted outline that steps with each row; then each row's other
	fields right against its own part of the section (da-il6ni), the most
	recently used first (`rowsUnder`, the order taken on
	load and on coming back to the foreground). A drag pans those sideways,
	gliding on after a swipe; the strip and the active section stay, until
	the section is wider than half the widget and pans with them. A field
	that starts glides into the section, and one that ends out of it into the
	first slot of its row (`bumped`). A PINCH on the rows — two fingers, or
	ctrl+wheel, a trackpad's — zooms them (da-l1z6h): each field's button,
	its font and its height together, so smaller fits more rows into the
	widget and larger fewer, the rows still locked to the pictures' height.
	The zoom is a setting the widget moves itself (`onzoom`), between
	`ZOOM`'s bounds, and kept on this device across reloads, by the layout,
	tab and widget, as every option a widget moves is (da-ray91, da-gh1ep).
	While two fingers are down, and until the rows have settled after the
	last is up, no tap logs or opens anything (`untapped`). A tutorial game's fields (`tapLogs`)
	keep the cluster: the first in the middle of the widget and the rest ring
	by ring around it (`ringOf`).

	A TAP OPENS a field (vividn's frames, 2026-11-03), a switch alone in
	1-click mode and every field in 2-click (below): its pill goes on the
	bar and its view takes the widget, top to bottom with no labels —
	‹ back and the field; its TIME, `−q` (five minutes earlier), and for a
	field with a length `−m` (a moment) and `5m` (five minutes long), each
	becoming a two-part pill whose left part adds another five where it
	still can and whose × clears it, `−m` and the length one pill and never
	both; a switch field's STATES, the first of its spec in the middle and
	the rest ring by ring, the one it is in marked with the CLI's yellow `!`
	and off left to the commands; and its KEYS at the bottom, a tap putting
	`key=` on the line, with `+ data` taking the caret there. Its time is the
	page's selection (#lib/cursor.ts), so the `-qq` and length pills are the
	nowview's own. A state tapped LOGS AT ONCE with them (vividn, 16:07: one
	click); any other field runs from the commands, which take the pills.
	In 1-click mode, the default, a field that needs nothing more is not
	opened at all (da-gra0z, `pressKind`): a tap on a moment logs it, on a
	block starts or ends it, with the pills there are.
	In 2-click mode (`clicks`, da-u017i) every field opens, and a tap only
	CHOOSES the state: its pill is placed and its chip ringed, and the command
	pane's ready «switch → state» logs it. A long press, on a field or a
	state, does the other mode's tap (da-z45a6). Whatever puts a field pill
	on the bar opens that field: a hold, the now strip, a script.
	A run spends them, the field's with them, and collect is back at rest.
	Back takes the field's pill and every pill made since the tap, and puts
	back the time and `-m` there were before it; Escape is Back.

	Ignored fields get no button: the setup widget lists them under a new
	field, and a press runs `track <field>`. `showIgnored` (da-xhio) is the way to SEE them
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
	pending command, both filters applying. And a press on a PICTURE gives
	the fields drawn near that height the middle of the cluster, ahead of
	the rest (`rowsUnder`, #lib/lanes.ts): that one only ORDERS, so
	everything is still there. Nothing moves under a finger: while a
	pointer is over the bar the order is held until it leaves. A field open
	stays open whatever filters the bar: the length made in its view would.

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
<script lang="ts">
	import { flushSync, onMount, untrack } from 'svelte';
	import ButtonRows from './ButtonRows.svelte';
	import LengthArrow from './LengthArrow.svelte';
	import NowStrip from './NowStrip.svelte';
	import type { Mark } from './nowStrip.js';
	import { CLICK_SLOP } from './inspect.js';
	import { TAU } from './detent.js';
	import { speedOver, tracked, type Sample } from './grip.js';
	import {
		FULL,
		aligned,
		atRest,
		panned,
		ringIn,
		dealt,
		lookOf,
		yIn,
		zoomed,
		pressArgv,
		stateLook,
		type Look,
		type SwitchLabel,
		type FieldButton,
		type FieldKind,
		type Standings,
		type Align,
		type View
	} from './fields.js';
	import { get } from 'svelte/store';
	import { act } from './act.js';
	import { holds } from './hold.js';
	import { fieldPill, focusLine, pills, pictured, prefill, refs, selected, setFieldPill, text, type Pill } from './bar.js';
	import { onAnyChange } from './changes.js';
	import { currentDb } from './database.js';
	import { run } from './run.js';
	import { entryArgv } from './calls.js';
	import { SPAN_ID, cursor, pointAt, pointed, type Cursor } from './cursor.js';
	import { grammar } from '#lib/layout/engine.ts';
	import { previewOf } from './preview.js';
	import { labelOf, withPills } from './selection.js';
	import { closer, rowsUnder, type Near } from './lanes.js';
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
		/**
		 * What a press on a picture landed on or near, field by rank
		 * (`nearest` in #lib/lanes.ts), or null.
		 *
		 * Those fields come FIRST. Nothing is hidden by it: a person who
		 * clicked empty space still finds every field, in its usual order.
		 */
		rank = null,
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
		 * A tap logs at once, as before collect (da-uzyih.4940): a tutorial
		 * game's fields alone (LessonFields.svelte) set it. The games score a
		 * single tap — the moment game its reaction time — and re-teaching
		 * them on collect's tap-then-state flow is vividn's call, not taken
		 * yet. A moment or block logs on the tap, whatever `clicks` says; a
		 * switch field opens its states, and a state logs `switch <field>
		 * <state>` whole, with no pill on the app's bar.
		 */
		tapLogs = false,
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
		/** How large the rows at rest are drawn, a button's font and height together (da-l1z6h). */
		zoom = 1,
		/** A pinch let go of, or a ctrl+wheel: the zoom it left, for the session to keep on this device. */
		onzoom,
		/** Run a press's line. */
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
		rank?: Map<string, Near> | null;
		showIgnored?: boolean;
		standings?: Standings;
		clicks?: 'one' | 'two';
		tapLogs?: boolean;
		side?: 'left' | 'right';
		nowDoc?: Document | null;
		nowPending?: Mark[];
		nowAt?: number | null;
		zoom?: number;
		onzoom?: (zoom: number) => void;
		onrun: (argv: string[]) => void;
		onfill: (argv: string[]) => void;
	} = $props();

	/** Does a bare tap log what needs nothing more (`pressKind`)? */
	const oneTap = $derived(tapLogs || clicks === 'one');

	let bar: HTMLElement | undefined = $state.raw();
	// The hold's action (#lib/hold.ts) on the bar ButtonRows draws — an
	// action cannot be handed through a component, so it is attached here.
	// So are the picture layout's pinch, pan and wheel, and, after the hold,
	// what keeps two fingers from tapping.
	onMount(() => {
		const held = holds(bar!, { within: 'button[data-field], button[data-state]', run: holdField });
		const unzoom = zooming(bar!);
		const untap = untapped(bar!);
		const box = new ResizeObserver(() => (tall = bar!.clientHeight));
		box.observe(bar!);
		return () => (held.destroy(), unzoom(), untap(), box.disconnect());
	});

	/** How long the rows glide to a new zoom (`.spot.shelf.glide`): a tap in it is not a press. */
	const SETTLE_MS = 300;
	/**
	 * No tap while two fingers are down (da-gh1ep, vividn, 2026-11-05: «If two
	 * fingers are on the collect view or you're in the middle of the zoom
	 * tapping action should not be registered»): from a second finger landing
	 * until every finger is up, and `SETTLE_MS` on, no click on the bar is a
	 * press, wherever it is, and a hold one of the fingers started is called
	 * off. Registered after `holds`, on the same node, so its cancel comes
	 * after the hold's own pointerdown has started a timer for the finger.
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
			if (down.delete(e.pointerId) && !down.size && pinched) [pinched, calm] = [false, e.timeStamp + SETTLE_MS];
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
	/**
	 * What the bar held as the field was tapped: its selection and whether
	 * it said `-m`. Back puts them back, so a time pill made before the tap
	 * stays and one made since goes (vividn, 14:49).
	 */
	let entered: { cursor: Cursor | null; moment: boolean } | null = null;
	/** Where the first of the cluster stands in its box, from the box's top left corner (`around`). */
	let anchor = $state.raw({ x: 0, y: 0 });
	/** How far a drag has panned the cluster from the center, kept so it cannot leave the room; none at rest. */
	let pan: { x: number; y: number } | null = $state.raw(null);
	/** The fields' pan while a field is open, for its view to close back to. */
	let restPan: { x: number; y: number } | null = null;
	/** The size of the room the cluster stands in, and of the cluster's own box. */
	let paneW = $state(0);
	let paneH = $state(0);
	let extent = $state.raw({ w: 0, h: 0 });
	/** Whether a pointer — a mouse, a finger — is over the bar right now. */
	let over = $state(false);
	/**
	 * The ranking the bar is DRAWN under.
	 *
	 * It follows `rank`, except while a pointer is over the bar: no button
	 * is moved out from under a finger (vividn asked for calm). It catches
	 * up the moment the pointer leaves, which is what this statement
	 * re-running on `over` is for.
	 */
	let shown: Map<string, Near> | null = $state.raw(null);
	$effect.pre(() => {
		if (!over) shown = rank;
	});

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
	type Entry = { kind: 'hint' } | { kind: 'field'; button: FieldButton; promoted: boolean; edge: boolean };
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
	 * The fields at rest, last used first, a press's fields ahead of them
	 * (`rowsUnder`): the order they take their slots in collect's rows
	 * (`atRest`, da-lw7ue), or in a tutorial game's ring.
	 */
	const under = $derived(buttons.length === 0 || picture ? [] : rowsUnder(buttons, 1, standings, shown));
	const clustered = $derived(under.flatMap(({ lead, rest }) => [...lead, ...rest]));
	/** Whether the fields stand in collect's rows at rest (`atRest`), and not a tutorial game's cluster. */
	const resting = $derived(!tapLogs && clustered.length > 0);
	/** The fields a press on a picture put first, marked so (da-qu16). */
	const promoted = $derived(new Set(under.flatMap(({ lead }) => lead.map((b) => b.field))));
	/** The fields in the order they claim their places: most recently used at the last sort first. */
	const placing = $derived(
		[...buttons]
			.sort((a, b) => (standings.get(b.field)?.last ?? -1) - (standings.get(a.field)?.last ?? -1))
			.map((button): Entry => ({ kind: 'field', button, promoted: false, edge: false }))
	);
	/** The bar's height, and each placed slot's size as drawn, in px. */
	let tall = $state(0);
	let sizes = $state.raw(new Map<string, { w: number; h: number }>());
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

	/** The strip's width at rest: about a fifth of the widget (vividn: «maybe 20%»). */
	const strip = $derived(Math.round(paneW / 5));
	/** Where each field stands at rest, and the active section's outline. */
	const shelves = $derived.by(() => {
		const hs = [...sizes.values()].map((s) => s.h);
		return atRest(
			clustered.map((b) => ({ ...b, w: sizes.get(`f:${b.field}`)?.w ?? 0 })),
			{ height: paneH, rowH: hs.length ? Math.max(...hs) : 30, gap: GAP, strip }
		);
	});
	/** The strip and the active section stay while the section is under half the widget; past it the section pans too. */
	const frozen = $derived(shelves.section <= paneW / 2);
	/** Each active field's band on the strip, a tap's target over its picture. */
	const nowBands = $derived(fields.filter((b) => b.active && !b.ignored));
	// The other side pans the other way: a pan kept from the last starts again at the strip.
	$effect.pre(() => {
		void side;
		untrack(() => (pan = null));
	});
	/** Where a field stands across the room at rest, `x` from the strip's edge: mirrored on the right. */
	const across = (x: number, field: string) => (side === 'right' ? paneW - x - (sizes.get(`f:${field}`)?.w ?? 0) : x);

	/** Keep `sizes` to the slot's size as drawn. */
	function sized(node: HTMLElement, key: string) {
		const observer = new ResizeObserver(() => {
			const { width: w, height: h } = node.getBoundingClientRect();
			const was = sizes.get(key);
			if (was?.w !== w || was?.h !== h) sizes = new Map(sizes).set(key, { w, h });
		});
		observer.observe(node);
		// A key let go of takes its size with it, so a stale one is never read.
		const drop = () => sizes.delete(key) && (sizes = new Map(sizes));
		return {
			update: (next: string) => void (next !== key && (drop(), (key = next))),
			destroy: () => (observer.disconnect(), drop())
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
			if (!picture) {
				if (!e.ctrlKey || !resting || picked !== null) return;
				e.preventDefault();
				return onzoom?.((scale = bounded(scale * Math.exp(-e.deltaY / 200))));
			}
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

	/**
	 * A press on a picture, while zoomed: the view pans to the height of the
	 * field it landed nearest (`closer` in #lib/lanes.ts).
	 */
	$effect.pre(() => {
		const near = rank;
		untrack(() => towards(near));
	});
	function towards(near: Map<string, Near> | null) {
		if (!near || !picture || !zoomedIn) return;
		const first = [...near].sort(([, a], [, b]) => closer(a, b))[0];
		if (!first) return;
		const button = buttons.find((b) => b.field === first[0]);
		if (button) view = panned(view, yIn(view, button.y) - 0.5);
	}

	/** A look as the custom properties every button below is painted from. */
	function vars(look: Look) {
		return `--fill: ${look.fill ?? 'transparent'}; --line: ${look.line}; --text: ${look.text}`;
	}

	function statesOf(shape: FieldKind): string[] {
		return shape.kind === 'change' ? shape.states : [];
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
	 *
	 * A tutorial game's fields (`tapLogs`) keep the TOGGLE (da-05oit): the
	 * field's pill on the bar, the page narrowed to it, and held again the
	 * pill goes, every other pill and the typed line staying.
	 */
	function hold(button: FieldButton) {
		if (!tapLogs) return press(button, heldKind(button));
		setFieldPill($fieldPill === button.field ? null : button.field, button.color);
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
		const held = tapLogs ? 'filter the page to it' : does(heldKind(button));
		return [...(record ? [] : [button.field]), ...(button.ignored ? ['ignored'] : []), press, `hold to ${held}`].join(' · ');
	}

	// ---- the PILLS a field's view makes (da-w6437) ----------------------
	//
	// Its time is the page's selection (#lib/cursor.ts), so `−q` and `5m` are
	// the very `-qq` and length pills the nowview makes and draws, and the
	// cursor's reconciler is what a × on the bar goes through. A state and
	// `-m` are pills of their own.

	const STATE_ID = 'state';
	const MOMENT_ID = 'moment';
	/** Each `−q` and each `5m` a tap adds, in minutes. */
	const STEP = 5;

	/** The length a collect drag left, which a tap adds no five to; a length set anywhere else adds them again. */
	let draggedTo = $state<number | null>(null);
	/** The state pill's state, and whether `-m` is on the bar. */
	const chosen = $derived($pills.find((p) => p.id === STATE_ID)?.text ?? null);
	const moment = $derived($pills.some((p) => p.id === MOMENT_ID));
	/** The time pill and the length the bar holds. */
	const timePill = $derived($pills.find((p) => p.kind === 'time') ?? null);
	const span = $derived($pills.some((p) => p.id === SPAN_ID) ? ($cursor?.span ?? null) : null);
	/** A tap adds a `q` while the time is so many fives before now, and five minutes while the length is fives: a drag leaves neither. */
	const addsQ = $derived(!!timePill && $cursor?.rel !== undefined && $cursor.rel > 0 && $cursor.rel % STEP === 0);
	const addsM = $derived(span !== null && span % STEP === 0 && span !== draggedTo);

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
		if (picked !== null && (picking === null || (!tapLogs && $fieldPill !== picked))) untrack(close);
	});

	// ---- an ENTRY selected (da-xdz73) ------------------------------------
	//
	// vividn, 2026-11-03: «if you select an hid pill, it could show
	// information about this individual datum … no need to show by metadata
	// here, just the data / put a -A button which toggles showing all the
	// data». The bar's `hid` pill, its last entry, while no field is open
	// and it was not selected on a picture, whose press ranks the fields
	// instead (da-qu16) until vividn rules between the two. A field pill on
	// the bar keeps the fields too, its hold being theirs (da-05oit):
	// its field and time, then its data; `−A` adds every other column, as
	// `get -A` prints them. Its actions are the command pane's (da-fg4w4).

	/** The entry shown: the hid pill's last, read again on every write. */
	const hid = $derived($pictured ? null : ($selected.at(-1) ?? null));
	let entry: Record<string, unknown> | null = $state.raw(null);
	/** `−A`: every column, metadata included. */
	let everything = $state(false);
	$effect(() => {
		if (!hid) return void ((entry = null), (everything = false));
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
				const got = await run(entryArgv(hid));
				let next = null;
				try {
					next = got.code === 0 ? JSON.parse(got.stdout) : null;
				} catch {
					// Not a document: no card.
				}
				if (live) entry = next;
			} while (again && live);
			reading = false;
		};
		void read();
		const unhear = onAnyChange(() => void read(), currentDb());
		return () => ((live = false), unhear());
	});
	/** Whether the entry's card is up, in place of the fields. */
	const card = $derived(!!entry && !!hid && !$fieldPill);
	/** The entry's own clock, `13:23`, at the offset it was logged at. */
	function clockOf(e: Record<string, unknown>): string {
		const at = e.at as { utc: string; offset_minutes: number } | null;
		return at ? new Date(Date.parse(at.utc) + at.offset_minutes * 60_000).toISOString().slice(11, 16) : '';
	}
	/** A value as it reads: a string bare, anything else its JSON. */
	const said = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v));
	/** The rows shown: the data's keys, and with `−A` every other column too. */
	const rows = $derived.by((): [string, unknown][] => {
		if (!entry) return [];
		const data = Object.entries((entry.data as Record<string, unknown> | null) ?? {});
		return everything ? [...data, ...Object.entries(entry).filter(([k]) => k !== 'data')] : data;
	});
	/** Back from an entry: its pill goes. */
	function unselect() {
		pills.update((all) => all.filter((p) => p.kind !== 'hid'));
	}

	/** Where a selection is, without its length. */
	const where = (c: Cursor) => (c.rel !== undefined ? { rel: c.rel, from: c.from } : { at: c.at, step: 1, from: c.from });

	/** `−q`: five minutes earlier. */
	function earlier() {
		const c = get(cursor);
		pointAt({ ...(c ? where(c) : {}), rel: (c?.rel ?? 0) + STEP, span: c?.span });
	}
	/** The time pill's ×: the length, if any, ends at now again. */
	function clearTime() {
		const c = get(cursor);
		pointAt(c?.span ? { rel: 0, span: c.span } : null);
	}
	/** `5m`: five minutes more, in place of `-m`. */
	function longer() {
		draggedTo = null;
		setMoment(false);
		const c = get(cursor);
		pointAt({ ...(c ? where(c) : { rel: 0 }), span: (c?.span ?? 0) + STEP });
	}
	/** The length's ×. */
	function clearLength() {
		draggedTo = null;
		const c = get(cursor);
		if (c) pointAt(where(c));
	}
	// ---- a pill DRAGGED (da-axrdv) ------------------------------------------
	//
	// vividn: «you can grab and hold them and nudge them or drag them to the
	// left and right to make quick adjustments to the relative time and
	// duration. For precise seeking you can still use the nowview.» Sideways,
	// a minute for every few pixels and faster the further it goes, along
	// the nowview's time: right is later, so the length, whose far edge is
	// to the left, grows to the left. A dragged pill goes plain and adds no
	// more fives: `−qq` is `-t 13:41`, the length `12m`.

	/** Px of drag for the first minute. */
	const JOG_PX = 6;
	/** How a click right after a drag is told from one: it is the drag's own. */
	const JOG_CLICK_MS = 300;
	let jog: { kind: string; x: number; was: Cursor; moved: boolean } | null = null;
	let jogged = -Infinity;

	function jogDown(kind: string, e: PointerEvent) {
		const was = kind === 'time' || kind === 'length' ? get(pointed) : null;
		if (!was || disabled) return;
		jog = { kind, x: e.clientX, was, moved: false };
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
	}
	function jogMove(e: PointerEvent) {
		if (!jog) return;
		const dx = e.clientX - jog.x;
		if (!jog.moved && Math.abs(dx) < CLICK_SLOP) return;
		jog.moved = true;
		const by = Math.sign(dx) * Math.round((Math.abs(dx) / JOG_PX) ** 1.4);
		const c = jog.was;
		if (jog.kind === 'time') return pointAt({ at: new Date(+c.at + by * 60_000), span: c.span, from: c.from });
		pointAt({ ...where(c), span: (draggedTo = Math.max(1, (c.span ?? 0) - by)) });
	}
	function jogUp() {
		if (jog?.moved) jogged = performance.now();
		jog = null;
	}

	/** `−m` is a toggle, and a moment has no length. */
	function toggleMoment() {
		if (!moment) clearLength();
		setMoment(!moment);
	}
	function setMoment(on: boolean) {
		pills.update((all) => [
			...all.filter((p) => p.id !== MOMENT_ID),
			...(on ? [{ id: MOMENT_ID, kind: 'moment', text: '-m' }] : [])
		]);
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
		if (tapLogs) {
			close();
			return emit(pressArgv(button, state));
		}
		if (logs && typesKeys('change', button)) {
			// Closed, not Back: the time pills made here stay for the line filled.
			setFieldPill(null);
			close();
			return onfill(pressArgv(button, state));
		}
		pills.update((all) => {
			const rest = all.filter((p) => p.id !== STATE_ID);
			const pill: Pill = { id: STATE_ID, kind: 'state', text: shellJoin([state]), color: button.stateColors[state] ?? button.color };
			const at = rest.findIndex((p) => p.kind === 'field') + 1;
			return [...rest.slice(0, at), pill, ...rest.slice(at)];
		});
		if (logs) onrun(['change']);
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
		if (tapLogs) return void ((restPan = pan), (pan = null), (picked = button.field));
		entered = { cursor: get(cursor), moment };
		if ($fieldPill !== button.field) setFieldPill(button.field, button.color);
		restPan = pan;
		pan = null;
		picked = button.field;
	}

	/** Close the field's view: the fields under it are as they were. */
	function close() {
		picked = null;
		pan = restPan;
		restPan = null;
		entered = null;
		if (chosen !== null) pills.update((all) => all.filter((p) => p.id !== STATE_ID));
	}

	/** Back: the field's pill and the ones made since it was tapped go, and a time pill already there stays. */
	function back() {
		const was = entered;
		if (!tapLogs) setFieldPill(null);
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
	 * Stand the cluster's buttons around the first (da-vg8vr): `ringIn` gives
	 * the spots from their measured sizes and the room's, and the box is
	 * cropped to them, the first's center the `anchor`. Measured again
	 * whenever the room or a button is resized or a button comes or goes, as
	 * a state learned while it is open does.
	 */
	function around(box: HTMLElement) {
		const room = box.parentElement!;
		// Unrounded: `offsetWidth` rounds down.
		const size = (el: HTMLElement) => el.getBoundingClientRect();
		const place = () => {
			const spots = [...box.children] as HTMLElement[];
			if (spots.length === 0) return void ((extent = { w: 0, h: 0 }), (anchor = { x: 0, y: 0 }));
			const h = Math.max(...spots.map((s) => size(s).height));
			const widths = spots.map((s) => size(s).width);
			const w = { end: widths[0], states: widths.slice(1) };
			const ring = ringIn(w, h, GAP, { w: room.clientWidth, h: room.clientHeight, side: 0, lid: 0 });
			const ys = [ring.end, ...ring.states].map((p) => p.y);
			// Taller than the room, it is dealt into the room's rows instead, which run past its sides.
			const deal = Math.max(...ys) - Math.min(...ys) + h > room.clientHeight;
			const { end, states } = deal ? dealt(w, h, GAP, room.clientHeight) : ring;
			// The first is the middle of the room, whichever way they were stood:
			// in a short box, half a row of `ringIn`'s rows past it, which pans.
			// A tutorial game's fields are all in view instead, as before collect
			// (`tapLogs`, da-uzyih.4940): `ringIn` stands the rows about the
			// room's middle, the origin, so taking that as the middle, not the
			// first button, keeps every row in the room with nothing to pan.
			const mid = tapLogs && !deal ? { x: 0, y: 0 } : end;
			const at = [end, ...states].map((p) => ({ x: p.x - mid.x, y: p.y - mid.y }));
			const edges = spots.map((el, i) => ({ el, left: at[i].x - widths[i] / 2, top: at[i].y - h / 2 }));
			const [left, top] = [Math.min(...edges.map((e) => e.left)), Math.min(...edges.map((e) => e.top))];
			for (const e of edges) {
				e.el.style.left = `${e.left - left}px`;
				e.el.style.top = `${e.top - top}px`;
			}
			extent = {
				w: Math.max(...edges.map((e, i) => e.left + widths[i])) - left,
				h: Math.max(...edges.map((e) => e.top + h)) - top
			};
			// A tutorial game's dealt rows, centered along an axis they fit and
			// from the room's edge along one they overrun: a reveal scrolls the
			// room only right and down (`fold`), so a state dealt past its left
			// was out of reach (da-uzyih.4943, «garden» on a 280 px phone).
			const [W, H] = [room.clientWidth, room.clientHeight];
			anchor = tapLogs && deal ? { x: Math.min(extent.w, W) / 2, y: Math.min(extent.h, H) / 2 } : { x: -left, y: -top };
		};
		const observer = new ResizeObserver(place);
		observer.observe(room);
		const watch = () => {
			for (const spot of box.children) observer.observe(spot);
			// Placed now as well, not a frame later: the buttons are in the
			// DOM when an action runs, and measuring them lays them out.
			place();
		};
		const children = new MutationObserver(watch);
		children.observe(box, { childList: true });
		watch();
		return { destroy: () => (observer.disconnect(), children.disconnect()) };
	}

	/**
	 * `pan` kept to what the cluster needs (da-vg8vr) — vividn: «allow
	 * panning in 2 dimensions if states run off the sides»: along an axis it
	 * fits, none; along one it overruns, as far as brings its far edge to
	 * the room's and no further, like a scroll.
	 */
	function kept(x: number, y: number) {
		// At rest the rows pan sideways alone, as far as brings the furthest field into the room.
		if (resting && picked === null) {
			const most = Math.max(0, shelves.end + GAP - paneW);
			return { x: side === 'right' ? Math.min(most, Math.max(x, 0)) : Math.min(0, Math.max(x, -most)), y: 0 };
		}
		const axis = (at: number, to: number, start: number, size: number) =>
			Math.min(Math.max(at, Math.min(0, to - start - size)), Math.max(0, -start));
		return {
			x: axis(x, paneW, paneW / 2 - anchor.x, extent.w),
			y: axis(y, paneH, paneH / 2 - anchor.y, extent.h)
		};
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
	/** The rows' zoom as drawn: `zoom`, inside the bounds whatever was stored, or a pinch's while it moves. */
	let scale = $derived(bounded(zoom));
	/** The fingers down on the room, and a pinch of two of them at rest: how far apart they began, and the zoom then. */
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
		const pilled = tapLogs || picked === $fieldPill ? null : fields.find((b) => b.field === $fieldPill);
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
		// Touch alone: a mouse or a pen is captured by nothing, so one let go of
		// outside the room never reaches `up` and would stay in the map, a finger
		// for the next touch to pinch against. A mouse zooms by ctrl+wheel.
		if (e.pointerType === 'touch') fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		// A second finger at rest is a pinch: the pan stops. `untapped` calls off a hold and every tap.
		if (fingers.size === 2 && resting && picked === null) [pinch, drag] = [{ d: apart(), from: scale }, null];
		if (e.button === 0 && !pinch)
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
		if (fingers.has(e.pointerId)) fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
		if (pinch && fingers.size === 2 && pinch.d > 0) scale = bounded((pinch.from * apart()) / pinch.d);
		if (drag?.id !== e.pointerId) return;
		const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
		drag.xs = tracked(drag.xs, e.clientX, e.timeStamp);
		drag.ys = tracked(drag.ys, e.clientY, e.timeStamp);
		if (!drag.moving && Math.hypot(dx, dy) <= CLICK_SLOP) return;
		if (!drag.moving) (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		drag.moving = true;
		pan = kept(drag.from.x + dx, drag.from.y + dy);
	}
	function up(e: PointerEvent) {
		fingers.delete(e.pointerId);
		if (pinch && fingers.size < 2) {
			pinch = null;
			onzoom?.(scale);
		}
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
	/**
	 * A reveal — a focus, a scroll into view — scrolls the room itself (da-71p3u):
	 * taken as the pan that shows the same, so the room stays unscrolled and a
	 * drag starts from where the cluster is drawn.
	 */
	function fold(e: Event) {
		const room = e.currentTarget as HTMLElement;
		if (!(room.scrollLeft || room.scrollTop)) return;
		const [x, y] = [shift.x - room.scrollLeft, shift.y - room.scrollTop];
		flushSync(() => (pan = kept(x, y)));
		room.scrollLeft = room.scrollTop = 0;
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
		if (el.dataset.state !== undefined && picking) return choose(picking, el.dataset.state, clicks !== 'one');
		const button = buttons.find((b) => b.field === el.dataset.field);
		if (button) hold(button);
	};
</script>

<svelte:window onkeydown={onWindowKeyDown} />

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

<!-- A pill of the view's own, two parts: the left one presses `more` (none: it only says), the × clears it.
     A time's or a length's jogs sideways, so a drag across it is never the pane's tab swipe (`data-pans`, da-ut8e5). -->
{#snippet twoPart(kind: string, label: string, more: (() => void) | null, clear: () => void)}
	<span class="part-pill" data-pill={kind}>
		<!-- Not `disabled` with nothing to add: a drag goes on holding it. -->
		<button
			class="more"
			data-more
			data-pans={kind === 'moment' ? undefined : 'x'}
			{disabled}
			aria-disabled={!more || undefined}
			onpointerdown={(e) => jogDown(kind, e)}
			onpointermove={jogMove}
			onpointerup={jogUp}
			onpointercancel={jogUp}
			onclick={() => performance.now() - jogged > JOG_CLICK_MS && more?.()}
			>{#if kind === 'length'}<LengthArrow />{/if}{label}</button
		><button
			class="clear"
			{disabled}
			aria-label="clear {label}"
			onclick={clear}>×</button
		>
	</span>
{/snippet}

<div class="fields" class:picture data-align={picture ? 'picture' : 'rows'}>
<ButtonRows
	items={entries}
	{arrange}
	maxRows={Infinity}
	center={!picture && buttons.length > 0}
	key={keyOf}
	class={picking || card ? 'picking' : clustered.length ? 'clustered' : ''}
	bind:bar
	onpointerenter={() => (over = true)}
	onpointerleave={() => (over = false)}
>
	<!-- Given a line colour, or `var(--line)` voids its border and the rows
	     are counted two pixels short of the buttons they hold. -->
	{#snippet probe()}
		<button class="widget" style="--line: transparent" tabindex="-1">probe</button>
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
	{#if card && entry && hid}
		{@const button = fields.find((b) => b.field === entry!.field)}
		{@const state = entry.state}
		<!-- An entry selected (da-xdz73): its field and time, then its data. -->
		<div class="collect open entry" data-entry={hid}>
			<div class="head">
				<button class="nav" use:act={'show'} data-back {disabled} title="back to the fields" aria-label="back" onclick={unselect}>
					<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg> back
				</button>
				<span class="ref">{labelOf(hid, $refs)}</span>
			</div>
			<div class="card">
				<div class="title">
					<span class="name" style:color={button?.color}>{entry.field}</span>
					{#if state !== null && state !== undefined}<span style:color={button?.stateColors[said(state)]}>{said(state)}</span>{/if}
					<span class="clock">{clockOf(entry)}</span>
					<button class="nav round all" class:on={everything} data-all aria-pressed={everything} title="show all of it, as get -A" onclick={() => (everything = !everything)}>−A</button>
				</div>
				<dl>
					<!-- Unkeyed: under −A a data key may share a column's name (an import's `data.at`). -->
					{#each rows as [key, value]}
						<div data-row={key}><dt>{key}</dt><dd>{said(value)}</dd></div>
					{/each}
				</dl>
			</div>
		</div>
	{:else if picking || clustered.length}
		{@const here = picking}
		{@const states = here ? statesOf(here.shape).filter((s) => s !== 'false') : []}
		<!-- The fields' place (da-w6437): at rest the fields in their rows
		     (da-lw7ue); a field tapped, its time at the top, its states in the
		     middle and its keys at the bottom, the fields hidden under it. -->
		<div class="collect" class:open={!!here}>
			{#if here}
				<div class="top">
				<div class="head">
					<button class="nav" use:act={'show'} data-back {disabled} title="back to the fields" aria-label="back" onclick={back}>
						<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg> back
					</button>
					<span class="name" style:color={here.color} data-open-field={here.field}>{here.field}</span>
					{#if chosen !== null}<span class="to">→ <span style:color={here.stateColors[chosen] ?? here.color}>{chosen}</span></span>{/if}
				</div>
				<div class="time" data-time-row>
					{#if timePill}
						{@render twoPart('time', timePill.text, addsQ ? earlier : null, clearTime)}
					{:else}
						<button class="nav round" data-q {disabled} title="five minutes earlier" onclick={earlier}>−q</button>
					{/if}
					<!-- `−m` and the length are one pill, never both (vividn: «they don't
					     make sense in conjunction»); the other's button stays to swap. A
					     moment field has no length to give: `−q` alone (vividn, 16:07). -->
					{#if here.shape.kind === 'moment'}
						<!-- nothing more -->
					{:else if moment}
						{@render twoPart('moment', '−m', toggleMoment, toggleMoment)}
					{:else if span !== null}
						{@render twoPart('length', `${span}m`, addsM ? longer : null, clearLength)}
					{/if}
					{#if here.shape.kind !== 'moment' && !moment}
						<button class="nav round" data-m {disabled} title="a moment, no length" onclick={toggleMoment}>−m</button>
					{/if}
					{#if here.shape.kind !== 'moment' && span === null}
						<button class="nav round" data-five {disabled} title="five minutes long" onclick={longer}>5m</button>
					{/if}
				</div>
				</div>
			{/if}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="room"
				bind:clientWidth={paneW}
				bind:clientHeight={paneH}
				data-pans={pans || undefined}
				style:--zoom={scale}
				onpointerdown={down}
				onpointermove={move}
				onpointerup={up}
				onpointercancel={up}
				onscroll={fold}
				onclickcapture={swallow}
			>
				{#if !here && resting}
					<!-- At rest (da-lw7ue): the strip, the active section, then each row by recency. -->
					<svg class="backing" aria-hidden="true">
						<path d={frozen ? shelves.backing : `M0 0H${strip}V${paneH}H0Z`} transform={side === 'right' ? `matrix(-1 0 0 1 ${paneW} 0)` : undefined} />
					</svg>
					<div class="present" class:right={side === 'right'} style:width="{strip}px" data-strip>
						<NowStrip doc={nowDoc} pending={nowPending} at={nowAt} width={strip} height={paneH} mirrored={side === 'right'} />
						{#each nowBands as band (band.field)}
							<button
								class="now-state"
								data-now-field={band.field}
								tabindex="-1"
								style:top="{band.y * paneH}px"
								style:height="{Math.max(3, band.height * paneH)}px"
								{disabled}
								title="{band.field}{typeof band.current === 'string' ? ` · ${band.current}` : ''} · tap to select the field"
								aria-label="select {band.field}"
								onclick={() => setFieldPill(band.field, band.color)}
							></button>
						{/each}
					</div>
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
							data-role="field"
							data-row={shelves.rows.get(button.field)}
							data-section={button.active ? 'active' : 'recent'}
							data-promoted={promoted.has(button.field) ? '' : undefined}
							style:left="{across(stand?.x ?? 0, button.field)}px"
							style:top="{stand?.top ?? 0}px"
							style:transform={pinned ? null : `translateX(${shift.x}px)`}
							use:sized={`f:${button.field}`}
						>
							{@render fieldButton(button)}
						</div>
					{/each}
				{:else}
				<div
					class="ring"
					use:around
					style:left="{paneW / 2 - anchor.x + shift.x}px"
					style:top="{paneH / 2 - anchor.y + shift.y}px"
					style:width="{extent.w}px"
					style:height="{extent.h}px"
				>
					{#if here}
						{#each states as state (state)}
							{@const look = stateLook(here, state)}
							<div class="spot" data-role="state">
								<!-- A state RUNS at once, drawn as acting (da-z8z40) — unless the field has keys to type, or a tap only chooses it (2-click, da-u017i). -->
								<button
									class="widget states"
									use:act={clicks === 'two' ? 'arm' : typesKeys('change', here) ? 'fill' : 'run'}
									class:on={here.current === state}
									class:chosen={chosen === shellJoin([state])}
									style={vars(look)}
									{disabled}
									data-state={state}
									title={shellJoin(['change', here.field, state])}
									onclick={() => choose(here, state)}
								>
									{state}
								</button>
							</div>
						{/each}
					{:else}
						{#each clustered as button (button.field)}
							<div class="spot" data-role="field" data-promoted={promoted.has(button.field) ? '' : undefined}>{@render fieldButton(button)}</div>
						{/each}
					{/if}
				</div>
				{/if}
			</div>
			{#if here}
				<div class="data" data-data-row>
					{#each here.keys as key (key.name)}
						<button class="key" class:required={key.required} data-key={key.name} {disabled} onclick={() => putKey(key.name)}>{key.name}=</button>
					{/each}
					<button class="nav more-data" data-more-data {disabled} onclick={() => focusLine(true)}>+ data</button>
				</div>
			{/if}
		</div>
	{/if}
	{/snippet}
</ButtonRows>
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
	   scroll is still the browser's. */
	.picture :global(.bar) {
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

	/* The back icon and the `+`: the two ends of the states, and neither of
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

	/* The fields' place (da-w6437): a field's view top to bottom — back and
	   its name, its time, its states in the room, its keys — or at rest the
	   room alone. */
	.collect {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		container: collect / size;
	}

	.top {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	/* A pane too short for the whole view scrolls it, the room holding a
	   few rows of states whatever the pane (the desktop's is one row). */
	.collect.open {
		padding: 0.25rem;
		overflow: hidden auto;
	}

	.collect.open .room {
		flex: 1 0 auto;
		min-height: 7.5rem;
	}

	/* A pane a few rows tall, a phone's main tab: back, the field and its
	   time share one row, a little smaller, so a row of states stands under
	   them in the pane and not below it. */
	@container collect (max-height: 140px) {
		.top {
			flex-direction: row;
			align-items: center;
			flex-wrap: wrap;
		}

		.top .time {
			justify-content: flex-start;
		}

		.nav.round {
			min-width: 2rem;
			min-height: 2rem;
		}

		.collect.open .room {
			min-height: 2rem;
		}
	}

	.head,
	.time,
	.data {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.time {
		justify-content: center;
	}

	.data {
		justify-content: center;
		padding-top: 0.4rem;
		border-top: 1px dashed var(--rule);
	}

	.head .name,
	.card .name {
		font-weight: 600;
	}

	/* An entry selected (da-xdz73): a card in the middle of the widget. */
	.entry .card {
		margin: auto;
		max-width: 100%;
		padding: 0.6rem 0.8rem;
		border: 1px solid var(--rule);
		border-radius: 6px;
		font-size: 0.85rem;
	}

	.card .title {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}

	.card .name {
		text-decoration: underline;
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

	.head .to {
		color: var(--muted);
	}

	/* The cluster's room: placed by `around` and panned by a drag. */
	.room {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		/* A finger's drag pans the cluster, never the page. */
		touch-action: none;
	}

	.ring {
		position: absolute;
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

	/* The mirror (`side`): the strip and the section at the right edge. */
	.present.right {
		left: auto;
		right: 0;
	}

	.present.right {
		border-right: none;
		border-left: 1px solid var(--rule);
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
		transition: z-index 0s 0.3s;
	}

	/* The rows' zoom (da-l1z6h): the button and all in it, so its slot,
	   which `atRest` counts the rows from, is measured at the zoom. */
	.spot.shelf > button {
		zoom: var(--zoom);
	}

	.spot.shelf.glide {
		transition:
			left 0.3s ease-out,
			top 0.3s ease-out,
			z-index 0s 0.3s;
	}

	.spot.shelf.pinned {
		z-index: 2;
		transition: z-index 0s;
	}

	.spot.shelf.pinned.glide {
		transition:
			left 0.3s ease-out,
			top 0.3s ease-out,
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

	.nav[data-back] {
		border-radius: 999px;
		padding: 0.3rem 0.6rem;
	}

	.nav svg {
		width: 1em;
		height: 1em;
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
	}

	/* `−q`, `−m`, `5m`: round, before a tap makes them pills. */
	.nav.round {
		justify-content: center;
		min-width: 2.4rem;
		min-height: 2.4rem;
		border-radius: 999px;
		padding: 0;
	}

	/* A pill of the view's own, two parts (vividn, 14:25: «-q|x so that you
	   can either add more qs or clear the pill»). */
	.part-pill {
		display: inline-flex;
		border: 1.5px solid var(--accent);
		border-radius: 999px;
		overflow: hidden;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}

	.part-pill button {
		display: inline-flex;
		align-items: center;
		gap: 0.15em;
		background: none;
		border: none;
		color: var(--ink-bright);
		font: inherit;
		font-size: 0.95rem;
		padding: 0.35rem 0.7rem;
		cursor: pointer;
	}

	.part-pill .more:disabled,
	.part-pill .more[aria-disabled] {
		cursor: default;
	}

	/* A time or a length is held and dragged sideways. */
	.part-pill:is([data-pill='time'], [data-pill='length']) .more {
		touch-action: none;
	}

	.part-pill .clear {
		border-left: 1.5px solid var(--accent);
		padding: 0.35rem 0.55rem;
	}

	.key {
		background: none;
		border: 1px dashed var(--rule);
		border-radius: 999px;
		padding: 0.25rem 0.6rem;
		color: var(--ink);
		font: inherit;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.key.required {
		border-style: solid;
	}

	.more-data {
		border-radius: 999px;
	}

	/* The state whose run is pending or failed: ringed. */
	.states.chosen {
		box-shadow: 0 0 0 2px var(--ink-bright);
	}

	/* The state the field is in already: a switch to it is the CLI's yellow
	   ! (vividn, 16:07). */
	.states[data-state] {
		position: relative;
	}

	/* Over the chip's corner, so marking a state moves none of them. */
	.states[data-state].on::after {
		content: '!';
		position: absolute;
		top: 0;
		right: 0.25rem;
		color: var(--warn);
		font-weight: 700;
	}

	/* A flex box, so it is exactly its button's size and nothing of a line box. */
	.spot {
		position: absolute;
		display: flex;
		white-space: nowrap;
	}

	/* The state the field is in now: solid, as the field's own chip draws
	   it (`stateLook`), and bold — the rest are a tint. */
	.states.on,
	.widget.change.on {
		font-weight: 700;
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
