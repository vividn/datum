// Which database the page runs against: `?db=<name>` (da-vk1x).
//
// vividn, 2026-09-16 22:26: "I like the ?db query". A shared app across tests
// is a shared database, and a person will want their second database open in
// a tab. Both want the same thing — the page runs every command against a
// database named in the URL — and the URL is the whole of the state: it
// survives a reload, it is what a link carries, and nothing is stored.
//
// The name reaches the CLI as its `DATUM_DB`, set at the one seam every
// command goes through (#lib/run.ts `dbEnv`), and never in argv: argv is what
// an entry records as typed (da-7xne1). What a reader sees of it is the
// page — the title.
//
// `_internal` is refused. It is the app's own database, where the saved
// layouts live (da-wpgy), and a page that ran the panes against it would be
// reading its own furniture as data. Layouts are the device's, not the
// database's: the layout store keeps its explicit `--db _internal`
// (#lib/layout/store.ts) and is untouched by any of this.

import { browser } from '$app/env';

/** The database a page with no `?db` runs against. */
export const DEFAULT_DB = 'datum';

/** The app's own database — never the page's. */
export const INTERNAL_DB = '_internal';

/** The query parameter that names it. */
export const DB_PARAM = 'db';

/**
 * Whether `value` is a database NAME rather than a path — the CLI's own
 * rule (`looks_like_path` in `crates/datum-commands/src/config.rs`).
 *
 * A name is what the CLI turns into `/data/<name>.db` inside the preopen,
 * and so the only thing the worker can open a file for. A path in the URL
 * is refused rather than resolved: there is no filesystem out there to
 * reach, and `/data/<name>.db` is not where it would point.
 */
export const isDbName = (value: string): boolean =>
	value !== '' && !value.includes('/') && !value.startsWith('.') && !value.endsWith('.db');

/**
 * The database a URL's query names, and the name it refused.
 *
 * Absent or empty is the default — `?db=` is the same as no `?db` at all,
 * not a database with no name. A refusal still runs the page on the
 * default, so the notice has a working app under it.
 */
export function dbIn(search: string | URLSearchParams): { name: string; refused: string | null } {
	const params = typeof search === 'string' ? new URLSearchParams(search) : search;
	const asked = params.get(DB_PARAM)?.trim() ?? '';
	if (asked === '') return { name: DEFAULT_DB, refused: null };
	if (asked === INTERNAL_DB || !isDbName(asked)) return { name: DEFAULT_DB, refused: asked };
	return { name: asked, refused: null };
}

/**
 * The database THIS page runs against.
 *
 * Read off `location` at each call rather than held in a store: the seam is
 * imported during prerendering, where there is no location, and a
 * client-side navigation changes the query under a module that would
 * otherwise still be holding the old one.
 */
export function currentDb(): string {
	return browser ? dbIn(location.search).name : DEFAULT_DB;
}

/** The name the query asked for and did not get, or null. */
export function refusedDb(): string | null {
	return browser ? dbIn(location.search).refused : null;
}

/**
 * `path` with this page's `?db` on it — what every link and `goto` in the
 * app uses, so navigating stays inside the database you are in.
 */
export function withDb(path: string, db = currentDb()): string {
	if (db === DEFAULT_DB) return path;
	const [base, hash = ''] = path.split('#');
	const [pathname, query = ''] = base.split('?');
	const params = new URLSearchParams(query);
	params.set(DB_PARAM, db);
	return `${pathname}?${params}${hash ? `#${hash}` : ''}`;
}

/** A database's color: what the shell writes for app.css's tokens to read. */
export interface DbColor {
	/** Degrees. */
	hue: number;
	/** A factor on the palette's saturations. */
	sat: number;
	/** Points of lightness added to the grounds. */
	shift: number;
}

/** The lowest `sat` a name can get. */
export const SAT_MIN = 0.4;

/** The darkest `shift` a name can get, at any `sat`. */
export const SHIFT_MIN = -3;

/**
 * The lightest `shift` a name can get at `sat`: more as saturation falls, since
 * the accent, bluest where it is tightest, gains contrast as it loses saturation.
 * Measured, and held, by the walk in brand.test.mjs (da-wikou).
 */
export const shiftMax = (sat: number): number => 2 + 5 * (1 - sat);

/**
 * The color the page is painted in while it is on `name` (da-y992, da-wikou).
 *
 * vividn, 2026-09-16 22:26: "the base color of the app changes depending on
 * the db you select. That way it is both a logo and functional."
 *
 * Derived from the name and nothing else — FNV-1a, a hash whose whole
 * virtue here is that it is the same everywhere: the same database is the
 * same color across reloads, across tabs and across devices, with nothing
 * stored and no setting to disagree with. The default database keeps its
 * palette to the byte, so the app a person already knows has not changed.
 *
 * Saturation leans to 1, so most databases stay vivid and a few are muted.
 * Both it and the shift come from the hash run through murmur3's finalizer:
 * FNV's top bits barely differ between `db1` and `db2`.
 */
export function colorOf(name: string): DbColor {
	if (name === DEFAULT_DB) return { hue: 240, sat: 1, shift: 0 };
	let hash = 0x811c9dc5;
	for (let i = 0; i < name.length; i += 1) {
		hash ^= name.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193);
	}
	let mixed = hash ^ (hash >>> 16);
	mixed = Math.imul(mixed, 0x85ebca6b);
	mixed ^= mixed >>> 13;
	mixed = Math.imul(mixed, 0xc2b2ae35);
	mixed ^= mixed >>> 16;
	const sat = Math.round(100 * (1 - (1 - SAT_MIN) * ((mixed & 0xffff) / 0x10000) ** 2)) / 100;
	const shift = SHIFT_MIN + (shiftMax(sat) - SHIFT_MIN) * ((mixed >>> 16) / 0x10000);
	return { hue: (hash >>> 0) % 360, sat, shift: Math.floor(shift * 10) / 10 };
}
