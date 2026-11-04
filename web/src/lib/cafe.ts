// The moment lesson's scene (da-nl1co, da-uzyih.4865): things set on a
// café plate, or a cup beside it, one at a time, at a gentle pace, each
// kind its own moment field, and eaten or drunk soon after. The scene is
// SCRIPTED — a seeded schedule — so it knows what passed and when, and a
// round's record (the entries the taps logged) can be put beside it.
//
// Pure, so the page and a test compute the same scene from the same seed.

/** One kind of thing on the table: its field, its picture. */
export interface Kind {
	field: string;
	glyph: string;
}

export const KINDS: Kind[] = [
	{ field: 'coffee', glyph: '☕' },
	{ field: 'tea', glyph: '🍵' },
	{ field: 'croissant', glyph: '🥐' },
	{ field: 'cake', glyph: '🍰' },
	{ field: 'bagel', glyph: '🥯' }
];

/** One thing set down: its kind's field, and when it appears, in ms from the round's start. */
export interface Item {
	field: string;
	at: number;
}

/** A tap as the record has it: the field it logged, and when, in ms from the round's start. */
export type Tap = Item;

/** A thing stands this long once it appears, then is eaten or drunk, over `GOING_MS`. */
export const STAND_MS = 1500;
export const GOING_MS = 600;
/** The table stands empty at least this long between two things, so one is gone before the next appears. */
const REST_MS = 1400;

/** A tap this soon after a thing of its kind appears is that thing's: while it is on the table, and 0.4 s after it is gone. */
export const MATCH_MS = STAND_MS + GOING_MS + 400;

/** The next round's seed: a test sets one; null draws a fresh one each round. */
export const round: { seed: number | null } = { seed: null };

/** mulberry32: a small seeded generator, a float in [0, 1) per call. */
function random(seed: number): () => number {
	let a = seed >>> 0;
	return () => {
		a = (a + 0x6d2b79f5) >>> 0;
		let t = a;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

/** A round's script: `count` things, the first 3 s in, then one every 3.5 to 6 s. */
export function scene(seed: number, count = 12): { items: Item[]; duration: number } {
	const next = random(seed);
	const items: Item[] = [];
	let at = 3000;
	for (let i = 0; i < count; i++) {
		items.push({ field: KINDS[Math.floor(next() * KINDS.length)].field, at: Math.round(at) });
		at += STAND_MS + GOING_MS + REST_MS + next() * 2500;
	}
	return { items, duration: items[items.length - 1].at + MATCH_MS + 1000 };
}

export interface Score {
	/** Each thing, and the tap matched to it (null: missed). */
	items: { item: Item; tap: Tap | null }[];
	/** Taps that matched nothing. */
	extra: Tap[];
}

/**
 * The record beside the scene: each tap is matched to a thing of its kind
 * that appeared at most `MATCH_MS` before it, the nearest pairs first,
 * each thing and each tap once.
 */
export function score(items: Item[], taps: Tap[]): Score {
	const pairs: { i: number; j: number; d: number }[] = [];
	items.forEach((item, i) =>
		taps.forEach((tap, j) => {
			const d = tap.at - item.at;
			if (tap.field === item.field && d >= 0 && d <= MATCH_MS) pairs.push({ i, j, d });
		})
	);
	pairs.sort((a, b) => a.d - b.d);
	const tapOf = new Map<number, number>();
	const used = new Set<number>();
	for (const { i, j } of pairs)
		if (!tapOf.has(i) && !used.has(j)) {
			tapOf.set(i, j);
			used.add(j);
		}
	return {
		items: items.map((item, i) => ({ item, tap: tapOf.has(i) ? taps[tapOf.get(i)!] : null })),
		extra: taps.filter((_, j) => !used.has(j))
	};
}
