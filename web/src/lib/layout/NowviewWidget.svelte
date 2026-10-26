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
	is the widget's setting, handed out with `change` — and since da-vhwq
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
	(`dependsOnNow`, $lib/panes.ts).
	Every instant read off the band below is read as the picture is drawn
	NOW, slid by `slid`.

	THE BAND IS A PLACE YOU POINT INTO (da-rskp). Its gestures live over a
	pane that swipes between tabs and a page that scrolls, so the state
	machine is written down in `$lib/nowpoint.ts` and this component only
	runs it. vividn's table, 2026-10-22 (da-xg1vi), the dayview's too:

	  tap on a mark           → INSPECT: the entry behind it, as an hid pill
	  tap it again            → the SELECTION there instead (vividn, 15:04)
	  tap on ground           → a SELECTION where it fell; the window does not move
	  touch the time axis     → SCRUB: the time under the finger, following it
	    and drag                to the lift — not snapped, and no pan
	  drag sideways           → PAN in time; the selection does not move
	  long press, then drag   → a RANGE
	  long press, then LIFT   → the same SELECTION, a mark or not
	  right click             → the same selection, on a desk
	  shift+drag (desktop)    → the same range; a long press with a mouse is
	                            nobody's habit (MY reading, not vividn's words)
	  long press in the NOW   → a duration ending NOW — the sentence «I have been
	    region, then drag left  doing this for the last N minutes»
	  two fingers / ctrl+wheel→ the window's LENGTH (da-5goc), untouched here

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
	comment — a mark does not carry one, and see $lib/inspect.ts.

	What a tap MEANS depends on where the window is. On now the axis is
	RELATIVE, so the selection is too: the −5 / −10 / −15 / −20 gridlines
	spell themselves `-q` … `-qqqq` and everywhere else is the relative
	`-t -7`, and the white line stays on its gridline as the minutes pass
	(`$lib/cursor.ts`). Panned away the axis is absolute, and so is the
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

	THE NOW SECTION (da-6csum) is the CLI's strip of each field's current
	state, and it stands where now is: at now while the window holds now,
	at the right edge of a window in the past and pinned to the left of one
	wholly in the future, a gap between it and the timeline either side.
	vividn: «any time the view is panned off the default, the "now" label
	under the now section becomes a filled in yellow button with black now
	written on it. Clicking that button restores the default.» So off now
	the yellow «now» under the strip is a BUTTON laid over it, which takes
	the window back AND the time-based pills away with it — and keeps the
	window's length: «stretching should always leave now where it is», and
	a stretch alone does not leave now. `← selection` appears when a
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
	import { createEventDispatcher, onDestroy, onMount, tick } from 'svelte';
	import CliPane from '$lib/CliPane.svelte';
	import Pill from '$lib/Pill.svelte';
	import { pills, selected } from '$lib/bar.js';
	import { clockLabel } from '$lib/clock.js';
	import { cursor, SPAN_ID, pointAt, pointed, type Cursor } from '$lib/cursor.js';
	import { inspectMark, instantOn, msPerPx, selectedTip, timeAxis } from '$lib/hover.js';
	import { markOf, pressSpent } from '$lib/inspect.js';
	import { readLanes } from '$lib/lanes.js';
	import { advance, LONG_MS, SLOP_PX, snapTo, tapStep, undecided, type Mode, type Press } from '$lib/nowpoint.js';
	import { historyArg, ladderAround, NOWVIEW_HISTORY, nowviewSpec, stepMinutes, zoomHistory } from '$lib/panes.js';

	/** How far back the window reaches: a step of NOWVIEW_STEPS, or any length a handle's drag left it at. */
	export let history = NOWVIEW_HISTORY;
	/** The instant the window is centred on (`-t`); none is now. */
	export let at: string | undefined = undefined;
	/**
	 * Time runs down the page (da-78evn). Every gesture below is measured
	 * ALONG the time axis, which the picture itself says (`axis.y`); this
	 * asks for the picture, and turns the chrome around it.
	 */
	export let vertical = false;
	export let epoch = 0;
	/** Whether the window reaches back at all: off is the snapshot (da-hklaz). */
	export let showHistory = true;

	const dispatch = createEventDispatcher<{ change: string }>();

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
	let centre: { arg: string; by: 'sel' | 'pan' } | null = at ? { arg: at, by: 'pan' } : null;

	/** The window in minutes; a history datum would not read is the default's. */
	$: minutes = stepMinutes(history) > 0 ? stepMinutes(history) : stepMinutes(NOWVIEW_HISTORY);
	$: instant = centre?.arg;
	/** How finely a tap here says its time, in seconds. */
	$: grain = tapStep(minutes);

	/**
	 * The `-t` the picture on screen was asked for, which is the window's
	 * only until a pan moves it inside what that picture drew (da-61nm8):
	 * the picture is drawn a window's worth past its box either side, and
	 * a pan that stays in it slides the picture along (`rest`) and asks
	 * datum for nothing. Everything else that moves the window asks.
	 */
	let askedFor = instant;
	$: spec = nowviewSpec(showHistory ? history : '0', askedFor, vertical);

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

	function ask(..._because: unknown[]): void {
		const was = askedFor;
		askedFor = centre?.arg;
		stale = askedFor !== was ? svgEl() : null;
		// The picture on screen may already be this one, slid: slid back.
		void tick().then(place);
	}

	// A new step or orientation is a new picture, of where the window is.
	$: ask(history, vertical, showHistory);
	/** The axis a drag along the band travels, for the pane's tab swipe. */
	$: pans = vertical ? 'y' : 'x';
	/** Whether the window has left now — and so whether a tap makes an absolute pill. */
	$: offNow = !!instant;

	// A selection made somewhere ELSE points this window; one made here does
	// not, and neither does a RELATIVE one — that is measured from now, and
	// the window it was dropped on is the one on now. A selection CLEARED
	// takes back only the window it was pointing.
	//
	// A subscription rather than a `$:`, because this ASSIGNS the window and
	// a reactive statement that both reads and writes one variable re-runs
	// itself for ever.
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

	/** Back to now, and the time-based pills — and so the selection — with it. */
	function toNow(): void {
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
	let rest = 0;

	/**
	 * The window's middle in TIME — the instant it is centred on, or half
	 * of it back from now — and the window about a middle. Never read off
	 * the band's box, which holds the now strip as well as the window: at
	 * the window's end, its start or at now inside it (da-6csum), so the
	 * box's middle is the window's only where the strip happens to balance.
	 * NaN for a hand-written `at` that is no instant the page can read.
	 */
	const middleOf = (at: string | undefined) => (at ? Date.parse(at) : nowMs() - minutes * 30_000);
	const windowAbout = (middle: number): [number, number] => [middle - minutes * 30_000, middle + minutes * 30_000];

	/** The instant a point on screen is, by the picture's own geometry, as it is slid now. */
	const instantAt = (x: number, y: number) => (axis ? instantOn(axis, along(x, y)) + slid : NaN);

	/**
	 * The window's own idea of NOW: where its span ends while it is on now —
	 * the now-line's instant, as far on as the picture has flowed.
	 */
	const nowMs = () => (offNow || !axis ? Date.now() : (drawn ? drawn.now + slid : axis.to));

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
	let refills = 0;
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
	$: offNow, flow();

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
	let mode: Mode = 'press';
	/** Where the pointer is, for the timer that has no event of its own. */
	let last = { x: 0, y: 0 };
	/** Whether the press has lasted long enough to be a range. */
	let long = false;
	/** How far the band is dragged while a pan is in hand, in px. */
	let panned = 0;
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
	let waiting: { x: number; y: number } | null = null;

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
		if (mode === 'pan') panned = along(x, y) - along(press.x, press.y);
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
		const at = axis ? instantOn(axis, snapTo(along(x, y), gridlines())) + slid : null;
		if (at !== null) pointAt(selection(at));
		lanesAt(x, y, at);
	}

	/** The fields drawn at this height, for the fields widget to put first. */
	function lanesAt(x: number, y: number, at: number | null): void {
		// The lanes the press crossed, for the fields widget to answer
		// (da-qu16, $lib/lanes.ts): the band takes the pointer itself, so
		// nothing else on the page would hear the press.
		//
		// AFTER the selection, and not before it: a selection cleared
		// clears the lanes, and a tap at now IS a cleared selection
		// ($lib/cursor.ts) — yet it still re-orders the fields by its
		// height. The snapshot has no axis and so selects no time at all;
		// there the height is the whole of what a press says (vividn, 17:01).
		const svg = svgEl();
		if (svg) readLanes(svg, { x, y, t: at });
	}

	/**
	 * The window the pan has dragged it to: the drag moves the content, so
	 * the span moves the other way. The picture stays where the finger left
	 * it, and datum is asked for another only when the window has left what
	 * this one drew (da-61nm8) — or when either window, the one the pan left
	 * or the one it reached, does not hold now, a pan from one past window
	 * to another included. The now strip stands at now in a window holding
	 * now, and slides with it; in one that does not it is pinned to an edge,
	 * which a slide would carry off (da-6csum).
	 */
	function commitPan(): void {
		if (!axis) return drop();
		const was = middleOf(instant);
		const [lo, hi] = Number.isFinite(was)
			? windowAbout(was - panned * msPerPx(axis))
			: ends().map((p) => instantOn(axis!, p - panned) + slid);
		centre = { arg: new Date((lo + hi) / 2).toISOString(), by: 'pan' };
		rest += panned;
		panned = 0;
		if (lo < drawnFrom() || hi > drawnTo() || !holdsNow([lo, hi]) || (view && !holdsNow(view))) ask();
		else void tick().then(place);
	}

	/** Whether now is inside a window's two ends, a second's slack either side. */
	const holdsNow = ([lo, hi]: [number, number]) => lo - EDGE_MS <= Date.now() && Date.now() <= hi + EDGE_MS;

	function finish(event: PointerEvent): void {
		if (!press) return;
		const still = Math.hypot(event.clientX - press.x, event.clientY - press.y) <= SLOP_PX;
		if (mode === 'pan') commitPan();
		// A still lift is a TAP (da-xg1vi): the entry under it, or the time
		// where there is none. A still LONG one is the time wherever it
		// lands. One that traveled is a scroll the browser took, and points
		// nowhere.
		else if (!pressSpent() && still && undecided(mode)) {
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

	/** The gesture in hand: what the band is scaled by, about which x. */
	let zoom: { origin: number; scale: number } | null = null;

	/**
	 * The gesture stretches TIME, never a mark (da-6fdaf): a dot, a glyph
	 * or a label is scaled back by the CSS below, and a multi-state block's
	 * stripes here, on the pattern's own transform, as the day strip's are
	 * (da-hmtoq, DayStrip.svelte — WebKit draws a CSS `transform` on a
	 * pattern without it). On each step of the gesture, and on each picture
	 * `place` reads: a ctrl+wheel holds no press, so a picture may land
	 * under it (one under a finger waits for the lift, $lib/hover.ts).
	 */
	function unstretch(): void {
		const back = 1 / (zoom?.scale || 1);
		for (const pattern of el?.querySelectorAll<SVGPatternElement>('.nowview-band svg.svg pattern') ?? []) {
			pattern.dataset.drawn ??= pattern.getAttribute('patternTransform') ?? '';
			pattern.setAttribute('patternTransform', `scale(${vertical ? `1 ${back}` : `${back} 1`}) ${pattern.dataset.drawn}`);
		}
	}
	$: zoom, vertical, unstretch();

	/**
	 * Where the anchor is drawn, in px across the picture's own box.
	 *
	 * A window with `-t` is centred on its instant (da-5flt: the yellow
	 * line there is now, wherever now falls, not the instant), drawn where
	 * the picture puts it — the middle of the window, which the now strip
	 * beside it moves off the box's (da-6csum). One without
	 * ends at now, which datum draws as the now-line (`current-time-line`,
	 * dayview_svg.rs) wherever its geometry puts it; the snapshot draws
	 * none, and its anchor is the right edge.
	 */
	function anchorX(svg: Element): number {
		const box = svg.getBoundingClientRect();
		const start = axis?.y ? box.y : box.x;
		const line = svg.querySelector('.current-time-line');
		const [lo, hi] = ends();
		const at = Date.parse(instant ?? '');
		if (instant && axis && Number.isFinite(at)) return axis.lo + (at - axis.from) / msPerPx(axis) - start;
		if (instant || !line) return (instant ? (lo + hi) / 2 : hi) - start;
		const marker = line.getBoundingClientRect();
		return (axis?.y ? marker.y + marker.height / 2 : marker.x + marker.width / 2) - start;
	}

	/** Begin a gesture over the picture, if there is one to scale. */
	function start(): boolean {
		const svg = svgEl();
		if (!svg) return false;
		zoom = { origin: anchorX(svg), scale: 1 };
		return true;
	}

	/**
	 * End the gesture: round the scale to a step and let go of the band.
	 *
	 * The scale is dropped before the answer arrives, so the picture on
	 * screen is datum's own from here on — the transform is the gesture's
	 * preview of a window, not a substitute for one.
	 */
	function commit(): void {
		const scale = zoom?.scale ?? 1;
		zoom = null;
		const next = zoomHistory(history, scale);
		if (next !== history) dispatch('change', next);
	}

	// ── two fingers ────────────────────────────────────────────────────

	/** The fingers down on the picture, by pointer id. */
	const fingers = new Map<number, { x: number; y: number }>();
	/** How far apart they were when the pinch began. */
	let apart = 0;

	const spread = () => {
		const [a, b] = [...fingers.values()];
		return Math.hypot(a.x - b.x, a.y - b.y);
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
		listen(true);
		if (event.pointerType === 'mouse') return begin(event);
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (fingers.size === 1) return begin(event);
		// A second finger is a pinch, and the first finger's gesture was
		// never one: whatever it had become is dropped before the scale
		// starts, so nothing of it is committed on the way out.
		drop();
		if (fingers.size !== 2) return;
		apart = spread();
		if (apart > 0) start();
	}

	function onPointerMove(event: PointerEvent): void {
		if (fingers.has(event.pointerId)) fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (zoom && fingers.size === 2) zoom = { ...zoom, scale: spread() / apart };
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

	function onWheel(event: WheelEvent): void {
		if (!event.ctrlKey) return;
		event.preventDefault();
		if (!zoom) start();
		const held = zoom;
		if (!held) return;
		zoom = { ...held, scale: held.scale * Math.pow(2, -event.deltaY / NOTCH) };
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
	let roomy = true;

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
	let handles: Handle[] = [];
	/** The widget's box on screen, for the wheel, which is drawn `fixed` beside it. */
	let boxAt = { x: 0, y: 0 };

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
	$: handles, clearFar();

	/** The instants at the window's two ends, as the picture on screen draws them. */
	let view: [number, number] | null = null;

	/**
	 * How far to slide the picture, along its UNMOVED axis, so the window is
	 * where `centre` says when the picture on screen was asked for
	 * elsewhere — after a pan inside it, and at each redraw of it, so an
	 * on-now picture that moves with the clock does not take a panned
	 * window along.
	 */
	function restFor(unmoved: NonNullable<typeof axis>): number {
		const target = Date.parse(instant ?? '');
		if (instant === askedFor || !Number.isFinite(target)) return 0;
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
		const moved = svg.style.transform;
		svg.style.transform = 'none';
		try {
			return timeAxis(svg, Date.parse(askedFor ?? ''));
		} finally {
			svg.style.transform = moved;
		}
	}

	/**
	 * Where each margin pill sits, in px across the widget's box: over the
	 * mark the selection was drawn as.
	 *
	 * Read off the DRAWN marks (`$lib/hover.ts` puts the arrow at the
	 * selection and the band over the range), so a pill in the margin and
	 * the line under it cannot disagree by a pixel — including for a
	 * relative selection, which is placed against the picture's own
	 * now-line and not against the clock.
	 */
	let spots: { time: number | null; dur: number | null } = { time: null, dur: null };

	/**
	 * Where the picture writes «now» under its now section, in px across
	 * and down the widget's box: off now the button takes its place
	 * (da-6csum), and the words under it are hidden. None when the strip
	 * is too narrow for datum to write them.
	 */
	let nowAt: { x: number; y: number } | null = null;

	function place(): void {
		if (!el) return;
		unstretch();
		const home = el.getBoundingClientRect();
		roomy = home.width >= TINY_PX;
		boxAt = { x: home.x, y: home.y };
		const svg = svgEl();
		// Read unmoved, then slid by `rest` alone: a redraw mid-gesture must
		// not cache the gesture's own offset, which `commitPan` adds again.
		const unmoved = svg && unmovedAxis(svg);
		const moved = unmoved && svg !== stale ? restFor(unmoved) : rest;
		axis = unmoved && { ...unmoved, lo: unmoved.lo + moved, hi: unmoved.hi + moved };
		const stretches = svg?.querySelectorAll('[data-lo]') ?? [];
		const edge = (el: Element | undefined, name: string) => Date.parse(el?.getAttribute(name) ?? '');
		spanned = [edge(stretches[0], 'data-from'), edge(stretches[stretches.length - 1], 'data-to')];
		// What is read off the picture below is read where it will be drawn.
		if (Math.abs(moved - rest) >= 0.5) {
			rest = moved;
			return void tick().then(place);
		}
		drawn = readDrawn(svg);
		flow();
		const about = middleOf(instant);
		view = !axis
			? null
			: Number.isFinite(about)
				? windowAbout(about)
				: (ends().map((p) => instantOn(axis!, p) + slid) as [number, number]);
		// The window has left what the picture drew — the clock moved an
		// on-now picture out from under it: a picture of the window, then.
		// Asked once: `ask` makes `askedFor` the window's, and from then on
		// this `place` and every one after it has nothing left to ask for.
		if (view && axis && instant !== askedFor && (view[0] < drawnFrom() - EDGE_MS || view[1] > drawnTo() + EDGE_MS)) ask();
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
		const word = svg?.querySelector<SVGElement>('.now-label');
		const at = word?.getBoundingClientRect();
		nowAt = at?.width ? { x: at.left + at.width / 2 - home.x, y: at.top + at.height / 2 - home.y } : null;
		if (word) word.style.visibility = offNow ? 'hidden' : '';
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
	let wheel: { minutes: number; text: string; sign: string; x: number; y: number; pick: number } | null = null;
	let wheelEl: HTMLDivElement | undefined;

	$: around = wheel ? ladderAround(wheel.minutes, minutes) : { up: [], down: [] };
	/** The wheel's notches as drawn: how many steps from the handle (up is longer), and the step. */
	$: notches = [
		...around.up.slice(0, SHOWN).map((step, k) => ({ k: k + 1, step })),
		...around.down.slice(0, SHOWN).map((step, k) => ({ k: -(k + 1), step }))
	];

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
		if (next !== history) dispatch('change', next);
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
	} | null = null;
	/** Where a scaling handle is drawn, along the axis on screen. */
	let dragged = 0;

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
			// under the finger; held to a window the ladder's ends allow.
			const s = (finger - grip.anchor) / (grip.at - grip.anchor);
			const bound = Math.min(Math.max(s, minutes / stepMinutes('12h')), minutes / stepMinutes('30s'));
			zoom = { ...zoom, scale: bound };
			dragged = grip.anchor + (grip.at - grip.anchor) * bound;
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
		if (was.mode === 'scale') {
			const scale = zoom?.scale ?? 1;
			zoom = null;
			const next = historyArg(minutes / scale);
			if (next !== history) dispatch('change', next);
		} else if (was.mode === 'wheel' && wheel) {
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
	$: marginPills = $pills.filter((p) => p.id === 'cursor' || p.id === SPAN_ID);

	// ── the tooltip on a selected entry ────────────────────────────────
	//
	// vividn: «maybe when something is selected in the nowview, it could also
	// surface some bare info about it in a floating tooltip (start and end
	// time as well as field and state + comment (maybe?)».
	//
	// Whatever SELECTED it: a tap here, a row tapped
	// in the tail. The picture is asked which of its marks name the
	// selected entries (`selectedTip`, $lib/hover.ts) and the tooltip
	// floats over them, so a selection this window does not draw shows
	// nothing — «and nowview if that particular piece of data is
	// represented there».
	//
	// The COMMENT is left out, and $lib/inspect.ts says why: a mark carries
	// its field, its state and its instants and nothing else, so a comment
	// would be a whole `datum get` per gesture.

	/** What the tooltip says and where it floats, in the viewport's own px. */
	let tip: { field: string; state: string; when: string; x: number; y: number } | null = null;

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
	$: readTip($selected);

	/**
	 * Which way a selection the window does not contain lies.
	 *
	 * Off the cached axis rather than off the drawn marks: a selection
	 * outside the window draws nothing at all, so there would be no mark
	 * to read, and the button has to appear the moment the window leaves it.
	 */
	$: away =
		!view || !$pointed
			? null
			: +$pointed.at < view[0] - EDGE_MS
				? 'left'
				: +$pointed.at > view[1] + EDGE_MS
					? 'right'
					: null;

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
	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div
		class="nowview-band"
		style:--zoom={zoom?.scale ?? 1}
		style:--zoom-x="{zoom?.origin ?? 0}px"
		style:--pan="{rest + panned}px"
		data-pans={pans}
		data-points="own"
		data-mode={mode}
		on:pointerdown={onPointerDown}
		on:contextmenu={onContextMenu}
		on:touchmove|nonpassive={onTouchMove}
		on:wheel|nonpassive={onWheel}
	>
		<!-- `refills` rides on the epoch: a bump re-runs the command, as a
		     write's does, once half of what was drawn ahead is spent. -->
		<CliPane {spec} epoch={epoch + refills} />
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
					on:pointerenter={(e) => e.pointerType === 'mouse' && open(h)}
					on:pointerdown={(e) => grab(e, h)}
					on:focus={() => open(h)}
					on:blur={blur}
					on:keydown={onWheelKey}>{h.text}</button
				>
			{/each}
		</div>
	{/if}

	<!-- The wheel: the ladder's steps either side of the handle it is open
	     on, longer toward the picture, the next one clear and the rest
	     fading. `fixed`, so a pane's edge does not cut its notches off. -->
	{#if wheel && !zoom}
		<!-- svelte-ignore a11y-no-static-element-interactions -->
		<div
			class="nowview-wheel"
			data-pans={pans}
			bind:this={wheelEl}
			style:left="{boxAt.x + wheel.x}px"
			style:top="{boxAt.y + wheel.y}px"
			style:--turn="{wheel.pick * (vertical ? NOTCH_PX.y : NOTCH_PX.x)}px"
			on:pointerleave={leave}
			on:focusout={blur}
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
					on:click={() => stepTo(n.step)}>{wheel.sign}{n.step}</button
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
	{#if offNow && nowAt && !zoom}
		<button
			type="button"
			class="nowview-now"
			data-pans={pans}
			title="back to now, and the time pills away"
			style:left="{nowAt.x + (vertical ? 0 : panned)}px"
			style:top="{nowAt.y + (vertical ? panned : 0)}px"
			on:click={toNow}>now</button
		>
	{/if}
	{#if roomy && away && !zoom}
		<button
			type="button"
			class="nowview-jump {away} selection"
			data-pans={pans}
			title="the window onto the selection"
			on:click={toSelection}
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

	/* The gesture's preview: the band scaled about the anchor, which is
	   therefore the one column of it that does not move, and slid by the
	   pan — and, at rest, by a pan that stayed inside the drawing. Width
	   only — the window is a span of time, and the field rows are not
	   part of it. */
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
		scale: calc(1 / var(--zoom)) 1;
	}
	.nowview-band :global(svg.svg text[text-anchor='end']:not(defs *)) {
		transform-origin: right;
	}
	.nowview-band :global(svg.svg text:not([text-anchor='middle']):not([text-anchor='end']):not(defs *)) {
		transform-origin: left;
	}
	.vertical .nowview-band :global(svg.svg :is(circle, text, use, polygon, rect.point-bg, line):not(defs *)) {
		scale: 1 calc(1 / var(--zoom));
	}
	/* But the dash under the now strip runs along time, as long as the
	   strip: it stretches with it. */
	.nowview .nowview-band :global(svg.svg line.current-time-line-far:not(defs *)) {
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
	   ($lib/hover.ts), because what a drag from here makes is a range, and
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
