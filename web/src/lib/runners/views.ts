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
// against SQLite-wasm: pick the window, pull the entries, group them by
// field. The marks each entry makes are $lib/visuals.ts; every colour and
// every pixel is Rust's.

import { getEntriesInRange, getAllFields, getFieldSpecs, getStateEntriesAt } from '../db.js';
import { renderLayout, type FieldReq, type FieldStyle } from '../layout.js';
import { effectiveState, fieldMarks, firstSwitchMs, styleOf } from '../visuals.js';
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

function clockLabel(date: Date): string {
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/** Group entries by field, oldest first — the order the walk needs. */
function byField(entries: Entry[]): Map<string, Entry[]> {
	const out = new Map<string, Entry[]>();
	for (const entry of entries) {
		const list = out.get(entry.field);
		if (list) list.push(entry);
		else out.set(entry.field, [entry]);
	}
	for (const list of out.values()) {
		list.sort((a, b) => Date.parse(a.occurTime.utc) - Date.parse(b.occurTime.utc));
	}
	return out;
}

async function stylesByField(): Promise<Record<string, FieldStyle>> {
	const styles: Record<string, FieldStyle> = {};
	for (const [field, spec] of await getFieldSpecs()) {
		const style = styleOf(spec);
		if (style) styles[field] = style;
	}
	return styles;
}

/** Wrap a rendered SVG as the command result whose stdout it is. */
function svgResult(argv: string[], svg: string): RunResult {
	return { argv, code: 0, stdout: svg, stderr: '' };
}

/**
 * `datum dayview` — today, as the local day runs.
 *
 * Only today: `--n-days`, `--start-date` and the rest of dayview's window
 * flags are the real CLI's job, and answering them here would grow a
 * second datum in TypeScript. An argv carrying any of them falls through
 * to the unknown-command message, the way `tail mood` does.
 */
export async function dayview(argv: string[]): Promise<RunResult> {
	const now = new Date();
	const dayStart = new Date(now);
	dayStart.setHours(0, 0, 0, 0);
	const dayEnd = new Date(dayStart);
	dayEnd.setDate(dayEnd.getDate() + 1);

	const startMs = dayStart.getTime();
	const endMs = dayEnd.getTime();
	const nowMs = now.getTime();

	const entries = await getEntriesInRange(new Date(startMs - DAYVIEW_BUFFER_MS), dayEnd);
	const fields: FieldReq[] = [];
	for (const [field, list] of byField(entries)) {
		// The last switch of the day runs to the end of the day, or to now
		// if the day hasn't finished yet — a state doesn't extend into the
		// future.
		fields.push(fieldMarks(field, list, startMs, endMs, Math.min(endMs, nowMs)));
	}

	const svg = await renderLayout({
		view: 'dayview',
		now_ms: nowMs,
		days: [
			{ label_lines: dayLabel(dayStart), start_ms: startMs, end_ms: endMs, fields }
		],
		styles: await stylesByField()
	});
	return svgResult(argv, svg);
}

/**
 * `datum nowview` — the last 30 minutes, plus the current-state panel.
 *
 * The CLI prints a terminal band when no `--output-file` is given and
 * writes the SVG when one is; a browser pane has no terminal, so this is
 * the SVG path. The one place the browser knows something the layout
 * cannot: the `HH:MM` clock, which is the viewer's own zone.
 */
export async function nowview(argv: string[]): Promise<RunResult> {
	const now = new Date();
	const endMs = now.getTime();
	const startMs = endMs - HISTORY_MINUTES * 60 * 1000;
	const fetchStart = new Date(startMs - NOWVIEW_BUFFER_MS);

	const entries = await getEntriesInRange(fetchStart, now);
	const grouped = byField(entries);
	// A field whose open state predates the fetch window has no entry in
	// it at all, and would silently lose its band. `build_dayview_fields`
	// seeds every tracked field for the same reason.
	for (const field of await getAllFields()) {
		if (!grouped.has(field)) grouped.set(field, []);
	}

	// The two state oracles the panel and the lead-in need, each one query
	// over the whole store rather than over the fetched window.
	const leadState = new Map<string, unknown>();
	for (const entry of await getStateEntriesAt(fetchStart)) {
		leadState.set(entry.field, effectiveState(entry, fetchStart.getTime()));
	}
	const panelStates: Record<string, unknown> = {};
	for (const entry of await getStateEntriesAt(now)) {
		panelStates[entry.field] = effectiveState(entry, endMs);
	}

	const fields: FieldReq[] = [];
	for (const [field, list] of grouped) {
		const leadEnd = firstSwitchMs(list) ?? endMs;
		const state = leadState.get(field);
		const lead =
			leadEnd > fetchStart.getTime() && state !== undefined
				? { startMs: fetchStart.getTime(), endMs: leadEnd, state }
				: undefined;
		fields.push(fieldMarks(field, list, startMs, endMs, endMs, lead));
	}

	const svg = await renderLayout({
		view: 'nowview',
		now_ms: endMs,
		fields,
		styles: await stylesByField(),
		panel_states: panelStates,
		clock: clockLabel(now)
	});
	return svgResult(argv, svg);
}
