// The days' data layers, kept past a reload (da-crq76, da-m6i2a).
//
// vividn, 2026-10-26: «will the website have easy access to the cache for
// quick rendering without having to go through the cli?». The day strip and
// the nowview hold each day's data layer in page memory (#lib/dayLayers.ts),
// so a reload started them empty and asked the CLI for every day. Here each
// day's SVG is kept in IndexedDB, by database, kind (`layer`, or `layer-v`
// drawn down the page) and date, and the layers start from it; the one
// `dayview --digests` check then redraws the days whose layer changed, a new
// datum's drawing included. Today's is its latest, which stands in until
// today is drawn again (da-8xy6u).
//
// IndexedDB and not OPFS: OPFS is the database worker's, under the
// `datum-cli-files` lock with sync access handles, and a picture kept there
// would wait its turn behind every command. A cache, never a source of
// truth: a store that will not open, a row that does not read or a write
// refused for quota is a day the CLI draws, as before.

/** A database's kept days: by kind, the most recently used last; each by date, its SVG. */
export type Kept = Map<string, Map<string, string>>;

type Row = { db: string; size: string; date: string; svg: string; at: number };

const NAME = 'datum-held-days';
const STORE = 'days';
/** The dates a band argv keeps, the oldest dropped: a month, as the strip holds in memory. */
const MONTH = 31;
/** The band argvs a database keeps, the least recently used dropped: a resize is a new one. */
const SIZES = 3;
/** How long a strip waits for the store before it draws without it. */
const PATIENCE = 500;

let opened: Promise<IDBDatabase | null> | null = null;
const loads = new Map<string, Promise<Kept>>();
const loaded = new Map<string, Kept>();

const done = <T>(request: IDBRequest<T>): Promise<T> =>
	new Promise((resolve, reject) => {
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});

function open(): Promise<IDBDatabase | null> {
	return (opened ??= new Promise<IDBDatabase | null>((resolve) => {
		try {
			const request = indexedDB.open(NAME, 1);
			request.onupgradeneeded = () => request.result.createObjectStore(STORE, { keyPath: ['db', 'size', 'date'] });
			request.onsuccess = () => resolve(request.result);
			request.onerror = request.onblocked = () => resolve(null);
		} catch {
			resolve(null);
		}
	}));
}

/** Every row of `db`'s and nothing else: arrays sort after strings, so `[db, []]` bounds them. */
const rowsOf = (db: string, size?: string) =>
	size === undefined ? IDBKeyRange.bound([db], [db, []]) : IDBKeyRange.bound([db, size], [db, size, []]);

async function read(db: string): Promise<Kept> {
	const store = (await open())?.transaction(STORE).objectStore(STORE);
	const rows: Row[] = store ? await done(store.getAll(rowsOf(db))) : [];
	const used = new Map<string, number>();
	const kept: Kept = new Map();
	for (const row of rows) {
		if (typeof row?.svg !== 'string' || typeof row.date !== 'string' || typeof row.size !== 'string') continue;
		used.set(row.size, Math.max(used.get(row.size) ?? 0, Number(row.at) || 0));
		if (!kept.has(row.size)) kept.set(row.size, new Map());
		kept.get(row.size)!.set(row.date, row.svg);
	}
	return new Map([...kept].sort(([a], [b]) => used.get(a)! - used.get(b)!));
}

/** `db`'s kept days, once read; undefined until then. */
export const keptNow = (db: string): Kept | undefined => loaded.get(db);

/**
 * `db`'s kept days: an empty store when it cannot be read, or not within
 * `PATIENCE`. A read that lands late is still merged in, this session's own
 * writes winning, and evicted then: the rows of earlier sessions are only
 * ever dropped through what this map holds, and the bigger the store the
 * slower its read.
 */
export function keptDays(db: string): Promise<Kept> {
	if (!loads.has(db)) {
		const empty: Kept = new Map();
		const reading = typeof indexedDB === 'undefined' ? Promise.resolve(empty) : read(db).catch(() => empty);
		const late = new Promise<Kept>((resolve) => setTimeout(() => resolve(empty), PATIENCE));
		loads.set(
			db,
			Promise.race([reading, late]).then((kept) => (loaded.set(db, kept), kept))
		);
		void reading.then((rows) => {
			const kept = loaded.get(db);
			if (!kept || kept === rows || !rows.size) return;
			const session = [...kept];
			kept.clear();
			for (const [size, days] of rows) kept.set(size, days);
			for (const [size, days] of session) {
				const earlier = kept.get(size);
				kept.delete(size);
				kept.set(size, earlier ? new Map([...earlier, ...days]) : days);
			}
			write(db, kept);
		});
	}
	return loads.get(db)!;
}

/** Keep `svg` as `db`'s picture of `date` at band argv `size`. */
export function keepDay(db: string, size: string, date: string, svg: string): void {
	const kept = loaded.get(db);
	if (!kept) return;
	const days = kept.get(size) ?? new Map<string, string>();
	kept.delete(size);
	kept.set(size, days);
	days.set(date, svg);
	write(db, kept, { db, size, date, svg, at: Date.now() });
}

/** Write `row`, if any, and drop from `kept` and the store the dates past `MONTH` of each size and the sizes past `SIZES`. */
function write(db: string, kept: Kept, row?: Row): void {
	const old: [string, string][] = [];
	for (const [size, days] of kept)
		for (const date of [...days.keys()].sort().slice(0, Math.max(0, days.size - MONTH))) {
			days.delete(date);
			old.push([size, date]);
		}
	const gone = [...kept.keys()].slice(0, Math.max(0, kept.size - SIZES));
	for (const size of gone) kept.delete(size);
	if (!row && !old.length && !gone.length) return;
	void open()
		.then((idb) => {
			if (!idb) return;
			const tx = idb.transaction(STORE, 'readwrite');
			// A refused write (quota) is a day the CLI draws next time, not an error.
			tx.onerror = (e) => e.preventDefault();
			const store = tx.objectStore(STORE);
			if (row) store.put(row);
			for (const [size, date] of old) store.delete([db, size, date]);
			for (const size of gone) store.delete(rowsOf(db, size));
		})
		.catch(() => {});
}
