<!--
	`datum tail` as a real table.

	The one pane whose content is a grid, not a block. On a terminal datum
	draws its table by padding every cell to a column width it measured
	against `$COLUMNS`; a browser has no column count, and a pane showing
	that padded text scrolled sideways over spaces while its own columns
	sat unused (da-6ec2).

	So the geometry — and only the geometry — moves to the browser. The
	cells come from `datum tail --json`'s `columns`, which is
	`table_columns_with` in crates/datum-commands/src/render/mod.rs — the
	same cell-building `format_table_with` pads: the same columns in the
	same order, the same `_n` ordinals, the same colours, each cell painted
	with the CLI's own ANSI and rendered through `ansiSpans` exactly as a
	line of REPL output is. Nothing about what a cell SAYS is decided here.
	The header words are the CLI's too, always whole: the CLI's `f` and `s`
	shrinks save a terminal's columns, which a pane does not count (da-qrc6).

	Newest at the bottom, as `datum tail` prints.

	How many rows, and which, is `tail -w`'s answer (da-lan0): the pane's
	height decides the count — `n = ⌊(box − header) / row⌋`, exactly what
	fits and nothing else (da-qrc6, overriding da-zfkx's floor of ten), so
	a taller pane shows more of the past, a short one fewer, and no gap at
	rest; a box too short for one row shows the header alone. The listing
	is `tail -n <n + a page>`: the screen, and a page above it already
	held, so the first scroll up moves at once (da-gtk48, vividn: "tail
	should already have more loaded from the very beginning and then
	scrolling should feel completely natural"). The header is pinned
	(a sticky `thead`) as `tail -w` pins it, so it is there at every scroll
	position. Scrolling up asks for the page above it long before the
	reader gets there — while all but a screen of the last page is still
	above them — and asks for several screens at once (da-j6frl, vividn: "rows should load long before you get
	to them"), `tail -n <size> --above <oldest shown>` (the CLI's own bound,
	that row's place in the listing, so two rows at one instant page in
	like any others, da-mhcla), prepended with the reader's
	place kept. The reader scrolls on
	through what is held while it is in flight. A write lands as the epoch bump does
	today: pinned at the bottom the newest page simply replaces the screen;
	scrolled up, the place is kept and a quiet `↓ new` at the foot says
	something landed below. A page the reader pulled in is scrolled up
	until they come back to the bottom, even where it leaves their row at
	the live end (da-nkkjb). The table holds at most `CAP` rows — past it,
	the far end goes, and `↓ new` brings the live end back.

	Two things it does NOT decide: the rows (every one is the CLI's), and
	the row height. That is measured off a hidden one-line probe table in
	the pane's own font, so the count is right before the first listing is
	asked for and the table's own height never feeds back into it (a
	ResizeObserver on the table would). A row whose state wraps is taller
	than one line, so a pane full of long states overflows and scrolls —
	never a gap.

	A FIELD PILL on the bar narrows the listing to that field (da-8810n,
	vividn: "tail widget too") — `tail <field>`, its own first positional, and
	the rows on screen are dropped when it changes, because they are another
	field's.

	And the columns, when the pane is too narrow for all of them: `hid`
	goes first, then `_n` (da-kk0d) — the two a row is also reached by
	(a tap on it), and the two that broke `field` and `state` mid-word on
	a phone. Too narrow is measured, not written down: a second hidden
	probe draws the screen's rows at their narrowest with words whole, and
	the widest set of columns whose widths the box holds is the one drawn.
	The probe draws every column whichever are shown, so what it says never
	depends on the decision; and only the screen's rows, as the listing
	last replaced it, so a page landing never takes a column away
	(da-gtk48: an older row too wide for the box took `hid` mid-scroll).
	The widths the table lays out then are held (`hold`) while rows land.
-->
<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import { ansiSpans, type AnsiSpan } from './ansi.js';
	import AnsiText from './AnsiText.svelte';
	import StateCell from './StateCell.svelte';
	import { fieldPill, select, selected } from './bar.js';
	import { isSelected } from './selection.js';
	import { tapped } from './tap.js';
	import type { PaneBox } from './panes.js';
	import { runKey } from './run.js';
	import { tailListing, type TailColumn, type TailRow } from './tail.js';

	/** Any change re-reads the listing. The pane bumps it after each run. */
	export let epoch = 0;
	/** Shown when the listing is empty. */
	export let empty = '';
	/** The pane's content box, once measured (`CliPane.svelte`'s `box`). */
	export let box: PaneBox | null = null;
	/** The pane's scrolling body: where the reader is, and what is pinned. */
	export let scroller: HTMLElement | undefined = undefined;

	/** Most rows the table holds. Past it, older rows replace newer ones. */
	const CAP = 300;
	/** Most rows of a page drawn in one task. */
	const DRAW = 25;

	/**
	 * Rows the page above asks for, with `n` on screen: four screens, at
	 * least 100. A page costs about the same whatever its size (a second
	 * at five years, da-j6frl), so a bigger one brings rows faster than a
	 * steady scroll uses them. It is asked for once the reader has scrolled
	 * a screen up from where the last one landed — all but a screen of a
	 * page still above them — and never so big that the cap would cut the
	 * reader's own screen when it lands: the page, the lead above the
	 * reader (a page less a screen) and their screen are two pages, which
	 * fit in `CAP` with a screen to spare. A pane so tall that a page is
	 * only a screen has no lead, and asks at the top as it did.
	 */
	const pageSize = (n: number) => Math.max(n, Math.min(Math.max(100, 4 * n), Math.floor((CAP - n) / 2)));

	/** One row as laid out: the CLI's cells by column key, and the row itself. */
	interface Shown {
		hid: string;
		time: string | null;
		cells: Record<string, string>;
		/** The cells painted, once: the same spans are not painted again (da-j6frl). */
		spans: Record<string, AnsiSpan[]>;
		/** What `tail --json` said: the `state` cell is drawn from it (da-fknn8). */
		row: TailRow;
	}

	/** The CLI's column order, for a table merged from more than one listing. */
	const STOCK = ['_n', 'time', 'field', 'state', 'dur', 'hid'];
	const rank = (key: string) => (STOCK.includes(key) ? STOCK.indexOf(key) : STOCK.length);
	/** The columns a narrow pane gives up, in the order it gives them up. */
	const SACRIFICE = ['hid', '_n'];

	let keys: string[] = [];
	let headers: Record<string, string> = {};
	/** Oldest first, newest at the bottom. */
	let rows: Shown[] = [];
	/** The rows of the screen as last replaced: what the width probe decides the columns by (da-gtk48). */
	let probed: Shown[] = [];
	/** The columns' widths, held while rows arrive, and the box width they were measured in; null lays them out afresh. */
	let widths: Record<string, number> | null = null;
	let widthsIn = 0;
	let error = '';
	/** How many reads have been started, and the newest one on screen. */
	let started = 0;
	let shown = -1;

	let thead: HTMLTableSectionElement | undefined;
	let tbody: HTMLTableSectionElement | undefined;
	/** The probe: a header row and a one-line row, hidden, to measure by. */
	let probeHead: HTMLTableSectionElement | undefined;
	let probeRows: HTMLTableSectionElement | undefined;
	/** The width probe: every column, every held row, words whole. */
	let probeWide: HTMLDivElement | undefined;
	/** How many of `SACRIFICE` are not drawn; and the fewest the box and epoch allow. */
	let dropped = 0;
	let least = 0;
	$: cols = keys.filter((key) => !SACRIFICE.slice(0, dropped).includes(key));
	/** Where the widget itself last scrolled to: the event that reports it is not the reader's. */
	let own: number | null = null;
	/** Where the body was at its last scroll event, or where the widget has put it since: below it now is the reader going up. */
	let last = 0;

	/**
	 * The field the listing is narrowed to — the bar's field pill (da-8810n)
	 * — or null for every field. Read inside `refresh` and `page`, so both
	 * the screen and the pages above it are of the same field.
	 */
	let field: string | null = null;

	/** Rows landed below the reader's view (or the live end was let go). */
	let unseen = false;
	/** The page above came back with nothing new: the listing's beginning. */
	let exhausted = false;
	/** Bumped whenever the screen is replaced, so a page in flight for the old one is dropped. */
	let generation = 0;
	let paging = false;
	/**
	 * The reader asked for the past: a page is in flight or landed, and
	 * the screen is scrolled up until they come back down to the bottom
	 * (da-nkkjb). Not `nearBottom()`: a screen that fitted its box exactly
	 * keeps the reader's row at the live end after a page, and a refresh
	 * reading that as pinned replaced the page they had just asked for.
	 */
	let away = false;
	/** Room a kept place needed past the slack, until the screen is replaced. */
	let extra = 0;

	/**
	 * The count the screen, as drawn, settled on, when the probe's is not
	 * it: a narrow pane wraps the header's words and the cells that may
	 * break, and a wrapped line is a line the one-line probe did not count.
	 * Which rows are shown decides the columns, and so what wraps, so the
	 * count is found by drawing: `fits`, the most rows drawn that all fit,
	 * and `over`, the fewest that did not, close in on it, so it cannot
	 * swing between two. Tried afresh from the probe's count when the box
	 * or the epoch changes. And the room the last fresh screen left under
	 * its rows — kept across an epoch, since that screen is still shown.
	 */
	const unsettled = { n: null as number | null, fits: 0, over: Infinity };
	let drawn = { box: null as PaneBox | null, epoch: -1, ...unsettled, slack: Infinity };
	$: if (box !== drawn.box) {
		drawn = { box, epoch, ...unsettled, slack: Infinity };
		least = 0;
	}
	$: if (epoch !== drawn.epoch) {
		drawn = { ...drawn, epoch, ...unsettled };
		least = 0;
	}

	/** The count the box holds, and the slack under it. Null until measured, or while the pane is hidden. */
	$: fit = measure(box, probeHead, probeRows);
	$: n = fit ? (drawn.n ?? fit.n) : null;
	$: slack = fit ? Math.min(fit.slack, drawn.slack) : 0;
	// A box that holds no row still asks for one: its columns are the
	// header. A new box lays the listing out again at the same count: its
	// width rewraps — but asks the CLI only if the count changed (da-4glsf).
	$: if (n !== null && box) void refresh(Math.max(1, n), epoch);
	$: visible = n === 0 ? [] : rows;

	/**
	 * A new field is a new listing: the rows held are another field's, so
	 * they go, and the next `refresh` takes the pinned branch and asks
	 * afresh. The counts that were settled against the old rows go with
	 * them — a narrowed listing wraps differently and is measured again.
	 */
	$: narrow($fieldPill);

	function narrow(next: string | null): void {
		if (next === field) return;
		field = next;
		rows = [];
		probed = [];
		exhausted = false;
		unseen = false;
		away = false;
		extra = 0;
		least = 0;
		generation++;
		drawn = { box, epoch, ...unsettled, slack: Infinity };
		if (n !== null) void refresh(Math.max(1, n), epoch, true);
	}

	/**
	 * How many rows `box` holds: the header row, then rows of the one-line
	 * row's height — all alike, since no border is shared between the
	 * header and the first (the rule under the header is a shadow, so it
	 * stays pinned with it). And what is left under
	 * them, which a spacer takes: a scroller cannot scroll past its end,
	 * so without it a page prepended to a table shorter than its box could
	 * not keep the reader's row where it was — it would sit the slack
	 * lower.
	 *
	 * A box no taller than the header — the observer's first report, before
	 * layout, or a pane on a tab not shown — is not a size: nothing is asked
	 * for until a real one arrives.
	 */
	function measure(
		box: PaneBox | null,
		head: HTMLTableSectionElement | undefined,
		rows: HTMLTableSectionElement | undefined
	): { n: number; slack: number; row: number } | null {
		if (!box || !head || !rows) return null;
		const headPx = head.getBoundingClientRect().height;
		const row = rows.getBoundingClientRect().height;
		if (row <= 0 || box.height <= headPx) return null;
		const n = Math.floor((box.height - headPx) / row);
		return { n, slack: Math.max(0, box.height - headPx - n * row), row };
	}

	/**
	 * Measure a fresh screen of `asked` rows as drawn: how many fit under
	 * the header, newest up. Fewer than are shown, and the count comes
	 * down to them; all of them with a line's room to spare, and it goes
	 * up by what the room holds — never past a count that did not fit.
	 */
	function redraw(of: number, asked: number): void {
		if (!box || !fit || !thead || !tbody || of !== drawn.epoch) return;
		// The screen is the newest `asked`; the rows above it are history.
		const shown = Math.min(tbody.rows.length, asked);
		let room = box.height - thead.getBoundingClientRect().height;
		let fits = 0;
		for (const tr of [...tbody.rows].slice(-shown).reverse()) {
			const h = tr.getBoundingClientRect().height;
			if (h > room + 0.5) break;
			room -= h;
			fits++;
		}
		let { n, over } = drawn;
		if (fits < shown) {
			over = Math.min(over, shown);
			n = Math.max(drawn.fits, fits);
		} else if (shown === asked) {
			// Every row the log had to give is here: room for more is more.
			n = Math.max(shown, Math.min(over - 1, shown + Math.floor(room / fit.row)));
		}
		const most = fits === shown ? Math.max(drawn.fits, shown) : drawn.fits;
		drawn = { ...drawn, n, over, fits: most, slack: Math.max(0, room) };
	}

	/**
	 * Decide the columns from the width probe, as drawn: the fewest of
	 * `SACRIFICE` given up for the rest to fit the box — and, while the
	 * box and the epoch stand, never fewer than already were, so paging in
	 * or asking for more rows cannot bring a column back to crowd them. A
	 * change forgets the counts that did not fit: the rows rewrap.
	 */
	async function decide(): Promise<void> {
		await tick();
		const cells = probeWide?.firstElementChild?.children;
		if (!box || !cells?.length) return;
		const width: Record<string, number> = {};
		const pad: Record<string, number> = {};
		for (const cell of cells) {
			const key = (cell as HTMLElement).dataset.key ?? '';
			width[key] = cell.getBoundingClientRect().width;
			pad[key] = parseFloat(getComputedStyle(cell).paddingRight);
		}
		// A probe not laid out — the pane on a tab not shown — measures nothing.
		if (Object.values(width).some((w) => w <= 0)) return;
		// The last column drawn has no right padding; in the probe it may not be last.
		const needs = (set: string[]) =>
			set.reduce((sum, key) => sum + width[key], 0) - (set.length ? pad[set[set.length - 1]] : 0);
		let fits = least;
		while (fits < SACRIFICE.length) {
			const set = keys.filter((key) => !SACRIFICE.slice(0, fits).includes(key));
			if (needs(set) <= box.width + 0.5) break;
			fits++;
		}
		least = fits;
		if (fits === dropped) return;
		dropped = fits;
		drawn = { ...drawn, fits: 0, over: Infinity };
		await tick();
	}

	/**
	 * Hold the columns at the widths the table lays them out at now, so
	 * rows landing above the reader lay out in them and no column moves
	 * under them (da-gtk48). Measured `again` when the screen is replaced,
	 * and when the box's width changed or a cell no longer fits its column. The
	 * state column takes what the others leave. Read off the drawn header, not
	 * the width probe: the probe says which columns fit, at their narrowest;
	 * these are the widths the table gave them.
	 */
	async function hold(again: boolean): Promise<void> {
		const over = () => [...(tbody?.querySelectorAll('td') ?? [])].some((td) => td.scrollWidth > td.clientWidth + 0.5);
		if (!again && widths && box?.width === widthsIn && !over()) return;
		widths = null;
		await tick();
		const ths = thead?.rows[0]?.cells;
		if (!ths) return;
		widths = Object.fromEntries([...ths].map((th) => [th.dataset.key, th.getBoundingClientRect().width]));
		widthsIn = box?.width ?? 0;
		await tick();
	}

	/**
	 * Scroll to `top` as the widget, not the reader: the event it raises is
	 * told apart by where it landed. And `last` is there from now, not from
	 * that event: a reader who scrolls on before the frame is out shares the
	 * one event with it, and up from where the widget put them is up
	 * (da-gaiko).
	 */
	function scrollTo(top: number): void {
		if (!scroller) return;
		scroller.scrollTop = top;
		own = last = scroller.scrollTop;
	}

	/**
	 * A row's place in the listing, the CLI's order: by time — a timeless
	 * row's is the stamp it was created at (da-2vgj1) — then by slug.
	 */
	const at = (row: Shown) => (row.time === null ? -Infinity : Date.parse(row.time));
	const above = (a: Shown, b: Shown) => at(a) < at(b) || (at(a) === at(b) && a.row.slug < b.row.slug);

	/** A cell a row has no listing's word for: `keys` is every listing's columns. */
	const BLANK = ansiSpans('');

	function toShown(table: { rows: TailRow[]; columns: TailColumn[] }): Shown[] {
		return table.rows.map((row, i) => {
			const cells = Object.fromEntries(table.columns.map((column) => [column.key, column.cells[i] ?? '']));
			const spans = Object.fromEntries(Object.entries(cells).map(([key, cell]) => [key, ansiSpans(cell)]));
			return { hid: row.hid, time: row.time, cells, spans, row };
		});
	}

	/**
	 * Take a listing's columns on: new keys join, the newest header word
	 * wins, and the CLI's shrink of a word (`f` for `field`, `s` for
	 * `state`, printed when no row is wide) is the whole word.
	 *
	 * There is no `action` column to skip any more (da-ftg8p): the CLI's
	 * own table stopped conjuring one for a TRACK document, for the reason
	 * this pane used to filter it out — a column that came and went with
	 * the row count made the count chase its own width (n rows show it, it
	 * takes six characters from the state column, the rows wrap taller,
	 * fewer fit, it leaves, round again). The declaration still reads as
	 * itself: its state cell is `TRACKED`.
	 */
	function adopt(columns: TailColumn[], replace: boolean): void {
		if (replace) {
			keys = [];
			headers = {};
		}
		for (const column of columns) {
			headers[column.key] = column.key.startsWith(column.header) ? column.key : column.header;
			if (!keys.includes(column.key)) keys.push(column.key);
		}
		headers = headers;
		keys = keys.sort((a, b) => rank(a) - rank(b));
	}

	const nearBottom = () =>
		!scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight <= 2;

	/** The first row on screen and where it is, to put it back there after a change. */
	function anchor(): { hid: string; y: number } | null {
		if (!scroller || !tbody) return null;
		const top = scroller.getBoundingClientRect().top;
		for (const tr of tbody.rows) {
			const rect = tr.getBoundingClientRect();
			if (rect.bottom > top) return { hid: tr.dataset.hid ?? '', y: rect.top - top };
		}
		return null;
	}

	async function keep(a: ReturnType<typeof anchor>): Promise<void> {
		await tick();
		if (!a || !scroller || !tbody) return;
		const tr = tbody.querySelector<HTMLTableRowElement>(`tr[data-hid="${a.hid}"]`);
		if (!tr) return;
		const y = tr.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
		const top = scroller.scrollTop + (y - a.y);
		// The slack is measured and the box floored, so a page above can
		// leave the scroller a pixel or two short of the place; lengthen
		// it, not clamp.
		const short = top - (scroller.scrollHeight - scroller.clientHeight);
		if (short > 0) {
			extra += Math.ceil(short);
			await tick();
		}
		scrollTo(top);
	}

	/**
	 * The last listing asked for, and what it asked with. A resize that
	 * leaves the count alone asks the same `tail -n`, whose answer only an
	 * epoch or a field can change: it reuses this one, in flight or landed,
	 * rather than re-run it — 1.3 s in the one queue on five years of data
	 * (da-4glsf).
	 */
	let asked: { with: string; listing: ReturnType<typeof tailListing> } | null = null;
	/** This table in the worker's queue: its newer listing drops its older, unstarted one. */
	const key = runKey();

	async function refresh(count: number, _epoch: number, force = false): Promise<void> {
		const started_at = ++started;
		const ask = JSON.stringify([count, _epoch, field]);
		// The screen and a page above it (da-gtk48): the first scroll up moves at once.
		if (force || asked?.with !== ask)
			asked = { with: ask, listing: tailListing({ count: count + pageSize(count), ...(field ? { field } : {}) }, key) };
		const mine = asked;
		let table: { rows: TailRow[]; columns: TailColumn[] };
		try {
			const listing = await mine.listing;
			// Dropped unrun for a newer listing, which draws.
			if (!listing) return;
			table = listing;
		} catch (e) {
			// A failed listing is asked for again, not remembered.
			if (asked === mine) asked = null;
			if (started_at < shown) return;
			shown = started_at;
			error = e instanceof Error ? e.message : String(e);
			return;
		}
		// Two refreshes can be in flight at once and finish out of order;
		// the loser must not paint the older listing over the newer one.
		// Last started wins, not last to return.
		if (started_at < shown) return;
		shown = started_at;
		error = '';
		// A write can land anywhere — older than the first row, too — so
		// the start of the log is known again only by asking: the next
		// pull at the top.
		exhausted = false;
		const page = toShown(table);
		if (force || rows.length === 0 || (!paging && !away && nearBottom())) {
			// Pinned: the screen is the CLI's newest page and nothing else.
			generation++;
			away = false;
			extra = 0;
			rows = page;
			probed = page.slice(-count);
			adopt(table.columns, true);
			unseen = false;
			await decide();
			await hold(true);
			// The screen as drawn can change the room under it; pinned is
			// pinned under that room, not the one before it.
			redraw(_epoch, count);
			await tick();
			if (scroller) scrollTo(scroller.scrollHeight);
		} else {
			// Scrolled up: keep the place. The page is the truth for the
			// span it covers — a row of ours inside that span but not in
			// it was deleted or retimed — and what we hold above the span
			// stays. A page that does not reach down to our newest row
			// is not contiguous (more than a page landed, or the far end
			// was let go at the cap): leave the screen, say so below.
			const a = anchor();
			const held = new Set(rows.map((row) => row.hid));
			const fresh = page.some((row) => !held.has(row.hid));
			const newest = rows[rows.length - 1];
			const first = page[0];
			if (first && !above(newest, first)) {
				const inPage = new Set(page.map((row) => row.hid));
				rows = [...rows.filter((row) => !inPage.has(row.hid) && above(row, first)), ...page];
				if (rows.length > CAP) rows = rows.slice(rows.length - CAP);
				adopt(table.columns, false);
			}
			if (fresh) unseen = true;
			await decide();
			await hold(false);
			await keep(a);
		}
	}

	/** The page above the oldest row on screen, prepended with the place kept. */
	async function page(): Promise<void> {
		if (paging || exhausted || !n || rows.length === 0) return;
		paging = true;
		const of = generation;
		try {
			const table = await tailListing({ count: pageSize(n), above: rows[0].hid, ...(field ? { field } : {}) });
			if (of !== generation) return;
			// A row retimed since it was listed can come back above itself.
			const held = new Set(rows.map((row) => row.hid));
			const fresh = toShown(table).filter((row) => !held.has(row.hid));
			if (fresh.length === 0) return void (exhausted = true);
			away = true;
			adopt(table.columns, false);
			// Drawn `DRAW` rows a task, the newest first, so no one frame
			// waits on a whole page (da-j6frl); the place is the one before
			// the first, so a scroll offset's rounding is not carried on.
			const a = anchor();
			for (let end = fresh.length; end > 0; end -= DRAW) {
				if (of !== generation) return;
				rows = [...fresh.slice(Math.max(0, end - DRAW), end), ...rows];
				if (rows.length > CAP) {
					rows = rows.slice(0, CAP);
					unseen = true;
				}
				await hold(false);
				await keep(a);
				if (end > DRAW) await new Promise((resolve) => setTimeout(resolve));
			}
		} catch {
			// A page that failed is a page not shown; the table stands.
		} finally {
			paging = false;
		}
	}

	/** `↓ new`: back to the live end, fresh. */
	function jump(): void {
		if (n !== null) void refresh(Math.max(1, n), epoch, true);
	}

	// The reader's scrolling, on the pane's body. Up to within a page of
	// the top, less a screen, the page above; at the bottom, nothing is unseen. A table that fits exactly
	// cannot be scrolled at all, so a wheel or a finger pulling down at the
	// top asks the same way.
	let attached: HTMLElement | undefined;
	$: attach(scroller);

	function attach(el: HTMLElement | undefined): void {
		if (el === attached) return;
		detach();
		attached = el;
		if (!el) return;
		el.addEventListener('scroll', onScroll, { passive: true });
		el.addEventListener('wheel', onWheel, { passive: true });
		el.addEventListener('touchstart', onTouchStart, { passive: true });
		el.addEventListener('touchmove', onTouchMove, { passive: true });
	}

	function detach(): void {
		if (!attached) return;
		attached.removeEventListener('scroll', onScroll);
		attached.removeEventListener('wheel', onWheel);
		attached.removeEventListener('touchstart', onTouchStart);
		attached.removeEventListener('touchmove', onTouchMove);
		attached = undefined;
	}

	function onScroll(): void {
		if (!scroller) return;
		const top = scroller.scrollTop;
		const up = top < last;
		last = top;
		// Fewer rows than before clamp the scroll on their own, and a
		// clamp is a scroll event too — so a scroll of ours is known by
		// where it landed, and a scroll that cannot be a reader's, in a
		// body with nothing to scroll, is nobody's.
		const mine = own !== null && top === own;
		own = null;
		if (mine) return;
		if (nearBottom()) unseen = away = false;
		const lead = n && fit ? (pageSize(n) - n) * fit.row : 0;
		if (up && top <= lead && scroller.scrollHeight > scroller.clientHeight + 1) void page();
	}

	function onWheel(e: WheelEvent): void {
		if (e.deltaY < 0 && scroller && scroller.scrollTop <= 0) void page();
	}

	let touchY: number | null = null;
	function onTouchStart(e: TouchEvent): void {
		touchY = e.touches[0]?.clientY ?? null;
	}
	function onTouchMove(e: TouchEvent): void {
		const y = e.touches[0]?.clientY;
		if (touchY === null || y === undefined || !scroller) return;
		if (y - touchY > 24 && scroller.scrollTop <= 0) {
			touchY = null;
			void page();
		}
	}

	onDestroy(detach);
</script>

<!-- The probe: what a header row and a one-line row measure in this pane.
     Hidden, and a `tfoot` rather than a head and a body, so that nothing
     that reads the table — `thead th`, `tbody tr`, the body's text — sees
     it: it is a ruler, not a row. -->
<table class="probe" aria-hidden="true">
	<tfoot bind:this={probeHead}><tr><th>&nbsp;</th></tr></tfoot>
	<tfoot bind:this={probeRows}><tr><td>&nbsp;</td></tr></tfoot>
</table>
<!-- The width probe: each column at its narrowest with no word broken,
     so a cell's width here is what its column needs. Laid out as a table,
     but no `table`, `tr` or `td`: nothing that looks for the table's cells
     finds its words twice. -->
<div class="probe wide" aria-hidden="true" bind:this={probeWide}>
	<div class="tr">
		{#each keys as key (key)}
			<div class="cell {key}" data-key={key}><AnsiText spans={ansiSpans(headers[key] ?? key)} /></div>
		{/each}
	</div>
	{#each probed as row (row.hid)}
		<div class="tr">
			{#each keys as key (key)}
				<div class="cell {key}">
					{#if key === 'state'}<StateCell row={row.row} painted={row.cells.state ?? ''} />{:else}<AnsiText
							spans={row.spans[key] ?? BLANK}
						/>{/if}
				</div>
			{/each}
		</div>
	{/each}
</div>
{#if shown < 0}
	<p class="quiet">…</p>
{:else if error}
	<p class="error">{error}</p>
{:else if rows.length === 0}
	<p class="quiet">{empty}</p>
{:else}
	<table class:held={widths !== null}>
		<colgroup>
			{#each cols as key (key)}
				<col style:width={widths && key !== 'state' ? `${widths[key]}px` : null} />
			{/each}
		</colgroup>
		<thead bind:this={thead}>
			<tr>
				{#each cols as key (key)}
					<th class={key} data-key={key}
						><AnsiText spans={ansiSpans(headers[key] ?? key)} /></th
					>
				{/each}
			</tr>
		</thead>
		<tbody bind:this={tbody}>
			{#each visible as row, i (row.hid)}
				<!-- A tap selects the entry, as its hid pill (da-r13j). The
				     rows are not focusable, so there is no keyboard selection. -->
				<tr
					data-hid={row.hid}
					class:selected={isSelected($selected, row.hid)}
					on:mousedown={(e) => e.shiftKey && e.preventDefault()}
					on:click={(e) => tapped(e) && select(row.hid, e, visible.map((r) => r.hid))}
					class:start={exhausted && i === 0}
					title={exhausted && i === 0 ? 'the start of the log' : undefined}
				>
					{#each cols as key (key)}
						<td class={key} data-key={key}
							>{#if key === 'state'}<StateCell
									row={row.row}
									painted={row.cells.state ?? ''}
								/>{:else}<AnsiText spans={row.spans[key] ?? BLANK} />{/if}</td
						>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
	<div class="slack" style:height="{slack + extra}px"></div>
	{#if unseen}
		<div class="foot">
			<button type="button" class="new" on:click={jump}>↓ new</button>
		</div>
	{/if}
{/if}

<style>
	/* A selected entry (da-r13j): the accent the hid pill is filled with. */
	tr.selected {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
	}

	tbody tr {
		cursor: pointer;
	}

	table {
		/* Fill the pane and never exceed it: the cells that can wrap do,
		   and the pane does not scroll sideways at any width. */
		width: 100%;
		border-collapse: collapse;
		font: inherit;
		text-align: left;
	}

	/* At the widths `hold` measured: a row landing does not move a column. */
	table.held {
		table-layout: fixed;
	}

	.probe {
		position: absolute;
		top: 0;
		left: 0;
		width: auto;
		visibility: hidden;
		pointer-events: none;
	}

	/* At its narrowest: 1px wide, the columns at their min-content, and a
	   word of `field` or `state` one unbreakable run. */
	.wide {
		display: table;
		width: 1px;
	}

	.wide .tr {
		display: table-row;
	}

	.wide .cell {
		display: table-cell;
		padding: 0.1em 0.4em 0.1em 0;
	}

	.wide .cell:last-child {
		padding-right: 0;
	}

	.wide .field,
	.wide .state {
		overflow-wrap: normal;
	}

	/* Pinned, as `tail -w` pins its header. A sticky box stops at the
	   scroller's padding edge, so it is pulled up by the pane's `.body`
	   top padding (Pane.svelte) to sit flush with the pane's top, leaving
	   no strip for the rows scrolled under it to show through. */
	thead {
		position: sticky;
		top: -0.33rem;
	}

	th {
		font-weight: normal;
		/* On the pane's ground, so the rows under it do not show through. */
		background: var(--ground);
		/* The CLI's header is plain text over the columns; a rule under it
		   does the work the terminal's blank line does. A shadow, not a
		   border: a collapsed border stays with the table, not the header. */
		/* Not `--rule`: one hair off it (#2a2a44 against #2a2a4a), and the
		   two are not the same pixel. Making them one is a visual change,
		   which this is not. */
		box-shadow: inset 0 -1px 0 #2a2a44;
		padding-bottom: calc(0.15em + 1px);
		/* One line, whatever the rows hold (da-1o33i): `.field`'s
		   `overflow-wrap: anywhere` let a long state squeeze the column to a
		   character and the header to three lines, its height following the
		   rows as they paged in and out. */
		white-space: nowrap;
	}

	th,
	td {
		padding: 0.1em 0.4em 0.1em 0;
		vertical-align: baseline;
	}

	th:last-child,
	td:last-child {
		padding-right: 0;
	}

	/* The columns a terminal keeps on one line, kept on one line: a clock
	   broken across two rows is unreadable, and the handles (`_3`, the
	   short hid) are things you type back verbatim. */
	.time,
	.dur,
	._n,
	.hid {
		white-space: nowrap;
		width: 1px; /* shrink-to-fit; the state column takes the slack. */
	}

	/* Right-aligned, as the terminal right-aligns them on the clock. */
	.time,
	.dur {
		text-align: right;
	}

	/* An uncertain time is `HH:MM ± `, the ± in its seconds' place, and the
	   space after it is what keeps its clock over the others (da-dj8v0). */
	.time {
		white-space: pre;
	}

	/* The two that carry arbitrary text. A long state wraps inside its own
	   cell rather than pushing the table past the pane. */
	.field,
	.state {
		overflow-wrap: anywhere;
	}

	/* Paged up to the log's first row: a dotted edge over it, drawn as a
	   background so the row's height, and the count, are untouched. */
	.start td {
		background: linear-gradient(to right, var(--quiet) 50%, transparent 0) top / 6px 1px repeat-x;
	}

	/* What the count leaves under the last row: under a page, it is the room
	   the reader's row needs to stay put. */
	.slack {
		width: 100%;
	}

	/* The `↓ new` marker: no height of its own, so the count is untouched;
	   stuck to the foot of the scrolling body, at the right. */
	.foot {
		position: sticky;
		bottom: 0;
		height: 0;
	}

	.new {
		position: absolute;
		right: 0;
		bottom: 0;
		font: inherit;
		font-size: 0.8em;
		color: var(--accent);
		background: var(--ground);
		border: 1px solid var(--rule);
		border-radius: 4px;
		padding: 0.1em 0.5em;
		cursor: pointer;
	}

	.quiet {
		margin: 0;
		color: var(--quiet);
		font-style: italic;
	}

	.error {
		margin: 0;
		color: var(--error);
	}
</style>
