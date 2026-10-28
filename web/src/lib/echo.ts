// The CLI's action echo, as data (da-xfa4, on top of da-yc9w's `--show json`).
//
// The bottom bar is the app's status line: what it shows after a command
// is one SECTION PER DOCUMENT — the action and its hid, the info line,
// and the data on one line. Getting there by parsing the two lines the
// CLI prints would be the thing `docs/web-conventions.md` #4 forbids
// twice over: a second formatter, and a web that rebuilds a string datum
// already owns. So the web asks datum for those strings instead.
//
// `--show json` is asked for at the ONE seam a reader's command passes
// through — the CLI widget's `runArgv`, and `engine.alone` when no CLI widget is
// mounted — and asked for in the ENV, `DATUM_SHOW=json`, never in argv
// (da-vx31e): the CLI records argv as the entry's `raw_input`, so a flag
// appended there was quoted by `why` as part of a line nobody typed. vividn:
// "The whole web interface can take use json as the default show and then
// render on it's own terms which is nice separation." So the CLI widget's
// scrollback carries the same documents the bar does, and a
// multi-document result gets the same treatment in both.
//
// The web's own flags (`--width`, #lib/panes.ts `fitArgv`) are NOT part of
// what a press teaches: the scrollback echoes the argv the caller asked
// for, not the one the runner received (CliScrollback.svelte, engine.press).

import type { CommandRunner, RunResult } from './run.js';

/**
 * One run of the info line, as the CLI painted it (da-yu9s).
 *
 * `color` is the hex the terminal would have used — the chip's background
 * for the field badge and the state cell, the foreground for a marker —
 * and `null` where the run is unpainted. The texts concatenated are the
 * whole of `info`, so a surface places them and never composes one.
 */
export interface InfoSegment {
	text: string;
	color: string | null;
}

/** One document of the CLI's echo — `--show json`'s own field names. */
export interface EchoDoc {
	/** `CREATE`, `EXISTS`, `DELETE`, … */
	action: string;
	/** The short, prefix-addressable id the CLI prints. */
	hid: string;
	/** The full id (`coffee:2026-03-16T09:15Z`), behind the hid. */
	id: string;
	/** Action + hid + id, as the default show prints the first line. */
	header: string;
	/** The second line: time, field, state. */
	info: string;
	/** The data block's rendered pairs, one per line. */
	data: string[];
	/** The same pairs joined for one line. */
	data_inline: string;
	/** The field the entry tracks. */
	field?: string;
	/** The state as logged — a string, a bool, null, an object. */
	state?: unknown;
	/**
	 * `#rrggbb` for the ACTION word — the colour the terminal paints the tag
	 * (da-ckw3, `Outcome::hex`): green a CREATE, red a DELETE or a FAILED,
	 * yellow an EXISTS, cyan an UPDATE/RENAME/RETIME, blue an OWRITE, orange
	 * a NODIFF. Absent from a module too old to carry it, which is why every
	 * surface falls back to its own accent.
	 */
	action_color?: string;
	/** `#rrggbb` for the field, as `fields --json` publishes it. */
	field_color?: string;
	/** `#rrggbb` for the state, or null where the state paints none. */
	state_color?: string | null;
	/**
	 * `info` split into its coloured runs, so the line can be painted
	 * without looking for the field inside it — `switch mood mood` names it
	 * twice, and only the CLI knows which is which. Absent from a module
	 * too old to carry them, which is why the renderer falls back to `info`.
	 */
	info_segments?: InfoSegment[];
}

/**
 * The commands that carry no action echo, so there is nothing to ask for.
 *
 * Measured against the built CLI rather than reasoned about. A reader that
 * ACCEPTS `--show json` answers with its own JSON document instead of the
 * table or picture the pane asked for; one that draws no JSON ignores
 * `DATUM_SHOW` (it would exit 1 on the flag). The set stays a denylist and
 * not an allowlist: an argv whose first word is NOT a command — an alias
 * the CLI expands in its own pre-pass, a field's own (`mood happy`)
 * included — is a write command under another name, and must be asked.
 *
 * `tests/echo.test.mjs` runs every word here and every echoing verb
 * through the real CLI and holds this list to what the CLI does, so it
 * cannot drift quietly.
 *
 * `help` and `repl` are on the list but NOT in `datum help`'s own command
 * table — the CLI accepts words it does not advertise, which is how a
 * sweep over that table misses them (da-1hdg). They are the first thing a
 * new user types into the bar, and `help --show json` is a parse error.
 */
const NO_ECHO = new Set([
	// Readers and drawn views.
	'tail',
	'head',
	'grep',
	'now',
	'nowview',
	'dayview',
	'view',
	'count',
	'stats',
	'ids',
	'fields',
	'history',
	'why',
	'conflicts',
	'flag',
	'check',
	'info',
	'meta',
	// Command groups whose subcommands take no `--show`.
	'db',
	'alias',
	'config',
	'sync',
	'kit',
	// Writers with no echo of their own.
	'track',
	'ignore',
	'retime',
	'reparse',
	// Words the CLI answers but does not list in `datum help`.
	'help',
	'repl'
]);

/** What asks the CLI for its echo as data — `--show json`, outside argv. */
const SHOW_JSON = 'DATUM_SHOW=json';

/**
 * The command word an argv runs, past the global flags in front of it.
 *
 * `--db _internal tail` is a `tail` (#lib/layout/store.ts runs that shape),
 * and a `-` on its own is `db backup`'s stdout argument, not a flag.
 */
function commandOf(argv: string[]): string | undefined {
	for (let i = 0; i < argv.length; i += 1) {
		const word = argv[i];
		if (word === '--db' || word === '-D') i += 1;
		else if (word.length > 1 && word.startsWith('-')) continue;
		else return word;
	}
	return undefined;
}

/**
 * The env that asks `argv` for its echo as data: `DATUM_SHOW=json`, or
 * nothing.
 *
 * Nothing for a command with no echo (above), nor for an argv with no
 * command word (`--version`, `--help`), which has no echo either. The CLI
 * does the rest: a `--show` the reader typed wins over the env — someone
 * who asks for `--show all` gets `--show all`.
 *
 * The env reaches EVERY side of a multi-side `a , b` run, so it is asked
 * for only when every side has an echo: a reader on one side would answer
 * with its JSON instead of its table. When all do, the run is the
 * several-document case the bar draws a section each for.
 */
export function echoEnv(argv: string[]): string[] {
	const sides: string[][] = [[]];
	for (const word of argv) {
		if (word === ',') sides.push([]);
		else sides[sides.length - 1].push(word);
	}
	const echoes = sides.every((side) => {
		const command = commandOf(side);
		return command !== undefined && !NO_ECHO.has(command) && !learnedNoEcho.has(command) && !readsSpec(command, side);
	});
	return echoes ? [SHOW_JSON] : [];
}

/**
 * `setup`, `setup F`, `setup F --json` and `setup --tag T` read; only a field
 * with a change after it, or `--delete` wherever it stands, echoes the spec
 * entry (da-ez40f).
 */
function readsSpec(command: string, side: string[]): boolean {
	if (command !== 'setup') return false;
	if (side.includes('--delete')) return false;
	const [field, ...flags] = side.slice(side.indexOf('setup') + 1);
	return field === undefined || field.startsWith('-') || !flags.some((w) => w.startsWith('-') && w !== '--json');
}

/** The command words learned this session to go without an echo. */
const learnedNoEcho = new Set<string>();

/**
 * Run `argv` with its echo asked for, and when what answered has no echo
 * after all, run it again without asking. `more` is env of the caller's own,
 * which both runs are given: the second is still the command it asked for.
 *
 * A kit command (`chorelist`) is a reader NO_ECHO cannot name, because the
 * database installs it (da-5x6x0). Given `--show json` it prints its rows
 * as a JSON ARRAY, where a writer's echo is a `{"docs": …}` object, so an
 * array is the tell. Its view runs read-only, so asking twice changes
 * nothing, and the second answer is the table the person asked for.
 *
 * (An alias is named by the database too, and one that expands to a writer
 * with no echo — the chores kit's `newchore`, a `track` — refused the flag
 * (da-tckse). It ignores the env, so there is nothing to ask again.)
 *
 * A one-side line's word is then remembered for the session,
 * so it is asked twice once and not every time; a word remembered wrongly
 * only goes without its echo.
 */
export async function runEchoed(
	run: CommandRunner,
	argv: string[],
	stdin?: string,
	more: string[] = []
): Promise<RunResult> {
	const env = echoEnv(argv);
	const result = await run(argv, stdin, undefined, undefined, undefined, [...env, ...more]);
	const reader = result.code === 0 && result.stdout.startsWith('[');
	if (env.length === 0 || !reader) return result;
	// A line of several sides does not say which side answered, so it is
	// asked again without being learned from.
	if (!argv.includes(',')) learnedNoEcho.add(commandOf(argv) ?? '');
	return run(argv, stdin, undefined, undefined, undefined, more);
}

/**
 * The documents in a command's stdout, or `null` when there are none.
 *
 * Null is the whole of the fallback contract (the bead's trap 1): a
 * command with no echo, a module too old to know `--show json`, a stdout
 * cut off mid-write. The caller then shows the raw lines the CLI printed,
 * which is the truth it has.
 *
 * The document is the LAST line that parses, because `--show json` prints
 * it last and a command that printed something else first (a warning on
 * stdout) still has its echo at the end.
 */
export function parseEcho(stdout: string): EchoDoc[] | null {
	const lines = stdout.split('\n');
	for (let i = lines.length - 1; i >= 0; i -= 1) {
		const line = lines[i].trim();
		if (!line.startsWith('{')) continue;
		let parsed: unknown;
		try {
			parsed = JSON.parse(line);
		} catch {
			continue;
		}
		const docs = (parsed as { docs?: unknown })?.docs;
		if (Array.isArray(docs) && docs.length && docs.every(isDoc)) return docs as EchoDoc[];
	}
	return null;
}

/** Every string the renderer places has to be there, and be a string. */
function isDoc(value: unknown): boolean {
	const d = value as EchoDoc;
	return (
		!!d &&
		typeof d.action === 'string' &&
		typeof d.hid === 'string' &&
		typeof d.id === 'string' &&
		typeof d.header === 'string' &&
		typeof d.info === 'string' &&
		typeof d.data_inline === 'string'
	);
}
