<!--
	The day strip: a stack of days, one picture each, stitched by the web.

	The comparison a person actually makes is *is today like yesterday,
	like last Tuesday* — which needs the earlier days on screen at the same
	scale, under the same x-axis (da-cgyh). So the strip is a scroller of
	the days before today with today under it, and the whole column reads
	as ONE picture.

	How it is drawn (da-iu9i). Each day is its own `dayview` — one day,
	no date column, no ruler, no outer margin (`bandArgv` in $lib/panes.ts)
	— so what comes back is a band exactly as wide as the column and
	exactly `dayHeight` tall, all of it data area. A day lands as soon as
	it is drawn instead of the whole past arriving at once, which is the
	snappiness; the stitching is then the web's job and it has four parts:

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

	The controls are all on the divider above today, which is the one line
	in the strip that belongs to no day:

	- the `+` where the label column meets the bands — the 00 mark. Drag
	  it up and down for the day height, left and right for the label
	  width, arrows for both. During the drag the bands are SCALED to the
	  size being previewed and re-asked only on release: one `dayview` per
	  band per frame is a render loop, and this is the "better preview
	  rendering when changing the day height/label width".
	- the freeze-today toggle at the far left, wearing the same mark the
	  pane menu's bar-on-bottom wears. On, today is pinned under the
	  scroller and never scrolls away (what the strip has always done);
	  off, today is simply the last band in the scroller and goes with the
	  rest.

	The strip also ZOOMS onto an hour range (da-ukcf): two fingers pinching
	over it, or ctrl+wheel on a desktop, narrow the window; a two-finger
	drag or shift+wheel pans it; a double-tap — or the pane menu's `Reset
	widget` — gives the whole day back. Every band at once, because they
	share the x-axis. `dayview` has no window flag, so a window is a
	picture drawn for the whole day at the window's SCALE and shown through
	the band: one offset-and-scale (`windowMap` in $lib/panes.ts) puts the
	hours in it, and the strip's own gridlines and ruler are drawn from the
	same map. Under the gesture nothing is asked at all — the pictures on
	screen are mapped where they are, and the release is what asks datum
	for the window at its own width, so the lines come back crisp.

	How many days the box SHOWS: as many as it holds (da-zb0d). The strip
	fills its container and the pane it sits in is what says how tall it
	is. How many days the stack HOLDS is a second number (da-bewc): a
	stack holding exactly what its box shows overflows it by at most the
	one partly revealed day, and both the wheel and the finger ran out
	within the gesture. So the stack holds a FORTNIGHT at least, however
	short the box, and that past is what the reader scrolls back through.

	What the reader sets — `dayHeight`, `labelWidth`, `freezeToday`,
	`hourFrom`/`hourTo` — is a prop in and a `change` out; the dashboard
	keeps them in the widget's settings, and the strip itself writes
	nothing down.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy, onMount, tick } from 'svelte';
	import { browser } from '$app/environment';
	import Pane from './Pane.svelte';
	import CliOutput from './CliOutput.svelte';
	import { minute } from './clock.js';
	import { run, type RunResult } from './run.js';
	import {
		AXIS_HEIGHT,
		DIVIDER,
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
		dayPitch,
		geometryArgv,
		pictureWidth,
		todayBox,
		wholeDay,
		windowMap,
		zoomWindow
	} from './panes.js';
	import { settle } from './layout/resizing.js';

	/** Any change re-runs every band. The frame bumps it after each run. */
	export let epoch = 0;

	/** How tall one day is drawn, in pixels — datum's `--day-height`. */
	export let dayHeight: number = DEFAULT_DAY_HEIGHT;
	/** How wide the strip's own date-stamp column is, in pixels. */
	export let labelWidth: number = DEFAULT_LABEL_WIDTH;
	/** Is today pinned under the scroller, or the last band inside it? */
	export let freezeToday = true;
	/** The hour range every band is drawn onto — the whole day at rest (da-ukcf). */
	export let hourFrom: number = HOUR_FROM;
	export let hourTo: number = HOUR_TO;

	/** What the reader moved, folded into the widget's settings by the owner. */
	type Setting = {
		dayHeight?: number;
		labelWidth?: number;
		freezeToday?: boolean;
		hourFrom?: number;
		hourTo?: number;
	};
	const dispatch = createEventDispatcher<{ change: Setting }>();

	/** How far back the strip will reach, however tall its box grows. */
	const MAX_DEPTH = 60;
	/** The least past the stack holds, so there is somewhere to scroll to. */
	const FORTNIGHT = 14;
	/** What one press of an arrow key moves the handle, on either axis. */
	const STEP = 5;

	const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

	// Whatever the owner hands in — a saved setting from an older layout
	// included — is drawn at a size the strip can draw legibly.
	$: dayHeight = clamp(Math.round(dayHeight) || DEFAULT_DAY_HEIGHT, MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
	$: labelWidth = clamp(
		Math.round(labelWidth) || DEFAULT_LABEL_WIDTH,
		MIN_LABEL_WIDTH,
		MAX_LABEL_WIDTH
	);

	let body: HTMLDivElement | undefined;
	let stripEl: HTMLDivElement | undefined;
	let stackEl: HTMLDivElement | undefined;
	/** The box the pane gives the strip, in whole pixels, once measured. */
	let width = 0;
	let height = 0;
	/** Is the newest day the bottom-most band in view? */
	let atBottom = true;

	/** The band's own width on screen: the box less the stamp column. */
	$: bandWidth = Math.max(0, width - labelWidth);

	// ── the hour window ────────────────────────────────────────────────
	//
	// The window the bands were DRAWN for, and the one they are shown
	// through: the same at rest, and a gesture is the difference between
	// them. Which is why the gesture costs nothing — the pictures are
	// mapped where they are, and datum is asked again only on release.

	/** The window a gesture is previewing, or null when none is in hand. */
	let live: [number, number] | null = null;
	/** The window the pictures are asked for: the widget's setting, made sane. */
	$: committed = clampWindow(hourFrom, hourTo);
	/** The window on screen. */
	$: view = live ?? committed;
	/** How wide a day has to be drawn for `committed` to fill the band. */
	$: pictureBox = bandWidth > 0 ? pictureWidth(bandWidth, committed[1] - committed[0]) : 0;

	/**
	 * Each picture is mapped by the width IT was drawn at, not by the one
	 * just asked for.
	 *
	 * Which matters for the beat after a release: the window has changed,
	 * every band has been asked again, and what is on screen meanwhile is
	 * the picture drawn for the window BEFORE. Mapped by the new width it
	 * would be stretched by the ratio between them and show the wrong
	 * hours — a jump, and then a jump back as each answer lands, one CLI
	 * process apart. Mapped by its own, it shows exactly the hours it is
	 * meant to and merely a little soft, which is the gesture's preview
	 * carrying on until the crisp answer arrives.
	 */
	const IDENTITY = { offset: 0, scale: 1 };
	$: mapFor = (full: number) =>
		full > 0 && bandWidth > 0 ? windowMap(view, full, bandWidth) : IDENTITY;
	/** The gridlines and the ruler are drawn from the GEOMETRY's own width. */
	$: gridMap = mapFor(geometryBox);
	/** Where along the band a gridline or a ruler tick falls. */
	$: bandX = (x: number) => gridMap.offset + gridMap.scale * x;
	/** Is there a window at all, or is the whole day on screen? */
	$: zoomed = !wholeDay(view);
	/** The transform each band's picture is shown under. */
	$: bandZoom = (end: number) => mapFor(drawnAt[end]);

	/**
	 * The days the box SHOWS. The room is whatever is left once the ruler,
	 * the divider and — while today is frozen — today's own band have
	 * theirs; a day only partly revealed still counts, because it is a day
	 * that has to be drawn and the first one the stack scrolls up to.
	 */
	$: room = Math.max(
		0,
		height - AXIS_HEIGHT - DIVIDER - (freezeToday ? bandBox(dayHeight) : 0)
	);
	$: slots = room > 0 ? Math.ceil((room - bandBox(dayHeight)) / dayPitch(dayHeight)) + 1 : 0;
	/** Of those slots, the ones the PAST gets: unfrozen, today is in the scroller too. */
	$: fits = Math.max(0, freezeToday ? slots : slots - 1);
	/**
	 * The days the stack HOLDS: a fortnight at least, and as many as the
	 * box shows when it shows more (da-bewc). Zero is a box with no room
	 * above today at all, which has nothing to scroll either way.
	 */
	$: depth = slots > 0 ? Math.min(MAX_DEPTH, Math.max(FORTNIGHT, fits)) : 0;

	/** The days in the scroller, oldest first — today among them when it is not frozen. */
	$: rows = [
		...Array.from({ length: depth }, (_, i) => depth - i),
		...(freezeToday ? [] : [0])
	];
	/** Every day that needs a picture: today first, it being the one in front. */
	$: ends = [0, ...rows.filter((end) => end !== 0)];

	/** The sizes a drag is previewing, or the committed ones at rest. */
	$: shownDayHeight = sizing ? sizing.height : dayHeight;
	$: shownLabelWidth = sizing ? sizing.width : labelWidth;
	/**
	 * How far the drawn bands are stretched to stand in for the size being
	 * previewed. One at rest, which is the picture at its own size.
	 */
	$: scaleY = shownDayHeight / dayHeight;
	$: scaleX = bandWidth > 0 ? Math.max(0, width - shownLabelWidth) / bandWidth : 1;

	/**
	 * The jagged line shows exactly when the band against the divider is
	 * not the newest one — which is to say, when the reader has scrolled
	 * up into older days.
	 */
	$: jagged = depth > 0 && !atBottom;

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
		});
		const observer = new ResizeObserver(([entry]) => {
			settled.push({
				w: Math.floor(entry.contentRect.width),
				h: Math.floor(entry.contentRect.height)
			});
		});
		if (body) observer.observe(body);
		return () => {
			observer.disconnect();
			settled.stop();
			if (refreshing) cancelAnimationFrame(refreshing);
		};
	});

	// ── the pictures ───────────────────────────────────────────────────

	/** Each band on screen, by how many days back it is. 0 is today's. */
	let shown: Record<number, RunResult> = {};
	/** And the width each of those pictures was drawn at — see `mapFor`. */
	let drawnAt: Record<number, number> = {};
	/** The width the geometry on hand was read at. */
	let geometryBox = 0;

	/**
	 * A band's picture — or the message that came instead of one, said out
	 * loud rather than left as a box that never finishes loading.
	 */
	const picture = (result: RunResult) => ({
		text: result.stdout || result.stderr,
		error: !result.stdout
	});

	/** How many draws have been started, and which one each picture came from. */
	let started = 0;
	const from: Record<number, number> = {};
	let geometryFrom = 0;

	/**
	 * Every picture asked for at the current band width, day height and
	 * epoch, by its argv — held as the promise, so two draws in flight for
	 * the same day share one run. A taller box asks for the days it gained
	 * and re-draws nothing else, because their argv has not changed.
	 *
	 * A new EPOCH at the same size is the one case that is not a straight
	 * drop: the days the reader can SEE are re-drawn and the rest keep the
	 * picture they have. A fortnight of bands is a fortnight of CLI
	 * processes, and putting fifteen of them in front of whatever is typed
	 * next cost three and a half seconds a command — for days that are not
	 * on the screen.
	 *
	 * Which is why every run remembers the epoch it was ASKED at: a day
	 * kept that way holds a picture from before the change, and when the
	 * reader scrolls it into view `refreshVisible` asks for that day and
	 * no other (da-dtwz).
	 */
	type Asked = { epoch: number; run: Promise<RunResult> };
	let cache = { key: '', size: '', runs: new Map<string, Asked>() };

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
	let midnights = 0;
	let onDay: string | null = null;

	/**
	 * The days of the stack the reader can see, plus today — what a
	 * refresh must re-draw. Null before the stack is measured, which is a
	 * first draw: everything.
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

	/** One hour gridline: where it sits along a band's x, and whether it is a major. */
	type Hour = { hour: number; x: number; major: boolean; label: string };
	/** What every band shares, read off today's layout rather than re-derived. */
	type Geometry = { hours: Hour[]; today: string | null };
	let geometry: Geometry | null = null;

	// Not on mount: the static build prerenders this page, and there is no
	// database (or `run`) outside the browser.
	// The pictures are asked for at the WINDOW's width, not the band's:
	// the band is what they are shown through (`pictureBox`).
	$: request = { width: pictureBox, dayHeight, epoch, midnights };
	$: if (browser && request.width > 0) draw(request, ends);

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
	$: if (browser && $minute > 0) tock();

	function tock(): void {
		const at = request;
		if (at.width <= 0) return;
		for (const argv of [geometryArgv(at.width, at.dayHeight), bandArgv(at.width, at.dayHeight, 0)])
			cache.runs.delete(argv.join(' '));
		draw(at, [0]);
	}

	function draw(
		at: { width: number; dayHeight: number; epoch: number; midnights: number },
		ends: number[]
	): void {
		const size = `${at.width}/${at.dayHeight}/${at.midnights}`;
		const key = `${size}/${at.epoch}`;
		if (cache.key !== key) cache = { key, size, runs: kept(at, cache.size === size) };
		const token = ++started;
		const ask = (argv: string[]): Promise<RunResult> => {
			const id = argv.join(' ');
			let pending = cache.runs.get(id);
			if (!pending) cache.runs.set(id, (pending = { epoch: at.epoch, run: run(argv) }));
			return pending.run;
		};

		// The geometry first: the gridlines, the ruler and the date stamps
		// all hang off it, and it is one small JSON rather than a picture.
		void ask(geometryArgv(at.width, at.dayHeight)).then((result) => {
			if (token < geometryFrom) return;
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
			void ask(bandArgv(at.width, at.dayHeight, end)).then((result) => {
				// A resize while a draw is in flight and two draws overlap;
				// they can finish out of order, and the loser would paint the
				// older answer over the newer one. Last started wins, band by
				// band — and until it lands, the one on screen stays.
				if (token < (from[end] ?? 0)) return;
				from[end] = token;
				shown = { ...shown, [end]: result };
				drawnAt = { ...drawnAt, [end]: at.width };
				void anchor();
			});
		}
	}

	/**
	 * What the previous cache hands on: at a new size, nothing; at a new
	 * epoch, every day the reader cannot see. The geometry always goes
	 * again — it is what tells the strip what day it is.
	 */
	function kept(
		at: { width: number; dayHeight: number },
		sameSize: boolean
	): Map<string, Asked> {
		const visible = visibleDays();
		if (!sameSize || visible === null) return new Map();
		const again = new Set(visible.map((end) => bandArgv(at.width, at.dayHeight, end).join(' ')));
		again.add(geometryArgv(at.width, at.dayHeight).join(' '));
		return new Map([...cache.runs].filter(([id]) => !again.has(id)));
	}

	/** `dayview --show json`, as much of it as the strip draws with. */
	function readGeometry(stdout: string): Geometry | null {
		try {
			const json = JSON.parse(stdout);
			const ticks: { is_major?: boolean; label?: string }[] = json.axis_ticks ?? [];
			const hours: Hour[] = (json.hours ?? []).map(
				(mark: { hour: number; x: number }, i: number) => ({
					hour: mark.hour,
					x: mark.x,
					major: !!ticks[i]?.is_major,
					label: ticks[i]?.label ?? String(mark.hour).padStart(2, '0')
				})
			);
			return hours.length ? { hours, today: json.days?.[0]?.date ?? null } : null;
		} catch {
			return null;
		}
	}

	async function anchor(): Promise<void> {
		if (!atBottom) return;
		await tick();
		toNewest();
	}

	function onScroll(): void {
		atBottom = !stackEl || stackEl.scrollTop + stackEl.clientHeight >= stackEl.scrollHeight - 1;
		refreshVisible();
	}

	/** The frame a pending refresh is waiting for; a scroll is many events. */
	let refreshing = 0;

	/**
	 * Ask again for the days now on screen that were drawn before the
	 * current epoch (da-dtwz).
	 *
	 * The cache keeps a day the reader could not see rather than re-running
	 * a fortnight of CLI processes in front of the next command — and the
	 * next beat is the next COMMAND, so scrolling up to the entry just
	 * logged for ten days ago showed a band drawn before it existed. Here
	 * the scroll itself is the beat, and it asks for the stale bands that
	 * came into view and for nothing else: days already at this epoch were
	 * re-drawn when it turned.
	 */
	function refreshVisible(): void {
		if (refreshing || !browser) return;
		refreshing = requestAnimationFrame(() => {
			refreshing = 0;
			const at = request;
			if (at.width <= 0) return;
			const id = (end: number) => bandArgv(at.width, at.dayHeight, end).join(' ');
			const stale = (visibleDays() ?? []).filter(
				(end) => (cache.runs.get(id(end))?.epoch ?? at.epoch) < at.epoch
			);
			if (!stale.length) return;
			for (const end of stale) cache.runs.delete(id(end));
			draw(at, stale);
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
	$: stampFont = Math.max(
		7,
		Math.min(14, Math.floor(Math.min(shownDayHeight / 3 - 4, shownLabelWidth / 4.2)))
	);
	/** How many of the three lines fit, tallest first — datum shrinks 3 → 2 → 1 the same way. */
	$: stampLines = Math.max(1, Math.min(3, Math.floor(shownDayHeight / (stampFont + 2))));
	const stamp = (iso: string | null, back: number, lines: number) =>
		iso ? stampFor(iso, back).slice(0, lines) : [];

	// ── the hour ruler, drawn once under today ─────────────────────────

	/** datum's own axis font: the strip's height less its padding. */
	const AXIS_FONT = 9;

	/**
	 * Which hours the ruler names, and where the text sits.
	 *
	 * The majors (every third hour, as datum marks them), thinned until
	 * two labels no longer touch and inset so neither runs off an end —
	 * `fit_axis_labels` in dayview_svg.rs, at the one size the strip draws
	 * its ruler at. The trailing 24 is never named: it is the day boundary
	 * and it is the next band's 00.
	 */
	$: ruler = (() => {
		if (!geometry) return [] as { x: number; at: number; label: string }[];
		const half = AXIS_FONT * 0.6 + 3;
		let prev = -Infinity;
		return geometry.hours
			.slice(0, -1)
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
	})();

	// ── the handle: the day height and the label width together ────────

	function setSizes(px: number, wide: number): void {
		dayHeight = clamp(Math.round(px), MIN_DAY_HEIGHT, MAX_DAY_HEIGHT);
		labelWidth = clamp(Math.round(wide), MIN_LABEL_WIDTH, MAX_LABEL_WIDTH);
		dispatch('change', { dayHeight, labelWidth });
	}

	/** Where a drag started, what the values were when it did, and the preview. */
	let sizing: { x: number; y: number; height: number; width: number } | null = null;
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
	function grab(event: PointerEvent): void {
		event.preventDefault();
		const handle = event.currentTarget as HTMLElement;
		handle.setPointerCapture(event.pointerId);
		handle.focus();
		from0 = { height: dayHeight, width: labelWidth };
		sizing = { x: event.clientX, y: event.clientY, height: dayHeight, width: labelWidth };
	}
	function sizeTo(event: PointerEvent): void {
		if (!sizing) return;
		// Up is taller (da-hsba) and right is a wider column, which is the
		// direction the `+` is being pushed in each case. The bands follow
		// as scaled pictures; they are re-asked on the drop.
		sizing = {
			...sizing,
			height: clamp(
				Math.round(from0.height + (sizing.y - event.clientY)),
				MIN_DAY_HEIGHT,
				MAX_DAY_HEIGHT
			),
			width: clamp(
				Math.round(from0.width + (event.clientX - sizing.x)),
				MIN_LABEL_WIDTH,
				MAX_LABEL_WIDTH
			)
		};
	}
	function drop(): void {
		if (sizing) setSizes(sizing.height, sizing.width);
		sizing = null;
	}

	function onHandleKey(event: KeyboardEvent): void {
		const up = event.key === 'ArrowUp' ? 1 : event.key === 'ArrowDown' ? -1 : 0;
		const right = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
		if (!up && !right) return;
		event.preventDefault();
		setSizes(dayHeight + up * STEP, labelWidth + right * STEP);
	}

	function toggleFreeze(): void {
		freezeToday = !freezeToday;
		dispatch('change', { freezeToday });
		void anchor();
	}

	// ── the zoom: two fingers, ctrl+wheel, and a double-tap back ───────

	/** Ask the owner for `next`, and show it meanwhile. */
	function setWindow(next: [number, number]): void {
		live = null;
		if (next[0] === committed[0] && next[1] === committed[1]) return;
		[hourFrom, hourTo] = next;
		dispatch('change', { hourFrom, hourTo });
	}

	/** The band's box on screen — what a gesture's x is a fraction of. */
	function bandBoxNow(): DOMRect | null {
		const band = stripEl?.querySelector('.band');
		const box = band?.getBoundingClientRect();
		return box && box.width > 0 ? box : null;
	}

	/** Let go of the gesture: the window it was previewing becomes the ask. */
	function commit(): void {
		const next = live;
		pinching = null;
		if (next) setWindow(next);
		live = null;
	}

	/** The fingers down on the strip, by pointer id, and their x. */
	const fingers = new Map<number, number>();
	/** The pinch in hand: where it began, and the band it began over. */
	let pinching: { window: [number, number]; apart: number; mid: number; box: DOMRect } | null =
		null;

	const xs = () => [...fingers.values()];
	const spread = () => Math.abs(xs()[0] - xs()[1]);
	const middle = () => (xs()[0] + xs()[1]) / 2;
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
			['pointercancel', onPointerUp]
		] as [string, EventListener][]) {
			if (on) window.addEventListener(type, handler);
			else window.removeEventListener(type, handler);
		}
	}

	function onPointerDown(event: PointerEvent): void {
		// The strip's own controls are not the picture: the `+` handle, the
		// freeze toggle and the jagged line have the pointer when it is on
		// them, and a press on one is not the start of a gesture.
		if ((event.target as HTMLElement).closest('button')) return;
		tapped(event);
		if (event.pointerType === 'mouse') return;
		fingers.set(event.pointerId, event.clientX);
		if (fingers.size === 1) listen(true);
		if (fingers.size !== 2) return;
		const box = bandBoxNow();
		if (!box) return;
		// A wheel's pause commits it; the fingers are the gesture now.
		clearTimeout(settling);
		pinching = { window: view, apart: spread(), mid: middle(), box };
	}

	function onPointerMove(event: PointerEvent): void {
		if (!fingers.has(event.pointerId)) return;
		fingers.set(event.pointerId, event.clientX);
		if (!pinching || fingers.size !== 2) return;
		const { window: was, apart, mid, box } = pinching;
		const by = apart >= PINCH_FLOOR ? spread() / apart : 1;
		live = zoomWindow(was, by, (mid - box.x) / box.width, (middle() - box.x) / box.width);
	}

	function onPointerUp(event: PointerEvent): void {
		fingers.delete(event.pointerId);
		if (pinching && fingers.size < 2) commit();
		if (fingers.size === 0) listen(false);
	}

	/**
	 * `touch-action: pan-y` keeps the browser off a pinch, but a two-finger
	 * drag is still a scroll it may claim — and claiming it sends
	 * `pointercancel` mid-gesture. While two fingers are down the move is
	 * ours.
	 */
	function onTouchMove(event: TouchEvent): void {
		if (fingers.size >= 2) event.preventDefault();
	}

	/**
	 * Two presses in the same place, in quick succession: the whole day
	 * back. A tap anywhere on the picture, as a pinch is; the same gesture
	 * a map gives back its whole world for.
	 */
	const DOUBLE_TAP = 350;
	let tap = { at: -Infinity, x: 0, y: 0 };
	function tapped(event: PointerEvent): void {
		const near = Math.abs(event.clientX - tap.x) < 30 && Math.abs(event.clientY - tap.y) < 30;
		if (near && event.timeStamp - tap.at < DOUBLE_TAP) {
			tap = { at: -Infinity, x: 0, y: 0 };
			setWindow([HOUR_FROM, HOUR_TO]);
			return;
		}
		tap = { at: event.timeStamp, x: event.clientX, y: event.clientY };
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
		listen(false);
	});

	/**
	 * The jagged line's teeth: a vertex every 2 units across a 400-wide
	 * viewBox, stretched to the strip's width — about 5px a stroke at
	 * 1000px and 2px on a 390px phone, still a tear rather than a blur.
	 */
	const ZIGZAG = Array.from({ length: 201 }, (_, i) => `${i * 2},${i % 2 ? 5 : 1}`).join(' ');
</script>

<Pane title="dayview" bind:body>
	<div
		class="strip"
		bind:this={stripEl}
		on:pointerdown={onPointerDown}
		on:touchmove|nonpassive={onTouchMove}
		on:wheel|nonpassive={onWheel}
		style:--band="{bandBox(shownDayHeight)}px"
		style:--pitch="{dayPitch(shownDayHeight)}px"
		style:--today="{todayBox(shownDayHeight)}px"
		style:--label="{shownLabelWidth}px"
		style:--axis="{AXIS_HEIGHT}px"
		style:--divider="{DIVIDER}px"
		style:--seam="{(freezeToday ? bandBox(shownDayHeight) : 0) + AXIS_HEIGHT}px"
		style:--sx={scaleX}
		style:--sy={scaleY}
	>
		{#if rows.length > 0}
			<div class="stack" bind:this={stackEl} on:scroll={onScroll}>
				{#each rows as end (end)}
					<div class="row" class:now={end === 0}>
						<div class="stamp" style:font-size="{stampFont}px">
							{#each stamp(geometry?.today ?? null, end, stampLines) as line}<span>{line}</span
								>{/each}
						</div>
						<div
							class="band"
							class:past={end > 0}
							data-days-ago={end}
							data-days="1"
							style:--zoom-x="{bandZoom(end).offset}px"
							style:--zoom={bandZoom(end).scale}
						>
							<div class="shot" class:previewing={!!sizing}>
								{#if shown[end]}<CliOutput {...picture(shown[end])} />{/if}
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}

		<!--
			The two lines the `+` is the intersection of (vividn: «a little +
			intersection at the 00 marker dividing the day labels from the
			bar»): the divider between two dates, and the spine between the
			date column and the bands, which is the 00 mark.
		-->
		<div class="divider"></div>
		<div class="spine"></div>

		<!--
			The strip's own controls, on that line: the freeze toggle at the far
			left of it, the `+` at the 00 mark where the stamp column meets the
			bands. Placed against the strip rather than inside the divider, and
			clamped into it, so a strip too short to hold a whole day — which
			clips today's band from the TOP (da-zh2o) — still has them on screen.
		-->
		<button
			class="freeze"
			type="button"
			role="switch"
			aria-checked={freezeToday}
			title={freezeToday ? 'today is pinned at the bottom' : 'today scrolls with the rest'}
			aria-label="freeze today at the bottom"
			on:click={toggleFreeze}
		>
			<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
				<rect class="frame" x="1.5" y="1.5" width="13" height="13" rx="2.5" />
				<rect x="2" y="11" width="12" height="3" rx="1" />
			</svg>
		</button>

		<button
			class="grip day-grip"
			role="slider"
			aria-label="day height and date-label width"
			aria-valuemin={MIN_DAY_HEIGHT}
			aria-valuemax={MAX_DAY_HEIGHT}
			aria-valuenow={shownDayHeight}
			aria-valuetext="{shownDayHeight} pixels per day, {shownLabelWidth} pixels of date label"
			data-label-width={shownLabelWidth}
			on:pointerdown={grab}
			on:pointermove={sizeTo}
			on:pointerup={drop}
			on:pointercancel={drop}
			on:keydown={onHandleKey}
		>
			<svg viewBox="0 0 13 13" width="13" height="13" aria-hidden="true">
				<line x1="6.5" x2="6.5" y1="0.5" y2="12.5" />
				<line x1="0.5" x2="12.5" y1="6.5" y2="6.5" />
			</svg>
		</button>

		{#if jagged}
			<!--
				A real element, not paint: the tear between the divider and a
				day that is not the newest, and the way back. Clicking or
				pressing it puts the newest day back against the divider.
			-->
			<button
				class="jagged"
				on:click={toNewest}
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
					{#each stamp(geometry?.today ?? null, 0, stampLines) as line}<span>{line}</span>{/each}
				</div>
				<div
					class="band"
					data-days-ago="0"
					data-days="1"
					style:--zoom-x="{bandZoom(0).offset}px"
					style:--zoom={bandZoom(0).scale}
				>
					<div class="shot" class:previewing={!!sizing}>
						{#if shown[0]}<CliOutput {...picture(shown[0])} />{:else}<p class="quiet">…</p>{/if}
					</div>
				</div>
			</div>
		{/if}

		<!-- The ruler, drawn once, under whichever band is at the bottom. -->
		<div class="axis">
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
	}

	/* The room above the divider, whatever is left of the box once the
	   ruler and today's band have theirs. The past in it may run past it
	   by up to one day (see `depth`), and then it scrolls. */
	.stack {
		flex: 1 1 auto;
		min-height: 0;
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
	.stack .row:first-child {
		margin-top: auto;
	}

	/* One day: its stamp in the strip's own column, its picture beside it.
	   The days TOUCH — the border IS the divider between them, 1px, and
	   the first row carries a transparent one so every day costs the same
	   pitch wherever it falls. */
	.row {
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
	/* Today's band is the strip's bottom edge: a floor, not a height, so
	   in a shorter strip it is clipped whole from the top. */
	.row.pinned {
		height: auto;
		min-height: var(--band);
		border-top: 0;
	}

	.stamp {
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
		/* The stamp is a label on the picture, not a control. */
		user-select: none;
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
	   the window's scale, and this is the part of it the band shows. The
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
	   the whole day, and a band whose picture is still the PREVIOUS,
	   wider window's needs mapping until its own answer lands, double-tap
	   included. The variables are the BAND's for the same reason — the map
	   is each picture's own (`mapFor`). */
	.band :global(svg.svg) {
		transform: translateX(var(--zoom-x)) scaleX(var(--zoom));
		transform-origin: 0 0;
	}

	/* The spine: the same 1px rule, the other way, at the 00 mark. */
	.spine {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--label);
		width: var(--divider);
		background: var(--rule);
		pointer-events: none;
		z-index: 2;
	}

	/* 1px, full width, and nothing else: the line itself. */
	.divider {
		flex: none;
		height: var(--divider);
		background: var(--rule);
		z-index: 2;
	}

	/* The controls that sit ON that line. Absolute against the strip, at
	   the height the seam is — `--seam` is what the flow below the divider
	   costs — and clamped into the box, so a strip shorter than one day
	   still has them (the old notch's rule, da-zh2o). */
	.freeze,
	.grip,
	.jagged {
		position: absolute;
		bottom: min(var(--seam), calc(100% - 13px));
		transform: translateY(50%);
		z-index: 3;
	}
	.freeze,
	.grip {
		padding: 0;
		border: 0;
		background: none;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--quiet);
		/* A fingertip, on the phone as on the desk. */
		width: 26px;
		height: 26px;
	}
	.freeze {
		left: 0;
		cursor: pointer;
	}
	.grip {
		left: var(--label);
		margin-left: -13px;
		cursor: move;
		/* The browser must not read a drag on a handle as a scroll of the
		   page behind it — the pointer events below are the whole gesture. */
		touch-action: none;
	}
	.freeze:hover,
	.grip:hover,
	.freeze:focus-visible,
	.grip:focus-visible {
		color: var(--accent);
	}
	.freeze[aria-checked='true'] {
		color: var(--accent);
	}
	.freeze:focus-visible,
	.grip:focus-visible {
		outline: 1px solid var(--accent);
	}
	.freeze rect {
		fill: currentColor;
	}
	.freeze .frame {
		fill: none;
		stroke: currentColor;
		stroke-opacity: 0.6;
	}
	.grip line {
		stroke: currentColor;
		stroke-width: 1.5;
		stroke-linecap: round;
	}

	/* Over the divider, clear of the `+`: the tear, and the way back. */
	.jagged {
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
