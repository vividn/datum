// Saved layouts: datum entries in the `_internal` database (da-wpgy).
//
// vividn, 2026-09-10: "there should some sort of internal database, but it can
// still use datum's format directly rather than having special tables" — so
// a layout is an ordinary entry of field `layout` in a second datum database,
// slug `layout:<name>`, the whole `Layout` as its data, and it syncs the way
// any database does. Saving in place is `update`, which leaves the old tree
// in the entry's history; restoring an older one is `undo`. Everything goes
// through `run(argv)`, and what comes back is read from the CLI's own JSON.

import { run } from '../run.js';
import type { Layout } from './types.js';

/** A saved layout as the chooser lists it. */
export interface LayoutListing {
	name: string;
	designedFor?: Layout['designedFor'];
	savedAt: string;
}

const slug = (name: string) => `layout:${name}`;

/** `datum --db _internal <argv>`; its stdout, or a rejection with its stderr. */
async function datum(...argv: string[]): Promise<string> {
	const result = await run(['--db', '_internal', ...argv]);
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

async function row(name: string): Promise<Row> {
	const found = (await rows()).find((r) => r.slug === slug(name));
	if (!found) throw new Error(`no saved layout named ${name}`);
	return found;
}

export async function listLayouts(): Promise<LayoutListing[]> {
	return (await rows()).map(({ data, modify_time }) => ({
		name: data.name,
		...(data.designedFor ? { designedFor: data.designedFor } : {}),
		savedAt: modify_time
	}));
}

/** The saved tree, as the last save (or restore) left it. */
export async function loadLayout(name: string): Promise<Layout> {
	const { version, root, designedFor } = (await row(name)).data;
	return { version, name, root, ...(designedFor ? { designedFor } : {}) };
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
