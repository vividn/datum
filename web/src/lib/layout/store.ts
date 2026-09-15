// Saved layouts: datum entries in the `_internal` database (da-wpgy) — and
// the ones a KIT shipped, in the data database beside them (da-gg0v).
//
// vividn, 2026-09-10: "there should some sort of internal database, but it can
// still use datum's format directly rather than having special tables" — so
// a layout is an ordinary entry of field `layout` in a second datum database,
// slug `layout:<name>`, the whole `Layout` as its data, and it syncs the way
// any database does. Saving in place is `update`, which leaves the old tree
// in the entry's history; restoring an older one is `undo`. Everything goes
// through `run(argv)`, and what comes back is read from the CLI's own JSON.
//
// TWO STORES, ONE NAME SPACE (da-gg0v). vividn, 2026-09-15 18:53: "layouts
// have to go inside the db proper, not the _internal db, yes? I guess there
// could be layouts in both; some layouts might be db specific." So there
// are: `_internal` holds the person's own, and the DATA database holds
// whatever a kit installed there (`crates/datum-core/src/db/kit_layout.rs`
// writes exactly the rows this file reads). A kit layout is db-specific by
// construction and reaches every device of that database, because the data
// syncs. Everything WRITTEN here still goes to `_internal`: a save of a kit
// layout's name is the person's own copy, and it shadows the kit's.
//
// The kit half is CACHED rather than read on demand, because `shippedTree`
// (shell.ts, the reset's target) has to answer synchronously while a menu
// is being built. `refreshKitLayouts` is what fills it: at boot, when
// another tab writes to the data db, and whenever the chooser is opened —
// so a kit installed at the prompt is offered without a reload.

import { run } from '../run.js';
import type { Layout } from './types.js';

/**
 * Where a layout came from: the app's own (`defaults.ts`), a kit that
 * installed it into this database, or this person's save. A save shadows
 * either of the other two under the same name.
 */
export type LayoutSource = 'builtin' | 'user' | `kit:${string}`;

/** A saved layout as the chooser lists it. */
export interface LayoutListing {
	name: string;
	designedFor?: Layout['designedFor'];
	savedAt: string;
	source: LayoutSource;
}

const slug = (name: string) => `layout:${name}`;

/** `datum --db _internal <argv>`; its stdout, or a rejection with its stderr. */
function datum(...argv: string[]): Promise<string> {
	return stdoutOf(['--db', '_internal', ...argv]);
}

/** `datum <argv>` against the DATA database — where a kit installs. */
function data(...argv: string[]): Promise<string> {
	return stdoutOf(argv);
}

async function stdoutOf(argv: string[]): Promise<string> {
	const result = await run(argv);
	if (result.code !== 0) throw new Error(result.stderr || `datum ${argv[0]} exited ${result.code}`);
	return result.stdout;
}

/**
 * The tree, as `-k` data. Every value JSON, so a name like `123` stays a
 * string; an empty `designedFor=` removes one a previous save had.
 */
const keys = (layout: Layout) => [
	'-k',
	`version=${layout.version}`,
	'-k',
	`root=${JSON.stringify(layout.root)}`,
	'-k',
	`designedFor=${layout.designedFor ? JSON.stringify(layout.designedFor) : ''}`
];

interface Row {
	hid: string;
	slug: string;
	data: Layout;
	modify_time: string;
}

/** Every saved layout's entry: `tail --json` on the field, newest first. */
async function rows(): Promise<Row[]> {
	return JSON.parse(await datum('tail', 'layout', '--json', '-n', '100000')).rows;
}

/** A row's data as a `Layout` — the two stores hold the same shape. */
const layoutOf = ({ data: d }: Row): Layout => ({
	version: d.version,
	name: d.name,
	root: d.root,
	...(d.designedFor ? { designedFor: d.designedFor } : {})
});

// ---- the kit half (da-gg0v) -------------------------------------------

/** What a kit shipped, by name; filled by `refreshKitLayouts`. */
let kits = new Map<string, { layout: Layout; kit: string }>();

/** The layout kit `name` shipped into this database, if any. Synchronous. */
export const kitLayout = (name: string): Layout | null => kits.get(name)?.layout ?? null;

/** Which kit shipped `name`, if one did. */
export const kitOf = (name: string): string | null => kits.get(name)?.kit ?? null;

/**
 * Re-read the data database's `layout` entries into the cache.
 *
 * Only rows carrying a `kit_id` — a `layout` entry without one was written
 * by something other than an install, and is not a layout this app shipped
 * anyone. A database that cannot be read (a fresh profile, a failed
 * process) leaves the cache as it was rather than emptying it.
 */
export async function refreshKitLayouts(): Promise<void> {
	let listed: Row[];
	try {
		listed = JSON.parse(await data('tail', 'layout', '--json', '-n', '100000')).rows;
	} catch {
		return;
	}
	const next = new Map<string, { layout: Layout; kit: string }>();
	for (const row of listed) {
		const kit = (row.data as { kit_id?: unknown }).kit_id;
		if (typeof kit !== 'string' || !row.data?.name) continue;
		if (!next.has(row.data.name)) next.set(row.data.name, { layout: layoutOf(row), kit });
	}
	kits = next;
}

async function row(name: string): Promise<Row> {
	const found = (await rows()).find((r) => r.slug === slug(name));
	if (!found) throw new Error(`no saved layout named ${name}`);
	return found;
}

/**
 * Every layout either store holds, the person's own first.
 *
 * One name space across the two: a name present in both is the USER's, as
 * a save shadows a kit's layout everywhere else. The kit's is still
 * reachable — `shippedTree` is what a reset goes back to — it is simply
 * not listed twice.
 */
export async function listLayouts(): Promise<LayoutListing[]> {
	const mine: LayoutListing[] = (await rows()).map(({ data, modify_time }) => ({
		name: data.name,
		...(data.designedFor ? { designedFor: data.designedFor } : {}),
		savedAt: modify_time,
		source: 'user' as const
	}));
	const taken = new Set(mine.map((l) => l.name));
	const shipped: LayoutListing[] = [...kits].flatMap(([name, { layout, kit }]) =>
		taken.has(name)
			? []
			: [
					{
						name,
						...(layout.designedFor ? { designedFor: layout.designedFor } : {}),
						savedAt: '',
						source: `kit:${kit}` as const
					}
				]
	);
	return [...mine, ...shipped];
}

/** The saved tree, as the last save (or restore) left it. */
export async function loadLayout(name: string): Promise<Layout> {
	return layoutOf(await row(name));
}

/** A layout under a name no entry has yet. Rejects if the name is taken. */
function create(layout: Layout): Promise<string> {
	return datum('occur', 'layout', '--id', '%name', '-k', `name=${JSON.stringify(layout.name)}`, ...keys(layout));
}

/** Save in place — a new version of the entry — or create it if absent. */
export async function saveLayout(layout: Layout): Promise<void> {
	const updated = await run(['--db', '_internal', 'update', slug(layout.name), ...keys(layout)]);
	if (updated.code === 0) return;
	await create(layout).catch((e: Error) => {
		throw new Error(`${updated.stderr}\n${e.message}`);
	});
}

/** A new layout under `newName`, leaving the original as it was. */
export async function saveAs(layout: Layout, newName: string): Promise<Layout> {
	const copy = { ...layout, name: newName };
	await create(copy);
	return copy;
}

/** The entry's versions, newest first, as `history --json` lists them. */
export async function versions(name: string): Promise<{ version: number; modifyTime: string }[]> {
	const trail = JSON.parse(await datum('history', slug(name), '--json'));
	return trail.map((v: { version: number; modify_time: string }) => ({
		version: v.version,
		modifyTime: v.modify_time
	}));
}

/**
 * Back to `version`, or — without one — undo the last change, which after a
 * restore is the way forward again. Either way it is a new version, so
 * nothing is lost by restoring. Resolves with the tree now saved.
 */
export async function restore(name: string, version?: number): Promise<Layout> {
	await datum('undo', slug(name), ...(version === undefined ? [] : ['--to-version', String(version)]));
	return loadLayout(name);
}

/** By hid, since `del` splits its argument at commas and a name may have one. */
export async function deleteLayout(name: string): Promise<void> {
	await datum('del', (await row(name)).hid, '--force');
}
