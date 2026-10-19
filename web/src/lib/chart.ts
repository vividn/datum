// The chart widget's logic (da-xv7ti), kept pure and away from the markup:
// what one key of some fields' entries looks like over time.
//
// vividn, 2026-10-19: «widgets (in general) to be able to display and analyze
// quantitative and qualititative time series data, so that we can see
// average weight over time of various different things plotted together».
//
// The rows are `datum tail F --json`'s, one read a field: nothing here reads
// a database. What it does is arrange them — a key's values as points and
// their average a bucket, one series a field, when every value is a number;
// otherwise each value counted a bucket, stacked.
//
// Time is the wall clock the entry was LIVED on (da-1v9gp): `time` carries
// its own offset, so its first nineteen characters, read as if UTC, are the
// clock it was logged at — a day here is the day it was there, whatever zone
// the reader is in now. `x` is that clock in days since 1970-01-01.

import { defaultColor } from './fields.js';

export type Bucket = 'day' | 'week' | 'month';
export type Show = 'both' | 'points' | 'average';

export interface ChartSettings {
	fields: string[];
	/** The data key, `state` for the entry's state, or '' for the first numeric key there is. */
	key: string;
	bucket: Bucket;
	/** How many days back from today, or `all`. */
	range: string;
	show: Show;
}

export const DEFAULT_CHART: ChartSettings = { fields: [], key: '', bucket: 'day', range: '30d', show: 'both' };

export const BUCKETS: Bucket[] = ['day', 'week', 'month'];
export const RANGES = ['7d', '30d', '90d', '365d', 'all'];
export const SHOWS: Show[] = ['both', 'points', 'average'];

/** How many of a field's entries one read takes. */
export const ROWS = 2000;

/** What the widget offers its menu (`offers`, options.ts): the fields, and the keys of the fields chosen. */
export const FIELDS_OFFER = 'chart:fields';
export const keysOffer = (fields: string[]) => `chart:keys:${fields.join(',')}`;

/** The settings as a layout holds them, each one checked, whatever a saved layout wrote. */
export function chartSettings(s: Record<string, unknown>): ChartSettings {
	const pick = <T extends string>(v: unknown, of: readonly T[], d: T): T => (of.includes(v as T) ? (v as T) : d);
	return {
		fields: Array.isArray(s.fields) ? s.fields.filter((f): f is string => typeof f === 'string') : [],
		key: typeof s.key === 'string' ? s.key : '',
		bucket: pick(s.bucket, BUCKETS, 'day'),
		range: pick(s.range, RANGES, '30d'),
		show: pick(s.show, SHOWS, 'both')
	};
}

/** One read of a field: its newest page, or the page `above` an entry, older. */
export const chartArgv = (field: string, above?: string): string[] => [
	'tail',
	field,
	'-n',
	String(ROWS),
	...(above ? ['--above', above] : []),
	'--json'
];

/**
 * Every row of `field` back to `start` (a lived x; -Infinity for the whole
 * field): its newest page, then a page above the oldest row read, until a
 * page comes back short or reaches before `start`. `read` runs one argv and
 * answers its rows, oldest first as `tail` lists them, or null.
 */
export async function readField(
	field: string,
	start: number,
	read: (argv: string[]) => Promise<Row[] | null>
): Promise<Row[]> {
	let rows: Row[] = [];
	for (let above: string | undefined; ; ) {
		const page = await read(chartArgv(field, above));
		if (!page) return rows;
		rows = [...page, ...rows];
		const oldest = page[0];
		const at = oldest?.time ?? oldest?.create_time;
		if (page.length < ROWS || !at || livedX(at) < start) return rows;
		above = oldest.hid;
	}
}

/** The first lived day `range` reaches back to, from `now`; -Infinity for `all`. */
export const startOf = (range: string, now: number): number => {
	const days = parseInt(range, 10);
	return Number.isFinite(days) ? Math.floor(now) + 1 - days : -Infinity;
};

/** The part of a `tail --json` row the chart reads. */
export interface Row {
	hid: string;
	field: string;
	time: string | null;
	create_time?: string;
	state: unknown;
	data?: Record<string, unknown> | null;
}

export interface Point {
	x: number;
	y: number;
}

export interface Series {
	name: string;
	color: string;
	points: Point[];
	/** Each bucket's mean, at the bucket's middle, in time order. */
	means: Point[];
}

/** One field's values in one bucket, stacked: its bar's first day and width in days. */
export interface Stack {
	x: number;
	width: number;
	field: string;
	parts: { name: string; color: string; count: number }[];
}

export type Chart =
	| { kind: 'empty'; key: string; from: number; to: number }
	| { kind: 'numeric'; key: string; from: number; to: number; series: Series[]; min: number; max: number }
	| { kind: 'categorical'; key: string; from: number; to: number; stacks: Stack[]; names: { name: string; color: string }[]; max: number };

const DAY = 86_400_000;

/** The lived clock of a `time`, in days (see the note above). */
export const livedX = (time: string): number => Date.parse(time.slice(0, 19) + 'Z') / DAY;

/** The reader's clock now, as `livedX` reads one. */
export const nowX = (now: Date = new Date()): number =>
	Date.UTC(now.getFullYear(), now.getMonth(), now.getDate(), now.getHours(), now.getMinutes()) / DAY;

/** The bucket a day falls in: its first day and how many days it holds. Weeks start on Monday. */
export function bucketOf(x: number, bucket: Bucket): { x: number; width: number } {
	const day = Math.floor(x);
	if (bucket === 'day') return { x: day, width: 1 };
	// 1970-01-01 was a Thursday, three days after a Monday.
	if (bucket === 'week') return { x: Math.floor((day + 3) / 7) * 7 - 3, width: 7 };
	const d = new Date(day * DAY);
	const first = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1) / DAY;
	return { x: first, width: Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1) / DAY - first };
}

/** The part of a bucket inside the window. */
const clipped = (b: { x: number; width: number }, from: number, to: number) => {
	const x = Math.max(b.x, from);
	return { x, width: Math.min(b.x + b.width, to) - x };
};

/** The least of `xs`, Infinity for none — a loop, since a whole field's rows are too many arguments for `Math.min`. */
const least = (xs: number[]) => xs.reduce((m, x) => (x < m ? x : m), Infinity);
const most = (xs: number[]) => -least(xs.map((x) => -x));

/** A value read as a number: a JSON number, or a string that is one and nothing else. */
const numeric = (v: unknown): number | null =>
	typeof v === 'number' && Number.isFinite(v)
		? v
		: typeof v === 'string' && /^\s*[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?\s*$/i.test(v)
			? Number(v)
			: null;

const valueOf = (row: Row, key: string): unknown => (key === 'state' ? row.state : row.data?.[key]);

const present = (v: unknown) => v !== null && v !== undefined && !(Array.isArray(v) && v.length === 0);

/** Every key the rows carry a value under, `state` first where any has one: the key choice's words. */
export function keysOf(rows: Row[]): string[] {
	const keys = new Set<string>();
	if (rows.some((r) => present(r.state))) keys.add('state');
	for (const r of rows) for (const [k, v] of Object.entries(r.data ?? {})) if (present(v)) keys.add(k);
	return [...keys];
}

const isNumeric = (rows: Row[], key: string) => {
	const values = rows.map((r) => valueOf(r, key)).filter(present);
	return values.length > 0 && values.every((v) => numeric(v) !== null);
};

/** The key a chart shows: the one asked for, or with none the first numeric key there is. */
export const keyFor = (rows: Row[], asked: string): string =>
	asked || keysOf(rows).find((k) => isNumeric(rows, k)) || keysOf(rows)[0] || 'state';

/**
 * The chart of `rows` under `settings`, now being `now` (a lived x).
 * `colors` is each field's resolved colour (`fields --json`), and
 * `stateColors` each field's states' — a state's own colour where the key
 * is `state`.
 */
export function chartOf(
	rows: Row[],
	settings: ChartSettings,
	now: number,
	colors: Record<string, string> = {},
	stateColors: Record<string, Record<string, string>> = {}
): Chart {
	// Only the fields chosen: a read still landing may hold one just dropped.
	const timed = rows
		.filter((r) => r.time && settings.fields.includes(r.field))
		.map((r) => ({ ...r, x: livedX(r.time!) }))
		.filter((r) => Number.isFinite(r.x));
	const to = Math.floor(now) + 1;
	const earliest = least(timed.map((r) => r.x));
	const start = startOf(settings.range, now);
	const from = Number.isFinite(start) ? start : Number.isFinite(earliest) ? bucketOf(earliest, settings.bucket).x : to - 1;
	const window = timed.filter((r) => r.x >= from && r.x < to);
	const key = keyFor(window, settings.key);
	const inWindow = window.filter((r) => present(valueOf(r, key)));
	if (inWindow.length === 0) return { kind: 'empty', key, from, to };

	if (isNumeric(inWindow, key)) {
		const series = settings.fields.map((field): Series => {
			const points = inWindow
				.filter((r) => r.field === field)
				.map((r) => ({ x: r.x, y: numeric(valueOf(r, key))! }))
				.sort((a, b) => a.x - b.x);
			const sums = new Map<number, { width: number; sum: number; n: number }>();
			for (const p of points) {
				const b = clipped(bucketOf(p.x, settings.bucket), from, to);
				const s = sums.get(b.x) ?? { width: b.width, sum: 0, n: 0 };
				sums.set(b.x, { ...s, sum: s.sum + p.y, n: s.n + 1 });
			}
			const means = [...sums].map(([x, s]) => ({ x: x + s.width / 2, y: s.sum / s.n })).sort((a, b) => a.x - b.x);
			return { name: field, color: colors[field] ?? defaultColor(field), points, means };
		});
		const ys = series.flatMap((s) => s.points.map((p) => p.y));
		return { kind: 'numeric', key, from, to, series, min: least(ys), max: most(ys) };
	}

	// Categorical: each value counted where it fell, a list's every item, in
	// a bar a field side by side within the bucket. A value is painted as
	// datum paints a state of that name: the field's own colour for it where
	// the key is the state, else its name's (`defaultColor`).
	const colorOf = new Map<string, string>();
	const cells = new Map<string, Stack & { counts: Map<string, number> }>();
	const share = settings.fields.length;
	for (const r of inWindow) {
		const v = valueOf(r, key);
		const b = clipped(bucketOf(r.x, settings.bucket), from, to);
		const nth = settings.fields.indexOf(r.field);
		const at = cells.get(`${b.x} ${nth}`) ?? {
			x: b.x + (nth * b.width) / share,
			width: b.width / share,
			field: r.field,
			parts: [],
			counts: new Map()
		};
		cells.set(`${b.x} ${nth}`, at);
		for (const item of Array.isArray(v) ? v : [v]) {
			// A boolean reads on / off, as on every screen (#949); its colour is still the value's.
			const raw = typeof item === 'string' ? item : JSON.stringify(item);
			const name = typeof item === 'boolean' ? (item ? 'on' : 'off') : raw;
			if (!colorOf.has(name)) colorOf.set(name, (key === 'state' && stateColors[r.field]?.[raw]) || defaultColor(raw));
			at.counts.set(name, (at.counts.get(name) ?? 0) + 1);
		}
	}
	const names = [...colorOf].map(([name, color]) => ({ name, color })).sort((a, b) => a.name.localeCompare(b.name));
	const stacks = [...cells.values()]
		.map(({ counts, ...cell }) => ({
			...cell,
			parts: names.filter((n) => counts.has(n.name)).map((n) => ({ ...n, count: counts.get(n.name)! }))
		}))
		.sort((a, b) => a.x - b.x);
	const max = most(stacks.map((s) => s.parts.reduce((t, p) => t + p.count, 0)));
	return { kind: 'categorical', key, from, to, stacks, names, max };
}

/** A lived x as the day it names, `19 Oct`. */
export const dayLabel = (x: number): string =>
	new Date(Math.floor(x) * DAY).toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' });
