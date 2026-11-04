// The switch lesson's scene (da-uzyih.4867): a house seen from above, and a
// person, a dot, wandering from room to room and out into the garden and
// back, and about each room while there, from one piece of its furniture to
// the next (da-uzyih.4880). Where they are is one switch field, `room`,
// a state a room. The scene is SCRIPTED — a seeded walk — so it knows where
// the dot really was, and a round's record (the entries the taps logged)
// can be laid beside it.
//
// Pure, so the page and a test compute the same scene from the same seed.

/** The lesson's one field. */
export const FIELD = 'room';

/**
 * A piece of furniture, its shapes on the plan: a rect `[x, y, width, height, rx]`, a circle `[cx, cy, r]`.
 * Drawn in its room's color, unless `colors` gives each shape its own.
 */
export interface Thing {
	name: string;
	shapes: number[][];
	colors?: string[];
}

/** A cluster of flowers, one at each point: five petals in `petal` around a yellow heart. */
function flowers(name: string, petal: string, ...at: [number, number][]): Thing {
	const flower = ([x, y]: [number, number]) => [
		...[0, 1, 2, 3, 4].map((k) => [x + 2.6 * Math.sin((k * 2 * Math.PI) / 5), y - 2.6 * Math.cos((k * 2 * Math.PI) / 5), 1.9]),
		[x, y, 1.4]
	];
	return { name, shapes: at.flatMap(flower), colors: at.flatMap(() => [petal, petal, petal, petal, petal, '#ffd43b']) };
}

/**
 * A place the dot can be: its state, the floor it covers on the plan, its
 * middle, where a walk into it ends, the other spots it stands at while
 * there, each by a piece of furniture, and the furniture, which says what
 * room it is.
 */
export interface Room {
	state: string;
	x: number;
	y: number;
	width: number;
	height: number;
	at: [number, number];
	spots: [number, number][];
	things: Thing[];
}

/**
 * The plan, in its viewBox (`PLAN`): four rooms two by two, the bathroom and
 * the living room over the bedroom and the kitchen, and the garden against
 * the house's right wall.
 */
export const ROOMS: Room[] = [
	{
		state: 'kitchen',
		x: 104,
		y: 56,
		width: 100,
		height: 52,
		at: [154, 86],
		spots: [
			[186, 86],
			[130, 84]
		],
		things: [
			{ name: 'counter', shapes: [[120, 94, 32, 12, 1]] },
			{ name: 'stove', shapes: [[176, 58, 24, 18, 1], [182, 63, 2.5], [194, 63, 2.5], [182, 71, 2.5], [194, 71, 2.5]] }
		]
	},
	{
		state: 'living_room',
		x: 104,
		y: 4,
		width: 100,
		height: 52,
		at: [154, 34],
		spots: [
			[127, 30],
			[183, 32]
		],
		things: [
			{ name: 'sofa', shapes: [[112, 40, 30, 14, 3], [112, 49, 30, 5, 1]] },
			{ name: 'table', shapes: [[172, 42, 22, 10, 2]] }
		]
	},
	{
		state: 'bedroom',
		x: 4,
		y: 56,
		width: 100,
		height: 52,
		at: [54, 86],
		spots: [
			[50, 99],
			[88, 84]
		],
		things: [
			{ name: 'bed', shapes: [[8, 76, 34, 28, 2], [10, 79, 8, 22, 2]] },
			{ name: 'wardrobe', shapes: [[78, 94, 22, 12, 1]] }
		]
	},
	{
		state: 'bathroom',
		x: 4,
		y: 4,
		width: 100,
		height: 52,
		at: [54, 34],
		spots: [
			[46, 42],
			[80, 24]
		],
		things: [
			{ name: 'bath', shapes: [[8, 22, 28, 31, 6], [11, 25, 22, 25, 5]] },
			{ name: 'sink', shapes: [[88, 13, 5]] }
		]
	},
	{
		state: 'garden',
		x: 204,
		y: 4,
		width: 112,
		height: 104,
		at: [264, 60],
		spots: [
			[270, 38],
			[246, 78],
			[280, 66]
		],
		things: [
			{ name: 'tree', shapes: [[292, 30, 13], [298, 86, 10]] },
			{ name: 'bench', shapes: [[232, 88, 28, 8, 1]] },
			flowers('cosmos', '#f783ac', [258, 14], [268, 20], [263, 25]),
			flowers('daisies', '#f8f9fa', [270, 91], [279, 98], [268, 102])
		]
	}
];

export const PLAN = { width: 320, height: 112 };

/** The walls, with a gap at each doorway. */
export const WALLS = [
	'M204 22 V4 H4 V108 H204 V90 M204 74 V38',
	'M104 4 V22 M104 38 V74 M104 90 V108',
	'M4 56 H46 M62 56 H146 M162 56 H204'
].join(' ');

/** Each doorway: the two places it joins, and its middle, where a walk through it crosses. */
export const DOORS: { between: [string, string]; at: [number, number] }[] = [
	{ between: ['bathroom', 'living_room'], at: [104, 30] },
	{ between: ['bedroom', 'kitchen'], at: [104, 82] },
	{ between: ['bathroom', 'bedroom'], at: [54, 56] },
	{ between: ['living_room', 'kitchen'], at: [154, 56] },
	{ between: ['living_room', 'garden'], at: [204, 30] },
	{ between: ['kitchen', 'garden'], at: [204, 82] }
];

/** The dot taking a place: the state, and when it crosses the doorway into it, in ms from the round's start. */
export interface Change {
	state: string;
	at: number;
}

/** A tap as the record has it: the same. */
export type Tap = Change;

/** How long a round is. */
export const ROUND_MS = 60_000;

/** How long a walk from one place to the next one's middle takes; it crosses the doorway halfway. */
export const WALK_MS = 2000;

/** While the dot stays in a place, how often it moves to another spot there, and how long the step takes. */
export const HOP_MS = 2500;
export const STEP_MS = 800;

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

const room = (state: string): Room => ROOMS.find((r) => r.state === state)!;
const door = (a: string, b: string) => DOORS.find(({ between }) => between.includes(a) && between.includes(b))!;
const neighbors = (state: string): string[] =>
	DOORS.filter(({ between }) => between.includes(state)).map(({ between }) => between.find((s) => s !== state)!);

/**
 * A round's script: the dot starts in a place at 0 — never `not`, the
 * state already held, so it is switched to — and walks through a doorway
 * into a neighboring one every 5 to 11 s (da-uzyih.4880; it was 6 to 14),
 * until 3 s before the end.
 */
export function scene(seed: number, not?: string): Change[] {
	const next = random(seed);
	const from = (choices: string[]) => choices[Math.floor(next() * choices.length)];
	const changes = [{ state: from(ROOMS.map((r) => r.state).filter((s) => s !== not)), at: 0 }];
	for (let at = 5000 + next() * 6000; at < ROUND_MS - 3000; at += 5000 + next() * 6000)
		changes.push({ state: from(neighbors(changes[changes.length - 1].state)), at: Math.round(at) });
	return changes;
}

/** The state a timeline (in time order) holds at `t`: its last change at or before it, or null before its first. */
export function stateAt(timeline: Change[], t: number): string | null {
	let state: string | null = null;
	for (const change of timeline) if (change.at <= t) state = change.state;
	return state;
}

const toward = (p: [number, number], q: [number, number], k: number): [number, number] => [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k];

/**
 * Where the dot stands at `t` while it stays in `changes[i]`'s place: at its
 * middle as it arrives, then every hop at another of its spots, a step
 * away, until its walk out begins. Where motion is unwelcome the hops are
 * half as many, and jump.
 */
function within(changes: Change[], i: number, t: number, motion: boolean): [number, number] {
	const { at, spots } = room(changes[i].state);
	const all = [at, ...spots];
	const half = motion ? WALK_MS / 2 : 0;
	const [hop, step] = motion ? [HOP_MS, STEP_MS] : [2 * HOP_MS, 0];
	const from = i ? changes[i].at + half : 0;
	const until = i + 1 < changes.length ? changes[i + 1].at - half : Infinity;
	let [here, spot] = [at, 0];
	for (let k = 1; ; k++) {
		const go = from + k * hop;
		if (go > t || go + step > until) return here;
		// Never the spot it stands at, and not always the same round.
		spot = (spot + 1 + ((i * 5 + k * 3) % (all.length - 1))) % all.length;
		if (t < go + step) return toward(here, all[spot], (t - go) / step);
		here = all[spot];
	}
}

/**
 * Where the dot is drawn at `t`: about its place, or, while it walks, on
 * the line from where it stood through the doorway to the next place's
 * middle. Where motion is unwelcome it jumps.
 */
export function dotAt(changes: Change[], t: number, motion = true): [number, number] {
	const i = motion ? changes.findIndex((c, i) => i > 0 && Math.abs(t - c.at) < WALK_MS / 2) : -1;
	if (i < 0) return within(changes, Math.max(0, changes.findLastIndex((c) => c.at <= t)), t, motion);
	const [a, b] = [within(changes, i - 1, changes[i].at - WALK_MS / 2, true), room(changes[i].state).at];
	const d = door(changes[i - 1].state, changes[i].state).at;
	const u = (t - (changes[i].at - WALK_MS / 2)) / (WALK_MS / 2);
	return u < 1 ? toward(a, d, u) : toward(d, b, u - 1);
}

/**
 * The share of the scene's first `length` ms the record held the place the
 * dot was: it matches wherever its last tap is that place, and before its
 * first tap it holds none.
 */
export function accuracy(changes: Change[], taps: Tap[], length = ROUND_MS): number {
	const mine = [...taps].sort((a, b) => a.at - b.at);
	const edges = [...new Set([0, length, ...changes.map((c) => c.at), ...mine.map((t) => t.at)])]
		.filter((t) => t <= length)
		.sort((a, b) => a - b);
	let same = 0;
	for (let i = 0; i < edges.length - 1; i++) {
		const held = stateAt(mine, edges[i]);
		if (held !== null && held === stateAt(changes, edges[i])) same += edges[i + 1] - edges[i];
	}
	return same / length;
}
