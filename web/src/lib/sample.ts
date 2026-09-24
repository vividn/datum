// The DEBUG database's sample week (da-6ecm5).
//
// vividn, 2026-09-21: "Maybe make some other DEBUG widgets, like generating
// some nice sample data for the past few hours or something." And on #118:
// "adding sample data shouldn't delete existing data or vice versa. Ideally
// the sample data just fills in the last 7 days with some nice sample data."
// An empty widget looks identical to a broken one, so this is SHAPE, not
// volume: a week in which nowview, dayview, tail, stats and the fields
// widget each have something true-looking to show.
//
// It is a list of datum commands, run through the seam like any other, so
// what it writes is what a person could have typed. No database is named:
// the page's own is the one written, and the widget that runs this exists
// only in `DEBUG` (DebugPane.svelte). The times are built from the clock at
// the moment of seeding, jittered afresh every time, and none is after now.
//
// Every sample entry carries `sample=true`, and THAT is what the sample is
// when it is counted or cleared — never its fields: DEBUG is for testing,
// and a `coffee` logged there by hand is not the sample's to delete. The
// tag is how "what seeding wrote" outlives the page that wrote it.

import { stripAnsi } from './ansi.js';
import { run, type RunResult } from './run.js';

/** The key every sample entry carries, and the one thing clearing looks at. */
const TAG = 'sample=true';

/**
 * One entry of a day: when (minutes after local midnight), by how many
 * minutes either way it may wander, and the words — `a|b` is a choice made
 * afresh each seed. The anchors are far enough apart that no jitter puts a
 * block's end before its start.
 */
type Slot = [minutes: number, jitter: number, words: string[]];

/**
 * A weekday: `sleep` and `work` blocks (two of work, a gap between), `walk`
 * a block given by its duration, `mood happy 20m` a state-change block —
 * data-to and data-from, both — `project` and `mood` switching between
 * states, and `coffee` and `water` as occur dots.
 */
const WEEKDAY: Slot[] = [
	[430, 25, ['end', 'sleep']],
	[460, 15, ['occur', 'coffee']],
	[480, 30, ['switch', 'mood', 'focused|calm']],
	[535, 5, ['switch', 'project', 'datum|email']],
	[545, 5, ['start', 'work']],
	[630, 40, ['occur', 'water', 'amount=300']],
	[730, 15, ['end', 'work']],
	[775, 8, ['start', 'walk', '25m']],
	[800, 8, ['start', 'work']],
	[870, 30, ['occur', 'coffee']],
	[930, 30, ['switch', 'project', 'email|admin|datum']],
	[965, 30, ['occur', 'water', 'amount=250']],
	[1005, 15, ['switch', 'mood', 'happy', '20m']],
	[1060, 20, ['end', 'work']],
	[1260, 30, ['switch', 'mood', 'tired|calm']],
	[1385, 30, ['start', 'sleep']]
];

/** A weekend day: a later morning, a long walk, no work. */
const WEEKEND: Slot[] = [
	[520, 40, ['end', 'sleep']],
	[550, 30, ['occur', 'coffee']],
	[570, 40, ['switch', 'mood', 'calm|happy']],
	[690, 30, ['start', 'walk', '70m']],
	[780, 60, ['occur', 'water', 'amount=300']],
	[900, 60, ['switch', 'project', 'admin']],
	[960, 45, ['occur', 'coffee']],
	[1230, 40, ['switch', 'mood', 'tired|calm']],
	[1420, 20, ['start', 'sleep']]
];

const pad = (n: number, width = 2) => String(n).padStart(width, '0');
const random = (options: string) => {
	const each = options.split('|');
	return each[Math.floor(Math.random() * each.length)];
};

/**
 * The last seven days to `now`, oldest first, as argv.
 *
 * At least one block is still OPEN at the end — the one the now chunk and
 * "double tap an open block" need. `work` or `sleep` usually is; when
 * neither is (an evening, a weekend afternoon), a `read` started a little
 * before now is.
 */
export function sampleCommands(now = new Date()): string[][] {
	const from = now.getTime() - 7 * 24 * 60 * 60 * 1000;
	const at: [Date, string[]][] = [];
	for (let ago = 7; ago >= 0; ago -= 1) {
		const day = new Date(now.getFullYear(), now.getMonth(), now.getDate() - ago);
		const slots = day.getDay() % 6 === 0 ? WEEKEND : WEEKDAY;
		for (const [minutes, jitter, words] of slots) {
			const wander = Math.round((Math.random() * 2 - 1) * jitter);
			// Seconds and milliseconds too: two seeds landing an entry of one
			// field on the same instant would be the same slug, and refused.
			const when = new Date(
				day.getFullYear(),
				day.getMonth(),
				day.getDate(),
				0,
				minutes + wander,
				Math.floor(Math.random() * 60),
				Math.floor(Math.random() * 1000)
			);
			if (when.getTime() > from && when <= now) at.push([when, words.map(random)]);
		}
	}
	const open = (field: string) =>
		at.findLast(([, [verb, f]]) => f === field && verb !== 'occur')?.[1][0] === 'start';
	if (!open('work') && !open('sleep'))
		at.push([new Date(now.getTime() - (20 + Math.random() * 30) * 60 * 1000), ['start', 'read']]);
	return at
		.sort(([a], [b]) => a.getTime() - b.getTime())
		.map(([when, words]) => [
			...words,
			'-d',
			`${when.getFullYear()}-${pad(when.getMonth() + 1)}-${pad(when.getDate())}`,
			'-t',
			`${pad(when.getHours())}:${pad(when.getMinutes())}:${pad(when.getSeconds())}.${pad(when.getMilliseconds(), 3)}`,
			TAG
		]);
}

/** One command the seeder ran, and what came back. */
export type Step = Pick<RunResult, 'argv' | 'code' | 'stdout' | 'stderr'>;

/** Run `argv`, report it, and throw if it failed — a half-seeded database must not claim success. */
async function step(argv: string[], onStep: (s: Step) => void): Promise<RunResult> {
	const result = await run(argv);
	onStep(result);
	if (result.code !== 0) throw new Error(`\`datum ${argv.join(' ')}\` exited ${result.code}: ${result.stderr.trim()}`);
	return result;
}

/** The sample's entries now in the database, by full hid — read back, never counted from what was sent. */
async function tagged(onStep: (s: Step) => void): Promise<string[]> {
	const found = await step(['grep', '"sample":true', '-o', '%full_hid%'], onStep);
	return stripAnsi(found.stdout)
		.split('\n')
		.filter((line) => /^[0-9a-z]+$/.test(line));
}

/**
 * Throw the sample away: the entries tagged as its own, and nothing else —
 * not the layout row, not a `coffee` logged by hand. Resolves with the
 * count left, read back (0 unless something refused).
 */
export async function clearSample(onStep: (s: Step) => void): Promise<number> {
	const hids = await tagged(onStep);
	if (!hids.length) return 0;
	await step(['delete', hids.join(','), '--show', 'none'], onStep);
	return (await tagged(onStep)).length;
}

/**
 * Add a sample week, ending now. It only ADDS: a sample already there
 * stays, and the caller says so. Resolves with the sample entries before
 * and after, read back, and how many were sent; rejects at the first
 * command that fails.
 */
export async function seedSample(
	onStep: (s: Step) => void
): Promise<{ before: number; after: number; sent: number }> {
	const before = (await tagged(onStep)).length;
	const commands = sampleCommands();
	for (const argv of commands) await step(argv, onStep);
	return { before, after: (await tagged(onStep)).length, sent: commands.length };
}
