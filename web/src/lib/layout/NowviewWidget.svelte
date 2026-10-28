<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a HANDLE over each major label of its time axis (da-57vj4), which
	replaced the − / + beside the far one (da-ndzl). Hover a handle, or
	press a finger on it, and a wheel of the ladder's steps opens around it
	(down to 30s, halving past 5m through the 2m and 1m a game needs, up to
	12h): a click or a swipe across the axis gives the handle that time in
	the same place, so the window rescales by their ratio, and a drag along
	the axis moves the handle's time under the finger, to a window of any
	length. Never to the snapshot of each field's state, which is
	`showHistory` turned off in the widget's menu (da-hklaz). The window
	is the widget's setting, handed out with `onchange` — and since da-vhwq
	that lands in THIS SESSION, not the tree: a step under a LOCKED layout
	used to move `history` in the tree and light the layout's dot, which is
	the defect that bead was written against. «Save options with layout» is
	what hands the window to the layout, and only when the reader chooses it.

	The snapshot draws no axis, so it has no handles: nothing lies over its
	chips, and the way out of it is the menu's «show history».

	THE PICTURE FLOWS (da-hklaz). vividn: «it should already have all future
	and past data loaded, so it's just a matter of shifting it over (could
	this even be done in realtime without second jitters)?» So on now the
	picture is drawn a window's worth AHEAD of now as well (`--ahead`), its
	lanes and its whole-minute marks in `.flow` groups clipped at the now
	line, and this slides those groups along with the clock (`flow`) — a
	device pixel at a time where a second moves them a unit or more, less
	often where it does not. datum is asked again on a write, as ever, and
	once half of what it drew ahead is spent; never per tick
	(`dependsOnNow`, #lib/panes.ts).
	Every instant read off the band below is read as the picture is drawn
	NOW, slid by `slid`.

	THE BAND IS A PLACE YOU POINT INTO (da-rskp). Its gestures live over a
	pane that swipes between tabs and a page that scrolls, so the state
	machine is written down in `#lib/nowpoint.ts` and this component only
	runs it. vividn's table, 2026-10-22 (da-xg1vi), the dayview's too:

	  tap on a mark           → INSPECT: the entry behind it, as an hid pill
	  tap it again            → the SELECTION there instead (vividn, 15:04)
	  tap on ground           → a SELECTION where it fell; the window does not move
	  touch the time axis     → SCRUB: the time under the finger, following it
	    and drag                to the lift — not snapped, and no pan
	  drag sideways           → PAN in time; the selection does not move,
	                            and never past now (da-v8gzq): a finger's
	                            flick coasts on after the lift
	  long press, then drag   → a RANGE
	  long press, then LIFT   → the same SELECTION, a mark or not
	  right click             → the same selection, on a desk
	  shift+drag (desktop)    → the same range; a long press with a mouse is
	                            nobody's habit (MY reading, not vividn's words)
	  long press in the NOW   → a duration ending NOW — the sentence «I have been
	    region, then drag left  doing this for the last N minutes»
	  two fingers / ctrl+wheel→ the window's LENGTH (da-5goc), about the fingers
	                            or the pointer (da-3zpdx), untouched here

	«Swiping left/right must not select time», so the duration ending now
	waits for the long press like every other range, and «for quickly
	selecting a time, a user can touch in the time axis, so that won't slow
	them down» — which is what let a plain tap on a mark mean the ENTRY,
	where until da-xg1vi it meant time and the inspect was a double tap.

	The long press does not COMMIT (da-pzzoz): the `.hold-mark` below — a ring
	closing over the wait, and a dashed stem through the band once it is up
	— is the sign a thumb gets that the press registered. The feedback is
	owed, not optional.

	An inspect, wherever it is made, floats a TOOLTIP over the mark: the
	field, the state and the two instants (vividn's «bare info»). Not the
	comment — a mark does not carry one, and see #lib/inspect.ts.

	What a tap MEANS depends on where the window is. On now the axis is
	RELATIVE, so the selection is too: the −5 / −10 / −15 / −20 gridlines
	spell themselves `-q` … `-qqqq` and everywhere else is the relative
	`-t -7`, and the white line stays on its gridline as the minutes pass
	(`#lib/cursor.ts`). Panned away the axis is absolute, and so is the
	pill: `-t HH:MM`, the one the dayview has always made (vividn, 17:01:
	«off-now, yes use absolute time»).

	A click on the DAYVIEW still centres this window on the instant, which
	is what it has always done; a click in a nowview does not, and says so
	by marking the selection `from: 'nowview'`. `data-points="own"` is the
	other half of that: it tells the `picture` action to leave the band's
	clicks alone, the same idiom as `data-pans`.

	The pills the selection makes are drawn TWICE — in the bottom bar, as
	every pill is, and here in the band's MARGIN beside the instant each one
	names (vividn: «let's show the pills in the margin of the nowview»). One
	store, two views, `Pill.svelte` for both. The margin is real room: a
	swipe that starts in it changes tabs, because `data-pans` is the BAND's
	and not the widget's.

	THE NOW SECTION is the CLI's strip of each field's current state, and it
	belongs to now (da-dv45l, vividn: «it is no longer pinned on the side,
	but rather it just disappears when panning around»): at the timeline's
	end on now, a fifth of the picture, and off now standing at now on the
	timeline, moving off with it, gone once now is out of the window. A time
	pill draws no section (vividn: «For selected times, don't display the
	outset block, just have the nowview as it would be with a dashed line and
	continuous in time around it»): the CLI is handed the pill as `-t` and
	the window's middle as `--center`, draws a dashed line at it and counts
	the ticks — and so the handles — from it, and the pill sits over the
	line. A tap here moves the line and not the window: one on now stops
	where it is for as long as the pill lasts. A handle's stretch scales about
	the pill's instant, or now, wherever the window is; a pinch, about the
	fingers (da-3zpdx).
	vividn: «any time the view is panned off the default, the "now" label
	under the now section becomes a filled in yellow button with black now
	written on it. Clicking that button restores the default.» So off now
	the yellow «now →» is a BUTTON laid over the «now» under the strip, and
	held to the edge now lies beyond once the strip has gone off it (an
	arrow says which way), which takes the window back AND the time-based
	pills away with it — and keeps the window's length: «stretching should
	always leave now where it is», and a stretch alone does not leave now.
	On now with a pill it reads «reset». `← selection` appears when a
	selection exists that the window does not contain (vividn's second
	«maybe»).

	The band takes sideways motion of its own, so it says so with
	`data-pans="x"` (da-kcmp): a one-finger drag across it is never a swipe
	between the pane's tabs, whatever the pane it is in.

	The handles, the buttons and the margin pills are the WIDGET's, not the
	picture's: a redraw replaces the picture, never them — it only re-places
	them against the new picture's geometry.

	The band carries its own `data-mode` — `press` between gestures, then
	`hold`, `pan`, `range` or `now` — so someone driving it by hand can see
	in devtools which gesture the machine took.
-->
<script lang="ts">
	import { onDestroy, onMount, tick, untrack } from 'svelte';
	import { on } from 'svelte/events';
	import CliPane from '#lib/CliPane.svelte';
	import Pill from '#lib/Pill.svelte';
	import { pills, selected } from '#lib/bar.ts';
	import { clockLabel } from '#lib/clock.ts';
	import { cursor, SPAN_ID, pointAt, pointed, timeArgAt, type Cursor } from '#lib/cursor.ts';
	import { inspectMark, instantOn, msPerPx, selectedTip, timeAxis } from '#lib/hover.ts';
	import { markOf, pressSpent } from '#lib/inspect.ts';
	import { readLanes } from '#lib/lanes.ts';
	import { advance, LONG_MS, SLOP_PX, snapTo, tapStep, undecided, type Mode, type Press } from '#lib/nowpoint.ts';
	import { bandOf, compose } from '#lib/nowcrop.ts';
	import { historyArg, ladderAround, layerArgv, NOWVIEW_HISTORY, nowviewSpec, stepMinutes, zoomHistory } from '#lib/panes.ts';
	import { run } from '#lib/run.ts';

	interface Props {
		/** How far back the window reaches: a step of NOWVIEW_STEPS, or any length a handle's drag left it at. */
		history?: string;
		/** The instant the window is centred on (`--center`); none is now. */
		at?: string | undefined;
		/**
		 * Time runs down the page (da-78evn). Every gesture below is measured
		 * ALONG the time axis, which the picture itself says (`axis.y`); this
		 * asks for the picture, and turns the chrome around it.
		 */
		vertical?: boolean;
		epoch?: number;
		/** Whether the window reaches back at all: off is the snapshot (da-hklaz). */
		showHistory?: boolean;
		/** The history a pinch or a handle moved the window to. */
		onchange?: (history: string) => void;
	}

	let {
		history = NOWVIEW_HISTORY,
		at = undefined,
		vertical = false,
		epoch = 0,
		showHistory = true,
		onchange
	}: Props = $props();

	// A second of slack either side: a range that ends AT now sits on the
	// window's own edge, and rounding must not read that as «off it». Up
	// here, above everything that reads it.
	const EDGE_MS = 1000;

	/**
	 * Where the window is centred, and WHY — `null` is on now.
	 *
	 * The why is the difference between the two ways a window leaves now.
	 * `sel` means it is following the page's selection: a click on the
	 * dayview put it there, and clearing that selection brings it home. `pan`
	 * means a hand moved it, and then nothing brings it home but «now» —
	 * the minute tick and a write re-draw it in place (da-rskp).
	 *
	 * It carries the `-t` VALUE the CLI is handed rather than an instant,
	 * because a hand-written tree's `at` may be any time flag the CLI reads
	 * — `09:00` — and not an ISO stamp. Where the window IS in time is read
	 * off the picture (`axis`), never off this.
	 *
	 * Seeded from that `at`, as a pan: nothing in the app writes the prop,
	 * and a tree that names an instant means it.
	 */
	let centre = $state.raw<{ arg: string; by: 'sel' | 'pan' } | null>(untrack(() => (at ? { arg: at, by: 'pan' } : null)));

	/** The window in minutes; a history datum would not read is the default's. */
	const minutes = $derived(stepMinutes(history) > 0 ? stepMinutes(history) : stepMinutes(NOWVIEW_HISTORY));
	/** `-t`'s spelling of `m` minutes before now, to the second: `-15`, `-22m30s`, `-20s`. */
	const backArg = (m: number) => {
		const s = Math.round(m * 60);
		const [whole, sec] = [Math.floor(s / 60), s % 60];
		return sec ? `-${whole ? `${whole}m` : ''}${sec}s` : `-${whole}`;
	};
	/** The window's middle as the CLI is handed it (`--center`); none is on now. */
	const middle = $derived(centre?.arg);
	/**
	 * The instant the now section is about (da-zccjs): the time pill's, when
	 * there is one; none is now. vividn: «when a time pill is selected, the
	 * "now" part of the nowview should show the state at that exact
	 * instant». A range is about its end; one ending now (`rel: 0`) is about
	 * now.
	 */
	const pill = $derived($pointed && $pointed.rel !== 0 ? $pointed : null);
	/**
	 * The pill as the CLI's `-t`: the instant the page draws it at, so its
	 * line and its section agree. A relative one's moves on with the minute
	 * (`pointed`), and so is asked for again with it.
	 */
	const pillArg = $derived(pill?.at.toISOString());
	/** How finely a tap here says its time, in seconds. */
	const grain = $derived(tapStep(minutes));

	/**
	 * The `--center` the picture on screen was asked for — the window's
	 * middle, never the pill's instant, which is `-t` (da-zccjs) — and the
	 * window's only until a pan moves it inside what that picture drew (da-61nm8):
	 * the picture is drawn a window's worth past its box either side, and
	 * a pan that stays in it slides the picture along (`rest`) and asks
	 * datum for nothing. Everything else that moves the window asks.
	 */
	let askedFor = $state<string | undefined>();
	// A window on now with a pill is centred half its length back from now,
	// relative too: it is not drawn ahead, there being an instant to draw.
	const spec = $derived(
		nowviewSpec(
			showHistory ? history : '0',
			askedFor ?? (pillArg ? backArg(minutes / 2) : undefined),
			vertical,
			pillArg
		)
	);

	// Above `ask`, which the cursor's subscription below calls as the
	// component is set up, and which reads the picture on screen.
	let el: HTMLDivElement | undefined;

	const svgEl = () => el?.querySelector<SVGSVGElement>('svg.svg') ?? null;

	/**
	 * The picture on screen when another window was asked for: it stays
	 * where it was slid until datum's answer replaces it, so a pan that asks
	 * on its release does not throw it back first (da-6csum).
	 */
	let stale: Element | null = null;

	function ask(): void {
		const was = askedFor;
		askedFor = centre?.arg;
		stale = askedFor !== was ? svgEl() : null;
		// The picture on screen may already be this one, slid: slid back.
		void tick().then(place);
	}

	// A new step or orientation is a new picture, of where the window is.
	// Below `ask` and what it reads: an effect made as the component is set
	// up runs where it stands.
	$effect.pre(() => {
		void [history, vertical, showHistory];
		untrack(ask);
	});

	/** The axis a drag along the band travels, for the pane's tab swipe. */
	const pans = $derived(vertical ? 'y' : 'x');
	/** Whether the window has left now — and so whether a tap makes an absolute pill. */
	const offNow = $derived(!!middle);
	/** Whether the button under the section shows: «now» off now, «reset» at a pill (da-zccjs). */
	const homeward = $derived(offNow || !!pill);

	// A selection made somewhere ELSE points this window; one made here does
	// not, and neither does a RELATIVE one — that is measured from now, and
	// the window it was dropped on is the one on now. A selection CLEARED
	// takes back only the window it was pointing.
	//
	// A subscription rather than an effect, because this ASSIGNS the window
	// and an effect that both reads and writes one value re-runs itself for
	// ever.
	//
	// And of `cursor`, NOT of `pointed` — plus a guard on the instant it
	// last acted on. `pointed` is derived over the minute clock, and a
	// Svelte store calls its subscribers for every value it is given, the
	// same object included, so this ran once a minute and re-centred a
	// window a hand had panned away: asked at 12:00, panned to 11:45, back
	// at 12:00 a minute later (vremya on PR #48, da-rskp). The header above
	// says only «now» brings a hand-moved window home, and now it is
	// true. The guard is for the other door into the same bug: the pills
	// reconciler re-points the cursor at the SAME instant when a `dur=`
	// pill is dropped, and that is not a new selection either.
	/**
	 * The instant `follow` last pointed the window at; `null` after a clear.
	 *
	 * Declared BEFORE the subscription below, which calls `follow` at once
	 * with the value the store holds — a `let` under that line is still in
	 * its dead zone when that first call reads it, and the whole page throws.
	 */
	let followed: number | null = null;

	onDestroy(cursor.subscribe(follow));

	function follow(c: Cursor | null): void {
		if (!c) {
			followed = null;
			centre = centre?.by === 'pan' ? centre : null;
			ask();
			return;
		}
		if (c.rel !== undefined || c.from === 'nowview') return;
		if (+c.at === followed) return;
		followed = +c.at;
		centre = { arg: c.at.toISOString(), by: 'sel' };
		ask();
	}

	/** Back to now, and the time-based pills — and so the selection — with it: the «now» button, and the «reset» one at a pill. */
	function toNow(): void {
		if (coasting) land();
		centre = null;
		ask();
		pointAt(null);
	}

	/**
	 * The window onto the selection the reader cannot see.
	 *
	 * `sel`, not `pan`: the window is there BECAUSE of the selection, so
	 * clearing the selection brings it home — the same bargain a click on
	 * the dayview strikes.
	 */
	function toSelection(): void {
		const p = $pointed;
		if (p) centre = { arg: p.at.toISOString(), by: 'sel' };
		ask();
	}

	// ── the picture's geometry ──────────────────────────────────────────

	/**
	 * The picture's time axis on screen, as the last redraw left it.
	 *
	 * Cached rather than read per event: a gesture asks for it on every
	 * move, and it changes only when the picture is replaced or the box
	 * resized — which is exactly when `place` runs.
	 */
	let axis: ReturnType<typeof timeAxis> = null;

	/** Along the axis, whichever way it runs. */
	const along = (x: number, y: number) => (axis?.y ? y : x);

	/**
	 * The window's two ends along the axis, on screen: the picture's own box,
	 * as its holder lays it out — the drawing past it is the surroundings,
	 * and a transform moves the drawing, never the holder.
	 */
	function ends(): [number, number] {
		const r = svgEl()?.parentElement?.getBoundingClientRect();
		if (!r) return [NaN, NaN];
		return axis?.y ? [r.top, r.bottom] : [r.left, r.right];
	}

	/** How far the picture is slid along its axis at rest, in px: see `askedFor`. */
	let rest = $state(0);

	/**
	 * The window's middle in TIME — the instant it is centred on, or half
	 * of it back from now — and the window about a middle. Never read off
	 * the band's box, which holds the now strip as well as the window: at
	 * the window's end, its start or at now inside it (da-6csum), so the
	 * box's middle is the window's only where the strip happens to balance.
	 * NaN for a hand-written `at` that is no instant the page can read.
	 */
	const middleOf = (at: string | undefined) =>
		at ? (timeArgAt(at)?.getTime() ?? Date.parse(at)) : nowMs() - minutes * 30_000;
	const windowAbout = (middle: number): [number, number] => [middle - minutes * 30_000, middle + minutes * 30_000];

	/**
	 * Every stretch of the picture's axis on screen, as `axis` is read: a
	 * strip standing inside the window cuts it in two at its instant
	 * (da-9g3bf, da-zccjs), the time after it drawn a strip further on.
	 */
	let stretches: NonNullable<ReturnType<typeof timeAxis>>[] = [];

	/** The stretch a point `pos` along the axis is over, or the nearest: a point on the strip is its instant. */
	function stretchAt(pos: number): typeof axis {
		const off = (a: NonNullable<ReturnType<typeof timeAxis>>) => Math.max(a.lo - pos, pos - a.hi);
		return stretches.length > 1 ? stretches.reduce((a, b) => (off(b) < off(a) ? b : a)) : axis;
	}

	/** The instant `pos` px along the axis is, by the picture's own geometry, as it is slid now. */
	function instantAlong(pos: number): number {
		const a = stretchAt(pos);
		return a ? instantOn(a, pos) + slid : NaN;
	}

	/** The instant a point on screen is, by the picture's own geometry, as it is slid now. */
	const instantAt = (x: number, y: number) => instantAlong(along(x, y));

	/**
	 * The window's own idea of NOW: while it is on now, the now-line's
	 * instant, as far on as the picture has flowed; the clock's in a picture
	 * that does not flow, the one of a pill's section (da-zccjs).
	 */
	const nowMs = () => (offNow || !drawn ? Date.now() : drawn.now + slid);

	/**
	 * The instants the whole picture on screen is drawn between: past the
	 * window either side (`--surround`), and across a now strip that cuts it
	 * into two stretches (da-9g3bf), which `axis`, one stretch, stops at.
	 */
	let spanned: [number, number] = [NaN, NaN];

	/** Where the picture on screen starts drawing. */
	const drawnFrom = () => (Number.isFinite(spanned[0]) ? spanned[0] : (axis?.from ?? NaN));
	/** The instant the picture on screen stops drawing what is known: its now, drawn ahead. */
	const drawnTo = () => drawn?.now ?? (Number.isFinite(spanned[1]) ? spanned[1] : (axis?.to ?? NaN));

	// ── the data (da-0e06p) ────────────────────────────────────────────

	/**
	 * The days the picture crops, by days back from today, as the CLI drew
	 * their data layer for this box: the days the overlay on screen reaches,
	 * asked again after a write. `fresh` are the days drawn for `key`; one
	 * drawn for another box or epoch still stands in, mapped by its
	 * instants, until the new one lands.
	 */
	let layer = $state.raw<{ key: string; fresh: Set<number>; days: Map<number, string> }>({
		key: '',
		fresh: new Set(),
		days: new Map()
	});
	/** The days asked for and not yet answered, by what they were asked as, and the newest `key` asked. */
	const layering = new Set<string>();
	let wanted = '';
	/** This widget's in the worker's queue, for its layer runs. */
	const layerKey = `nowview-layer:${Math.random().toString(36).slice(2)}`;
	/** How many days back a pan takes the data: a week. */
	const MAX_BACK = 7;

	/** Ask for the days of data the overlay `svg` reaches that `layer` does not hold. */
	function needLayer(svg: SVGSVGElement): void {
		if (!svg.querySelector('[data-lo]')) return;
		const along = Number(svg.getAttribute(vertical ? 'height' : 'width'));
		const cross = bandOf(svg, vertical).extent;
		if (!(along > 0 && cross > 0)) return;
		const midnight = new Date().setHours(0, 0, 0, 0);
		// Rounded up: a drawing that starts even a millisecond before a
		// midnight reaches the day before it, so it is asked for too.
		const reach = Math.max(0, Math.min(MAX_BACK, Math.ceil((midnight - drawnFrom()) / 86_400_000) || 0));
		// Not the box's length along time: a day drawn at another is mapped
		// by its instants all the same.
		const key = `${cross} ${vertical} ${epoch} ${midnight}`;
		wanted = key;
		for (let back = 0; back <= reach; back++) {
			const asked = `${key} ${back}`;
			if ((key === layer.key && layer.fresh.has(back)) || layering.has(asked)) continue;
			layering.add(asked);
			// Keyed by the widget and the day, so a newer ask drops an older one not yet started.
			void run(layerArgv(along, cross, vertical, back), undefined, undefined, `${layerKey}:${back}`, undefined, undefined, true).then((r) => {
				layering.delete(asked);
				if (r.superseded || key !== wanted || r.code !== 0 || !r.stdout.trim()) return;
				const fresh = new Set(key === layer.key ? layer.fresh : []);
				layer = { key, fresh: fresh.add(back), days: new Map(layer.days).set(back, r.stdout) };
			});
		}
	}

	// A write asks for the data again, whether or not the overlay it also
	// asks for comes back any different.
	$effect(() => {
		void epoch;
		untrack(() => {
			const svg = svgEl();
			if (svg) needLayer(svg);
		});
	});

	/** The argv of the picture last laid out, sized: a picture of `spec` is the one asked for. */
	let drewFor: string[] = [];
	const current = () => spec.argv.every((word, i) => drewFor[i] === word);

	/** The overlay datum drew, with the days' data laid under it: its strip is now's (da-dv45l). */
	const crop = (overlay: string, argv: string[]) => {
		drewFor = argv;
		return compose(overlay, [...layer.days.values()], Date.now());
	};

	/**
	 * The gesture in hand: what the band is scaled by, about which x, and the
	 * scale the fingers or the wheel made, which `scale` shows as the window
	 * it would ask for. `held`, the window it asked for, once it is let go:
	 * the picture stays scaled, exactly as it was under the fingers, until
	 * datum's picture of that window replaces it (da-ygpto).
	 *
	 * A pinch or a ctrl+wheel scales about the fingers or the pointer
	 * (da-3zpdx, vividn: «if my fingers are on the time of two different
	 * occur dots moving the two fingers should track with the occur dots
	 * regardless if panning or zooming»): `at` is the instant under them
	 * where they landed, and `shift` how far the band is slid since, so
	 * the instant stays under their middle and, by the scale, each finger's
	 * under it. A handle's drag has neither: it scales about the anchor.
	 */
	let zoom = $state.raw<{ origin: number; scale: number; raw: number; held?: string; at?: number; shift?: number } | null>(null);

	// ── the flow ───────────────────────────────────────────────────────

	/**
	 * The picture on screen, when it is drawn ahead to be slid: the instant
	 * its now-line is, how many ms one of its user units is worth, how many
	 * CSS px a unit is on screen, and how far ahead of its now it draws.
	 * Read off its own attributes — the day's axis (`data-from` … `data-hi`)
	 * and the now-line's `data-now` — and its screen scale.
	 */
	let drawn: { now: number; msPerUnit: number; pxPerUnit: number; ahead: number } | null = null;
	/** How far the `.flow` groups are slid, in ms of the picture's time: 0 whenever `drawn` is null. */
	let slid = 0;
	/** The picture `refills` last asked to be replaced, by its now. */
	let refilled = NaN;
	/** Bumped to ask datum for the picture again: what it drew ahead is half spent. */
	let refills = $state(0);
	let wait: ReturnType<typeof setTimeout> | undefined;

	function readDrawn(svg: SVGSVGElement | null): typeof drawn {
		const day = svg?.querySelector('.flow') && svg.querySelector('[data-lo]');
		const line = svg?.querySelector('.current-time-line');
		if (!day || !line) return null;
		const num = (el: Element, name: string) => Number(el.getAttribute(name));
		const [from, to] = ['data-from', 'data-to'].map((a) => Date.parse(day.getAttribute(a) ?? ''));
		const [lo, hi, at] = [num(day, 'data-lo'), num(day, 'data-hi'), num(line, 'data-now')];
		const msPerUnit = (to - from) / (hi - lo);
		if (!(msPerUnit > 0) || !Number.isFinite(at)) return null;
		const now = from + (at - lo) * msPerUnit;
		const ctm = svg!.getScreenCTM();
		const pxPerUnit = Math.abs((vertical ? ctm?.d : ctm?.a) ?? 0) || 1;
		return { now, msPerUnit, pxPerUnit, ahead: to - now };
	}

	/**
	 * Slide the `.flow` groups to the clock, and come back when the clock
	 * has moved them again: where a second moves them a unit or more, once
	 * they have moved a device pixel, and never sooner than a frame
	 * (da-k2088: on every frame, a 2m window on a phone moved an eighth of
	 * one and restyled 60 times a second); a quarter of a unit's worth later
	 * where it does not — a 12h window moves a pixel in a couple of minutes.
	 * Off now, or on a picture not drawn ahead, nothing slides. The
	 * transform is set on the groups themselves: a custom property on the
	 * band restyled everything under it.
	 */
	function flow(): void {
		clearTimeout(wait);
		// A gesture let go of holds its picture as it was slid: a pinch that
		// took the window off now must not slide the data back under it. So
		// nothing here runs, the reset off now included, until `place` lets
		// the hold go as the picture of its window lands, and calls this again.
		if (zoom?.held) return;
		const groups = el?.querySelectorAll<SVGElement>('.nowview-band svg.svg .flow') ?? [];
		if (!drawn || offNow) {
			slid = 0;
			for (const g of groups) g.style.removeProperty('transform');
			clearFar();
			return;
		}
		slid = Math.max(0, Date.now() - drawn.now);
		const by = `${vertical ? 'translateY' : 'translateX'}(${-slid / drawn.msPerUnit}px)`;
		for (const g of groups) g.style.transform = by;
		clearFar();
		if (slid > drawn.ahead / 2 && refilled !== drawn.now) {
			refilled = drawn.now;
			refills += 1;
		}
		const pixel = drawn.msPerUnit / (drawn.pxPerUnit * devicePixelRatio);
		wait = setTimeout(flow, drawn.msPerUnit < 1000 ? Math.max(pixel, 1000 / 60) : Math.min(drawn.msPerUnit / 4, 1000));
	}

	// Leaving now stops the slide, and the window is placed on the picture
	// as any pan's is (`restFor`); coming back starts it. A trigger on
	// `offNow` alone: `place` starts the loop on each picture it reads, and
	// the loop then schedules itself for as long as there is one to slide.
	$effect.pre(() => {
		void offNow;
		untrack(flow);
	});

	/**
	 * The gridlines' positions along the axis, as the CLI ruled them.
	 *
	 * Which minutes a picture marks is the picture's to say — a 30-minute
	 * window rules five-minute steps, a 12-hour one rules hours — so a
	 * press snaps to what is DRAWN and never to a number computed here.
	 */
	function gridlines(): number[] {
		const svg = svgEl();
		return [...(svg?.querySelectorAll('.gridline') ?? [])].map((l) => {
			const r = l.getBoundingClientRect();
			return axis?.y ? r.top + r.height / 2 : r.left + r.width / 2;
		});
	}

	/**
	 * How wide the NOW region is for a finger: the strip at and beyond the
	 * now-line, plus this much of the band before it.
	 *
	 * 24px is a fingertip's half-width. The strip the CLI draws right of the
	 * now-line is the current states' (`.now-chunk`) and is only as wide as
	 * the clock label under it — around 30px in a wide pane and less in a
	 * narrow one — so aiming at the strip alone would be aiming at a target
	 * too small to hit. A window not on now has no now region at all.
	 */
	const NOW_PX = 24;

	function inNowRegion(x: number, y: number): boolean {
		const svg = svgEl();
		const line = svg?.querySelector('.current-time-line');
		const r = line?.getBoundingClientRect();
		if (!r) return false;
		// A now-line drawn in the surroundings is off the window, which then has no now region.
		const at = along(r.left + r.width / 2, r.top + r.height / 2);
		const [lo, hi] = ends();
		if (!(at >= lo - 1 && at <= hi + 1)) return false;
		return along(x, y) >= at - NOW_PX;
	}

	// ── the one-pointer gestures ───────────────────────────────────────

	/** The press in hand, and what it has become. */
	let press: Press | null = null;
	let mode: Mode = $state('press');
	/** Where the pointer is, for the timer that has no event of its own. */
	let last = { x: 0, y: 0 };
	/** Whether the press has lasted long enough to be a range. */
	let long = false;
	/** How far the band is dragged while a pan is in hand, in px. */
	let panned = $state(0);
	let held: ReturnType<typeof setTimeout> | undefined;
	/**
	 * What the press went down ON, for the tap that may inspect it
	 * (da-pzzoz). The element and not the id: a picture is redrawn under a
	 * finger by the minute tick, so the id is read at the LIFT — off the
	 * element under the pointer then, which `finish` is handed.
	 */
	let onMark: Element | null = null;
	/**
	 * Where the wait is DRAWN, in px across the widget's box, while a press
	 * is still undecided.
	 *
	 * The long press used to commit to a range, and the bar dropping at that
	 * moment was how a thumb learned the press had registered (da-rskp). It
	 * does not commit any more — a still long press is a time and a
	 * travelling one a range — so the feedback it took with it is owed back,
	 * and this is it: a ring that closes over the wait, and a dashed stem
	 * through the band once the wait is up, which is the shape the range's
	 * own line has. Nothing is selected while it shows: it says only that
	 * the finger has been heard.
	 */
	let waiting = $state.raw<{ x: number; y: number } | null>(null);

	/**
	 * Whether a point is on the TIME AXIS: the axis line and the ticks and
	 * labels beyond it, below the plot or, when time runs down, left of it.
	 */
	function onAxis(x: number, y: number): boolean {
		const line = svgEl()?.querySelector('path.axis')?.getBoundingClientRect();
		if (!line || !axis) return false;
		return axis.y ? x <= line.right + 2 : y >= line.top - 2;
	}

	function begin(event: PointerEvent): void {
		if (event.button > 0) return;
		press = {
			x: event.clientX,
			y: event.clientY,
			inNow: inNowRegion(event.clientX, event.clientY),
			atOnce: event.shiftKey
		};
		// The time axis SCRUBS (da-xg1vi, vividn: «if you touch in the time
		// axis and drag around, in this case, do not pan but rather be able to
		// precisely choose a time»): the time under the finger at once, and
		// wherever it goes until the lift — not snapped, which is the point.
		if (onAxis(press.x, press.y)) {
			mode = 'scrub';
			waiting = null;
			return pointAt(selection(instantAt(press.x, press.y)));
		}
		mode = 'press';
		long = false;
		onMark = markOf(event.target);
		last = { x: event.clientX, y: event.clientY };
		const home = el?.getBoundingClientRect();
		waiting = home ? { x: event.clientX - home.x, y: event.clientY - home.y } : null;
		clearTimeout(held);
		held = setTimeout(() => {
			long = true;
			step(last.x, last.y);
		}, LONG_MS);
	}

	/** A press the pinch took over, or a cancel: nothing of it happens. */
	function drop(): void {
		clearTimeout(held);
		press = null;
		onMark = null;
		waiting = null;
		mode = 'press';
		long = false;
		panned = 0;
		moves = [];
		caught = false;
	}

	/** Run the machine to where the pointer is now, and do what the mode does. */
	function step(x: number, y: number): void {
		if (!press || !axis) return;
		if (mode === 'scrub') return pointAt(selection(instantAt(x, y)));
		const was = mode;
		// The machine reads «sideways» as «along time»: hand it the axes
		// turned when time runs down.
		mode = axis.y
			? advance(mode, { ...press, x: press.y, y: press.x }, y, x, long)
			: advance(mode, press, x, y, long);
		// The wait's mark belongs to a press that is still undecided; a
		// gesture that has committed draws its own thing.
		if (mode !== 'press' && mode !== 'hold') waiting = null;
		if (mode === 'range' && was !== 'range') pointAt(selection(instantAt(press.x, press.y)));
		if (mode === 'pan') {
			raw = along(x, y) - along(press.x, press.y);
			moves = [...moves.slice(-7), { t: performance.now(), raw }];
			panned = stopAtNow(raw);
		}
		if (mode === 'range') range(instantAt(press.x, press.y), instantAt(x, y));
		if (mode === 'now') pointAt({ rel: 0, span: (nowMs() - instantAt(x, y)) / 60_000, from: 'nowview' });
	}

	/**
	 * The selection an instant in this window is: RELATIVE while the window
	 * is on now — the axis is, so a `-q` stays on its gridline and the entry
	 * is logged that long before the Enter — and absolute once it is not.
	 * Either is to the window's own step (`tapStep`): seconds on a window
	 * of a few minutes, whole minutes from 30 up.
	 */
	const selection = (ms: number, span?: number) =>
		offNow
			? { at: new Date(ms), step: grain, span, from: 'nowview' as const }
			: { rel: (Math.round((nowMs() - ms) / 1000 / grain) * grain) / 60, span, from: 'nowview' as const };

	/** A range between two instants: it ENDS at the later, because a duration runs back. */
	function range(a: number, b: number): void {
		const [from, to] = a <= b ? [a, b] : [b, a];
		pointAt(selection(to, (to - from) / 60_000));
	}

	/** A tap: the selection lands where it fell, and the window is left alone. */
	function tap(x: number, y: number): void {
		const at = axis ? instantAlong(snapTo(along(x, y), gridlines())) : null;
		if (at !== null) pointAt(selection(at));
		lanesAt(x, y, at);
	}

	/** The fields drawn at this height, for the fields widget to put first. */
	function lanesAt(x: number, y: number, at: number | null): void {
		// The lanes the press crossed, for the fields widget to answer
		// (da-qu16, #lib/lanes.ts): the band takes the pointer itself, so
		// nothing else on the page would hear the press.
		//
		// AFTER the selection, and not before it: a selection cleared
		// clears the lanes, and a tap at now IS a cleared selection
		// (#lib/cursor.ts) — yet it still re-orders the fields by its
		// height. The snapshot has no axis and so selects no time at all;
		// there the height is the whole of what a press says (vividn, 17:01).
		const svg = svgEl();
		if (svg) readLanes(svg, { x, y, t: at });
	}

	/**
	 * The window the pan has dragged it to: the drag moves the content, so
	 * the span moves the other way. The picture stays where the finger left
	 * it, and datum is asked for another only when the window has left what
	 * this one drew (da-61nm8). The now strip stands at now on the timeline,
	 * and slides with it (da-dv45l).
	 */
	function commitPan(): void {
		if (!axis) return drop();
		const was = middleOf(middle);
		// As far on as now, the window is on now again: it flows with the clock.
		const home = panned <= nowPan() + 0.5;
		const [lo, hi] = Number.isFinite(was)
			? windowAbout(was - panned * msPerPx(axis))
			: ends().map((p) => instantOn(axis!, p - panned) + slid);
		centre = home ? null : { arg: new Date((lo + hi) / 2).toISOString(), by: 'pan' };
		rest += panned;
		panned = 0;
		if (home || lo < drawnFrom() || hi > drawnTo()) ask();
		else void tick().then(place);
	}

	// ── the stop at now, and the coast (da-v8gzq) ──────────────────────
	//
	// vividn, 2026-10-27: «let's stop letting you pan into the future on
	// nowview, it should just elastically stop at now» and «maybe add some
	// acceleration so that you can pan with velocity/inertia».

	/** How far past now a pan goes at most, in px: it resists the whole way, and springs back. */
	const STRETCH_PX = 48;
	/** How fast a coast slows: its speed falls by e in this many ms, as a phone's list does. */
	const GLIDE_MS = 325;
	/** The fastest flick a coast takes, in px per ms. */
	const FLICK_MAX = 4;

	/** The pan's slide in px as the finger alone would make it, and its last moves, for the speed a lift carries. */
	let raw = 0;
	let moves: { t: number; raw: number }[] = [];
	/** The frame a coast or a spring back waits on, while one runs. */
	let coasting = 0;
	/** Whether the press in hand stopped a coast: then a still lift is no tap. */
	let caught = false;

	/** The pan, in px, that puts the window on now. */
	const nowPan = () => (axis ? (middleOf(middle) - middleOf(undefined)) / msPerPx(axis) : -Infinity);

	/** The band's slide for `raw` px: the finger's as far as now, and ever less of it past now. */
	function stopAtNow(raw: number): number {
		const limit = nowPan();
		if (!(raw < limit)) return raw;
		const over = limit - raw;
		return limit - over / (1 + over / STRETCH_PX);
	}

	/** The speed of the last tenth of a second before the lift, in px per ms; none for a finger held still. */
	function speed(): number {
		const end = moves.at(-1);
		if (!end || performance.now() - end.t > 60) return 0;
		const from = moves.find((m) => end.t - m.t <= 100) ?? end;
		const v = end.t > from.t ? (end.raw - from.raw) / (end.t - from.t) : 0;
		return Math.max(-FLICK_MAX, Math.min(FLICK_MAX, v));
	}

	/**
	 * Let the pan go at speed `v`: it coasts on, slowing, and past now brakes
	 * hard and eases back to it. The window is committed once, where it comes
	 * to rest; until then only the band's slide moves.
	 */
	function glide(v: number): void {
		let then = performance.now();
		const settled = () => Math.abs(v) < 0.02 && raw >= nowPan() - 0.5;
		const frame = (t: number) => {
			const dt = Math.min(t - then, 64);
			then = t;
			const limit = nowPan();
			if (raw < limit) {
				v *= Math.exp(-dt / 40);
				raw = limit + (raw - limit) * Math.exp(-dt / 80) + v * dt;
			} else {
				v *= Math.exp(-dt / GLIDE_MS);
				raw += v * dt;
			}
			if (settled()) return land();
			panned = stopAtNow(raw);
			coasting = requestAnimationFrame(frame);
		};
		if (settled()) return land();
		panned = stopAtNow(raw);
		coasting = requestAnimationFrame(frame);
	}

	/** The coast stopped, where it is — or at now, from past it: the window it left the band at. */
	function land(): void {
		cancelAnimationFrame(coasting);
		coasting = 0;
		raw = Math.max(raw, nowPan());
		panned = raw;
		commitPan();
	}

	/** The instant a stretch is about, in ms: the pill's, where the dashed line is, or now. */
	const instantMs = () => (pill ? +pill.at : Date.now());

	function finish(event: PointerEvent): void {
		if (!press) return;
		const still = Math.hypot(event.clientX - press.x, event.clientY - press.y) <= SLOP_PX;
		// A finger's flick coasts on; a mouse's drag stops where it is let go.
		if (mode === 'pan') {
			const v = event.pointerType === 'mouse' ? 0 : speed();
			drop();
			return glide(v);
		}
		// A still lift is a TAP (da-xg1vi): the entry under it, or the time
		// where there is none. A still LONG one is the time wherever it
		// lands. One that traveled is a scroll the browser took, and points
		// nowhere. One that stopped a coast was only that.
		if (!caught && !pressSpent() && still && undecided(mode)) {
			const entry = mode === 'press' && inspectMark(markOf(event.target) ?? onMark, event, svgEl());
			if (entry) lanesAt(press.x, press.y, axis ? instantAt(press.x, press.y) : null);
			else tap(press.x, press.y);
		}
		drop();
	}

	/** A right click is the time, as a long tap is (da-xg1vi): «long tap / right click → select a time». */
	function onContextMenu(event: MouseEvent): void {
		if (pressSpent() || !axis) return;
		event.preventDefault();
		tap(event.clientX, event.clientY);
	}

	// ── the zoom ───────────────────────────────────────────────────────

	// `zoom`, the gesture in hand, is declared above the flow, which reads it.

	/**
	 * The scale that shows the window a raw scale leaves: to the second, as
	 * `--history` reads it, and inside the ladder's ends — so the picture
	 * under the fingers is the picture of the window the release asks for.
	 */
	const shows = (raw: number, to = (m: number) => historyArg(m)) => minutes / stepMinutes(to(minutes / raw));

	/**
	 * The gesture stretches TIME, never a mark (da-6fdaf): a dot, a glyph
	 * or a label is scaled back by the CSS below, and a multi-state block's
	 * stripes here, on the pattern's own transform, as the day strip's are
	 * (da-hmtoq, DayStrip.svelte — WebKit draws a CSS `transform` on a
	 * pattern without it). On each step of the gesture, and on each picture
	 * `place` reads: a ctrl+wheel holds no press, so a picture may land
	 * under it (one under a finger waits for the lift, #lib/hover.ts).
	 */
	function unstretch(): void {
		// Not the strip's own copies (`compose`): the strip is scaled back whole, below.
		for (const pattern of el?.querySelectorAll<SVGPatternElement>('.nowview-band svg.svg pattern:not([id$="-strip"])') ?? []) {
			// And by what the crop stretched the day's data by (da-0e06p).
			const back = 1 / ((zoom?.scale || 1) * Number(pattern.getAttribute('data-scale') ?? 1));
			pattern.dataset.drawn ??= pattern.getAttribute('patternTransform') ?? '';
			pattern.setAttribute('patternTransform', `scale(${vertical ? `1 ${back}` : `${back} 1`}) ${pattern.dataset.drawn}`);
		}
		// And the strip at now, with its line and its word, about now (`compose`).
		const back = 1 / (zoom?.scale || 1);
		for (const g of el?.querySelectorAll<SVGGElement>('.nowview-band svg.svg .at-now') ?? []) {
			const at = Number(g.getAttribute('data-anchor'));
			g.setAttribute('transform', vertical ? `translate(0 ${at}) scale(1 ${back}) translate(0 ${-at})` : `translate(${at} 0) scale(${back} 1) translate(${-at} 0)`);
		}
	}
	$effect.pre(() => {
		void [zoom, vertical];
		untrack(unstretch);
	});

	/**
	 * Where the anchor is drawn, in px across the picture's own box.
	 *
	 * The anchor is the pill's instant, where its dashed line is (da-zccjs,
	 * da-dv45l), or now — read off the stretch holding it, as the strip cuts
	 * the axis at now — and past the window's edge when the window does not
	 * hold it. On now it is the picture's own now (`drawn.now`, its
	 * `data-now`), as it stands before the flow slides the data to the
	 * clock: the instant, and not the yellow line, which datum draws a gap
	 * past it — anchored on the line, a stretch put each instant a gap
	 * times the scale off where datum's picture of the window then drew it
	 * (da-ygpto). The snapshot draws no axis, and its anchor is the right
	 * edge.
	 */
	function anchorX(svg: SVGSVGElement): number {
		const box = svg.getBoundingClientRect();
		const start = axis?.y ? box.y : box.x;
		const at = homeward || !drawn ? instantMs() : drawn.now;
		const own = timeAxis(svg, at);
		return own ? own.lo + (at - own.from) / msPerPx(own) - start : ends()[1] - start;
	}

	/**
	 * Begin a gesture over the picture, if there is one to scale and none let
	 * go of is waiting for its picture: about the anchor, or about `pos`
	 * along the axis on screen, with the band already slid `by` there (a pan
	 * the first finger began).
	 */
	/**
	 * How far the fingers or the wheel have slid the band, held so the window
	 * they leave reaches no further on than now (da-v8gzq): past it, the
	 * gesture scales about now instead. The window's middle is `commit`'s.
	 */
	function shiftOf(z: NonNullable<typeof zoom>): number {
		const shift = z.shift ?? 0;
		if (z.at === undefined || !axis) return shift;
		const home = nowMs() - (minutes / z.scale) * 30_000;
		return Math.max(shift, (middleOf(middle) - z.at - (home - z.at) * z.scale) / msPerPx(axis));
	}

	function start(pos?: number, by = 0): boolean {
		const svg = svgEl();
		if (!svg || zoom?.held) return false;
		const box = svg.getBoundingClientRect();
		zoom =
			pos === undefined || !axis
				? { origin: anchorX(svg), scale: 1, raw: 1 }
				: { origin: pos - (axis.y ? box.y : box.x), scale: 1, raw: 1, at: instantAlong(pos - by), shift: by };
		return true;
	}

	/**
	 * End the gesture: ask for the window on screen, and hold the picture as
	 * it is until datum's picture of it lands (`place`). vividn: «there
	 * should never be a difference between what is seen while still holding
	 * your finger down and then when you release».
	 */
	function commit(): void {
		if (!zoom || zoom.held) return;
		const next = historyArg(minutes / zoom.scale);
		const { at } = zoom;
		const shift = shiftOf(zoom);
		const was = middleOf(middle);
		if (at === undefined || !axis || !Number.isFinite(was)) {
			zoom = next === history ? null : { ...zoom, held: next };
			return rescale(next);
		}
		// The same window: the fingers moved it and no more, a pan.
		if (next === history) {
			zoom = null;
			panned = shift;
			return commitPan();
		}
		// The window whose instant `at` stands where the fingers left it: the
		// middle as far from it as before, over the scale, less the slide —
		// `shift` in screen px, turned to ms by the picture's own axis at rest.
		const middleAt = at + (was - at - shift * msPerPx(axis)) / zoom.scale;
		const home = shift > (zoom.shift ?? 0);
		centre = home ? null : { arg: new Date(middleAt).toISOString(), by: 'pan' };
		zoom = { ...zoom, held: next };
		onchange?.(next);
	}

	/**
	 * The window to `next` minutes, scaled about the section's instant
	 * (da-zccjs, vividn: «when the view is panned away, scale about where the
	 * "now" would actually be, not the center of the window»): its middle
	 * moves so the instant keeps its place. A window on now with no pill ends
	 * at now, and keeps ending there; one with a pill leaves now for as long
	 * as the pill lasts, as a dayview's click moves it (`sel`).
	 */
	function rescale(next: string): void {
		if (next === history) return;
		const [at, was] = [instantMs(), middleOf(middle)];
		if ((centre || pill) && Number.isFinite(was)) {
			const by = stepMinutes(next) / minutes;
			centre = { by: centre?.by ?? 'sel', arg: new Date(at + (was - at) * by).toISOString() };
		}
		onchange?.(next);
	}

	// ── two fingers ────────────────────────────────────────────────────

	/** The fingers down on the picture, by pointer id. */
	const fingers = new Map<number, { x: number; y: number }>();
	/** How far apart they were along the axis when the pinch began, and where their middle was. */
	let apart = 0;
	let middleFrom = 0;

	/**
	 * Two fingers along the axis: how far apart, never under a fingertip
	 * (`NOW_PX`), so two fingers side by side across it scale by nothing
	 * wild — and their middle.
	 */
	const pair = () => {
		const [a, b] = [...fingers.values()].map((f) => along(f.x, f.y));
		return { spread: Math.max(Math.abs(a - b), NOW_PX), mid: (a + b) / 2 };
	};

	// The window listens, not the element: a finger that wanders off the
	// picture mid-pinch is still part of the pinch, and the element would
	// stop hearing it. A one-finger drag or a mouse wants the same, so the
	// listeners are armed by the first pointer down of either kind.
	let listening = false;

	function listen(on: boolean): void {
		if (on === listening) return;
		listening = on;
		for (const [type, handler] of [
			['pointermove', onPointerMove],
			['pointerup', onPointerUp],
			['pointercancel', onPointerCancel]
		] as [string, EventListener][]) {
			if (on) window.addEventListener(type, handler);
			else window.removeEventListener(type, handler);
		}
	}

	function onPointerDown(event: PointerEvent): void {
		// A press stops a coast where it is.
		if (coasting) {
			land();
			caught = true;
		}
		// A gesture let go of is still on screen until its picture comes.
		if (zoom?.held) return;
		listen(true);
		if (event.pointerType === 'mouse') return begin(event);
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (fingers.size === 1) return begin(event);
		// A second finger is a pinch, and the first finger's gesture was
		// never one: whatever it had become is dropped as the scale starts,
		// so nothing of it is committed on the way out — but a pan it began
		// stays where it slid the band, under the fingers.
		if (fingers.size === 2) {
			({ spread: apart, mid: middleFrom } = pair());
			start(middleFrom, panned);
		}
		drop();
	}

	function onPointerMove(event: PointerEvent): void {
		if (fingers.has(event.pointerId)) fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (zoom && !zoom.held && fingers.size === 2) {
			const { spread, mid } = pair();
			const raw = spread / apart;
			zoom = { ...zoom, raw, scale: shows(raw), shift: (zoom.shift ?? 0) + mid - middleFrom };
			middleFrom = mid;
		}
		if (!press) return;
		last = { x: event.clientX, y: event.clientY };
		step(event.clientX, event.clientY);
	}

	function onPointerUp(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (zoom && fingers.size < 2) commit();
		else finish(event);
		if (fingers.size === 0 && !press) listen(false);
	}

	function onPointerCancel(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (zoom && fingers.size < 2) commit();
		else drop();
		if (fingers.size === 0) listen(false);
	}

	/**
	 * `touch-action: pan-y` keeps the browser off a pinch, but a two-finger
	 * drag is still a scroll it may claim — and claiming it sends
	 * `pointercancel` mid-gesture. A gesture of ours along the band is the
	 * same: once it has committed, the move is ours.
	 */
	function onTouchMove(event: TouchEvent): void {
		if (fingers.size >= 2 || mode !== 'press') event.preventDefault();
	}

	// ── ctrl+wheel ─────────────────────────────────────────────────────

	/**
	 * A notch of the wheel is half a doubling, so one lands on the next
	 * step of the ladder, as a handle's wheel steps the far label. A trackpad
	 * pinch arrives here too, as a shower of small deltas: they accumulate
	 * into one gesture, which the pause at the end of it commits.
	 */
	const NOTCH = 200;
	let settle: ReturnType<typeof setTimeout> | undefined;
	/** What the wheel turned while a gesture let go of waited for its picture: turned once it lands. */
	let owed = 0;

	/** Where the pointer last turned the wheel, along the axis: what it scales about. */
	let turnedAt = 0;

	function onWheel(event: WheelEvent): void {
		if (!event.ctrlKey) return;
		event.preventDefault();
		turnedAt = along(event.clientX, event.clientY);
		if (zoom?.held) owed += event.deltaY;
		else turn(event.deltaY);
	}

	function turn(deltaY: number): void {
		if (!zoom) start(turnedAt);
		const held = zoom;
		if (!held) return;
		const raw = held.raw * Math.pow(2, -deltaY / NOTCH);
		zoom = { ...held, raw, scale: shows(raw, (m) => zoomHistory(history, minutes / m)) };
		clearTimeout(settle);
		settle = setTimeout(commit, 120);
	}

	// ── the handles, the margin and the buttons ────────────────────────

	/** The room a handle keeps clear along the axis either side of its label. */
	const GAP_PX = 4;

	/**
	 * Below this width the margin strip, its pills and `← sel` do not fit,
	 * and the widget shows the LINES alone.
	 *
	 * The tiny nowview in `mobile` is ~100px wide (measured on the built
	 * app): the old `now →` was 46px there and the shortest time pill —
	 * `-qq`, two sections of nothing — is 54px, so the two together already
	 * overflowed it, and a pill near the instant it names has no room to be
	 * near anything. 200px fit `now →` beside a `-t 14:30` pill with a third
	 * of the band still legible. vividn agreed, 17:01.
	 *
	 * The «now» button that replaced `now →` is NOT behind this (da-6csum):
	 * it is the only way home, and it stands where datum wrote «now», which
	 * a strip too narrow for the word does not have.
	 */
	const TINY_PX = 200;

	/** Whether this nowview is wide enough for the margin and the buttons. */
	let roomy = $state(true);

	/**
	 * A HANDLE over each major tick label the window shows (da-57vj4): its
	 * words (`-15m`), how far from the anchor they say it is in minutes,
	 * and its box, centred at `x`, `y` across and down the widget's. The
	 * far label is the first along time, which is how far back the window
	 * reaches; it is moved in off the window's edge, where the surroundings
	 * put it, and a nearer label it would lie over is not drawn. None when
	 * the picture draws no axis — the snapshot.
	 */
	type Handle = { text: string; minutes: number; x: number; y: number; w: number; h: number };
	/** What a wheel opens on and a press takes hold of: a handle's time and place, not its box. */
	type Held = Pick<Handle, 'text' | 'minutes' | 'x' | 'y'>;
	let handles: Handle[] = $state.raw([]);
	/** The widget's box on screen, for the wheel, which is drawn `fixed` beside it. */
	let boxAt = $state.raw({ x: 0, y: 0 });

	/**
	 * Hide each clock time (`.minute-label`) a handle stands over, as
	 * `place` hides a tick label the far one would cover. They slide with
	 * the flow, so this runs at each slide too (da-to5n4). So is one whose
	 * minute is still to come: the clip at the now-line hides only its far
	 * half, and the near one peeked in beside «now» (da-xwe0h).
	 */
	function clearFar(): void {
		const box = el?.getBoundingClientRect();
		const y = axis?.y;
		const spans = box
			? handles.map((h) => {
					const [c, half] = y ? [h.y + box.y, h.h / 2] : [h.x + box.x, h.w / 2];
					return [c - half - GAP_PX, c + half + GAP_PX];
				})
			: [];
		const now = drawn ? drawn.now + slid : Infinity;
		for (const l of el?.querySelectorAll<SVGElement>('svg.svg .minute-label') ?? []) {
			const r = l.getBoundingClientRect();
			const [a, b] = y ? [r.top, r.bottom] : [r.left, r.right];
			// Its mark, which datum draws just before it, holds the instant; a
			// millisecond spares `drawn.now`'s rounding.
			const ahead = Date.parse(l.previousElementSibling?.getAttribute('data-at') ?? '') - now > 1;
			l.style.visibility = ahead || spans.some(([from, to]) => a < to && b > from) ? 'hidden' : '';
		}
	}
	$effect.pre(() => {
		void handles;
		untrack(clearFar);
	});

	/** The instants at the window's two ends, as the picture on screen draws them. */
	let view = $state.raw<[number, number] | null>(null);

	/**
	 * How far to slide the picture, along its UNMOVED axis, so the window is
	 * where `centre` says when the picture on screen was asked for
	 * elsewhere — after a pan inside it, and at each redraw of it, so an
	 * on-now picture that moves with the clock does not take a panned
	 * window along.
	 */
	function restFor(unmoved: NonNullable<ReturnType<typeof timeAxis>>): number {
		const target = Date.parse(middle ?? '');
		if (middle === askedFor || !Number.isFinite(target)) return 0;
		// From the middle of the window the picture was drawn for, at its
		// scale: what a slide moves is time.
		const drewAbout = askedFor ? Date.parse(askedFor) : (readDrawn(svgEl())?.now ?? NaN) - minutes * 30_000;
		if (Number.isFinite(drewAbout)) return (drewAbout - target) / msPerPx(unmoved);
		const [lo, hi] = ends();
		return (lo + hi) / 2 - (unmoved.lo + (target - unmoved.from) / msPerPx(unmoved));
	}

	/**
	 * The picture's axis as it is drawn before any slide or gesture moves it:
	 * the stretch holding the window's middle, where its now strip cuts it in
	 * two (da-6csum) — all of a window the strip is pinned left of.
	 */
	function unmovedAxis(svg: SVGSVGElement): typeof axis {
		return unmoved(svg, () => timeAxis(svg, Date.parse(askedFor ?? '')));
	}

	/** Every stretch of the picture's axis as it is drawn before any slide or gesture moves it. */
	function unmovedStretches(svg: SVGSVGElement): NonNullable<ReturnType<typeof timeAxis>>[] {
		const starts = [...svg.querySelectorAll('[data-lo]')].map((g) => Date.parse(g.getAttribute('data-from') ?? ''));
		return unmoved(svg, () => [...new Set(starts)].flatMap((at) => timeAxis(svg, at) ?? []));
	}

	/** `read` with the picture's own slide taken off it. */
	function unmoved<T>(svg: SVGSVGElement, read: () => T): T {
		const moved = svg.style.transform;
		svg.style.transform = 'none';
		try {
			return read();
		} finally {
			svg.style.transform = moved;
		}
	}

	/**
	 * Where each margin pill sits, in px across the widget's box: over the
	 * mark the selection was drawn as.
	 *
	 * Read off the DRAWN marks (`#lib/hover.ts` puts the arrow at the
	 * selection and the band over the range), so a pill in the margin and
	 * the line under it cannot disagree by a pixel — including for a
	 * relative selection, which is placed against the picture's own
	 * now-line and not against the clock.
	 */
	let spots: { time: number | null; dur: number | null } = $state.raw({ time: null, dur: null });

	/**
	 * Where the picture writes «now» under its now section, in px across
	 * and down the widget's box: off now the button takes its place
	 * (da-6csum), and the words under it are hidden. None when the strip
	 * is too narrow for datum to write them.
	 */
	let nowAt = $state.raw<{ x: number; y: number } | null>(null);
	/** The band's two ends along time across the widget's box, and the «now» button's own size, to hold it inside. */
	let edges = $state.raw<[number, number]>([0, 0]);
	let homeWide = $state(0);
	let homeTall = $state(0);

	function place(): void {
		if (!el) return;
		unstretch();
		const home = el.getBoundingClientRect();
		roomy = home.width >= TINY_PX;
		boxAt = { x: home.x, y: home.y };
		const svg = svgEl();
		// The window a let-go gesture asked for is drawn: the scale goes, in
		// the same frame the picture of it comes. So it does once the window is
		// no longer the one asked for — a reset, or the session forgotten,
		// while it waited — rather than wait for a picture that never comes.
		if (zoom?.held && (!svg || history !== zoom.held || current())) {
			zoom = null;
			return void tick().then(() => {
				place();
				const turned = owed;
				owed = 0;
				if (turned) turn(turned);
			});
		}
		// Read unmoved, then slid by `rest` alone: a redraw mid-gesture must
		// not cache the gesture's own offset, which `commitPan` adds again.
		const unmoved = svg && unmovedAxis(svg);
		const moved = unmoved && svg !== stale ? restFor(unmoved) : rest;
		axis = unmoved && { ...unmoved, lo: unmoved.lo + moved, hi: unmoved.hi + moved };
		stretches = svg ? unmovedStretches(svg).map((a) => ({ ...a, lo: a.lo + moved, hi: a.hi + moved })) : [];
		const groups = svg?.querySelectorAll('[data-lo]') ?? [];
		const edge = (el: Element | undefined, name: string) => Date.parse(el?.getAttribute(name) ?? '');
		spanned = [edge(groups[0], 'data-from'), edge(groups[groups.length - 1], 'data-to')];
		if (svg) needLayer(svg);
		// What is read off the picture below is read where it will be drawn.
		if (Math.abs(moved - rest) >= 0.5) {
			rest = moved;
			return void tick().then(place);
		}
		drawn = readDrawn(svg);
		flow();
		const about = middleOf(middle);
		view = !axis
			? null
			: Number.isFinite(about)
				? windowAbout(about)
				: (ends().map((p) => instantOn(axis!, p) + slid) as [number, number]);
		// The window has left what the picture drew — the clock moved an
		// on-now picture out from under it: a picture of the window, then.
		// Asked once: `ask` makes `askedFor` the window's, and from then on
		// this `place` and every one after it has nothing left to ask for.
		if (view && axis && middle !== askedFor && (view[0] < drawnFrom() - EDGE_MS || view[1] > drawnTo() + EDGE_MS)) ask();
		// Before the early returns below: a snapshot draws no tick labels
		// and leaves by the first of them, and a selection is drawn there
		// too.
		readTip($selected);
		const centreOf = (selector: string) => {
			const r = el?.querySelector(selector)?.getBoundingClientRect();
			return r ? r.left + r.width / 2 - home.x : null;
		};
		// Kept inside the strip: a pill centred on a mark at either end of
		// the band would otherwise hang half of itself off the widget. The
		// pill's own width is read off the one on screen, so a two-section
		// `-t 2026-09-09T14:30` is held in as firmly as a `-qq`; a pill wider
		// than the strip is simply centred.
		const held = (x: number | null, selector: string) => {
			if (x === null) return null;
			const half = (el?.querySelector(selector)?.getBoundingClientRect().width ?? 0) / 2;
			const lo = Math.min(half, home.width / 2);
			return Math.max(lo, Math.min(Math.max(home.width - half, home.width / 2), x));
		};
		spots = {
			time: held(centreOf('svg.svg .cursor-head'), '.nowview-margin .pill.time'),
			dur: held(centreOf('svg.svg .cursor-span'), '.nowview-margin .pill.dur')
		};
		// The «now» under the strip, or, with none drawn, the axis's row and
		// the side of the window now lies on (da-dv45l).
		const word = svg?.querySelector<SVGElement>('.now-label');
		const row = (word ?? svg?.querySelector<SVGElement>('.tick-label'))?.getBoundingClientRect();
		edges = ends().map((p) => p - (vertical ? home.y : home.x)) as [number, number];
		const beyond = axis && nowMs() < instantAlong((ends()[0] + ends()[1]) / 2) ? -Infinity : Infinity;
		nowAt = row?.width
			? {
					x: vertical ? row.left + row.width / 2 - home.x : word ? row.left + row.width / 2 - home.x : beyond,
					y: vertical && !word ? beyond : row.top + row.height / 2 - home.y
				}
			: null;
		if (word) word.style.visibility = homeward ? 'hidden' : '';
		const labels = [...(svg?.querySelectorAll<SVGElement>('.tick-label') ?? [])];
		for (const l of labels) {
			l.removeAttribute('transform');
			l.style.visibility = '';
		}
		const ctm = svg?.getScreenCTM();
		if (!labels.length || !axis || !ctm) return void (handles = []);
		// The first along time: leftmost, or topmost when time runs down.
		const y = axis.y;
		const box = (l: SVGElement) => l.getBoundingClientRect();
		const span = (r: DOMRect) => (y ? [r.top, r.bottom] : [r.left, r.right]);
		const [lo, hi] = ends();
		const inside = (l: SVGElement) => {
			const [a, b] = span(box(l));
			return (a + b) / 2 >= lo - 1 && (a + b) / 2 <= hi + 1;
		};
		const shown = labels.filter(inside);
		if (!shown.length) return void (handles = []);
		const label = shown.reduce((a, b) => (span(box(b))[0] < span(box(a))[0] ? b : a));
		const inset = Math.max(0, lo + GAP_PX - span(box(label))[0]) / (y ? ctm.d : ctm.a);
		if (inset) label.setAttribute('transform', y ? `translate(0 ${inset})` : `translate(${inset} 0)`);
		const [from, to] = span(box(label));
		for (const l of labels) {
			const [a, b] = span(box(l));
			if (l !== label && a < to + GAP_PX * 2 && b > from - GAP_PX * 2) l.style.visibility = 'hidden';
		}
		// On the label's middle, but no higher than keeps a handle's target
		// under the axis line: the strip is only as tall as its 11px digits
		// (da-a2cxp).
		const under = (svg?.querySelector('.axis')?.getBoundingClientRect().top ?? 0) + HANDLE_PX / 2;
		handles = shown
			.filter((l) => l.style.visibility !== 'hidden')
			.map((l) => {
				const r = box(l);
				const text = l.textContent ?? '';
				const mid = r.top + r.height / 2;
				return {
					text,
					minutes: stepMinutes(text.replace(/^[-+]/, '')),
					x: r.left + r.width / 2 - home.x,
					y: (y ? mid : Math.max(mid, under)) - home.y,
					w: r.width + 8,
					h: Math.max(r.height + 4, HANDLE_PX)
				};
			})
			.filter((h) => h.minutes > 0);
	}

	// ── the handles' wheel and drag (da-57vj4) ─────────────────────────
	//
	// vividn: «make it clear that this is some sort of handle. When you
	// click and hold on it or hover over it, show the prev/next time
	// notches above and below … dragging up and down or selecting one of
	// these times switches the range time to next step … Dragging left and
	// right on this handle should place the handle in a different place on
	// the x axis scaling appropriately.»
	//
	// A handle pins its time to its PLACE, with the anchor (now, or the
	// instant a centred window is about) where it is. A step of the wheel
	// keeps the place and changes the time, so the window is rescaled by
	// their ratio; a drag along the axis keeps the time and moves the
	// place, to any window at all. datum's now strip is a fixed share of
	// the band (`NOW_STRIP_FRACTION`), so a place stays put across the
	// redraw a step asks for.

	/** How far apart the wheel's notches are, across the axis, in px. */
	const NOTCH_PX = { x: 22, y: 44 };
	/** How tall a handle's target is at least, its 11px words and all. */
	const HANDLE_PX = 20;
	/** How many of the ladder's steps the wheel shows either side: the next one clear, the rest fading. */
	const SHOWN = 3;

	/**
	 * The wheel open on a handle: its time, the sign it is written with,
	 * and its place across and down the widget's box — its OWN, not a
	 * label's, so the redraw a step asks for neither closes nor moves it.
	 * `pick` is how many notches a drag across the axis has turned it.
	 */
	let wheel = $state.raw<{ minutes: number; text: string; sign: string; x: number; y: number; pick: number } | null>(null);
	let wheelEl: HTMLDivElement | undefined = $state();

	const around = $derived(wheel ? ladderAround(wheel.minutes, minutes) : { up: [], down: [] });
	/** The wheel's notches as drawn: how many steps from the handle (up is longer), and the step. */
	const notches = $derived([
		...around.up.slice(0, SHOWN).map((step, k) => ({ k: k + 1, step })),
		...around.down.slice(0, SHOWN).map((step, k) => ({ k: -(k + 1), step }))
	]);

	function open(h: Held): void {
		if (wheel && Math.abs(wheel.x - h.x) < 1 && Math.abs(wheel.y - h.y) < 1) return;
		wheel = { minutes: h.minutes, text: h.text, sign: h.text.startsWith('+') ? '+' : '-', x: h.x, y: h.y, pick: 0 };
	}

	function close(): void {
		if (!grip) wheel = null;
	}

	/** The wheel's handle takes `step`: the window rescaled about the anchor, the wheel still open around it. */
	function stepTo(step: string | undefined): void {
		if (!wheel || !step) return;
		const to = stepMinutes(step);
		const next = historyArg((minutes * to) / wheel.minutes);
		wheel = { ...wheel, minutes: to, text: wheel.sign + step, pick: 0 };
		rescale(next);
	}

	/**
	 * Leaving the wheel closes it, unless the keyboard is on a handle. Heard
	 * on the window as well as the wheel: a wheel that opened under a still
	 * pointer was never entered, and so is never left.
	 */
	function leave(event: PointerEvent): void {
		if (event.pointerType !== 'mouse' || !wheel || grip) return;
		// Where it went: a move's target, or the element a leave went into.
		const to = event.type === 'pointermove' ? event.target : event.relatedTarget;
		if (onCentre(event)) return;
		const on = to instanceof Element ? to : null;
		if (on && (wheelEl?.contains(on) || on.closest('.nowview-handles'))) return;
		if (!document.activeElement?.matches('.handle:focus-visible')) close();
	}

	/**
	 * Whether a pointer is on the wheel's own handle. It takes no pointer
	 * itself — a press there is the handle's beneath it, and a click on a
	 * handle is not one on something laid over it — so this is read off
	 * its box: after a step there may be no label, and no handle, under it.
	 */
	function onCentre(event: PointerEvent): boolean {
		const r = wheelEl?.querySelector('.handle.held')?.getBoundingClientRect();
		return !!r && event.clientX >= r.left && event.clientX <= r.right && event.clientY >= r.top && event.clientY <= r.bottom;
	}

	/** Focus leaving a handle or the wheel closes it, unless it went into the wheel or the pointer is on it. */
	function blur(event: FocusEvent): void {
		const to = event.relatedTarget as Node | null;
		if (!(to && wheelEl?.contains(to)) && !wheelEl?.matches(':hover')) close();
	}

	function onWheelKey(event: KeyboardEvent): void {
		if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
		event.preventDefault();
		stepTo(event.key === 'ArrowUp' ? around.up[0] : around.down[0]);
	}


	/**
	 * A press on a handle, in hand. Undecided until it travels: along the
	 * axis it is the fine scale, across it the wheel, and the axis it
	 * travels first decides, once, as the pinch's does. A finger's press
	 * opens the wheel when it lands — a hover it has none of.
	 */
	let grip: {
		id: number;
		x: number;
		y: number;
		/** Where the handle is, and the anchor, along the axis on screen; the handle across it. */
		at: number;
		cross: number;
		anchor: number;
		text: string;
		mode: 'press' | 'wheel' | 'scale';
		touch: boolean;
	} | null = $state(null);
	/** Where a scaling handle is drawn, along the axis on screen. */
	let dragged = $state(0);

	const across = (dx: number, dy: number) => (vertical ? dx : -dy);

	function grab(event: PointerEvent, h: Held): void {
		const svg = svgEl();
		if (event.button > 0 || !svg || !axis) return;
		event.preventDefault();
		const box = svg.getBoundingClientRect();
		const at = vertical ? boxAt.y + h.y : boxAt.x + h.x;
		const anchor = (vertical ? box.y : box.x) + anchorX(svg);
		const touch = event.pointerType !== 'mouse';
		if (touch) open(h);
		const cross = vertical ? boxAt.x + h.x : boxAt.y + h.y;
		grip = { id: event.pointerId, x: event.clientX, y: event.clientY, at, cross, anchor, text: h.text, mode: 'press', touch };
		dragged = at;
		for (const [type, handler] of GRIP) window.addEventListener(type, handler);
	}

	function onGripMove(event: PointerEvent): void {
		if (!grip || event.pointerId !== grip.id) return;
		const [dx, dy] = [event.clientX - grip.x, event.clientY - grip.y];
		if (grip.mode === 'press' && Math.hypot(dx, dy) > SLOP_PX) {
			grip.mode = Math.abs(vertical ? dy : dx) >= Math.abs(vertical ? dx : dy) ? 'scale' : 'wheel';
			if (grip.mode === 'scale') {
				wheel = null;
				start();
			} else if (!wheel) {
				const h = handles.find((h) => h.text === grip?.text);
				if (h) open(h);
			}
		}
		if (grip.mode === 'scale' && zoom) {
			const finger = vertical ? event.clientY : event.clientX;
			// The picture scaled about the anchor so the handle's time is
			// under the finger: `shows` holds the window inside the ladder's
			// ends, and a finger dragged past the anchor (raw <= 0) asks for
			// the longest.
			const raw = (finger - grip.anchor) / (grip.at - grip.anchor);
			const scale = shows(raw > 0 ? raw : minutes / stepMinutes('12h'));
			zoom = { ...zoom, raw, scale };
			dragged = grip.anchor + (grip.at - grip.anchor) * scale;
		}
		if (grip.mode === 'wheel' && wheel) {
			const notch = vertical ? NOTCH_PX.y : NOTCH_PX.x;
			const pick = Math.round(across(dx, dy) / notch);
			wheel = { ...wheel, pick: Math.max(-around.down.length, Math.min(around.up.length, pick)) };
		}
	}

	function onGripUp(event: PointerEvent): void {
		if (!grip || event.pointerId !== grip.id) return;
		const was = grip;
		ungrip();
		if (was.mode === 'scale') commit();
		else if (was.mode === 'wheel' && wheel) {
			const pick = wheel.pick;
			stepTo(pick > 0 ? around.up[pick - 1] : pick < 0 ? around.down[-pick - 1] : undefined);
			if (was.touch) wheel = null;
		}
	}

	function onGripCancel(event: PointerEvent): void {
		if (!grip || event.pointerId !== grip.id) return;
		ungrip();
		zoom = null;
		if (wheel) wheel = { ...wheel, pick: 0 };
	}

	function ungrip(): void {
		grip = null;
		for (const [type, handler] of GRIP) window.removeEventListener(type, handler);
	}

	const GRIP = [
		['pointermove', onGripMove],
		['pointerup', onGripUp],
		['pointercancel', onGripCancel]
	] as [string, EventListener][];

	/** A wheel a finger opened stays until a press lands somewhere else. */
	function onOutside(event: PointerEvent): void {
		if (!wheel || grip || (event.target instanceof Node && wheelEl?.contains(event.target))) return;
		if (!onCentre(event)) return void (wheel = null);
		// On the wheel's handle, over whatever is beneath it: the handle's.
		event.stopPropagation();
		grab(event, wheel);
	}

	/** The pills the selection made: the ones the margin draws beside their instants. */
	const marginPills = $derived($pills.filter((p) => p.id === 'cursor' || p.id === SPAN_ID));

	// ── the tooltip on a selected entry ────────────────────────────────
	//
	// vividn: «maybe when something is selected in the nowview, it could also
	// surface some bare info about it in a floating tooltip (start and end
	// time as well as field and state + comment (maybe?)».
	//
	// Whatever SELECTED it: a tap here, a row tapped
	// in the tail. The picture is asked which of its marks name the
	// selected entries (`selectedTip`, #lib/hover.ts) and the tooltip
	// floats over them, so a selection this window does not draw shows
	// nothing — «and nowview if that particular piece of data is
	// represented there».
	//
	// The COMMENT is left out, and #lib/inspect.ts says why: a mark carries
	// its field, its state and its instants and nothing else, so a comment
	// would be a whole `datum get` per gesture.

	/** What the tooltip says and where it floats, in the viewport's own px. */
	let tip = $state.raw<{ field: string; state: string; when: string; x: number; y: number } | null>(null);

	/** The two instants as one label: a span, or the one instant of a dot. */
	function whenOf(from: number | null, to: number | null): string {
		if (from === null || to === null) return '';
		return from === to
			? clockLabel(new Date(from))
			: `${clockLabel(new Date(from))}–${clockLabel(new Date(to))}`;
	}

	function readTip(hids: string[]): void {
		const svg = svgEl();
		const found = svg && selectedTip(svg, hids);
		if (!found) return void (tip = null);
		const { box } = found;
		tip = {
			field: found.tip.field,
			state: found.tip.state,
			when: whenOf(found.tip.from, found.tip.to),
			x: Math.max(8, Math.min(box.left + box.width / 2, window.innerWidth - 8)),
			y: Math.min(box.bottom + 8, window.innerHeight - 44)
		};
	}

	// Read again whenever the selection changes AND whenever the picture is
	// redrawn or the box resized — `place` is the one that knows the latter,
	// and it runs off the same MutationObserver the margin pills are placed
	// by, so the tooltip cannot be left floating over a mark that is gone.
	$effect.pre(() => {
		const hids = $selected;
		untrack(() => readTip(hids));
	});

	/**
	 * Which way a selection the window does not contain lies.
	 *
	 * Off the cached axis rather than off the drawn marks: a selection
	 * outside the window draws nothing at all, so there would be no mark
	 * to read, and the button has to appear the moment the window leaves it.
	 */
	const away = $derived(
		!view || !$pointed
			? null
			: +$pointed.at < view[0] - EDGE_MS
				? 'left'
				: +$pointed.at > view[1] + EDGE_MS
					? 'right'
					: null
	);

	onMount(() => {
		if (!el) return;
		const redrawn = new MutationObserver(place);
		redrawn.observe(el, { childList: true, subtree: true });
		const resized = new ResizeObserver(place);
		resized.observe(el);
		window.addEventListener('pointerdown', onOutside, true);
		window.addEventListener('pointermove', leave);
		return () => {
			redrawn.disconnect();
			resized.disconnect();
			window.removeEventListener('pointerdown', onOutside, true);
			window.removeEventListener('pointermove', leave);
		};
	});

	onDestroy(() => {
		clearTimeout(wait);
		clearTimeout(settle);
		clearTimeout(held);
		cancelAnimationFrame(coasting);
		listen(false);
		ungrip();
	});
</script>

<div class="nowview" class:vertical bind:this={el}>
	<!-- The margin: the pills the selection made, each over the mark it
	     names, and ✕-able here as in the bar. NOT `data-pans`, so a swipe
	     that starts in it still changes the pane's tab (da-um7y). A strip
	     OVER the band lies along a horizontal axis only; a vertical one
	     leaves the pills to the bar (da-78evn). -->
	{#if roomy && !vertical}
		<div class="nowview-margin" aria-label="selection">
			{#each marginPills as pill (pill.id)}
				{@const x = pill.id === SPAN_ID ? spots.dur : spots.time}
				{#if x !== null}
					<span class="margin-pill" style:left="{x}px"><Pill {pill} /></span>
				{/if}
			{/each}
		</div>
	{/if}

	<!-- The band is pointed into, not a control: its marks take the keyboard. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="nowview-band"
		style:--zoom={zoom?.scale ?? 1}
		style:--zoom-x="{zoom?.origin ?? 0}px"
		style:--pan="{rest + panned + (zoom ? shiftOf(zoom) : 0)}px"
		data-pans={pans}
		data-points="own"
		data-mode={mode}
		onpointerdown={onPointerDown}
		oncontextmenu={onContextMenu}
		{@attach (el) => on(el, 'touchmove', onTouchMove, { passive: false })}
		{@attach (el) => on(el, 'wheel', onWheel, { passive: false })}
	>
		<!-- `refills` rides on the epoch: a bump re-runs the command once
		     half of what was drawn ahead is spent. A write re-runs only the
		     snapshot, the one picture here drawn from data: the overlay has
		     none, and a write asks for the day's data layer (da-0e06p). -->
		<CliPane {spec} epoch={(showHistory ? 0 : epoch) + refills} compose={crop} dims={false} />
	</div>

	<!-- The wait's mark: a ring that closes over LONG_MS, and a dashed stem
	     through the band once it is up. `pointer-events: none`, so it is a
	     picture of the gesture and never part of it. -->
	{#if waiting && (mode === 'press' || mode === 'hold')}
		<div
			class="hold-mark"
			class:held={mode === 'hold'}
			data-mode={mode}
			aria-hidden="true"
			style:left="{waiting.x}px"
			style:top="{waiting.y}px"
			style:--long="{LONG_MS}ms"
		>
			<span class="stem"></span>
			<span class="ring"></span>
		</div>
	{/if}

	<!-- The handles (da-57vj4): one over each major tick label, its words
	     drawn by the picture underneath. Hover or a finger opens the wheel;
	     a drag along the axis scales the window about the anchor. -->
	{#if handles.length && !zoom}
		<div class="nowview-handles" data-pans={pans}>
			<!-- Unkeyed: a step redraws the labels, and the handle the
			     keyboard is on stays the focused element through it. -->
			{#each handles as h}
				<button
					type="button"
					class="handle"
					data-at={h.text}
					aria-label="window handle {h.text}"
					title="{h.text}: hover or hold for the steps, drag along the axis to scale"
					style:left="{h.x}px"
					style:top="{h.y}px"
					style:width="{h.w}px"
					style:height="{h.h}px"
					onpointerenter={(e) => e.pointerType === 'mouse' && open(h)}
					onpointerdown={(e) => grab(e, h)}
					onfocus={() => open(h)}
					onblur={blur}
					onkeydown={onWheelKey}>{h.text}</button
				>
			{/each}
		</div>
	{/if}

	<!-- The wheel: the ladder's steps either side of the handle it is open
	     on, longer toward the picture, the next one clear and the rest
	     fading. `fixed`, so a pane's edge does not cut its notches off. -->
	{#if wheel && !zoom}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="nowview-wheel"
			data-pans={pans}
			bind:this={wheelEl}
			style:left="{boxAt.x + wheel.x}px"
			style:top="{boxAt.y + wheel.y}px"
			style:--turn="{wheel.pick * (vertical ? NOTCH_PX.y : NOTCH_PX.x)}px"
			onpointerleave={leave}
			onfocusout={blur}
		>
			{#each notches as n (n.k)}
				<button
					type="button"
					class="notch"
					class:picked={n.k === wheel.pick}
					data-step={n.step}
					data-k={n.k}
					tabindex="-1"
					style:--k={n.k}
					style:--fade={Math.abs(n.k) === 1 ? 1 : Math.abs(n.k) === 2 ? 0.45 : 0.18}
					onclick={() => stepTo(n.step)}>{wheel.sign}{n.step}</button
				>
			{/each}
			<span class="handle held" class:turned={wheel.pick !== 0} data-at={wheel.text} aria-hidden="true"
				>{wheel.text}</span
			>
		</div>
	{/if}
	{#if grip?.mode === 'scale'}
		<span
			class="handle dragged"
			aria-hidden="true"
			style:left="{vertical ? grip.cross : dragged}px"
			style:top="{vertical ? dragged : grip.cross}px">{grip.text}</span
		>
	{/if}

	<!-- Outside the band on purpose: a press on a button is not a press on
	     the picture, and keeping them siblings is what makes that true
	     without a single stopPropagation. They still declare the band's
	     axis, so a drag that starts on one is not a tab swipe either
	     (da-ndzl's test says so of the steps). -->
	{#if homeward && nowAt && !zoom}
		<!-- Over the «now» under the strip, and held to the edge now lies
		     beyond once the strip has gone off it (da-dv45l). -->
		{@const at = (vertical ? nowAt.y : nowAt.x) + panned}
		{@const half = (vertical ? homeTall : homeWide) / 2}
		{@const held = Math.max(edges[0] + half, Math.min(edges[1] - half, at))}
		<button
			type="button"
			class="nowview-now"
			data-pans={pans}
			title="back to now, and the time pills away"
			style:left="{vertical ? nowAt.x : held}px"
			style:top="{vertical ? held : nowAt.y}px"
			bind:offsetWidth={homeWide}
			bind:offsetHeight={homeTall}
			onclick={toNow}
			>{!offNow ? 'reset' : at < edges[0] + half ? (vertical ? '↑ now' : '← now') : vertical ? 'now ↓' : 'now →'}</button
		>
	{/if}
	{#if roomy && away && !zoom}
		<button
			type="button"
			class="nowview-jump {away} selection"
			data-pans={pans}
			title="the window onto the selection"
			onclick={toSelection}
			>{vertical ? (away === 'left' ? '↑ sel' : 'sel ↓') : away === 'left' ? '← sel' : 'sel →'}</button
		>
	{/if}

	{#if tip}
		<div class="nowview-tip" role="status" style:left="{tip.x}px" style:top="{tip.y}px">
			<span class="what"
				><span class="field">{tip.field}</span>{#if tip.state}<span class="state"
						>{tip.state}</span
					>{/if}</span
			>
			{#if tip.when}<span class="when">{tip.when}</span>{/if}
		</div>
	{/if}
</div>

<style>
	/* The frame's content box, for the pane inside (`display: contents` in a
	   frame) and for the steps, the margin pills and the buttons over it. */
	.nowview {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		/* A finger still scrolls whatever this pane sits in, and two fingers
		   are the zoom's, not the browser's page zoom. On the WHOLE widget,
		   margin included: sideways is the page's to claim otherwise, and a
		   swipe from the margin is the pane's tab swipe — which the browser
		   would take first. */
		touch-action: pan-y;
	}

	/* The band is what takes the pointer, and so what declares the axis:
	   a finger still scrolls whatever this pane sits in, and two fingers
	   are the zoom's, not the browser's page zoom. */
	.nowview-band {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		/* The picture is drawn past its box either side (`--surround`,
		   da-61nm8): the band is the window onto it. */
		overflow: clip;
	}

	/* And so is the picture's holder: the pane's body scrolls, and a
	   picture slid right would hand it the slide to scroll to. */
	.nowview-band :global(.picture) {
		overflow: clip;
	}

	/* The gesture's preview: the band scaled about the anchor or the
	   fingers, which is therefore the one column of it that does not move
	   (but with the fingers), and slid by the pan — and, at rest, by a pan
	   that stayed inside the drawing. Width only — the window is a span of
	   time, and the field rows are not part of it. */
	.nowview-band :global(svg.svg) {
		transform: translateX(var(--pan)) scaleX(var(--zoom));
		transform-origin: var(--zoom-x) center;
	}

	/* TIME RUNS DOWN (da-78evn): the same widget turned on its side. The
	   band takes the vertical drag, the page keeps the sideways one, and
	   the preview, the wait's stem and the buttons follow the axis. */
	.nowview.vertical {
		touch-action: pan-x;
	}
	.vertical .nowview-band :global(svg.svg) {
		transform: translateY(var(--pan)) scaleY(var(--zoom));
		transform-origin: center var(--zoom-x);
	}

	/* And a mark keeps its size (da-6fdaf), as on the day strip (da-dqbso):
	   a block is as long as its minutes, and every other mark is scaled
	   back about its own anchor, so only its place moves. */
	.nowview-band :global(svg.svg :is(circle, text, use, polygon, rect.point-bg, line):not(defs *)) {
		transform-box: fill-box;
		transform-origin: center;
		scale: calc(1 / (var(--zoom) * var(--sx, 1))) 1;
	}
	.nowview-band :global(svg.svg text[text-anchor='end']:not(defs *)) {
		transform-origin: right;
	}
	.nowview-band :global(svg.svg text:not([text-anchor='middle']):not([text-anchor='end']):not(defs *)) {
		transform-origin: left;
	}
	.vertical .nowview-band :global(svg.svg :is(circle, text, use, polygon, rect.point-bg, line):not(defs *)) {
		scale: 1 calc(1 / (var(--zoom) * var(--sx, 1)));
	}
	/* But what stands at now is scaled back as one, about now (`unstretch`):
	   the strip, the dash under it, the line and the word keep their size
	   and their place beside now. */
	.nowview .nowview-band :global(svg.svg .at-now :is(circle, text, use, polygon, rect, line):not(defs *)) {
		scale: none;
	}

	.vertical .hold-mark.held .stem {
		left: -50vw;
		top: -0.75px;
		width: 100vw;
		height: 1.5px;
		background: repeating-linear-gradient(to right, #ffffff 0 3px, transparent 3px 6px);
	}
	.vertical .nowview-jump {
		top: auto;
		left: 50%;
		transform: translateX(-50%);
	}
	.vertical .nowview-jump.left {
		left: 50%;
		top: 4px;
	}
	.vertical .nowview-jump.selection.right {
		right: auto;
		bottom: 30px;
	}

	/* A strip above the band: one pill tall, plus the half-row the frame's
	   own titles hang into it (PR #43) — measured at 8px on both screens,
	   which is why the pill sits at the strip's BOTTOM and not its top. A
	   pill drawn up there is a pill with a tab strip over it, which is what
	   the phone showed. Its own room besides: a swipe here is the pane's,
	   not the band's, because nothing in it declares `data-pans`. */
	.nowview-margin {
		position: relative;
		flex: 0 0 auto;
		height: 1.75rem;
		/* The pill reads at the command line's size where the bar draws it;
		   here it is a label over a band and takes a smaller one. */
		--input-font: 0.7rem;
	}

	/* Centred on the mark it names, and held inside the strip (see `held`). */
	.margin-pill {
		position: absolute;
		bottom: 0;
		transform: translateX(-50%);
		max-width: 100%;
	}

	.nowview-handles {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.nowview-handles .handle {
		pointer-events: auto;
	}

	/* THE HANDLES (da-57vj4): a frame round each major label, the label's
	   own words showing through, so the axis reads as something to take
	   hold of. The target is at least a fingertip's height; a finger's
	   drag on it is the handle's, never the page's. Outlined in `--muted`,
	   a control at rest, and not the rules' `--rule`, so it reads as one
	   to grab (da-9hmqg, vividn: «a bit higher contrast so that it's more
	   clear they are clickable/draggable»). Its words are not text to
	   select, so a drag is always the handle's (da-byzc0, vividn: «right
	   now when trying to drag it often just selects the text»). */
	.handle {
		position: absolute;
		box-sizing: border-box;
		transform: translate(-50%, -50%);
		padding: 0 3px;
		border: 1px solid var(--muted);
		border-radius: 4px;
		background: none;
		color: transparent;
		font: inherit;
		font-size: 11px;
		line-height: 1;
		white-space: nowrap;
		cursor: grab;
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
		z-index: 2;
	}
	.handle:hover,
	.handle:focus-visible {
		border-color: var(--ink);
	}

	/* The wheel, the handle it is open on in the middle of it: its words
	   drawn here, over the picture's, and the notches stacked toward the
	   picture (up, or right when time runs down) for the longer steps and
	   away from it for the shorter, turned by `--turn` while a drag picks. */
	.nowview-wheel {
		position: fixed;
		width: 0;
		height: 0;
		z-index: 12;
		touch-action: none;
	}
	.nowview-wheel .handle.held.turned {
		visibility: hidden;
	}
	.nowview-wheel .handle.held {
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
		color: var(--ink);
		background: var(--ground);
		border-color: var(--accent, var(--ink));
		min-width: 2.6rem;
		height: 20px;
		left: 0;
		top: 0;
	}
	.notch {
		position: absolute;
		left: 0;
		top: 0;
		box-sizing: border-box;
		min-width: 2.6rem;
		height: 20px;
		padding: 0 4px;
		transform: translate(-50%, calc(-50% - var(--k) * 22px + var(--turn)));
		border: 1px solid var(--rule);
		border-radius: 4px;
		background: var(--ground);
		color: var(--ink);
		font: inherit;
		font-size: 11px;
		line-height: 1;
		white-space: nowrap;
		opacity: var(--fade);
		cursor: pointer;
	}
	.vertical .notch {
		transform: translate(calc(-50% + var(--k) * 44px - var(--turn)), -50%);
	}
	.notch:hover,
	.notch.picked {
		opacity: 1;
		border-color: var(--ink);
	}

	/* A handle dragged along the axis: its words under the finger while
	   the picture scales about the anchor beneath it. */
	.handle.dragged {
		position: fixed;
		height: 20px;
		display: flex;
		align-items: center;
		color: var(--ink);
		background: var(--ground);
		border-color: var(--ink);
		pointer-events: none;
		z-index: 12;
	}

	/* «now» off now (da-6csum): the yellow word under the now section,
	   filled — the CLI's now yellow, `NOW_LINE_COLOR` — with black words,
	   on the word it stands for. */
	.nowview-now {
		position: absolute;
		transform: translate(-50%, -50%);
		padding: 1px 6px;
		border: none;
		border-radius: 4px;
		background: #ffcc00;
		color: #000000;
		font: inherit;
		font-size: 0.7rem;
		font-weight: 600;
		line-height: 1.2;
		white-space: nowrap;
		cursor: pointer;
		z-index: 2;
	}

	/* Against the band's edge, on its middle: small, in the page's own ink
	   over its own ground, so it reads as a control and not as a mark of
	   the picture. */
	.nowview-jump {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		padding: 2px 6px;
		border: 1px solid var(--rule);
		border-radius: 6px;
		background: var(--ground);
		color: var(--ink);
		font: inherit;
		font-size: 0.7rem;
		line-height: 1.2;
		white-space: nowrap;
		cursor: pointer;
		z-index: 2;
	}

	.nowview-jump.right {
		right: 4px;
	}

	.nowview-jump.left {
		left: 4px;
	}

	/* The selection's button is white-edged, as the selection is; on the
	   right it stands clear of the now section, which owns that edge. */
	.nowview-jump.selection {
		border-color: #ffffff;
	}

	.nowview-jump.selection.right {
		right: 62px;
	}

	/* THE WAIT'S MARK (da-pzzoz): what the bar dropping at the long press used
	   to say, now that it commits to nothing.

	   A ring that closes over exactly `--long` — it is drawn the instant
	   the finger lands and finishes as the press becomes a HOLD, so the
	   wait is something the thumb can WATCH rather than something it finds
	   out about at the end. Held, the ring fills and a dashed stem runs
	   through the band at the press: the selection's own dashes
	   (#lib/hover.ts), because what a drag from here makes is a range, and
	   the mark should look like the thing it is about to become. */
	.hold-mark {
		position: absolute;
		width: 0;
		height: 0;
		pointer-events: none;
		z-index: 3;
	}

	.hold-mark .ring {
		position: absolute;
		left: -11px;
		top: -11px;
		width: 22px;
		height: 22px;
		box-sizing: border-box;
		border: 1.5px solid #ffffff;
		border-radius: 50%;
		opacity: 0;
		animation: hold-wait var(--long) ease-out forwards;
	}

	@keyframes hold-wait {
		from {
			transform: scale(1.9);
			opacity: 0;
		}
		to {
			transform: scale(1);
			opacity: 0.8;
		}
	}

	.hold-mark.held .ring {
		animation: none;
		opacity: 1;
		background: rgba(255, 255, 255, 0.25);
	}

	/* Only once the wait is up: before that the ring alone is the whole of
	   it, so a plain tap — which lifts long before 400ms — leaves no line
	   across the band behind it. */
	.hold-mark .stem {
		display: none;
	}

	.hold-mark.held .stem {
		display: block;
		position: absolute;
		left: -0.75px;
		top: -50vh;
		width: 1.5px;
		height: 100vh;
		background: repeating-linear-gradient(
			to bottom,
			#ffffff 0 3px,
			transparent 3px 6px
		);
		opacity: 0.8;
	}

	/* A person's own reading of "the wait is up" is the ring; a person who
	   has asked for less motion gets it without the sweep. */
	@media (prefers-reduced-motion: reduce) {
		.hold-mark .ring {
			animation: none;
			opacity: 0.8;
		}
	}

	/* THE SELECTED ENTRY'S TOOLTIP (da-pzzoz) — vividn: «surface some bare
	   info about it in a floating tooltip (start and end time as well as
	   field and state». Fixed like the picture's hover label, and for the
	   same reason: a band clips its picture, and a tooltip inside it would
	   be clipped with it. `pointer-events: none` — the band under it keeps
	   every gesture. */
	.nowview-tip {
		position: fixed;
		z-index: 11;
		transform: translateX(-50%);
		display: flex;
		gap: 0.5em;
		align-items: baseline;
		max-width: calc(100vw - 16px);
		box-sizing: border-box;
		padding: 2px 8px;
		border: 1px solid #ffffff;
		border-radius: 6px;
		background: var(--ground);
		color: var(--ink);
		font-size: 0.72rem;
		line-height: 1.4;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		pointer-events: none;
	}

	.nowview-tip .field {
		font-weight: 600;
	}

	.nowview-tip .state::before {
		content: ': ';
		font-weight: 400;
		color: var(--muted);
	}

	.nowview-tip .when {
		color: var(--muted);
	}
</style>
