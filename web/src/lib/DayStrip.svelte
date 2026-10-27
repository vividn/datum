<!--
	The day strip: a stack of days, one picture each, stitched by the web.

	The comparison a person actually makes is *is today like yesterday,
	like last Tuesday* — which needs the earlier days on screen at the same
	scale, under the same x-axis (da-cgyh). So the strip is a scroller of
	the days before today with today under it, and the whole column reads
	as ONE picture.

	How it is drawn (da-iu9i). Each day is its own picture — one day,
	no date column, no ruler, no outer margin (`bandArgv` in $lib/panes.ts)
	— so what comes back is a band exactly as wide as the column and
	exactly `dayHeight` tall, all of it data area. Only the days a draw
	lacks are asked, together in one `dayview --pictures` run (da-p5lyt),
	instead of the whole past arriving at once, which is the snappiness;
	the stitching is then the web's job and it has four parts:

	- every band is asked at the SAME width and day height, so the hour
	  gridlines fall at the same x in each;
	- the strip draws its own gridlines across the whole stack, from the
	  x positions `--show json` reports (`hours[]`), so the lines run
	  through the dividers and down to the ruler rather than stopping at
	  each band's edge;
	- one background, the picture's own, behind bands and gaps alike;
	- the days TOUCH: between two of them is the 1px divider and nothing
	  else, where datum's own multi-day picture left fifteen.

	The strip draws the date stamps too, in a column of its own to the
	left of the bands, and the hour ruler once under today. Both used to
	be datum's, inside the picture — which is why the label column could
	not be resized: `label_width` was derived from the label font, which
	was derived from the day height, so moving either moved where every
	band's data area began. Drawn by the web, the column's width is just a
	setting (vividn: "the date labels should probably be rendered separately
	as well as the axis labels").

	The strip's sizes are moved by its LINES (below). During a drag the
	bands are SCALED to the size being previewed and re-asked only on
	release: one `dayview` per band per frame is a render loop, and this is
	the "better preview rendering when changing the day height/label width".
	And there is no freeze-today toggle (da-2hd21): vividn, 2026-10-15, «Move the
	  freeze today thing into the widget options menu, icon with words. So
	  no button in the main view, only widget menu». `freezeToday` is that
	  option. On, today is pinned under the scroller and never scrolls
	  away; off — the default — today is simply the last band in the
	  scroller and goes with the rest.

	The strip also ZOOMS onto an hour range (da-ukcf): two fingers pinching
	over it, or ctrl+wheel on a desktop, narrow the window; a two-finger
	drag or shift+wheel pans it; the `⊖` in the bottom corner of the date
	column — or the pane menu's `Reset widget` — gives the whole day back
	(da-8rqb9; it was a double tap until that bead). Every band at once,
	because they share the x-axis. A band is drawn once, for the whole day at
	the band's width, and a window is that picture shown through the band:
	one offset-and-scale (`windowMap` in $lib/panes.ts) puts the hours in
	it, and the strip's own gridlines and ruler are drawn from the same map.
	What the map stretches is TIME, so it is undone on every mark that is
	not a duration (da-dqbso, vividn: «occur dots (and lettering, icons,
	etc) shouldn't stretch out and get weird when zooming in … All the data
	is already there, it could be easily transformed to be correct without
	having to redraw»): a dot, a glyph, a label and a line keep their size,
	and a block is as long as its hours. So nothing is asked of datum for a
	zoom or a pan, under the gesture or on its release.

	A pinch DOWN the strip is the day height (da-hm1j7, vividn: «vertical
	zoom should affect the day height in an appropriate way so you really
	can just zoom in on data»). Each pinch is ONE axis, the DOMINANT one of
	where the two fingers landed: further apart down the strip than across
	it is the day height, otherwise the hours — so a sideways pinch never
	jitters the height, nor a vertical one the window. The height scales
	with the fingers' vertical spread, clamped to the strip's floor and
	ceiling, and is previewed exactly as the `+`'s drag is (the bands
	scaled); the stack scrolls with it so the point of the day the pinch
	began on stays under the fingers' middle (`keepUnder`), and the height
	is written once, on the release.

	How many days the box SHOWS: as many as it holds (da-zb0d). The strip
	fills its container and the pane it sits in is what says how tall it
	is. How many days the stack HOLDS is a second number (da-bewc): a
	stack holding exactly what its box shows overflows it by at most the
	one partly revealed day, and both the wheel and the finger ran out
	within the gesture. So the stack holds a FORTNIGHT at least, however
	short the box, and that past is what the reader scrolls back through.

	And it GROWS (da-vw4ow). Within a box's height of the oldest band the
	strip adds another fortnight above it — before the top, so the reader
	never hits it — and stops at the database's first entry, where a quiet
	«first entry · <date>» says there is nothing older. An added day is a
	placeholder at its final height and costs no run until it is scrolled
	into view (`refreshVisible`); the place on screen is held by the
	distance from the BOTTOM, which adding days above cannot change.

	A BAND IS A PLACE YOU POINT INTO TOO (da-ku7y). The nowview's
	recogniser is reused whole (`$lib/nowpoint.ts`) rather than written a
	second time, and this is the table it runs here — a band lies over a
	stack that scrolls, inside a pane that may swipe between tabs, so what
	belongs to whom is written down before the code:

	  down on a band
	    ├─ two fingers, side by side ──────► PINCH: the hour window (da-ukcf)
	    ├─ two fingers, one over the other ► PINCH: the day height (da-hm1j7)
	    └─ one finger or the mouse
	         ├─ mostly DOWNWARD, held or not ► the STACK's scroll, never ours
	         ├─ sideways, zoomed in ────────► PAN: the hour window follows the
	         │                               finger, as shift+wheel does
	         ├─ sideways, at an END ────────► the PANE's swipe, to the next tab:
	         │    (the whole day: both)      nothing to pan to, and a swipe
	         │                               never selects time
	         ├─ down 400 ms, THEN sideways ─► a RANGE
	         ├─ shift held, sideways ───────► the same range on a desk
	         ├─ down 400 ms, then LIFTING ──► the TIME where it fell, a mark or not
	         ├─ right click ────────────────► the same time, on a desk
	         └─ up, still still ────────────► a TAP: the entry under it, or the
	                                         time where there is none — and
	                                         on an entry already selected, the
	                                         time there instead

	  down on the ruler (the time axis) ► SCRUB: the time under the finger at
	                                      once, following it to the lift

	vividn's table, 2026-10-22 (da-xg1vi), which this is, for both views:
	«single tap → select a piece of data … single tap where there is no
	data, or in the time axis → select a time … long tap / right click →
	select a time … simple swipe or drag → pan; or, if already at the edge,
	pass the swipe to the container … long tap and drag → select a time
	period … swiping left/right must not select time», and «for quickly
	selecting a time, a user can touch in the time axis». The machine is
	$lib/nowpoint.ts; a tap and a right click are the `picture` action's
	($lib/hover.ts), because every drawn picture has it.

	The 400ms does not COMMIT (da-pzzoz): a still long press is a time and
	only a travelling one is a range. `.hold-mark` — a ring closing over
	the wait, the nowview's own — is the feedback that it registered.

	A press that starts on a line (below), the `⊖` or the
	jagged line is THEIRS, and one that starts off a band and the ruler —
	the date column, the room around them — is the pane's, so a swipe there
	still changes tabs. The bands say so themselves with `data-pans="x"`,
	and `data-pan-stop` names the ends the window is at.

	A range made here is the page's one selection ($lib/cursor.ts), marked
	`from: 'daystrip'`, which is how it comes to spell itself
	`start_time=09:00 end_time=10:30` rather than `-t` and `dur=`: a range
	on a day is long and is read as two clock times, and the two words on
	the bar are how a person finds out the CLI has them. It is ALWAYS one
	day's: the day is the day the press began in and the x along the band
	is the time, so a finger that wanders into the band above or below —
	which a drag along an 85px band mostly does — keeps selecting on its
	own day, at the hours it crossed. Dragged off either END of the band
	it stops at that day's midnight, which is `timeAt`'s own clamp
	($lib/hover.ts) and not a rule of this file's. The nowview draws the
	range, and the strip draws the nowview's, because `drawCursor` draws
	the one selection on every picture; neither re-spells the other's
	pills.

	The pills it makes are drawn twice, as the nowview's are: in the bottom
	bar and in the strip's own MARGIN, each over the mark it names. The
	margin lies over the top of the stack rather than taking a row from it
	— a row would cost the strip a day of past on every screen, selection
	or no selection — and it is there only while there is a selection.

	What the reader sets — `dayHeight`, `labelWidth`, `freezeToday`,
	`hourFrom`/`hourTo` — is a prop in and an `onchange` out; the dashboard
	keeps them in the widget's settings, and the strip itself writes
	nothing down.

	THE TWO SIZES ARE MOVED BY THE LINES (da-ray91). Every line between two
	days, the divider included, is a ns-resize handle a few pixels either
	side of it that moves the day height — but only where it crosses the
	DATE column (da-xg1vi): vividn, 2026-10-22, «it's way too easy to
	accidentally resize days on mobile when you're just trying to scroll»,
	so over the bands a line is no handle and a finger down it scrolls,
	always. The spine, the whole length of the strip, is an ew-resize one
	that moves the label width. Up is taller and right is wider.

	There was a `+` where the two met, which moved both and was the
	keyboard's; it is gone (da-5wa91), vividn, 2026-10-22: «it is not
	connected to anything and just floats around weirdly». The keyboard has
	the lines instead: the divider's, which is always on screen, is a
	slider for the day height (up and down), and the spine one for the
	label width (left and right), a `STEP` an arrow.

	A PAN NEVER SELECTS AND NEVER LABELS (da-xg1vi). A press that traveled
	— the stack's scroll, a pan of the hours, a pinch — selects nothing
	with its click and shows no tooltip; a finger's label waits for its
	lift (`pressTraveled`, $lib/inspect.ts, which every picture asks, the
	nowview's too). What selects is what the tree above says: a tap, a
	long tap, a long tap's drag, the ruler.

	THE DAY HEIGHT IS THE ONE THAT MAY BE ABSENT (da-r0ydu). No day height
	means ONE DAY FILLS THE WIDGET — `dayFilling(height)`, re-derived every
	time the pane is resized; pinned, today leaves a quarter of yesterday
	showing above it, `dayUnderSliver(height)` (da-2hd21) — and a number
	means the reader's own, which the box no longer moves. The handle sends
	`undefined` when it is dragged back onto the derived value, so the
	follow re-arms; there is no third state and no flag, because the owner
	keeps only the difference from what the layout says and an absence is
	what «following» IS.
-->
<script module lang="ts">
	import type { RunResult } from './run.js';

	/** A band's picture or the geometry, by the epoch it was asked at (see `cache`). */
	type Asked = { epoch: number; end: number | 'geometry'; run: Promise<RunResult> };
	/** Every picture asked at one size, by its argv (see `cache`). */
	type Cache = { key: string; size: string; runs: Map<string, Asked> };
	/** The size a band's picture was drawn at: one of another stands in, stretched, until it is seen. */
	type Drawn = { width: number; dayHeight: number; midnights: number };
	/** One hour gridline: where it sits along a band's x, and whether it is a major. */
	type Hour = { hour: number; x: number; major: boolean; label: string };
	/** What every band shares, read off today's layout rather than re-derived. */
	type Geometry = { hours: Hour[]; today: string | null };

	/**
	 * What each strip had drawn when it went, by database and widget
	 * (da-lt25u). A swipe to another layout unmounts the strip, and the
	 * swipe back asked the CLI for every band again: vividn, «we should
	 * cache the dayview bands on the web (at least for the past month)».
	 * A remount paints from here at once and re-asks only what a strip
	 * that had stayed would have: the visible days of a newer epoch, and
	 * today and the geometry once a minute has turned. A strip holds a
	 * month of its days at most, at each of its `SIZES`; a maximize is a
	 * remount too.
	 */
	type Held = {
		cache: Cache;
		sizes: Map<string, Cache>;
		shown: Record<number, RunResult>;
		drawn: Record<number, Drawn>;
		midnights: number;
		onDay: string | null;
		minute: number;
		width: number;
		height: number;
		geometry: Geometry | null;
		geometryBox: number;
	};
	const held = new Map<string, Held>();
	/**
	 * The strip mounted under each key, as what it would hold if it went now.
	 * Svelte 5 builds a pane's new place before it tears down the old one (a
	 * maximize and its restore), so the strip taking over reads the one it
	 * replaces from here: `held` has nothing of it yet.
	 */
	const mounted = new Map<string, () => Held>();
	/** The days back a strip's held pictures reach. */
	const MONTH = 31;
	/** The strips held, the longest gone dropped first: a widget closed for good is never mounted again. */
	const STRIPS = 8;
	/**
	 * The sizes a strip holds pictures at: the one it is drawn at and the
	 * two before it, so a maximize and its restore find their pictures held
	 * and ask nothing (da-8khuw). A size holds what was asked at it, the
	 * base stack and whatever was scrolled to.
	 */
	const SIZES = 3;
</script>

<script lang="ts">
	import { onDestroy, onMount, tick, untrack } from 'svelte';
	import { on } from 'svelte/events';
	import { browser } from '$app/env';
	import { currentDb } from './database.js';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import IntegrityMark from './IntegrityMark.svelte';
	import Pill from './Pill.svelte';
	import { pills } from './bar.js';
	import { minute } from './clock.js';
	import { pointAt, pointed, setCursor, SPAN_ID } from './cursor.js';
	import { timeAt } from './hover.js';
	import { spendPress } from './inspect.js';
	import { advance, LONG_MS, SLOP_PX, undecided, type Mode, type Press } from './nowpoint.js';
	import { run, runKey } from './run.js';
	import { firstEntryArgv } from './calls.js';
	import { syncStates } from './sync.js';
	import type { TailListing } from './tail.js';
	import {
		AXIS_HEIGHT,
		DIVIDER,
		SEAM,
		SLIVER,
		DEFAULT_DAY_HEIGHT,
		MIN_DAY_HEIGHT,
		MAX_DAY_HEIGHT,
		DEFAULT_LABEL_WIDTH,
		MIN_LABEL_WIDTH,
		MAX_LABEL_WIDTH,
		HOUR_FROM,
		HOUR_TO,
		bandArgv,
		bandBox,
		clampWindow,
		dayFilling,
		dayPitch,
		dayUnderSliver,
		geometryArgv,
		picturesArgv,
		type DayviewJson,
		todayBox,
		wholeDay,
		windowMap,
		zoomWindow
	} from './panes.js';
	import { settle } from './layout/resizing.js';

	interface Props {
		/** Any change re-runs every band. The frame bumps it after each run. */
		epoch?: number;
		/** Which widget this is — its tab's session key, `keyOf($layout, tab.id)` in Layout.svelte — and so whose pictures `held` keeps. */
		key?: string;
		/**
		 * How tall one day is drawn, in pixels — datum's `--day-height` — or
		 * UNDEFINED, which is the strip following its own box (da-r0ydu).
		 *
		 * The absence IS the derivation: `day` below is `dayFilling(height)`
		 * while nothing is here — `dayUnderSliver(height)` while today is
		 * pinned — so one day fills the widget at any size and follows it when
		 * the pane is resized. A number here is the reader's
		 * own, held against the box; the handle sends `undefined` back when it
		 * is dragged onto the derived value, and the follow re-arms. Nothing
		 * flags which of the two is in force — the absence is the flag.
		 */
		dayHeight?: number | undefined;
		/** How wide the strip's own date-stamp column is, in pixels. */
		labelWidth?: number;
		/** Is today pinned under the scroller, or the last band inside it? */
		freezeToday?: boolean;
		/** The hour range every band is drawn onto — the whole day at rest (da-ukcf). */
		hourFrom?: number;
		hourTo?: number;
		/** What the reader moved, for the owner to fold into the widget's settings (`Setting`). */
		onchange?: (moved: Setting) => void;
	}

	let {
		epoch = 0,
		key = '',
		dayHeight = undefined,
		labelWidth: givenLabelWidth = DEFAULT_LABEL_WIDTH,
		freezeToday = false,
		hourFrom = HOUR_FROM,
		hourTo = HOUR_TO,
		onchange
	}: Props = $props();

	/** What this strip drew before it was last unmounted, if anything (`held`). */
	const heldAs = `${currentDb()}\n${untrack(() => key)}`;
	const was = mounted.get(heldAs)?.() ?? held.get(heldAs);
	mounted.set(heldAs, holding);

	/**
	 * What the reader moved, folded into the widget's settings by the
	 * owner. `dayHeight: undefined` is a move like any other — the reader
	 * asking for the derived height back (see `setSizes`).
	 */
	type Setting = {
		dayHeight?: number | undefined;
		labelWidth?: number;
		freezeToday?: boolean;
		hourFrom?: number;
		hourTo?: number;
	};

	/** The least past the stack holds, so there is somewhere to scroll to — and what it grows by. */
	const FORTNIGHT = 14;
	/** What one press of an arrow key moves the handle, on either axis. */
	const STEP = 5;

	const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

	/**
	 * The height the READER holds, made drawable — or null, which is the
	 * strip following its box.
	 *
	 * Whatever the owner hands in, a saved setting from an older layout
	 * included, is drawn at a size the strip can draw legibly. A number
	 * OUTSIDE that range is clamped into it — a held 0 from a hand-edited
	 * or ancient tree is the floor, not the old default, because
	 * `DEFAULT_DAY_HEIGHT` no longer means «a height to fall back on»: it
	 * is only the stand-in below, before the box is measured. A value that
	 * is no number at all (NaN) holds nothing, and the strip follows —
	 * which is what an unusable setting should leave a reader with.
	 */
	const heldDay = $derived(
		typeof dayHeight === 'number' && Number.isFinite(dayHeight)
			? clamp(Math.round(dayHeight), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT)
			: null
	);
	/**
	 * The day height the strip DRAWS at: the reader's, or the one that
	 * makes one day fill the box it was just measured at — less the sliver
	 * of yesterday that says there is a past to scroll up to (da-2hd21),
	 * pinned or not.
	 *
	 * Before the box is measured there is no box to derive from, and the
	 * default stands in — nothing is drawn at it, since every ask is gated
	 * on a width the same measurement supplies.
	 */
	const boxDay = $derived.by(() => (height > 0 ? dayUnderSliver(height) : DEFAULT_DAY_HEIGHT));
	/** The day DRAWN: the reader's held height when there is one, else the box's. */
	const day = $derived(heldDay ?? boxDay);
	/** The label width drawn: the owner's made drawable, or a handle's until the owner's changes (`setSizes`). */
	let labelWidth = $derived(
		clamp(Math.round(givenLabelWidth) || DEFAULT_LABEL_WIDTH, MIN_LABEL_WIDTH, MAX_LABEL_WIDTH)
	);

	let body: HTMLDivElement | undefined = $state();
	let stripEl: HTMLDivElement | undefined;
	let stackEl: HTMLDivElement | undefined = $state();
	/** The box the pane gives the strip, in whole pixels, once measured. */
	let width = $state(was?.width ?? 0);
	let height = $state(was?.height ?? 0);
	/**
	 * Has the box been measured since mount? A remount paints what it held
	 * at once and asks for nothing at the box it held: a maximize is a
	 * remount into another box, and asked for the days that box showed at
	 * the old size before it was measured (da-8khuw).
	 */
	let measured = $state(!was);
	/** Is the newest day the bottom-most band in view? */
	let atBottom = $state(true);

	/** The band's own width on screen: the box less the stamp column. */
	const bandWidth = $derived(Math.max(0, width - labelWidth));

	// ── the hour window ────────────────────────────────────────────────
	//
	// The window the bands were DRAWN for, and the one they are shown
	// through: the same at rest, and a gesture is the difference between
	// them. Which is why the gesture costs nothing — the pictures are
	// mapped where they are, and datum is asked again only on release.

	/** The window a gesture is previewing, or null when none is in hand. */
	let live = $state.raw<[number, number] | null>(null);
	/** The window the widget holds: its setting, made sane. */
	const committed = $derived(clampWindow(hourFrom, hourTo));
	/** The window on screen. */
	const view = $derived(live ?? committed);

	/**
	 * Each picture is mapped by the width IT was drawn at, not by the one
	 * just asked for.
	 *
	 * Which matters for the beat after a resize: the band's width has
	 * changed, the bands on screen have been asked again, and what is
	 * shown meanwhile — and off screen until it is scrolled to — is the
	 * picture drawn at the width BEFORE. Mapped by the new width it would
	 * show the wrong hours — a jump, and then a jump back as each answer
	 * lands, one CLI process apart. Mapped by its own, it shows exactly the
	 * hours it is meant to until its answer arrives.
	 */
	const IDENTITY = { offset: 0, scale: 1 };
	const mapFor = (full: number) =>
		full > 0 && bandWidth > 0 ? windowMap(view, full, bandWidth) : IDENTITY;
	/** The gridlines and the ruler are drawn from the GEOMETRY's own width. */
	const gridMap = $derived.by(() => mapFor(geometryBox));
	/** Where along the band a gridline or a ruler tick falls. */
	const bandX = (x: number) => gridMap.offset + gridMap.scale * x;
	/** Is there a window at all, or is the whole day on screen? */
	const zoomed = $derived(!wholeDay(view));
	/**
	 * The ends the hour window is AT, which a finger cannot pan past: there
	 * the swipe is the pane's, to change tabs (da-xg1vi). `start` is the
	 * day's first hour, where a finger moving right (dx > 0, the window
	 * toward earlier) stops, and `end` its last; the whole day is at both
	 * (`takes`, PaneFrame.svelte).
	 */
	const panStop = $derived([committed[0] <= HOUR_FROM && 'start', committed[1] >= HOUR_TO && 'end'].filter(Boolean).join(' '));
	/** The transform each band's picture is shown under. */
	const bandZoom = (end: number) => mapFor(drawn[end]?.width);
	/**
	 * A `<pattern>` — a multi-state block's stripes, a clock change's
	 * hatch — tiles in the user space of the block it fills, so the map
	 * and a previewed size stretched it with the block: wider stripes at a
	 * shallower angle (da-hmtoq). Scaled back ahead of datum's own
	 * transform, it is the same on screen at every zoom, as a mark is
	 * (da-dqbso, the CSS below). On the attribute after every update,
	 * when a new picture is in its band too: WebKit computes a CSS
	 * `transform` on a pattern and draws it without. A label previewed as
	 * wide as the pane leaves the bands no width (`scaleX` 0): the pattern
	 * keeps the transform it had rather than taking a scale of Infinity.
	 * After every update that puts a picture or a band on screen, and as the
	 * map and the scales it reads change.
	 */
	$effect(() => {
		void [shown, rows, freezeToday, unsynced];
		for (const band of stripEl?.querySelectorAll<HTMLElement>('.band') ?? []) {
			const across = 1 / (bandZoom(Number(band.dataset.daysAgo)).scale * scaleX);
			if (!Number.isFinite(across)) continue;
			for (const pattern of band.querySelectorAll<SVGPatternElement>('svg.svg pattern')) {
				pattern.dataset.drawn ??= pattern.getAttribute('patternTransform') ?? '';
				const t = `scale(${across} ${1 / (scaleY * stand(Number(band.dataset.daysAgo)))}) ${pattern.dataset.drawn}`;
				if (pattern.getAttribute('patternTransform') !== t) pattern.setAttribute('patternTransform', t);
			}
		}
	});

	/**
	 * The days the box SHOWS. The room is whatever is left once the ruler,
	 * the divider and — while today is frozen — today's own band have
	 * theirs; a day only partly revealed still counts, because it is a day
	 * that has to be drawn and the first one the stack scrolls up to.
	 */
	const room = $derived(Math.max(0, height - AXIS_HEIGHT - (freezeToday ? SEAM + bandBox(day) : DIVIDER)));
	const slots = $derived(room > 0 ? Math.ceil((room - bandBox(day)) / dayPitch(day)) + 1 : 0);
	/** Of those slots, the ones the PAST gets: unfrozen, today is in the scroller too. */
	const fits = $derived(Math.max(0, freezeToday ? slots : slots - 1));
	/**
	 * The days the stack HOLDS: a fortnight at least, and as many as the
	 * box shows when it shows more (da-bewc) — at any box height, so the
	 * strip is always scrollable into the past (da-2hd21). A box with no
	 * room above a pinned today still gives the stack a `SLIVER` of a day,
	 * clipping today from the top (`.stack`'s `min-height`).
	 */
	const base = $derived(Math.max(FORTNIGHT, fits));
	/** The days the reader has scrolled the stack into, above `base` (`reachBack`). */
	let extra = $state(0);
	const depth = $derived(base + extra);
	/** What the stack keeps of the box however short it is: a sliver of a day. */
	const sliver = $derived.by(() => Math.round(bandBox(shownDayHeight) * SLIVER));
	/**
	 * The middle of the seam over a pinned today (da-2hd21), where the tear
	 * sits: today's band and its ruler, less what the sliver took.
	 */
	const seamAt = $derived.by(
		() => Math.min(bandBox(shownDayHeight), Math.max(0, height - AXIS_HEIGHT - SEAM - sliver)) + AXIS_HEIGHT + SEAM / 2
	);

	/** The days in the scroller, oldest first — today among them when it is not frozen. */
	const rows = $derived([
		...Array.from({ length: depth }, (_, i) => depth - i),
		...(freezeToday ? [] : [0])
	]);
	/**
	 * Every day that is drawn without being seen: today first, it being the
	 * one in front, then the base stack. A day grown above it is drawn only
	 * once it is on screen (`refreshVisible`).
	 */
	const ends = $derived(Array.from({ length: base + 1 }, (_, i) => i));

	/** The sizes a drag is previewing, or the committed ones at rest. */
	const shownDayHeight = $derived.by(() => (sizing ? sizing.height : day));
	const shownLabelWidth = $derived.by(() => (sizing ? sizing.width : labelWidth));
	/**
	 * How far the drawn bands are stretched to stand in for the size being
	 * previewed. One at rest, which is the picture at its own size.
	 */
	const scaleY = $derived(shownDayHeight / day);
	/**
	 * How far a band's picture is stretched to the day height at rest: one,
	 * or more or less for a picture drawn at another, which stands in until
	 * the day is on screen (da-8khuw). On the picture, as the hour window's
	 * map is, and not on `.shot`, which a resting transform would make the
	 * hover label's containing block (see `.shot.previewing`).
	 */
	const stand = (end: number) => day / (drawn[end]?.dayHeight ?? day);
	const scaleX = $derived(bandWidth > 0 ? Math.max(0, width - shownLabelWidth) / bandWidth : 1);

	/**
	 * The jagged line shows exactly when the band against the divider is
	 * not the newest one — which is to say, when the reader has scrolled
	 * up into older days.
	 */
	const jagged = $derived(depth > 0 && !atBottom);

	// The content box, not the element's own size: the body is padded, and
	// the strip is 100% of that box.
	//
	// A taller box is more room above today, so the stack grows: re-anchor
	// it if the reader was at the bottom, or the newest day drifts off the
	// top of the divider by exactly the room that was just made.
	//
	// Through `settle` (layout/resizing.ts): a border being dragged resizes
	// the box every frame, and a band per frame per day was the jitter
	// (da-jgrn). The size lands on the drop, or once a burst of resizes has
	// been over for a beat; the bands drawn already stay up meanwhile.
	onMount(() => {
		const settled = settle<{ w: number; h: number }>(({ w, h }) => {
			if (w !== width) width = w;
			if (h !== height) {
				height = h;
				void anchor();
			}
			if (!measured) {
				measured = true;
				refreshVisible();
			}
		});
		// A remount sliding in beside the layout in front is laid out in
		// THAT layout's root, which may be a pane's border smaller than its
		// own (da-466lv): it keeps the box it held until it lands, and is
		// observed again there, which reports the box it landed in (da-lt25u).
		const beside = () => !!body?.closest('.tab-body.beside');
		let landing = 0;
		const land = () => {
			landing = beside() ? requestAnimationFrame(land) : 0;
			if (!landing && body) {
				observer.unobserve(body);
				observer.observe(body);
			}
		};
		const observer = new ResizeObserver(([entry]) => {
			if (was && beside()) return void (landing ||= requestAnimationFrame(land));
			settled.push({
				w: Math.floor(entry.contentRect.width),
				h: Math.floor(entry.contentRect.height)
			});
		});
		if (body) observer.observe(body);
		return () => {
			observer.disconnect();
			settled.stop();
			cancelAnimationFrame(landing);
			if (refreshing) cancelAnimationFrame(refreshing);
		};
	});

	// ── the pictures ───────────────────────────────────────────────────

	/** Each band on screen, by how many days back it is. 0 is today's. */
	let shown: Record<number, RunResult> = $state.raw(was?.shown ?? {});
	/**
	 * And the size each of those pictures was drawn at: its width is what
	 * it is mapped by (`mapFor`), its day height what it is stretched by
	 * (`stand`), and one of another midnight stands in for nothing.
	 */
	let drawn: Record<number, Drawn> = $state.raw(was?.drawn ?? {});
	/** The width the geometry on hand was read at. */
	let geometryBox = $state(was?.geometryBox ?? 0);

	/**
	 * A band's picture — or the message that came instead of one, said out
	 * loud rather than left as a box that never finishes loading.
	 */
	/** Since when this copy may lack what another device has, while the session's first sync is under way (da-g5jjp). */
	const unsynced = $derived($syncStates[currentDb()]?.unsynced);

	const picture = (result: RunResult, unsynced?: number) => ({
		text: result.stdout || result.stderr,
		error: !result.stdout,
		unsynced
	});

	/** How many draws have been started, and which one each picture came from. */
	let started = 0;
	const from: Record<number, number> = {};
	let geometryFrom = 0;

	/**
	 * Every picture asked for at the current band width, day height and
	 * epoch, by its argv — held as the promise, so two draws in flight for
	 * the same day share one run. A taller box asks for the days it gained
	 * and re-draws nothing else, because their argv has not changed. A new
	 * size asks for the days on screen (`redraw`), and the sizes before it
	 * are kept aside (`sizes`), so going back to one asks for nothing.
	 *
	 * A new EPOCH at the same size is the one case that is not a straight
	 * drop: today is re-drawn, and every other day keeps the picture it
	 * has until the reader can SEE it, when it is checked (`check`) and
	 * re-drawn only if its picture changed. A fortnight of bands is a fortnight of CLI
	 * processes, and putting fifteen of them in front of whatever is typed
	 * next cost three and a half seconds a command — for days that are not
	 * on the screen.
	 *
	 * Which is why every run remembers the epoch it was ASKED at: a day
	 * kept that way holds a picture from before the change, and when the
	 * reader scrolls it into view `refreshVisible` asks for that day and
	 * no other (da-dtwz).
	 */
	let cache: Cache = was?.cache ?? { key: '', size: '', runs: new Map<string, Asked>() };
	/** The caches of the sizes before this one, the oldest first (`SIZES`). */
	const sizes = was?.sizes ?? new Map<string, Cache>();
	/**
	 * This strip in the worker's queue, with the band after it: a band's
	 * newer run drops its older, unstarted one, whose answer is no picture.
	 */
	const queueKey = runKey();

	/**
	 * A midnight, counted — the one thing that makes a CACHED picture
	 * wrong rather than merely old.
	 *
	 * A band's argv names its day relative to the CLI's today
	 * (`--end-date -3`), so when today moves every argv means a different
	 * day while the cache still holds yesterday's answer for it. The
	 * geometry run carries the date the CLI cut, so a change in it drops
	 * the pictures and asks again.
	 */
	let midnights = $state(was?.midnights ?? 0);
	let onDay = was?.onDay ?? null;

	/**
	 * The days of the stack the reader can see, plus today — what a
	 * refresh must re-draw. Null before the stack is measured.
	 */
	function visibleDays(): number[] | null {
		const rows = stackEl?.querySelectorAll('.row');
		if (!rows?.length) return null;
		const box = stackEl!.getBoundingClientRect();
		const seen = [0];
		for (const row of rows) {
			const at = row.getBoundingClientRect();
			if (at.bottom > box.top && at.top < box.bottom)
				seen.push(Number((row.querySelector('.band') as HTMLElement).dataset.daysAgo));
		}
		return seen;
	}

	let geometry: Geometry | null = $state.raw(was?.geometry ?? null);

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser. At the band's width whatever
	// the window: the window is how the band shows the picture (`mapFor`).
	const request = $derived({ width: bandWidth, dayHeight: day, epoch, midnights });
	$effect.pre(() => {
		const [at, all] = [request, ends];
		if (browser && measured && at.width > 0) untrack(() => redraw(at, all));
	});

	/**
	 * Draw the stack at `at`. A day showing a picture of another size, and
	 * not held at this one, keeps it, stretched, until it is on screen:
	 * `refreshVisible` asks for it then, as it does for a day of an older
	 * epoch. A resize asked for the whole base stack, a fortnight of
	 * processes in front of the days the reader sees (da-8khuw).
	 */
	function redraw(at: typeof request, ends: number[]): void {
		cacheFor(at);
		const atThisSize = (end: number) => cache.runs.has(bandArgv(at.width, at.dayHeight, end).join(' '));
		draw(at, ends.filter((end) => end === 0 || drawn[end]?.midnights !== at.midnights || atThisSize(end)));
		// The days on screen are read once the newest is pinned again: a
		// taller day leaves the stack where it was, over older days.
		void anchor();
		refreshVisible();
	}

	// The minute turned (da-r61i, `$lib/clock.ts`): today's band is the one
	// picture in the stack with a now in it, so the tick asks for that one
	// and for the geometry, and for nothing else — two processes a minute,
	// whatever the depth of the stack. The days above today are finished
	// days and their pictures are still true.
	//
	// The geometry goes again because it is what tells the strip what day
	// it is: a tab left open overnight would otherwise draw the new day's
	// band under yesterday's date stamps, today's band being the CLI's own
	// today. A changed date is a midnight, and `midnights` redraws the
	// stack (see `draw`) — so the strip now follows a midnight by itself,
	// where before it waited for a command.
	$effect.pre(() => {
		if (browser && $minute > 0) untrack(tock);
	});

	// A remount whose held pictures are of an earlier minute is that tick,
	// at once at the box it held (a box measured otherwise draws again
	// anyway). By the wall clock: `minute` counts only
	// while someone holds it, and a midnight away is a minute away.
	const wallMinute = () => Math.floor(Date.now() / 60_000);
	let late = !!was && was.minute !== wallMinute();
	$effect.pre(() => {
		if (late && measured && request.width > 0)
			untrack(() => {
				late = false;
				tock();
			});
	});

	function tock(): void {
		const at = request;
		if (at.width <= 0) return;
		for (const argv of [geometryArgv(at.width, at.dayHeight), bandArgv(at.width, at.dayHeight, 0)])
			cache.runs.delete(argv.join(' '));
		// A size held aside has a today of a minute ago too.
		for (const { runs } of sizes.values())
			for (const [id, asked] of runs) if (asked.end === 0 || asked.end === 'geometry') runs.delete(id);
		// Before the box is measured, its first redraw asks for today.
		if (measured) draw(at, [0]);
	}

	function draw(
		at: { width: number; dayHeight: number; epoch: number; midnights: number },
		ends: number[]
	): void {
		cacheFor(at);
		const token = ++started;
		const geometryId = geometryArgv(at.width, at.dayHeight).join(' ');
		const id = (end: number) => bandArgv(at.width, at.dayHeight, end).join(' ');
		// What is not held is asked in ONE run, the geometry and each band
		// as `--pictures` answers it: a process and its read are most of a
		// band's cost (da-p5lyt). Each is still held by its own argv.
		const missing = ends.filter((end) => !cache.runs.has(id(end)));
		const bare = !cache.runs.has(geometryId);
		if (missing.length || bare) {
			const argv = missing.length ? picturesArgv(at.width, at.dayHeight, missing) : geometryArgv(at.width, at.dayHeight);
			const label = [...(bare ? ['geometry'] : []), ...missing].join(',');
			const asked = run(argv, undefined, undefined, `${queueKey}:${label}`, undefined, undefined, true);
			const parts = asked.then(picturesOf);
			if (bare) cache.runs.set(geometryId, { epoch: at.epoch, end: 'geometry', run: parts.then((of) => of('geometry')) });
			for (const end of missing) cache.runs.set(id(end), { epoch: at.epoch, end, run: parts.then((of) => of(end)) });
		}

		// The geometry first: the gridlines, the ruler and the date stamps
		// all hang off it, and it is one small JSON rather than a picture.
		void cache.runs.get(geometryId)!.run.then((result) => {
			if (result.superseded || token < geometryFrom) return;
			geometryFrom = token;
			const fresh = readGeometry(result.stdout);
			if (!fresh) return;
			// Today moved: every cached picture is a day out (see `midnights`).
			if (onDay && fresh.today && fresh.today !== onDay) midnights += 1;
			onDay = fresh.today ?? onDay;
			geometry = fresh;
			geometryBox = at.width;
		});

		for (const end of ends) {
			void cache.runs.get(id(end))!.run.then((result) => {
				// A resize while a draw is in flight and two draws overlap;
				// they can finish out of order, and the loser would paint the
				// older answer over the newer one. Last started wins, band by
				// band — and until it lands, the one on screen stays.
				if (result.superseded || token < (from[end] ?? 0)) return;
				from[end] = token;
				shown = { ...shown, [end]: result };
				drawn = { ...drawn, [end]: { width: at.width, dayHeight: at.dayHeight, midnights: at.midnights } };
				void anchor();
			});
		}
	}

	/**
	 * Each band's answer out of a `--pictures` run, its SVG as its own
	 * `--days 1` run prints it, and the geometry's, the run with the
	 * pictures dropped: the bands hold them, and holding them twice
	 * doubled what the strip keeps (da-xemyx).
	 */
	function picturesOf(result: RunResult): (end: number | 'geometry') => RunResult {
		if (result.superseded || result.code !== 0) return () => result;
		let pictures: Record<string, string> = {};
		let geometry = result.stdout;
		try {
			const { pictures: of = {}, ...rest } = JSON.parse(result.stdout);
			pictures = of;
			geometry = JSON.stringify(rest);
		} catch {
			// No answer is no picture.
		}
		return (end) =>
			end === 'geometry'
				? { ...result, stdout: geometry }
				: { ...result, stdout: end in pictures ? `${pictures[end]}\n` : '' };
	}

	/**
	 * Make `cache` the one for `at`: the held one of its size, or a new one,
	 * the size left going to `sizes`; then, at a new epoch, `kept`.
	 */
	function cacheFor(at: typeof request): void {
		const size = `${at.width}/${at.dayHeight}/${at.midnights}`;
		const key = `${size}/${at.epoch}`;
		if (cache.key === key) return;
		if (cache.size !== size) {
			const back = sizes.get(size) ?? { key: '', size, runs: new Map<string, Asked>() };
			sizes.delete(size);
			if (cache.runs.size) sizes.set(cache.size, cache);
			while (sizes.size >= SIZES) sizes.delete(sizes.keys().next().value!);
			cache = back;
		}
		if (cache.key !== key) cache = { key, size, runs: kept(at) };
	}

	/**
	 * What a cache hands on to a new epoch at its size: every day but
	 * today, for `refreshVisible` to check once it is on screen (da-lt25u,
	 * da-efxk5). Today and the geometry always go again — today has the
	 * now in it, and the geometry is what tells the strip what day it is.
	 */
	function kept(at: { width: number; dayHeight: number }): Map<string, Asked> {
		const again = new Set([bandArgv(at.width, at.dayHeight, 0), geometryArgv(at.width, at.dayHeight)].map((argv) => argv.join(' ')));
		return new Map([...cache.runs].filter(([id]) => !again.has(id)));
	}

	/**
	 * Do the pictures held for `ends`, asked before `at`'s epoch, still
	 * stand? One `dayview --digests` answers for all of them (da-efxk5):
	 * vividn, «The data really shouldn't update much so we don't need to
	 * redraw from scratch everytime». A day whose picture hashes to its
	 * digest is kept, its element and all, as of this epoch; any other is
	 * asked again. Not by which instants a write moved: a block's title
	 * names where it ends, so a write a day later changes its picture too.
	 */
	function check(at: typeof request, ends: number[]): void {
		const mine = cache;
		const id = (end: number) => bandArgv(at.width, at.dayHeight, end).join(' ');
		const held = ends.map((end) => mine.runs.get(id(end))!);
		// A day already out for this epoch is not asked for again; a new one
		// asks for them all, since the newer check drops the older while it
		// waits, keyed as a band is.
		if (held.every((asked) => checking.get(asked) === at.epoch)) return;
		for (const asked of held) checking.set(asked, at.epoch);
		const argv = [...geometryArgv(at.width, at.dayHeight), '--digests', ends.join(',')];
		void run(argv, undefined, undefined, `${queueKey}:check`, undefined, undefined, true).then(async (result) => {
			if (result.superseded) return;
			for (const asked of held) if (checking.get(asked) === at.epoch) checking.delete(asked);
			let digests: Record<string, string> = {};
			try {
				digests = JSON.parse(result.stdout).digests ?? {};
			} catch {
				// No answer is no digest: each day is asked again.
			}
			const again: number[] = [];
			for (const [i, end] of ends.entries()) {
				const asked = held[i];
				const same = digests[end] !== undefined && digests[end] === (await sha256((await asked.run).stdout));
				if (cache !== mine) return;
				if (mine.runs.get(id(end)) !== asked || asked.epoch >= at.epoch) continue;
				if (same) asked.epoch = at.epoch;
				else {
					mine.runs.delete(id(end));
					again.push(end);
				}
			}
			if (again.length) draw(at, again);
		});
	}

	/** Each held picture a `check` is out for, and the epoch it is checked at. */
	const checking = new Map<Asked, number>();

	/** The SHA-256 of a picture as `--digests` takes it: the stdout less its newline. */
	async function sha256(stdout: string): Promise<string | null> {
		if (!crypto.subtle) return null;
		const bytes = new TextEncoder().encode(stdout.replace(/\n$/, ''));
		const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
		return Array.from(hash, (b) => b.toString(16).padStart(2, '0')).join('');
	}

	/** `dayview --show json`, as much of it as the strip draws with. */
	function readGeometry(stdout: string): Geometry | null {
		try {
			const json = JSON.parse(stdout) as DayviewJson;
			const ticks = json.axis_ticks ?? [];
			const hours: Hour[] = (json.hours ?? []).map((mark, i) => ({
				hour: mark.hour,
				x: mark.x,
				major: !!ticks[i]?.is_major,
				label: ticks[i]?.label ?? String(mark.hour).padStart(2, '0')
			}));
			return hours.length ? { hours, today: json.days?.[0]?.date ?? null } : null;
		} catch {
			return null;
		}
	}

	async function anchor(): Promise<void> {
		// A scroll whose event has not fired yet — it fires a frame later —
		// is newer than whatever grew the stack: read it now, or the re-pin
		// below takes back what the reader just did (da-6m5s).
		if (stackEl && stackEl.scrollTop !== seenTop) onScroll();
		if (!atBottom) return;
		await tick();
		toNewest();
	}

	/** Where `onScroll` last found the stack. */
	let seenTop = 0;

	function onScroll(): void {
		seenTop = stackEl?.scrollTop ?? 0;
		// A stack squeezed to no height by a shrinking box is one nobody can
		// have scrolled, so it keeps the newest day pinned for when the box
		// grows back (da-2hd21).
		atBottom =
			!stackEl ||
			stackEl.clientHeight === 0 ||
			stackEl.scrollTop + stackEl.clientHeight >= stackEl.scrollHeight - 1;
		refreshVisible();
		void reachBack();
	}

	// ── growing the past (da-vw4ow) ────────────────────────────────────

	/**
	 * The database's first entry, as of the epoch it was asked at: how many
	 * days before today it is, and its date — none, and no way back, in a
	 * database with nothing timed. Asked once an epoch, and only while the
	 * reader is near the top: a command costs this run only then.
	 */
	let first = $state.raw<{ epoch: number; back: number; date: string | null } | null>(null);
	let reaching = false;
	/** The stack reaches the first entry: nothing older to add. */
	const ended = $derived(first !== null && depth >= first.back);

	/** Is the reader within a box's height of the oldest band? */
	const nearTop = () => !!stackEl && stackEl.scrollTop < stackEl.clientHeight;

	// A command may have moved the first entry, or wiped the database: ask
	// again if the reader is up there, and otherwise forget the answer — out
	// of their sight, so the marker's room goes without moving them.
	$effect.pre(() => {
		if (first && first.epoch !== epoch) untrack(() => void (nearTop() ? reachBack() : keepPlace(() => (first = null))));
	});

	/** Within a box's height of the oldest band, add a fortnight above it. */
	async function reachBack(): Promise<void> {
		const today = geometry?.today;
		if (reaching || !today || !nearTop() || (first?.epoch === epoch && ended)) return;
		reaching = true;
		const asked = epoch;
		const known =
			first?.epoch === asked ? first : { back: 0, date: null, ...(await firstEntry(today)), epoch: asked };
		await keepPlace(() => {
			first = known;
			if (depth < known.back) extra += Math.min(FORTNIGHT, known.back - depth);
		});
		reaching = false;
		refreshVisible();
	}

	/**
	 * The oldest TIMED entry (`--metric occur`: a timeless one has no
	 * day), counted back from the CLI's `today`. A block's day is
	 * where it BEGINS, and its time is its end (da-zwct).
	 */
	async function firstEntry(today: string): Promise<{ back: number; date: string } | null> {
		try {
			const { rows } = JSON.parse((await run(firstEntryArgv())).stdout) as Partial<TailListing>;
			const row = rows?.[0];
			if (!row?.time) return null;
			const at = new Date(Date.parse(row.time) - (row.dur_seconds ?? 0) * 1000);
			const [y, m, d] = today.split('-').map(Number);
			const back = Math.round(
				(Date.UTC(y, m - 1, d) - Date.UTC(at.getFullYear(), at.getMonth(), at.getDate())) / 86_400_000
			);
			const pad = (n: number) => String(n).padStart(2, '0');
			return { back, date: `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}` };
		} catch {
			return null;
		}
	}

	/** Change what is above the reader without moving what they are looking at. */
	async function keepPlace(change: () => void): Promise<void> {
		const below = stackEl ? stackEl.scrollHeight - stackEl.scrollTop : 0;
		change();
		await tick();
		if (!stackEl) return;
		stackEl.scrollTop = stackEl.scrollHeight - below;
		seenTop = stackEl.scrollTop;
	}

	/** The frame a pending refresh is waiting for; a scroll is many events. */
	let refreshing = 0;

	/**
	 * Check the days now on screen that were drawn before the current
	 * epoch, and ask again for today if it was (da-dtwz, da-efxk5).
	 *
	 * The cache keeps a day the reader could not see rather than re-running
	 * a fortnight of CLI processes in front of the next command — and the
	 * next beat is the next COMMAND, so scrolling up to the entry just
	 * logged for ten days ago showed a band drawn before it existed. Here
	 * the scroll itself is the beat, and it asks for the stale bands that
	 * came into view and for nothing else: days already at this epoch were
	 * re-drawn when it turned. A day grown above the base has never been
	 * asked at all, and this is where it is (da-vw4ow) — or, grown again
	 * after a remount, asked already and held, and drawn from there.
	 */
	function refreshVisible(): void {
		if (refreshing || !browser) return;
		refreshing = requestAnimationFrame(() => {
			refreshing = 0;
			const at = request;
			if (at.width <= 0 || !measured) return;
			const id = (end: number) => bandArgv(at.width, at.dayHeight, end).join(' ');
			const visible = visibleDays() ?? [];
			const stale = visible.filter((end) => (cache.runs.get(id(end))?.epoch ?? at.epoch) < at.epoch);
			if (stale.includes(0)) cache.runs.delete(id(0));
			check(at, stale.filter((end) => end !== 0));
			const unseen = visible.filter((end) => !shown[end] || !cache.runs.has(id(end)));
			if (unseen.length) draw(at, unseen);
		});
	}

	/** Put the newest day back against the divider. The depth is unchanged. */
	function toNewest(): void {
		if (stackEl) stackEl.scrollTop = stackEl.scrollHeight;
		onScroll();
	}

	// ── the date stamps, drawn by the strip ────────────────────────────

	const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
	const MONTHS = [
		'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
		'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'
	];

	/**
	 * The three lines of the stamp `back` days before the ISO day `iso`,
	 * in datum's own words (`format_day_label` in commands/dayview.rs).
	 *
	 * Counted off the date the CLI cut, not off the browser's clock: the
	 * civic day is the CLI's to decide, and UTC arithmetic on a date it
	 * has already resolved is plain calendar counting with no zone in it.
	 */
	function stampFor(iso: string, back: number): string[] {
		const [y, m, d] = iso.split('-').map(Number);
		const at = new Date(Date.UTC(y, m - 1, d));
		at.setUTCDate(at.getUTCDate() - back);
		return [
			WEEKDAYS[at.getUTCDay()],
			`${MONTHS[at.getUTCMonth()]} ${String(at.getUTCDate()).padStart(2, '0')}`,
			String(at.getUTCFullYear())
		];
	}

	/** The stamp font: as tall as three lines of the band allow, and as wide as the column. */
	const stampFont = $derived(
		Math.max(7, Math.min(14, Math.floor(Math.min(shownDayHeight / 3 - 4, shownLabelWidth / 4.2))))
	);
	/** How many of the three lines fit, tallest first — datum shrinks 3 → 2 → 1 the same way. */
	const stampLines = $derived(Math.max(1, Math.min(3, Math.floor(shownDayHeight / (stampFont + 2)))));
	const stamp = (iso: string | null, back: number, lines: number) =>
		iso ? stampFor(iso, back).slice(0, lines) : [];
	/**
	 * The fields `check integrity` flags on each band's day (da-0lful). datum
	 * draws each finding into the band itself, a group named for its field,
	 * and the stamp carries the warning datum puts by its own date label.
	 * Read as a document, so a name comes back unescaped (`r&d`, not
	 * `r&amp;d`) whatever order the attributes are written in.
	 */
	const flagged = $derived(
		Object.fromEntries(Object.entries(shown).map(([end, result]) => [end, fieldsFlagged(result.stdout)])) as Record<
			number,
			string[]
		>
	);
	function fieldsFlagged(svg: string): string[] {
		if (!svg.includes('integrity-error')) return [];
		const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');
		const groups = [...doc.querySelectorAll('.integrity-error')];
		return [...new Set(groups.map((g) => g.getAttribute('data-field') ?? ''))];
	}

	// ── the hour ruler, drawn once under today ─────────────────────────

	/** datum's own axis font: the strip's height less its padding. */
	const AXIS_FONT = 9;

	/**
	 * Which hours the ruler names, and where the text sits.
	 *
	 * The majors (every third hour, as datum marks them), thinned until
	 * two labels no longer touch and inset so neither runs off an end —
	 * `fit_axis_labels` in dayview_svg.rs, at the one size the strip draws
	 * its ruler at. Where the window ends on midnight its tick is drawn and
	 * named `00`, inset like the leading one and the first to give way when
	 * squeezed, as the CLI's axis closes (da-zyfbl); a window ending inside
	 * the day has no 24 on the band to draw.
	 */
	const ruler = $derived.by(() => {
		if (!geometry) return [] as { x: number; at: number; label: string }[];
		const half = AXIS_FONT * 0.6 + 3;
		let prev = -Infinity;
		return geometry.hours
			.map((hour) => ({ hour, x: bandX(hour.x) }))
			// A window leaves most of the day off the ends of the band.
			.filter(({ x }) => x >= -1 && x <= bandWidth + 1)
			.map(({ hour, x }) => {
				const at = clamp(x, half, Math.max(half, bandWidth - half));
				// The majors are the hours datum names, thinned until two
				// labels no longer touch. Zoomed in, the hours BETWEEN them
				// have room too, and a ruler that named only every third of
				// four hours on screen would be a ruler with one number on it.
				const named = (hour.major || zoomed) && at - half >= prev;
				if (named) prev = at + half;
				return { x, at, label: named ? hour.label : '' };
			});
	});

	// ── the handles: the day lines the day height, the spine the label width ──

	/**
	 * FOLLOW UNTIL TOUCHED, THEN HOLD — and back onto the derived value
	 * RE-ARMS the follow (da-r0ydu).
	 *
	 * What leaves here is `undefined` when the reader has landed on the
	 * height the box would have chosen anyway, and the number otherwise.
	 * There is no flag for «following», because the owner already keeps
	 * only the DIFFERENCE from what the layout says (`overlay`,
	 * layout/session.ts) — so a value dragged and dragged back leaves
	 * nothing held, exactly as every other setting does, and «is it
	 * following?» is a property of the data.
	 *
	 * ⚑ ONLY A MOVE ALONG THE HEIGHT AXIS DECIDES IT. A handle moves its
	 * own size and hands the other on unchanged, on every drop and every
	 * arrow key, so the spine's drag arrives here with the height it
	 * already had — and on a box that happens to derive exactly the held
	 * value that would read as «dragged back onto the derived value» and
	 * quietly re-arm the follow, losing a height the reader set without
	 * touching that axis. Nothing would look wrong at the time (the
	 * pixels are identical by construction); it would surface later, as a
	 * pane resize moving a day they had chosen. The same goes for an arrow
	 * key at a clamp, which is a no-op that must not mean anything either.
	 * So an unmoved height leaves the hold exactly as it found it.
	 */
	function setSizes(px: number, wide: number): void {
		const next = clamp(Math.round(px), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
		if (next !== day) dayHeight = next === boxDay ? undefined : next;
		labelWidth = clamp(Math.round(wide), MIN_LABEL_WIDTH, MAX_LABEL_WIDTH);
		onchange?.({ dayHeight, labelWidth });
	}

	/**
	 * Which size a handle moves: `y` a line between two days (the day
	 * height), `x` the spine (the label width) — da-ray91, vividn: «allow
	 * scaling the days from anywhere in the line dividing the days».
	 */
	type Axes = 'y' | 'x';

	/** Where a drag started, what the values were when it did, and the preview. */
	let sizing = $state.raw<{ x: number; y: number; height: number; width: number; axes: Axes } | null>(null);
	let from0 = { height: 0, width: 0 };

	/**
	 * Take the pointer, and take it away from the browser.
	 *
	 * `preventDefault` is not decoration: without it Chromium reads a
	 * press-and-drag on a control as the start of a native drag and sends
	 * `pointercancel` after the first move, which is a handle that follows
	 * the finger for one frame and then lets go. It also suppresses the
	 * focus a press would give, so the focus is taken explicitly — the
	 * handle is a slider, and a slider the keyboard cannot reach after
	 * being clicked is half a control.
	 */
	function grab(event: PointerEvent, axes: Axes): void {
		event.preventDefault();
		const handle = event.currentTarget as HTMLElement;
		handle.setPointerCapture(event.pointerId);
		handle.focus();
		from0 = { height: day, width: labelWidth };
		sizing = { x: event.clientX, y: event.clientY, height: day, width: labelWidth, axes };
	}
	function sizeTo(event: PointerEvent): void {
		if (!sizing) return;
		// Up is taller (da-hsba) and right is a wider column. The bands
		// follow as scaled pictures; they are re-asked on the drop. A line
		// moves its own size alone, so a day line nudged sideways keeps the
		// column.
		const { axes } = sizing;
		sizing = {
			...sizing,
			height: axes === 'y'
				? clamp(Math.round(from0.height + (sizing.y - event.clientY)), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT)
				: from0.height,
			width: axes === 'x'
				? clamp(Math.round(from0.width + (event.clientX - sizing.x)), MIN_LABEL_WIDTH, MAX_LABEL_WIDTH)
				: from0.width
		};
	}
	function drop(): void {
		if (sizing) setSizes(sizing.height, sizing.width);
		sizing = null;
	}

	/** Make `node` a handle for `axes`: every day line, the spine. */
	function resizes(node: HTMLElement, axes: Axes) {
		const on: [string, EventListener][] = [
			['pointerdown', (event) => grab(event as PointerEvent, axes)],
			['pointermove', sizeTo as EventListener],
			['pointerup', drop],
			['pointercancel', drop],
			['keydown', (event) => onHandleKey(event as KeyboardEvent, axes)]
		];
		for (const [type, handler] of on) node.addEventListener(type, handler);
		return { destroy: () => on.forEach(([type, handler]) => node.removeEventListener(type, handler)) };
	}

	/** A focused line's arrows, its own axis alone: up/down the day height, left/right the label width. */
	function onHandleKey(event: KeyboardEvent, axes: Axes): void {
		const key = (more: string, less: string) => (event.key === more ? 1 : event.key === less ? -1 : 0);
		const up = axes === 'y' ? key('ArrowUp', 'ArrowDown') : 0;
		const right = axes === 'x' ? key('ArrowRight', 'ArrowLeft') : 0;
		if (!up && !right) return;
		event.preventDefault();
		setSizes(day + up * STEP, labelWidth + right * STEP);
	}

	// Today going into the scroller or out of it, from the widget's
	// options, changes what the bottom of the stack is: back to the newest.
	// After the update, not inside it: `anchor` reads the stack the change
	// leaves.
	$effect.pre(() => {
		void freezeToday;
		void tick().then(anchor);
	});

	// ── the zoom: two fingers, ctrl+wheel, and a button back ───────────

	/**
	 * The whole day back — the `⊖` in the strip's corner (da-8rqb9).
	 *
	 * A BUTTON and not a double tap. The double tap was the gesture from
	 * da-ukcf, and da-pzzoz gave the same two taps to the inspect and told
	 * them apart by POSITION: on a mark, inspect; on the picture's ground,
	 * the whole day. That split cannot hold, and vividn's own sentence is
	 * why — «when you have enough data, like me, everywhere has data under
	 * it». On a full day there is no ground left to land on, so the zoom
	 * reset went missing for exactly the readers with the most data. vividn,
	 * 2026-09-22: «double tap shouldn't be zoom reset, let's just have a
	 * small maginifying button with a zero in it in the corner».
	 */
	function resetZoom(): void {
		setWindow([HOUR_FROM, HOUR_TO]);
	}

	/** Ask the owner for `next`, and show it meanwhile. */
	function setWindow(next: [number, number]): void {
		live = null;
		if (next[0] === committed[0] && next[1] === committed[1]) return;
		[hourFrom, hourTo] = next;
		onchange?.({ hourFrom, hourTo });
	}

	/** The band's box on screen — what a gesture's x is a fraction of. */
	function bandBoxNow(): DOMRect | null {
		const band = stripEl?.querySelector('.band');
		const box = band?.getBoundingClientRect();
		return box && box.width > 0 ? box : null;
	}

	/**
	 * Let go of the gesture: the window it was previewing becomes the ask,
	 * or the day height a vertical pinch was previewing is written.
	 */
	function commit(): void {
		const next = live;
		const tall = pinching?.axis === 'y';
		pinching = null;
		if (tall) return drop();
		if (next) setWindow(next);
		live = null;
	}

	/** The fingers down on the strip, by pointer id, and where they are. */
	const fingers = new Map<number, { x: number; y: number }>();
	/**
	 * The pinch in hand: its axis, where it began, the band it began over,
	 * and — a vertical one — the band under the fingers and how far down
	 * it they were, which is the point kept under them (`keepUnder`).
	 */
	let pinching: {
		axis: 'x' | 'y';
		window: [number, number];
		apart: number;
		mid: number;
		box: DOMRect;
		under: Element | null;
		down: number;
	} | null = null;

	const at = () => [...fingers.values()];
	const spread = (axis: 'x' | 'y' = 'x') => Math.abs(at()[0][axis] - at()[1][axis]);
	const middle = (axis: 'x' | 'y' = 'x') => (at()[0][axis] + at()[1][axis]) / 2;
	/**
	 * How far apart two fingers must be before their spread is read as a
	 * zoom. Closer than this, the pair is a two-finger PAN: dividing by a
	 * few pixels turns a millimetre of drift into an order of magnitude.
	 */
	const PINCH_FLOOR = 20;

	// The window listens, not the element: a finger that wanders off the
	// strip mid-pinch is still part of the pinch, and the element would
	// stop hearing it. Armed by the first finger, so the last one off is
	// always heard and no finger is left behind on the map.
	function listen(on: boolean): void {
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
		// The strip's own controls are not the picture: the lines, the `⊖` and the jagged line have the pointer when it is on
		// them, and a press on one is not the start of a gesture.
		if ((event.target as HTMLElement).closest('button, .sizer')) return;
		if (event.pointerType === 'mouse') {
			// The mouse has no finger to register, but its drag still needs
			// the window's listeners: it may leave the strip mid-gesture.
			listen(true);
			return begin(event);
		}
		fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
		if (fingers.size === 1) {
			listen(true);
			return begin(event);
		}
		// A second finger is a pinch, and the first finger's gesture was
		// never one: whatever it had become is dropped before the scale
		// starts, so nothing of it is committed on the way out.
		dropPress();
		if (fingers.size !== 2) return;
		const box = bandBoxNow();
		if (!box) return;
		// A wheel's pause commits it; the fingers are the gesture now.
		clearTimeout(settling);
		// The axis is the pair's: two fingers further apart down the strip
		// than across it pinch the day height, and otherwise the hours.
		const axis = spread('y') > spread('x') ? 'y' : 'x';
		const found = axis === 'y' ? document.elementFromPoint(middle('x'), middle('y'))?.closest('.band') : null;
		const under = found && stackEl?.contains(found) ? found : null;
		const was = under?.getBoundingClientRect();
		pinching = {
			axis,
			window: view,
			apart: spread(axis),
			mid: middle(),
			box,
			under,
			down: was ? (middle('y') - was.top) / was.height : 0
		};
	}

	/**
	 * The vertical pinch: the day height scaled by the fingers' spread, as
	 * the handle's drag previews one (`sizing`), with the day under the
	 * fingers kept under them.
	 */
	function pinchHeight(): void {
		const { apart } = pinching!;
		const by = apart >= PINCH_FLOOR ? spread('y') / apart : 1;
		const height = clamp(Math.round(day * by), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
		sizing = { x: 0, y: 0, height, width: labelWidth, axes: 'y' };
		void tick().then(keepUnder);
	}

	/**
	 * Scroll the stack so the point the pinch began on is under the
	 * fingers' middle again, at the height now on screen. A band outside
	 * the scroller — a pinned today — has nothing to scroll.
	 */
	function keepUnder(): void {
		if (!pinching?.under || !stackEl || fingers.size < 2) return;
		const band = pinching.under.getBoundingClientRect();
		stackEl.scrollTop += band.top + pinching.down * band.height - middle('y');
	}

	function onPointerMove(event: PointerEvent): void {
		if (fingers.has(event.pointerId)) {
			fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
			if (pinching?.axis === 'y' && fingers.size === 2) return pinchHeight();
			if (pinching && fingers.size === 2) {
				const { window: was, apart, mid, box } = pinching;
				const by = apart >= PINCH_FLOOR ? spread() / apart : 1;
				live = zoomWindow(was, by, (mid - box.x) / box.width, (middle() - box.x) / box.width);
				return;
			}
		}
		if (!press) return;
		last = { x: event.clientX, y: event.clientY };
		step(event.clientX, event.clientY);
	}

	function onPointerUp(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (pinching && fingers.size < 2) {
			commit();
			dropPress();
		} else finish(event);
		if (fingers.size === 0 && !press) listen(false);
	}

	/**
	 * A cancel is the browser taking the gesture — the stack's own scroll,
	 * most often. Nothing of a pan is kept: the preview goes with it.
	 */
	function onPointerCancel(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (pinching && fingers.size < 2) commit();
		dropPress();
		if (fingers.size === 0) listen(false);
	}

	/**
	 * `touch-action: pan-y` keeps the browser off a pinch, but a two-finger
	 * drag is still a scroll it may claim — and claiming it sends
	 * `pointercancel` mid-gesture. A gesture of ours along a band is the
	 * same: once it has committed, the move is ours.
	 */
	function onTouchMove(event: TouchEvent): void {
		// Also what tells the pane the move is taken (`defaultPrevented`,
		// PaneFrame.svelte): a range on a band at both ends is no tab swipe.
		if (fingers.size >= 2 || !undecided(mode)) event.preventDefault();
	}

	// ── pointing into a band: a range, or a pan (da-ku7y) ──────────────
	//
	// The recogniser is the nowview's, whole (`$lib/nowpoint.ts`); the
	// table it runs here is in this file's header. What is the strip's own
	// is the window's ENDS — a pan pushing past one is the pane's swipe —
	// and the vertical guard, the stack's scroll being a real gesture a
	// band lies over.

	/** The press in hand on a band, and what it has become. */
	let press: Press | null = null;
	let mode: Mode = $state('press');
	/** Where the pointer is, for the timer that has no event of its own. */
	let last = { x: 0, y: 0 };
	/** Whether the press has lasted long enough to be a range. */
	let long = false;
	let longTimer: ReturnType<typeof setTimeout> | undefined;
	/**
	 * Where the wait is DRAWN, in px across the strip's box, while a press
	 * is still undecided — the nowview's `.hold-mark`, for the same reason:
	 * the 400ms no longer commits to a range, so the bar it used to drop is
	 * no longer the signal that the press registered.
	 */
	let waiting = $state.raw<{ x: number; y: number } | null>(null);
	/**
	 * The DAY the press went down in — the picture's own group, whose
	 * `data-from` / `data-to` every instant of this gesture is read
	 * against (`timeAt`, $lib/hover.ts).
	 *
	 * Which is the whole of «a range is one day's»: held for the length of
	 * the gesture, so wherever the finger goes it is asking THIS day what
	 * time it is there, and `timeAt`'s clamp stops it at the day's own
	 * midnights when it runs off an end.
	 */
	let pressDay: Element | null = null;
	/** The window a pan began from, and the band it began over. */
	let panning: { window: [number, number]; box: DOMRect } | null = null;

	function begin(event: PointerEvent): void {
		if (event.button > 0) return;
		if ((event.target as HTMLElement).closest('.axis')) return scrubFrom(event);
		const day = (event.target as HTMLElement)
			.closest('.band')
			?.querySelector('svg.svg [data-lo]');
		// Off the bands — the date column, the ruler, the room around them
		// — there is no day to point into, and the gesture is the pane's.
		if (!day) return;
		pressDay = day;
		press = {
			x: event.clientX,
			y: event.clientY,
			// The now region is the nowview's: this axis runs to midnight.
			inNow: false,
			atOnce: event.shiftKey
		};
		mode = 'press';
		long = false;
		last = { x: event.clientX, y: event.clientY };
		const home = stripEl?.getBoundingClientRect();
		waiting = home ? { x: event.clientX - home.x, y: event.clientY - home.y } : null;
		clearTimeout(longTimer);
		longTimer = setTimeout(() => {
			long = true;
			step(last.x, last.y);
		}, LONG_MS);
	}

	/** Let go of the press: a cancel, a pinch taking over, or the end of it. */
	function dropPress(): void {
		clearTimeout(longTimer);
		if (mode === 'pan') live = null;
		press = null;
		pressDay = null;
		panning = null;
		waiting = null;
		mode = 'press';
		long = false;
	}

	/** Run the machine to where the pointer is now, and do what the mode does. */
	function step(x: number, y: number): void {
		if (!press) return;
		if (mode === 'scrub') return scrubTo(x);
		const [dx, dy] = [x - press.x, y - press.y];
		// A mostly-vertical finger is the STACK's scroll, held first or not
		// (da-xg1vi: «a vertical drag on the data area scrolls, always»). It
		// is dropped rather than merely ignored, so the timer cannot take a
		// slow scroll away from the reader 400ms in.
		if (undecided(mode) && Math.abs(dy) > SLOP_PX && Math.abs(dy) > Math.abs(dx)) return dropPress();
		const was = mode;
		mode = advance(mode, press, x, y, long);
		// The wait's mark belongs to a press that is still undecided.
		if (mode !== 'press' && mode !== 'hold') waiting = null;
		if (mode === 'range' && was !== 'range') {
			// The bar drops the moment the press is long enough, before any
			// movement, so the finger knows it is no longer panning.
			const at = instantAt(press.x, press.y);
			if (at) pointAt({ at, from: 'daystrip' });
		}
		if (mode === 'pan' && was !== 'pan') {
			// At the end it is pushing past, the swipe is the pane's: there is
			// nothing to pan to, and nothing of this press happens.
			if (panStop.includes(dx > 0 ? 'start' : 'end')) return dropPress();
			const box = bandBoxNow();
			if (!box) return dropPress();
			panning = { window: committed, box };
		}
		if (mode === 'pan') panTo(x);
		if (mode === 'range') range(x, y);
	}

	/** The instant a point on screen is, on the day the press went down in. */
	const instantAt = (x: number, y: number): Date | null => pressDay && timeAt(pressDay, x, y);

	/**
	 * The range between the press and the pointer: it ENDS at the later
	 * instant, because a duration runs back from an entry's time (da-zwct),
	 * and it is the STRIP's, which is how it comes to be spelled
	 * `start_time=` / `end_time=`.
	 */
	function range(x: number, y: number): void {
		const [a, b] = [instantAt(press!.x, press!.y), instantAt(x, y)];
		if (!a || !b) return;
		// Both ENDS to the minute before the length is taken, because the
		// length is what the start pill is read back from (`startOf`,
		// $lib/cursor.ts) and a length rounded on its own disagrees with
		// them: 08:59:43 to 10:30:22 is 90.6 minutes, and «10:30 less 91»
		// is an 08:59 pill over a band the reader dropped on 09:00.
		const [from, to] = +a <= +b ? [onTheMinute(a), onTheMinute(b)] : [onTheMinute(b), onTheMinute(a)];
		pointAt({ at: new Date(to), span: (to - from) / 60_000, from: 'daystrip' });
	}

	/** An instant to the minute a pill can say. */
	const onTheMinute = (at: Date) => Math.round(+at / 60_000) * 60_000;

	/**
	 * The window the pan has dragged to, previewed as the pinch's is: the
	 * content follows the finger, so the hours move against it. Nothing is
	 * asked of datum until the release (`commit`).
	 */
	function panTo(x: number): void {
		if (!panning) return;
		const f = (x - press!.x) / panning.box.width;
		live = zoomWindow(panning.window, 1, 0.5, 0.5 + f);
	}

	/**
	 * The ruler is the TIME AXIS (da-xg1vi, vividn: «if you touch in the
	 * time axis and drag around, in this case, do not pan but rather be able
	 * to precisely choose a time»): a press there points the page at the
	 * time under it at once, and the time follows the finger until the lift.
	 * On the day the selection is on, where a band draws it — scrubbing
	 * moves a time along its own day — and today's otherwise.
	 */
	function scrubFrom(event: PointerEvent): void {
		const days = [...(stripEl?.querySelectorAll('.band svg.svg [data-lo]') ?? [])];
		const at = $pointed ? +$pointed.at : NaN;
		pressDay =
			days.find((d) => Date.parse(d.getAttribute('data-from')!) <= at && at < Date.parse(d.getAttribute('data-to')!)) ??
			stripEl?.querySelector('.band[data-days-ago="0"] svg.svg [data-lo]') ??
			null;
		if (!pressDay) return;
		press = { x: event.clientX, y: event.clientY, inNow: false, atOnce: false };
		mode = 'scrub';
		scrubTo(event.clientX);
	}

	function scrubTo(x: number): void {
		const box = pressDay?.closest('.band')?.getBoundingClientRect();
		const at = box && instantAt(x, box.y + box.height / 2);
		if (at) setCursor(at);
	}

	/**
	 * The end of a one-pointer gesture. A press that committed to nothing
	 * is left alone: it is the CLICK the `picture` action takes — the entry
	 * under it, or the time where there is none ($lib/hover.ts).
	 *
	 * Except a STILL LONG TAP, which is a TIME wherever it lands, a mark
	 * included (da-xg1vi, vividn: «long tap / right click → select a
	 * time»), and SPENDS the press, so the click that may follow it does
	 * not take the entry under it instead.
	 */
	function finish(event: PointerEvent): void {
		if (!press) return;
		const still = Math.hypot(event.clientX - press.x, event.clientY - press.y) <= SLOP_PX;
		if (mode === 'hold' && still) {
			const at = instantAt(press.x, press.y);
			if (at) setCursor(at);
			spendPress();
		}
		if (mode === 'pan') commit();
		dropPress();
	}

	// ── the selection's pills, in the strip's own margin ───────────────

	/** The pills the selection made — the ones drawn over their own marks. */
	const marginPills = $derived($pills.filter((p) => p.id === 'cursor' || p.id === SPAN_ID));

	/**
	 * Where each margin pill's LEFT edge goes, in px across the strip's box.
	 *
	 * A range's two pills BRACKET it — the start pill's right edge against
	 * the band's leading edge, the end pill's left edge against its
	 * trailing one — rather than sitting centred over their marks as the
	 * nowview's do. Its pills are `-qq` and `dur=25m`; these are
	 * `start_time=09:00` and `end_time=10:30`, three times as wide, over a
	 * range that on a whole day's band is often a few dozen pixels: centred,
	 * each would be drawn over the other. Bracketing, each still touches
	 * the edge it names and the pair can never collide. A selection with no
	 * length is one pill, and that one IS centred on its line.
	 *
	 * Read off the DRAWN marks (`drawCursor`, $lib/hover.ts), so a pill and
	 * the mark under it cannot disagree by a pixel. Only one band draws them
	 * — the one whose day holds the instant — whichever day of the stack the
	 * press fell in.
	 */
	let spots: { time: number | null; span: number | null } = $state.raw({ time: null, span: null });

	// Re-placed after anything that MOVES a mark: the selection itself, a
	// band's picture, the window, the box. After `tick`, so the placing
	// reads the marks this change drew and not the ones it replaced.
	$effect.pre(() => {
		void [$pointed, shown, view, width, shownDayHeight, shownLabelWidth];
		void placePills();
	});

	/**
	 * Twice, because the first pass is what puts the pills on screen: with
	 * no pill drawn there is no width to hold one inside the strip by, and
	 * a pill placed by a width of zero is a pill half off the end.
	 */
	async function placePills(): Promise<void> {
		for (let pass = 0; pass < 2; pass++) {
			await tick();
			measurePills();
		}
	}

	function measurePills(): void {
		if (!stripEl) return;
		const home = stripEl.getBoundingClientRect();
		const box = (selector: string) => stripEl?.querySelector(selector)?.getBoundingClientRect() ?? null;
		const wide = (selector: string) => box(selector)?.width ?? 0;
		const band = box('svg.svg .cursor-span');
		const head = box('svg.svg .cursor-head');
		const held = (x: number, w: number) => Math.max(0, Math.min(Math.max(0, home.width - w), x));
		const [startW, endW] = [wide('.daystrip-margin .pill.dur'), wide('.daystrip-margin .pill.time')];
		spots = {
			span: band ? held(band.left - home.x - startW, startW) : null,
			time: head
				? held(
						band ? band.right - home.x : head.left + head.width / 2 - home.x - endW / 2,
						endW
					)
				: null
		};
	}

	/**
	 * A notch of the wheel is half a doubling, so one lands a window at a
	 * round fraction of the one before it. A trackpad pinch arrives here
	 * too, as a shower of small deltas: they accumulate into one gesture,
	 * which the pause at the end of it commits.
	 *
	 * ctrl zooms and shift pans, which is what a browser's own reading of
	 * the two modifiers over a picture is; a BARE wheel is left alone —
	 * that is the stack scrolling back through the days.
	 */
	const NOTCH = 200;
	let settling: ReturnType<typeof setTimeout> | undefined;

	function onWheel(event: WheelEvent): void {
		if (pinching || (!event.ctrlKey && !event.shiftKey)) return;
		const box = bandBoxNow();
		if (!box) return;
		event.preventDefault();
		const from = live ?? committed;
		const where = clamp((event.clientX - box.x) / box.width, 0, 1);
		// Down and right are later in the day: the window follows the
		// wheel, so the picture moves against it.
		const panned = (event.deltaX || event.deltaY) / box.width;
		live = event.ctrlKey
			? zoomWindow(from, Math.pow(2, -event.deltaY / NOTCH), where)
			: zoomWindow(from, 1, where, where - panned);
		clearTimeout(settling);
		settling = setTimeout(commit, 120);
	}

	onDestroy(() => {
		clearTimeout(settling);
		clearTimeout(longTimer);
		listen(false);
		if (mounted.get(heldAs) === holding) mounted.delete(heldAs);
		held.delete(heldAs);
		held.set(heldAs, holding());
		if (held.size > STRIPS) held.delete(held.keys().next().value!);
	});

	function holding(): Held {
		const month = (of: Cache): Cache => ({
			...of,
			runs: new Map([...of.runs].filter(([, asked]) => asked.end === 'geometry' || asked.end <= MONTH))
		});
		function recent<T>(of: Record<number, T>): Record<number, T> {
			return Object.fromEntries(Object.entries(of).filter(([end]) => Number(end) <= MONTH));
		}
		return {
			cache: month(cache),
			sizes: new Map([...sizes].map(([size, of]) => [size, month(of)])),
			shown: recent(shown),
			drawn: recent(drawn),
			midnights,
			onDay,
			minute: wallMinute(),
			width,
			height,
			geometry,
			geometryBox
		};
	}

	/**
	 * The jagged line's teeth: a vertex every 2 units across a 400-wide
	 * viewBox, stretched to the strip's width — about 5px a stroke at
	 * 1000px and 2px on a 390px phone, still a tear rather than a blur.
	 */
	const ZIGZAG = Array.from({ length: 201 }, (_, i) => `${i * 2},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="strip"
		bind:this={stripEl}
		data-mode={mode}
		data-height={height}
		onpointerdown={onPointerDown}
		{@attach (el) => on(el, 'touchmove', onTouchMove, { passive: false })}
		{@attach (el) => on(el, 'wheel', onWheel, { passive: false })}
		style:--band="{bandBox(shownDayHeight)}px"
		style:--pitch="{dayPitch(shownDayHeight)}px"
		style:--today="{todayBox(shownDayHeight)}px"
		style:--label="{shownLabelWidth}px"
		style:--axis="{AXIS_HEIGHT}px"
		style:--divider="{DIVIDER}px"
		style:--tear="{freezeToday ? seamAt : AXIS_HEIGHT}px"
		style:--seam-line="{SEAM}px"
		style:--sliver="{sliver}px"
		style:--sx={scaleX}
		style:--sy={scaleY}
	>
		{#if rows.length > 0}
			<div class="stack" bind:this={stackEl} onscroll={onScroll}>
				{#if ended && first?.date}<p class="quiet first">first entry · {first.date}</p>{/if}
				{#each rows as end, i (end)}
					<div class="row" class:now={end === 0}>
						{#if i > 0}<div class="sizer day-line" data-pans="xy" use:resizes={'y'}></div>{/if}
						<div class="stamp" style:font-size="{stampFont}px">
							{#if flagged[end]?.length}<IntegrityMark fields={flagged[end]} />{/if}
							{#each stamp(geometry?.today ?? null, end, stampLines) as line}<span>{line}</span
								>{/each}
						</div>
						<div
							class="band"
							class:past={end > 0}
							data-days-ago={end}
							data-days="1"
							data-pans="x"
							data-pan-stop={panStop}
							style:--zoom-x="{bandZoom(end).offset}px"
							style:--zoom={bandZoom(end).scale}
							style:--stand={stand(end)}
						>
							<div class="shot" class:previewing={!!sizing}>
								{#if shown[end]}<CliOutput {...picture(shown[end], unsynced)} />{/if}
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}

		<!--
			The two lines that are the keyboard's sliders (da-5wa91): the
			divider, over a pinned today or under one that scrolls and never
			scrolled away, for the day height; and the spine between the date
			column and the bands, the 00 mark, for the label width.
		-->
		<div class="divider" class:seam={freezeToday}>
			<div
				class="sizer day-line today-line"
				data-pans="xy"
				role="slider"
				tabindex="0"
				aria-label="day height"
				aria-orientation="vertical"
				aria-valuemin={MIN_DAY_HEIGHT}
				aria-valuemax={MAX_DAY_HEIGHT}
				aria-valuenow={shownDayHeight}
				aria-valuetext="{shownDayHeight} pixels per day"
				use:resizes={'y'}
			></div>
		</div>
		<div
			class="sizer spine"
			data-pans="xy"
			role="slider"
			tabindex="0"
			aria-label="date-label width"
			aria-orientation="horizontal"
			aria-valuemin={MIN_LABEL_WIDTH}
			aria-valuemax={MAX_LABEL_WIDTH}
			aria-valuenow={shownLabelWidth}
			aria-valuetext="{shownLabelWidth} pixels of date label"
			use:resizes={'x'}
		></div>

		{#if zoomed}
			<!--
				The way back from a window, now that the double tap is the
				inspect's alone (da-8rqb9): a magnifier with a zero in it, at
				the bottom of the DATE COLUMN — the one column of the strip no
				picture is ever drawn in (`.gridlines` and every band begin at
				`--label`). What is under it is the blank gutter beside the
				ruler and the tail of the bottom band's date stamp; no mark,
				at any density, which is the whole point of the change.

				Shown only while there IS a window: at rest it would be a
				control that does nothing, and the strip's own `.jagged` sets
				the precedent — a way back appears when there is somewhere to
				come back from.
			-->
			<button
				class="unzoom"
				type="button"
				title="the whole day back"
				aria-label="reset the zoom to the whole day"
				onclick={resetZoom}
			>
				<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
					<circle class="lens" cx="6.5" cy="6.5" r="5" />
					<line x1="10.3" y1="10.3" x2="14.5" y2="14.5" />
					<text
						class="nought"
						x="6.5"
						y="6.5"
						text-anchor="middle"
						dominant-baseline="central"
						font-size="7">0</text
					>
				</svg>
			</button>
		{/if}

		{#if jagged}
			<!--
				A real element, not paint: the tear between the divider and a
				day that is not the newest, and the way back. Clicking or
				pressing it puts the newest day back against the divider.
			-->
			<button
				class="jagged"
				onclick={toNewest}
				aria-label="the band above the divider is not the newest day — bring it back"
			>
				<svg viewBox="0 0 400 6" preserveAspectRatio="none" aria-hidden="true">
					<polyline points={ZIGZAG} />
				</svg>
			</button>
		{/if}

		{#if freezeToday}
			<div class="row now pinned">
				<div class="stamp" style:font-size="{stampFont}px">
					{#if flagged[0]?.length}<IntegrityMark fields={flagged[0]} />{/if}
					{#each stamp(geometry?.today ?? null, 0, stampLines) as line}<span>{line}</span>{/each}
				</div>
				<div
					class="band"
					data-days-ago="0"
					data-days="1"
					data-pans="x"
					data-pan-stop={panStop}
					style:--zoom-x="{bandZoom(0).offset}px"
					style:--zoom={bandZoom(0).scale}
					style:--stand={stand(0)}
				>
					<div class="shot" class:previewing={!!sizing}>
						{#if shown[0]}<CliOutput {...picture(shown[0], unsynced)} />{:else}<p class="quiet">…</p>{/if}
					</div>
				</div>
			</div>
		{/if}

		<!-- The ruler, drawn once, under whichever band is at the bottom. -->
		<div class="axis" data-pans="xy">
			{#if geometry && bandWidth > 0}
				<svg class="ruler" width={bandWidth} height={AXIS_HEIGHT} aria-hidden="true">
					<line class="baseline" x1="0" x2={bandWidth} y1="0.5" y2="0.5" />
					{#each ruler as tick}
						<line class="tick" x1={tick.x} x2={tick.x} y1="0.5" y2="4.5" />
						{#if tick.label}
							<text
								class="tick-label"
								x={tick.at}
								y={AXIS_HEIGHT - 2}
								text-anchor="middle"
								font-size={AXIS_FONT}>{tick.label}</text
							>
						{/if}
					{/each}
				</svg>
			{/if}
		</div>

		<!--
			The selection's pills, each over the mark it names (da-ku7y).

			OVER the stack rather than a row of its own: a reserved row
			would cost the strip a day of past on every screen and in every
			box, selection or none, and the strip's whole argument is how
			much past a box can hold. Nothing in it declares `data-pans`, so
			a swipe that starts here changes the pane's tab, exactly as the
			nowview's margin does; it takes the pointer only where a pill
			actually is.
		-->
		{#if marginPills.length}
			<div class="daystrip-margin" aria-label="selection">
				{#each marginPills as pill (pill.id)}
					{@const x = pill.id === SPAN_ID ? spots.span : spots.time}
					{#if x !== null}
						<span class="margin-pill" style:left="{x}px"><Pill {pill} /></span>
					{/if}
				{/each}
			</div>
		{/if}

		<!-- The wait's mark, the nowview's own (da-pzzoz): a ring closing
		     over the 400ms, and the range's dashes once it is up. The 400ms
		     stopped committing, so the bar it used to drop is no longer the
		     sign that the press was heard. -->
		{#if waiting && (mode === 'press' || mode === 'hold')}
			<div
				class="hold-mark"
				class:held={mode === 'hold'}
				aria-hidden="true"
				style:left="{waiting.x}px"
				style:top="{waiting.y}px"
				style:--long="{LONG_MS}ms"
			>
				<span class="stem"></span>
				<span class="ring"></span>
			</div>
		{/if}

		<!--
			The gridlines the whole stack shares, at the x `--show json`
			reports for them — the thing that makes a column of separate
			pictures read as one. They run from the top of the scroller to
			the ruler, through every divider.
		-->
		{#if geometry && bandWidth > 0}
			<svg class="gridlines" width={bandWidth} height="100%" aria-hidden="true">
				{#each geometry.hours as hour}
					<line
						class="gridline"
						data-hour={hour.hour}
						x1={bandX(hour.x)}
						x2={bandX(hour.x)}
						y1="0"
						y2="100%"
						stroke-opacity={hour.major ? 0.4 : 0.1}
					/>
				{/each}
			</svg>
		{/if}
	</div>
</Pane>

<style>
	/* The box is the control: the strip is exactly as tall as the pane's
	   body, and `depth` is read off that height. A box shorter than one
	   day (da-zh2o) cuts today's band from the TOP — the overflow goes
	   past the start edge — so the ruler along its bottom is the last
	   thing to go, and the body never scrolls.

	   One background for the whole column, bands and gaps alike: datum's
	   own (`BG_COLOR` in dayview_svg.rs), so nothing shows between two
	   days but the divider. */
	.strip {
		--picture: #000;
		position: relative;
		height: 100%;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		overflow: hidden;
		background: var(--picture);
		/* One finger still scrolls the stack back through the days; two are
		   the window's, not the browser's page zoom (da-ukcf). */
		touch-action: pan-y;
		/* The strip is all picture — bands, stamps, ruler — pressed and
		   tapped, never copied: nothing selects, and a long press raises no
		   iOS callout (da-mu5op). */
		user-select: none;
		-webkit-user-select: none;
		-webkit-touch-callout: none;
	}

	/* The room above the divider, whatever is left of the box once the
	   ruler and today's band have theirs. The past in it may run past it
	   by up to one day (see `depth`), and then it scrolls. */
	.stack {
		/* No basis: the stack is the room left over, so a strip too short
		   for everything takes from the stack first, down to its sliver,
		   and only then from today — never in proportion to fourteen days. */
		flex: 1 1 0;
		min-height: var(--sliver);
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		/* No scrollbar: it would take layout width from the stack and not
		   from today's band below it, and the two would stop lining up. */
		scrollbar-width: none;
	}
	.stack::-webkit-scrollbar {
		display: none;
	}
	/* The newest day against the divider when the stack has room to spare:
	   the oldest row takes the slack above it. An auto margin rather than
	   `justify-content: flex-end`, which in a scroll box puts the overflow
	   past the start edge where no scroll reaches it. */
	.stack > :first-child {
		margin-top: auto;
	}
	/* Above the oldest band once it is the first entry's: nothing older. */
	.first {
		flex: none;
		padding: 6px 0 4px var(--label);
		font-size: 11px;
	}

	/* One day: its stamp in the strip's own column, its picture beside it.
	   The days TOUCH — the border IS the divider between them, 1px, and
	   the first row carries a transparent one so every day costs the same
	   pitch wherever it falls. */
	.row {
		position: relative;
		--line: var(--divider);
		display: flex;
		align-items: stretch;
		height: var(--band);
		flex: none;
		border-top: var(--divider) solid transparent;
		/* The height IS the day; the divider is what the row costs on top
		   of it. Under the page's border-box the two would share the same
		   pixels and a day would be a pixel short of its pitch. */
		box-sizing: content-box;
	}
	.row + .row {
		border-top-color: var(--rule);
	}
	/* Over today, pinned or not, the seam: thicker than a day's divider. */
	.row.now {
		--line: var(--seam-line);
		border-top-width: var(--line);
	}
	/* Today's band is the strip's bottom edge. In a strip too short for it
	   and the stack's sliver it gives way, clipped from the TOP: the picture
	   sits on the band's floor and runs out past its start edge, which the
	   band's `overflow: hidden` cuts. */
	.row.pinned {
		height: var(--band);
		flex: 0 1 auto;
		min-height: 0;
		border-top: 0;
	}
	.row.pinned > .band {
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
	}
	.row.pinned .shot {
		flex: none;
	}

	.stamp {
		position: relative;
		flex: 0 0 var(--label);
		width: var(--label);
		overflow: hidden;
		display: flex;
		flex-direction: column;
		justify-content: center;
		align-items: flex-end;
		gap: 1px;
		padding-right: 4px;
		box-sizing: border-box;
		color: var(--ink-bright);
		line-height: 1.15;
		white-space: nowrap;
	}

	.band {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
	}
	/* The picture at the size it was drawn, scaled only WHILE the handle is
	   being dragged — the preview.

	   The class matters: `transform` makes an element the containing block
	   of any `position: fixed` descendant, and the hover label is one
	   (hover.ts appends it inside the picture). A resting `scale(1, 1)`
	   would still do that, and the label would be positioned — and scaled
	   — against the band instead of the screen. */
	.shot.previewing {
		transform: scale(var(--sx), var(--sy));
		transform-origin: 0 0;
	}

	/* The hour window (da-ukcf): the picture is drawn for the WHOLE day at
	   the band's width, and this is the part of it the band shows. The
	   same map the gridlines and the ruler are drawn from, so the three
	   move together; the band clips the rest.

	   On the picture itself rather than on `.shot`, which is the hover
	   label's positioning parent: a transform there would make it the
	   containing block of a `position: fixed` tip (see above), where the
	   tip is the svg's SIBLING (hover.ts) and a transform on the svg does
	   not reach it. The pointer still finds the right instant either way —
	   `timeAt` reads the picture through its own `getScreenCTM`, which is
	   the transform included.

	   Always, not only while there is a window: the map is the identity at
	   the whole day, and a band whose picture is still from before a
	   resize needs mapping until its own answer lands. The variables are
	   the BAND's for the same reason — the map is each picture's own
	   (`mapFor`). And `--stand` stretches a picture drawn at another day
	   height to this one until its own answer lands (`stand`), here for the
	   same reason: not on `.shot`. */
	.band :global(svg.svg) {
		transform: translateX(var(--zoom-x)) scale(var(--zoom), var(--stand, 1));
		transform-origin: 0 0;
		/* And laid out at its OWN width, which is what the map above is
		   written against. CliOutput shrinks any picture to its box
		   (`max-width: 100%`, da-m0cj) — right for a nowview's fixed
		   canvas, and for a WINDOWED band it squeezed the whole day into
		   the band and then translated that squeezed copy off the left:
		   a zoomed strip drew a 30px-tall sliver of the day's last hours
		   and nothing else (found while building da-ku7y, whose gesture
		   reads the instant off this picture's own geometry). It looked
		   right because the gridlines, the ruler and the hour numbers
		   over it are the STRIP's own, drawn from the JSON. */
		max-width: none;
	}

	/* The map, and a size being previewed (`.shot.previewing`), stretch
	   TIME and the day; neither may stretch a mark (da-dqbso). A duration
	   — a block, a lane, the now strip — is as long as its hours and is
	   left to them; every other mark is scaled back about its own anchor
	   to the size the picture drawn at the new size gives it: a dot, a
	   glyph, an icon, the selection's arrow, a label grows with the day's
	   height (as datum sizes them) and not with the window, and a line
	   keeps its stroke. The rules after the first carry its guard: a
	   `:not` counts what it names, so without it the first one wins. */
	.band :global(svg.svg circle:not(defs *)),
	.band :global(svg.svg text:not(defs *)),
	.band :global(svg.svg use:not(defs *)),
	.band :global(svg.svg polygon:not(defs *)),
	.band :global(svg.svg rect.point-bg:not(defs *)),
	.band :global(svg.svg line:not(defs *)) {
		transform-box: fill-box;
		transform-origin: center;
		scale: calc(var(--sy) * var(--stand, 1) / (var(--zoom) * var(--sx))) 1;
	}
	.band :global(svg.svg line:not(defs *)) {
		scale: calc(1 / (var(--zoom) * var(--sx))) 1;
	}
	.band :global(svg.svg text:not([text-anchor='middle']):not([text-anchor='end'])) {
		transform-origin: left;
	}
	.band :global(svg.svg text[text-anchor='end']:not(defs *)) {
		transform-origin: right;
	}

	/* The spine: the same 1px rule, the other way, at the 00 mark. */
	.spine {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--label);
		width: var(--divider);
		background: var(--rule);
		z-index: 2;
		cursor: ew-resize;
	}

	/* The lines are handles too (da-ray91): a few pixels either side of
	   each, the spine's the label width and a day line's the day height —
	   the line over each day past the first in the stack, the row's top
	   border, and the divider. A day line only across the date column
	   (da-xg1vi): over the bands a finger down it is the scroll. Nothing
	   is drawn: the line already is. */
	.sizer {
		touch-action: none;
	}
	/* Focused, the ring is round the hit area: the spine's own 1px is no
	   ring a reader sees. */
	.day-line:focus-visible,
	.spine:focus-visible::before {
		outline: 1px solid var(--accent);
	}
	.spine:focus-visible {
		outline: none;
		background: var(--accent);
	}
	.spine::before {
		content: '';
		position: absolute;
		inset: 0 -4px;
	}
	.day-line {
		position: absolute;
		left: 0;
		width: var(--label);
		/* The border is above the row's padding box: centre on it. */
		top: calc(-4px - var(--line));
		height: calc(8px + var(--line));
		z-index: 1;
		cursor: ns-resize;
	}
	.divider > .day-line {
		top: -4px;
		height: calc(100% + 8px);
	}

	/* Full width, and nothing else: the line itself — the seam's over a
	   pinned today, a day's under one that scrolls. */
	.divider {
		position: relative;
		flex: none;
		height: var(--divider);
		background: var(--rule);
		z-index: 2;
	}
	.divider.seam {
		height: var(--seam-line);
	}

	/* The zoom reset, in the bottom corner of the date column (da-8rqb9).
	   Right-aligned inside that column.

	   A fingertip's 26px over a 16px
	   glyph: §12's box is not the drawing (the old beta ⓘ's pattern). */
	.unzoom {
		position: absolute;
		bottom: 0;
		left: max(0px, calc(var(--label) - 26px));
		width: 26px;
		height: 26px;
		padding: 0;
		border: 0;
		background: none;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--quiet);
		cursor: pointer;
		z-index: 4;
	}
	.unzoom:hover,
	.unzoom:focus-visible {
		color: var(--accent);
	}
	.unzoom:focus-visible {
		outline: 1px solid var(--accent);
	}
	.unzoom .lens,
	.unzoom line {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.4;
		stroke-linecap: round;
	}
	.unzoom .nought {
		fill: currentColor;
		stroke: none;
	}

	/* Over the divider, clear of the date column: the tear, and the way
	   back — over the ruler when today scrolls with the rest. Absolute
	   against the strip and clamped into the box, so a strip shorter than
	   one day still has it (the old notch's rule, da-zh2o). */
	.jagged {
		position: absolute;
		transform: translateY(50%);
		z-index: 3;
		bottom: min(var(--tear), calc(100% - 13px));
		left: calc(var(--label) + 16px);
		right: 0;
		height: 6px;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
		line-height: 0;
	}
	.jagged svg {
		display: block;
		width: 100%;
		height: 6px;
	}
	.jagged polyline {
		fill: none;
		stroke: var(--alarm);
		stroke-width: 1.2;
		vector-effect: non-scaling-stroke;
	}
	.jagged:focus-visible {
		outline: 1px solid var(--accent);
	}

	/* The ruler: the strip's own, so it is drawn once however many bands
	   there are, and at the column's x rather than each picture's. */
	.axis {
		flex: none;
		height: var(--axis);
		margin-left: var(--label);
		overflow: hidden;
		/* The time axis: a finger on it scrubs the time (da-xg1vi), so the
		   browser gets no scroll and the pane no swipe out of it. */
		touch-action: none;
		cursor: col-resize;
	}
	.ruler {
		display: block;
	}
	.baseline,
	.tick {
		stroke: var(--ink-bright);
		stroke-opacity: 0.6;
	}
	.tick-label {
		fill: var(--ink-bright);
	}

	/* The margin: a strip's width of room across the top of the stack,
	   the pills held inside it (see `placePills`). It lies over a band, so
	   it takes no pointer of its own — only the pills in it do, and they
	   are there only while there is a selection. */
	/* THE WAIT'S MARK (da-pzzoz). The nowview's, to the pixel and to the
	   millisecond: one gesture across two views cannot have two looks. The
	   stem is short here — a band is one day of a stack, and a line down
	   the whole strip would cross days the gesture is not about. */
	.hold-mark {
		position: absolute;
		width: 0;
		height: 0;
		pointer-events: none;
		z-index: 4;
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

	.hold-mark .stem {
		display: none;
	}

	.hold-mark.held .stem {
		display: block;
		position: absolute;
		left: -0.75px;
		top: calc(-1 * var(--band) / 2);
		width: 1.5px;
		height: var(--band);
		background: repeating-linear-gradient(to bottom, #ffffff 0 3px, transparent 3px 6px);
		opacity: 0.8;
	}

	@media (prefers-reduced-motion: reduce) {
		.hold-mark .ring {
			animation: none;
			opacity: 0.8;
		}
	}

	.daystrip-margin {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		height: 1.75rem;
		pointer-events: none;
		z-index: 3;
		/* The pill reads at the command line's size where the bar draws it;
		   here it is a label over a picture and takes a smaller one. */
		--input-font: 0.7rem;
	}

	/* Against the mark it names (see `measurePills`), and pressable where
	   the strip is not. */
	.margin-pill {
		position: absolute;
		top: 0;
		max-width: 100%;
		pointer-events: auto;
	}

	/* The stitching: over the bands, under nothing, and never in the way
	   of a pointer asking a mark what it is. */
	.gridlines {
		position: absolute;
		top: 0;
		bottom: var(--axis);
		left: var(--label);
		pointer-events: none;
		z-index: 1;
	}
	.gridline {
		stroke: white;
		vector-effect: non-scaling-stroke;
	}

	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}
</style>
