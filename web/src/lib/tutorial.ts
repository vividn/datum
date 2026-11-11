// datum.day's tutorial (da-uzyih; the frame, TU1, da-6cvrd).
//
// vividn, 2026-10-19: «a button that switches to the "tutorial" db, which is
// a special db, with many special tutorial widgets for helping you learn
// how to a. pay attention and track data in general b. use the datum
// software in particular. there would be many tabs in order which walk you
// through different aspects.» So the tutorial is a DATABASE, «tutorial»,
// and nothing else: the page runs against it as against any other
// (`?db=tutorial`), recolored by its name, and the reader's own database
// is never written. What makes it the tutorial is a KIT, `kits/tutorial.toml`
// — its lessons' fields, and `no_sync`, so the practice never reaches a
// chain — and its tabs, one kit layout per lesson, which the shell installs
// beside it on the first visit (`shipLayouts`, shell.ts). The lessons and
// their tabs are data, in `defaults.ts`; this is what is DONE with them.
//
// Each lesson names its own fields (`coffee` …, LESSONS), so its widgets
// show its own, and a game's Reset ends those and no others.

import { endArgv, ignoreArgv, nowArgv, specShowArgv, trackArgv, tutorialAliasArgv, tutorialKitArgv } from './calls.js';
import { currentDb, withDb } from './database.js';
import { FROZEN_AT } from './dbProtocol.js';
import { LESSONS, TUTORIAL_DB, type Lesson } from './layout/defaults.js';
import type { Layout } from './layout/types.js';
import { cliError, run, type RunResult } from './run.js';

export const lessonNamed = (name: unknown): Lesson | undefined => LESSONS.find((l) => l.name === name);

/** How a command runs: quietly, or `loudly`, where the reader sees it. */
export type Through = (argv: string[], stdin?: string, env?: string[]) => Promise<RunResult>;

export const quietly: Through = (argv, stdin, env) => run(argv, stdin, undefined, undefined, undefined, env);

/**
 * Shown in the CLI as it runs (`shown`, engine.ts; da-uzyih.4893, vividn, 2026-10-28: «let's not hide the plumbing»).
 * Imported as it runs: the engine imports the shell, which imports this module.
 */
export const loudly: Through = (argv, stdin, env) => import('./layout/engine.js').then(({ shown }) => shown(argv, stdin, env));

/** The env word that keeps a command off the undo stack: the page set this up, so a bare `undo` leaves it (da-uzyih.4937). */
const NOT_UNDOABLE = ['DATUM_NOT_UNDOABLE=1'];

/** A database's setup that nobody sees, and nobody's to undo. */
export const settingUpQuietly: Through = (argv, stdin, env = []) => quietly(argv, stdin, [...env, ...NOT_UNDOABLE]);

async function must(argv: string[], stdin?: string, env?: string[], through = quietly): Promise<string> {
	const result = await through(argv, stdin, env);
	if (result.code !== 0) throw cliError(result.stderr, `datum ${argv[0]} exited ${result.code}`);
	return result.stdout;
}

/**
 * The kit, into this database. Read when it is installed, not at import:
 * the shell imports this module, and node (the pure tests) cannot load a
 * `.toml`.
 */
/** The word a newcomer reads for the setup. */
const SETUP = 'setup-tutorial';

const tutorialKit = async (): Promise<string> => (await import('../../../kits/tutorial.toml?raw')).default;

/** The kit alone, quietly: a load that finds its tabs current. Unchanged, it writes nothing. */
export async function installTutorialKit(): Promise<string> {
	return must(tutorialKitArgv(), await tutorialKit(), undefined, settingUpQuietly);
}

/**
 * The whole setup, one command (da-uzyih.4937; vividn, 2026-11-01: «it should be
 * done as a single command, maybe aliased for the tutorial … Curious users could
 * dig further»): an alias of `kit install -`, whose stdin is the kit with each of
 * `tabs` as its `[[layout]]`. `alias list` says what it runs and `kit show tutorial`
 * what it installed. Shown as it runs, and nobody's to undo.
 */
export async function setUpTutorial(tabs: Layout[]): Promise<string> {
	await must(tutorialAliasArgv(SETUP), undefined, undefined, settingUpQuietly);
	const blocks = tabs.map(({ name, root }) => `\n[[layout]]\nname = ${JSON.stringify(name)}\nroot = ${JSON.stringify(JSON.stringify(root))}\n`);
	return must([SETUP], (await tutorialKit()) + blocks.join(''), NOT_UNDOABLE, loudly);
}

/** One number of a game's result, as its box shows it: «accuracy», «87%». */
export interface Outcome {
	name: string;
	value: string;
}

/** Where a game's box is: at rest, a game running, its record being read, its result shown. */
export type Phase = 'neutral' | 'playing' | 'scoring' | 'done';

/** A share as a whole percent; none of nothing is a dash. */
export const percent = (part: number, whole = 1): string => (whole ? `${Math.round((part / whole) * 100)}%` : '—');

/**
 * A game's Reset, once it is over (da-uzyih.4864, da-uzyih.4910): every state of `fields` active now
 * ended, so an on/off field that is on goes off and a switch field goes to
 * false — or, a field whose spec is `never_false` (the switch lesson's
 * `room`, da-uzyih.4880), which takes no end, IGNORED. It deletes nothing: vividn, 2026-10-27, «we should just leave the
 * data in the db, we can do analysis on it later in the tutorial». In the
 * tutorial alone: `run` acts on whatever database the page is on, and a
 * lesson's field name may be one a reader logs for real.
 */
export async function startOver(fields: string[]): Promise<void> {
	if (currentDb() !== TUTORIAL_DB) throw new Error(`Reset ends states only in the ${TUTORIAL_DB} database`);
	const { active } = JSON.parse(await must(nowArgv())) as { active: { field: string; state: unknown }[] };
	for (const { field, state } of active) {
		if (!fields.includes(field)) continue;
		const spec = JSON.parse(await must(specShowArgv(field))) as { never_false?: boolean };
		if (spec.never_false) {
			await must(ignoreArgv([field]), undefined, undefined, loudly);
			continue;
		}
		// Ended only from the state read (da-v2d29): one the reader changed since stands, and none is ended twice.
		// The CLI's refusals for either: `-L S: F is in T, so nothing is written` (changed since) and `No open
		// block for 'F'` (ended since); `ensure_last_state.rs` pins both words.
		const { code, stderr } = await loudly(endArgv(field, state));
		if (code !== 0 && !/so nothing is written|No open block/.test(stderr)) throw cliError(stderr, `datum off exited ${code}`);
	}
}

/**
 * A game's fields TRACKED as it begins, and IGNORED once it ends
 * (da-uzyih.4882; vividn, 2026-10-27: «the fields on the other tutorial
 * pages shoudl also be IGNORED after the game ends, and TRACKED when it
 * begins»). In the tutorial alone, as Reset is.
 *
 * Written `at` that instant (epoch ms), not when the queue reaches it: a tap
 * is logged at its own instant (da-szzgy), and a TRACKED stamped after one
 * would end the block it started. So the game passes one just before its
 * start, and one just after its taps.
 */
export async function keep(fields: string[], tracked: boolean, at: number): Promise<void> {
	if (currentDb() !== TUTORIAL_DB) throw new Error(`A game tracks and ignores fields only in the ${TUTORIAL_DB} database`);
	// One command for them all, shown (da-uzyih.4893), so no tap queues between two of them and the bar's `undo` still takes the tap.
	await must(tracked ? trackArgv(fields) : ignoreArgv(fields), undefined, [`${FROZEN_AT}${at}`], loudly);
}

/**
 * Into the tutorial: a load, as the database switcher's is (Brand.svelte),
 * on the page's root.
 */
export const openTutorial = (): void => location.assign(withDb('/', TUTORIAL_DB));
