<!--
	The nowview widget (da-pcis): datum's nowview picture through a CliPane,
	and a tiny − / + beside the axis's far label (da-ndzl) stepping how far
	back the picture reaches — down to 30s, halving past 5m through the 2m
	and 1m a game needs, and never to the snapshot of each field's state,
	which is `showHistory` turned off in the widget's menu (da-hklaz).
	The window's size is written nowhere: each button's title names the step
	it goes to (da-rex1). The step is the widget's setting, handed out with
	`change` — and since da-vhwq that lands in THIS SESSION, not the tree:
	pressing − under a LOCKED layout used to move `history` in the tree and
	light the layout's dot, which is the defect that bead was written
	against. «Save options with layout» is what hands the step to the
	layout, and only when the reader chooses it.

	The snapshot draws no axis, so it has no steps: nothing lies over its
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

	`now →` sits at the band's right edge whenever the window is not on now,
	and takes the window back AND the time-based pills away with it. Its
	mirror `← selection` appears when a selection exists that the window
	does not contain (vividn's second «maybe»).

	The band takes sideways motion of its own, so it says so with
	`data-pans="x"` (da-kcmp): a one-finger drag across it is never a swipe
	between the pane's tabs, whatever the pane it is in.

	The steps, the buttons and the margin pills are the WIDGET's, not the
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
	import { NOWVIEW_HISTORY, NOWVIEW_STEPS, nowviewSpec, stepMinutes, zoomHistory } from '$lib/panes.js';

	/** How far back the window reaches: one of NOWVIEW_STEPS. */
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

	/**
	 * Where the window is centred, and WHY — `null` is on now.
	 *
	 * The why is the difference between the two ways a window leaves now.
	 * `sel` means it is following the page's selection: a click on the
	 * dayview put it there, and clearing that selection brings it home. `pan`
	 * means a hand moved it, and then nothing brings it home but `now →` —
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

	// A history off the list (a hand-written tree) steps from the default.
	$: i = NOWVIEW_STEPS.indexOf(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	$: up = NOWVIEW_STEPS[i + 1];
	$: down = NOWVIEW_STEPS[i - 1];
	$: instant = centre?.arg;
	/** How finely a tap here says its time, in seconds. */
	$: grain = tapStep(stepMinutes(NOWVIEW_STEPS[i]));

	/**
	 * The `-t` the picture on screen was asked for, which is the window's
	 * only until a pan moves it inside what that picture drew (da-61nm8):
	 * the picture is drawn a window's worth past its box either side, and
	 * a pan that stays in it slides the picture along (`rest`) and asks
	 * datum for nothing. Everything else that moves the window asks.
	 */
	let askedFor = instant;
	$: spec = nowviewSpec(showHistory ? history : '0', askedFor, vertical);

	function ask(..._because: unknown[]): void {
		askedFor = centre?.arg;
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
	// says only `now →` brings a hand-moved window home, and now it is
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

	let el: HTMLDivElement | undefined;

	const svgEl = () => el?.querySelector<SVGSVGElement>('svg.svg') ?? null;

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

	/** The instant a point on screen is, by the picture's own geometry, as it is slid now. */
	const instantAt = (x: number, y: number) => (axis ? instantOn(axis, along(x, y)) + slid : NaN);

	/**
	 * The window's own idea of NOW: where its span ends while it is on now —
	 * the now-line's instant, as far on as the picture has flowed.
	 */
	const nowMs = () => (offNow || !axis ? Date.now() : (drawn ? drawn.now + slid : axis.to));

	/** The instant the picture on screen stops drawing what is known: its now, drawn ahead. */
	const drawnTo = () => drawn?.now ?? axis?.to ?? NaN;

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
	 * this one drew (da-61nm8).
	 */
	function commitPan(): void {
		if (!axis) return drop();
		const [lo, hi] = ends().map((p) => instantOn(axis!, p - panned) + slid);
		centre = { arg: new Date((lo + hi) / 2).toISOString(), by: 'pan' };
		rest += panned;
		panned = 0;
		if (lo < axis.from || hi > drawnTo()) ask();
		else void tick().then(place);
	}

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
	 * Where the anchor is drawn, in px across the picture's own box.
	 *
	 * A window with `-t` is centred on its instant (da-5flt: the yellow
	 * line there is now, wherever now falls, not the instant). One without
	 * ends at now, which datum draws as the now-line (`current-time-line`,
	 * dayview_svg.rs) wherever its geometry puts it; the snapshot draws
	 * none, and its anchor is the right edge.
	 */
	function anchorX(svg: Element): number {
		const box = svg.getBoundingClientRect();
		const start = axis?.y ? box.y : box.x;
		const line = svg.querySelector('.current-time-line');
		const [lo, hi] = ends();
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
	 * step of the ladder and the wheel steps as the buttons do. A trackpad
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

	// ── the steps, the margin and the buttons ──────────────────────────

	/** What one step takes along the axis, beside the far label: its target and a 2px gap. */
	const STEP_PX = { x: 22 + 2, y: 24 + 2 };

	/**
	 * Below this width the margin strip, its pills and `now →` do not fit,
	 * and the widget shows the LINES alone.
	 *
	 * The tiny nowview in `mobile` is ~100px wide (measured on the built
	 * app): `now →` is 46px there and the shortest time pill — `-qq`, two
	 * sections of nothing — is 54px, so the two together already overflow
	 * it, and a pill near the instant it names has no room to be near
	 * anything. 200px fits `now →` beside a `-t 14:30` pill with a third of
	 * the band still legible. vividn agreed, 17:01.
	 */
	const TINY_PX = 200;

	/** Whether this nowview is wide enough for the margin and the buttons. */
	let roomy = true;

	/**
	 * Where the far label is, in px across and down the widget's box, and
	 * how long it is along the axis: the first tick label the window shows,
	 * which is how far back it reaches (`-30m`, or `-15m` with the window
	 * centred on the cursor). The + goes before it and the − after (vividn:
	 * «move the + to the left side of the -nnm number»), so it is moved in
	 * off the window's edge, where the surroundings put it, by a step.
	 * Read off the picture as `anchorX` reads the now-line; none when it
	 * draws no axis — the snapshot. A nearer label the steps would lie over
	 * is not drawn: the far one says the window.
	 */
	let far: { x: number; y: number; length: number } | null = null;

	/**
	 * Hide each clock time (`.minute-label`) within a step of the far label,
	 * as `place` hides a nearer tick label; it is never the far label. They
	 * slide with the flow, so this runs at each slide too (da-to5n4).
	 */
	function clearFar(): void {
		const home = el?.getBoundingClientRect();
		const y = axis?.y;
		const step = y ? STEP_PX.y : STEP_PX.x;
		const from = far && home ? (y ? far.y + home.y : far.x + home.x) : NaN;
		const to = from + (far?.length ?? 0);
		for (const l of el?.querySelectorAll<SVGElement>('svg.svg .minute-label') ?? []) {
			const r = l.getBoundingClientRect();
			const [a, b] = y ? [r.top, r.bottom] : [r.left, r.right];
			l.style.visibility = a < to + step && b > from - step ? 'hidden' : '';
		}
	}
	$: far, clearFar();

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
		const [lo, hi] = ends();
		return (lo + hi) / 2 - (unmoved.lo + (target - unmoved.from) / msPerPx(unmoved));
	}

	/** The picture's axis as it is drawn before any slide or gesture moves it. */
	function unmovedAxis(svg: SVGSVGElement): typeof axis {
		const moved = svg.style.transform;
		svg.style.transform = 'none';
		try {
			return timeAxis(svg);
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

	function place(): void {
		if (!el) return;
		const home = el.getBoundingClientRect();
		roomy = home.width >= TINY_PX;
		const svg = svgEl();
		// Read unmoved, then slid by `rest` alone: a redraw mid-gesture must
		// not cache the gesture's own offset, which `commitPan` adds again.
		const unmoved = svg && unmovedAxis(svg);
		const moved = unmoved ? restFor(unmoved) : rest;
		axis = unmoved && { ...unmoved, lo: unmoved.lo + moved, hi: unmoved.hi + moved };
		// What is read off the picture below is read where it will be drawn.
		if (Math.abs(moved - rest) >= 0.5) {
			rest = moved;
			return void tick().then(place);
		}
		drawn = readDrawn(svg);
		flow();
		view = axis ? (ends().map((p) => instantOn(axis!, p) + slid) as [number, number]) : null;
		// The window has left what the picture drew — the clock moved an
		// on-now picture out from under it: a picture of the window, then.
		// Asked once: `ask` makes `askedFor` the window's, and from then on
		// this `place` and every one after it has nothing left to ask for.
		if (view && axis && instant !== askedFor && (view[0] < axis.from - EDGE_MS || view[1] > drawnTo() + EDGE_MS)) ask();
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
		const labels = [...(svg?.querySelectorAll<SVGElement>('.tick-label') ?? [])];
		for (const l of labels) {
			l.removeAttribute('transform');
			l.style.visibility = '';
		}
		const ctm = svg?.getScreenCTM();
		if (!labels.length || !axis || !ctm) return void (far = null);
		// The first along time: leftmost, or topmost when time runs down —
		// and then the steps go above and below it, in the axis strip.
		const y = axis.y;
		const box = (l: SVGElement) => l.getBoundingClientRect();
		const span = (r: DOMRect) => (y ? [r.top, r.bottom] : [r.left, r.right]);
		const [lo] = ends();
		const shown = labels.filter((l) => {
			const [a, b] = span(box(l));
			return (a + b) / 2 >= lo - 1;
		});
		if (!shown.length) return void (far = null);
		const label = shown.reduce((a, b) => (span(box(b))[0] < span(box(a))[0] ? b : a));
		const step = y ? STEP_PX.y : STEP_PX.x;
		const inset = Math.max(0, lo + step - span(box(label))[0]) / (y ? ctm.d : ctm.a);
		if (inset) label.setAttribute('transform', y ? `translate(0 ${inset})` : `translate(${inset} 0)`);
		const r = box(label);
		const [from, to] = span(r);
		for (const l of labels) {
			const [a, b] = span(box(l));
			if (l !== label && a < to + step && b > from - step) l.style.visibility = 'hidden';
		}
		far = y
			? { x: r.left + r.width / 2 - home.x, y: from - home.y, length: to - from }
			: { x: from - home.x, y: r.top + r.height / 2 - home.y, length: to - from };
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
	// A second of slack either side: a range that ends AT now sits on the
	// window's own edge, and rounding must not read that as «off it».
	const EDGE_MS = 1000;

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
		return () => {
			redrawn.disconnect();
			resized.disconnect();
		};
	});

	onDestroy(() => {
		clearTimeout(wait);
		clearTimeout(settle);
		clearTimeout(held);
		listen(false);
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

	{#if far && !zoom}
		<div
			class="nowview-steps"
			data-pans={pans}
			style:left="{far.x}px"
			style:top="{far.y}px"
			style:--far="{far.length}px"
		>
			<button
				type="button"
				class="step"
				aria-label="longer window"
				title={up ? `longer window (${up})` : 'longer window'}
				disabled={!up}
				on:click={() => dispatch('change', up)}>+</button
			>
			<button
				type="button"
				class="step"
				aria-label="shorter window"
				title={down ? `shorter window (${down})` : 'shorter window'}
				disabled={!down}
				on:click={() => dispatch('change', down)}>−</button
			>
		</div>
	{/if}

	<!-- Outside the band on purpose: a press on a button is not a press on
	     the picture, and keeping them siblings is what makes that true
	     without a single stopPropagation. They still declare the band's
	     axis, so a drag that starts on one is not a tab swipe either
	     (da-ndzl's test says so of the steps). -->
	{#if roomy && offNow && !zoom}
		<button
			type="button"
			class="nowview-jump right"
			data-pans={pans}
			title="back to now, and the time pills away"
			on:click={toNow}>{vertical ? 'now ↓' : 'now →'}</button
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
	.vertical .nowview-jump.right {
		right: auto;
		bottom: 4px;
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

	/* + just before the far label and − just after it, on its middle. The
	   glyph is tiny; the target is a fingertip's. */
	.nowview-steps {
		position: absolute;
		display: flex;
		gap: calc(var(--far) + 4px);
		transform: translate(calc(-22px - 2px), -50%);
	}
	.vertical .nowview-steps {
		flex-direction: column;
		transform: translate(-50%, calc(-24px - 2px));
	}

	.step {
		width: 22px;
		height: 24px;
		padding: 0;
		border: 0;
		background: none;
		color: var(--muted);
		font: inherit;
		font-size: 13px;
		line-height: 1;
		cursor: pointer;
	}

	.step:hover:not(:disabled) {
		color: inherit;
	}

	.step:disabled {
		opacity: 0.4;
		cursor: default;
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
	   right it stands inside `now →`, which owns that corner. */
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
