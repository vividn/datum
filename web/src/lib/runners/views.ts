// `datum dayview` and `datum nowview`, rendered in the browser.
//
// Both are compute-then-render in the CLI too — `compute_dayview` returns
// a layout and `render_dayview_svg` returns a string; only the last line
// of the command prints. So the browser runs datum's own two functions
// (through $lib/layout.ts) rather than a picture of its own, and what
// comes back on stdout is the same SVG document `datum dayview` writes
// (da-6h6p). The panes show it as an image, which is what CliOutput
// already does for the CLI's SVG stdout.
//
// This file is the part the CLI does against SQLite and the browser does
// against SQLite-wasm: pick the window, pull the rows, group them by
// field. Which mark each row makes is `crates/datum-layout-wasm`'s marks
// walk, dispatching on the CLI's own verb→shape table (da-t7ud); it used
// to be a hand-written copy in $lib/visuals.ts. Every colour and every
// pixel was already Rust's.

import { clockLabel, startOfDay } from '../clock.js';
import { getEntriesInRange, getAllFields, getFieldSpecs, getStateEntriesAt } from '../db.js';
import { renderLayout, type EntryReq, type FieldReq, type FieldStyle } from '../layout.js';
import type { Entry } from '../types.js';
import type { RunResult } from '../run.js';

/** `datum nowview`'s default history window (`DEFAULT_HISTORY_MINUTES`). */
const HISTORY_MINUTES = 30;

/**
 * How far behind the visible window entries are pulled from, so a block
 * that started before it still draws its visible part. Two days for
 * dayview and four history-windows for nowview — the same buffers
 * `build_day_fields` and `build_dayview_fields` use.
 */
const DAYVIEW_BUFFER_MS = 2 * 24 * 60 * 60 * 1000;
const NOWVIEW_BUFFER_MS = HISTORY_MINUTES * 4 * 60 * 1000;

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec'
];

/** datum's 3-line day label, `["Mon", "Apr 27", "2026"]`. */
function dayLabel(date: Date): string[] {
	return [
		DAYS[date.getDay()],
		`${MONTHS[date.getMonth()]} ${String(date.getDate()).padStart(2, '0')}`,
		String(date.getFullYear())
	];
}

/** A stored row, as the layout module reads one. */
function toEntryReq(entry: Entry): EntryReq {
	const occurMs = Date.parse(entry.occurTime.utc);
	return {
		occur_ms: Number.isFinite(occurMs) ? occurMs : null,
		verb: entry.verb,
		dur: entry.dur,
		state: entry.state,
		following_state: entry.followingState,
		slug: entry.slug
	};
}

/** Group rows by field. Ordering within a field is the walk's own job. */
function byField(entries: Entry[]): Map<string, EntryReq[]> {
	const out = new Map<string, EntryReq[]>();
	for (const entry of entries) {
		const list = out.get(entry.field);
		if (list) list.push(toEntryReq(entry));
		else out.set(entry.field, [toEntryReq(entry)]);
	}
	return out;
}

/**
 * Every field's stored spec, passed through untouched: the Rust side
 * reads the drawn keys out of the same blob the CLI deserializes.
 */
async function stylesByField(): Promise<Record<string, FieldStyle>> {
	const styles: Record<string, FieldStyle> = {};
	for (const [field, spec] of await getFieldSpecs()) styles[field] = spec;
	return styles;
}

/** Wrap a rendered SVG as the command result whose stdout it is. */
function svgResult(argv: string[], svg: string): RunResult {
	return { argv, code: 0, stdout: svg, stderr: '' };
}

/**
 * Geometry and window for one dayview band, in the CLI's own flags.
 *
 * The three pixel knobs go across untouched: what a total height means
 * for a day's band — `day_height_for_extent` in `datum-layout` — is the
 * same rule `datum dayview --height` applies, and it stays in Rust.
 * Empty means the defaults, which is the 2000px canvas the CLI writes to
 * a file.
 *
 * `dayStart` is `--end-date` already resolved to a local midnight: which
 * calendar day a date argument names is the runner's half of the job
 * (see `localDayStart` in ./local.ts), and everything after it — the
 * window, the buffer, the label — is the same code today's band runs.
 */
export interface DayviewOptions {
	width?: number;
	height?: number;
	day_height?: number;
	time_axis_height?: number;
	/** Local midnight of the day to draw. Defaults to today's. */
	dayStart?: Date;
}

/**
 * `datum dayview` — one day, as the local day runs.
 *
 * Today by default, and any single day `--end-date` names: the day strip
 * (da-cgyh) stacks earlier days above today by rendering one band per
 * day, which is one call here per band. `--n-days` and `--start-date`
 * are still the real CLI's job — a multi-day window means splitting the
 * blocks that cross a midnight between two bands, which is
 * `build_dayview_fields`'s rule and not one to rewrite here. A band is a
 * single-day window, so it needs none of it: `build_day_fields` clips to
 * the day it was given, and that is exactly what this function does per
 * band.
 *
 * An argv carrying a flag this file does not serve falls through to the
 * unknown-command message, the way `tail mood` does.
 */
export async function dayview(
	argv: string[],
	options: DayviewOptions = {}
): Promise<RunResult> {
	const now = new Date();
	const { dayStart: requested, ...geometry } = options;
	const dayStart = requested ?? startOfDay(now);
	const dayEnd = new Date(dayStart);
	dayEnd.setDate(dayEnd.getDate() + 1);

	const entries = await getEntriesInRange(
		new Date(dayStart.getTime() - DAYVIEW_BUFFER_MS),
		dayEnd
	);
	const fields: FieldReq[] = [];
	for (const [field, list] of byField(entries)) fields.push({ field, entries: list });

	const svg = await renderLayout({
		view: 'dayview',
		now_ms: now.getTime(),
		days: [
			{
				label_lines: dayLabel(dayStart),
				start_ms: dayStart.getTime(),
				end_ms: dayEnd.getTime(),
				fields
			}
		],
		styles: await stylesByField(),
		options: geometry
	});
	return svgResult(argv, svg);
}

/**
 * `datum nowview` — the last 30 minutes, plus each field's current state
 *
 * The CLI prints a terminal band when no `--output-file` is given and
 * writes the SVG when one is; a browser pane has no terminal, so this is
 * the SVG path. The one place the browser knows something the layout
 * cannot: the `HH:MM` clock, which is the viewer's own zone.
 *
 * `geometry` is dayview's, for the same reason and by the same route: the
 * default canvas is 520x276, which fits a wide pane's width and no pane's
 * height, so the picture overflowed downward (da-45wt). What a requested
 * height means for the band — `nowview_config`'s axis and margin
 * subtraction — is `datum nowview --height`'s own rule and stays in Rust.
 */
export async function nowview(
	argv: string[],
	geometry: { width?: number; height?: number } = {}
): Promise<RunResult> {
	const now = new Date();
	const fetchStart = new Date(now.getTime() - HISTORY_MINUTES * 60 * 1000 - NOWVIEW_BUFFER_MS);

	const grouped = byField(await getEntriesInRange(fetchStart, now));
	// A field whose open state predates the fetch window has no entry in
	// it at all, and would silently lose its band. `build_dayview_fields`
	// seeds every tracked field for the same reason.
	for (const field of await getAllFields()) {
		if (!grouped.has(field)) grouped.set(field, []);
	}

	// The two state oracles the now-chunk and the lead-in need, each one query
	// over the whole store rather than over the fetched window. The
	// trailing-edge rule that turns a winning row into a state is Rust's
	// (`db::now::effective_state`), so the rows go across as they are.
	const rowsAt = async (at: Date): Promise<Record<string, EntryReq>> => {
		const out: Record<string, EntryReq> = {};
		for (const entry of await getStateEntriesAt(at)) out[entry.field] = toEntryReq(entry);
		return out;
	};

	const svg = await renderLayout({
		view: 'nowview',
		now_ms: now.getTime(),
		fetch_start_ms: fetchStart.getTime(),
		fields: [...grouped].map(([field, entries]) => ({ field, entries })),
		styles: await stylesByField(),
		lead_entries: await rowsAt(fetchStart),
		current_states: await rowsAt(now),
		clock: clockLabel(now),
		options: geometry
	});
	return svgResult(argv, svg);
}
