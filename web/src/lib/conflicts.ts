// The conflicts widget's logic, kept out of its markup (da-6cutb): what
// `datum conflicts --json` answers, and the one `conflicts resolve` line a
// choice composes. Pure and import-free, so node tests it as it is.
//
// vividn, 2026-10-08: «there should be a button "conflicts" in the commands
// menu. If there are any conflicts, it should become red for the user to
// then directly deal with» and «yes there should be a conflicts widget that
// get opened by the conflicts command».
//
// The sides of a key are the CLI's, in the CLI's order (da-07594): the
// column holds them in clock order, so the first is A on every copy and the
// widget letters them by where they stand, never by anything of its own.

/** One side of a contested key: the value a device set, and the clock it set it at. */
export interface Side {
	value: unknown;
	device: string;
	hlc: string;
}

/** One entry of `datum conflicts --json`: each contested key's sides, A first. */
export interface Conflicted {
	hid: string;
	slug: string;
	deleted: boolean;
	conflicts: Record<string, Side[]>;
}

/** What `resolve` makes of the keys not picked; the flag is `--<name>`. None keeps what the entry shows. */
export const STRATEGIES = ['use-a', 'use-b', 'prefer-a', 'prefer-b', 'merge'] as const;
export type Strategy = (typeof STRATEGIES)[number];

/** The letter of the side at `index`: A, B, C… */
export const letterOf = (index: number): string => String.fromCharCode(65 + index);

/** A side's value as `datum conflicts` prints it: `deleted`'s sides are a delete and what would keep the entry. */
export const said = (key: string, value: unknown): string =>
	key === 'deleted' && typeof value === 'boolean' ? (value ? 'deleted' : 'kept') : JSON.stringify(value);

/** When a side was set, on this device's clock: the wall time an HLC begins with (`hlc_time`, datum-core). */
export const when = (hlc: string): string =>
	new Date(hlc.split('/')[0]).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });

/**
 * `conflicts resolve <hid> [--strategy] [KEY=PICK…]`. A pick is the CLI's
 * own: one letter names a side, anything else is a value. A key with no
 * pick is left out, and with no strategy either the line settles the entry
 * as it shows.
 */
export function resolveArgv(hid: string, how: Strategy | '' = '', picks: Record<string, string> = {}): string[] {
	return [
		'conflicts',
		'resolve',
		hid,
		...(how ? [`--${how}`] : []),
		...Object.entries(picks)
			.filter(([, pick]) => pick)
			.map(([key, pick]) => `${key}=${pick}`)
	];
}

/** Is `argv` the listing — `conflicts`, `conflicts list` — which opens the widget where it is typed? */
export const listsConflicts = (argv: string[]): boolean => ['conflicts', 'conflicts list'].includes(argv.join(' '));
