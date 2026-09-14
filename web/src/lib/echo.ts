// The CLI's action echo, as data (da-xfa4, on top of da-yc9w's `--show json`).
//
// The bottom bar is the app's status line: what it shows after a command
// is one SECTION PER DOCUMENT — the action and its hid, the info line,
// and the data on one line. Getting there by parsing the two lines the
// CLI prints would be the thing `docs/web-conventions.md` #4 forbids
// twice over: a second formatter, and a web that rebuilds a string datum
// already owns. So the web asks datum for those strings instead.
//
// `--show json` is appended the way `fitArgv` appends `--width`
// (panes.ts): at the ONE seam a reader's command passes through — the
// REPL's `runArgv`, and `engine.alone` when no REPL is mounted. vividn:
// "The whole web interface can take use json as the default show and then
// render on it's own terms which is nice separation." So the REPL's
// scrollback carries the same documents the bar does, and a
// multi-document result gets the same treatment in both.
//
// The web's own flags are NOT part of what a press teaches: the
// scrollback echoes the argv the caller asked for, not the one the runner
// received (Repl.svelte, engine.press).

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
}

/**
 * The commands that carry no action echo, so there is nothing to ask for.
 *
 * Measured against the built CLI on 2026-09-14 rather than reasoned about:
 * most of these do not merely lack an echo, they REJECT `--show` and exit
 * 1 ("unexpected flag: --show"), some of them at the subcommand (`db
 * backup -`, `alias list`, `spec list`). Appending the flag to one of them
 * would turn a working command into an error, so the set is a denylist and
 * not an allowlist: an argv whose first word is NOT a command — `mood
 * switch happy` field-first, or an alias the CLI expands in its own
 * pre-pass — is a write command under another name, and must get the flag.
 *
 * `tests/echo.test.mjs` runs every word here and every echoing verb
 * through the real CLI and holds this list to what the CLI does, so it
 * cannot drift quietly.
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
	'spec',
	'alias',
	'config',
	'sync',
	'kit',
	// Writers with no echo of their own.
	'track',
	'ignore',
	'retime',
	'reparse'
]);

/** The value form, as the CLI's `Show` reads it. */
const SHOW = '--show';
const JSON_SHOW = 'json';

/**
 * The command word an argv runs, past the global flags in front of it.
 *
 * `--db _internal tail` is a `tail` ($lib/layout/store.ts runs that shape),
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

/** Whether `argv` already names a `--show`, in either spelling. */
const hasShow = (argv: string[]): boolean =>
	argv.some((word) => word === SHOW || word.startsWith(`${SHOW}=`));

/**
 * Ask a reader's command for its echo as data.
 *
 * Left alone: a command with no echo (above), and one that names its own
 * `--show` — someone who asks for `--show all` gets `--show all`, as
 * someone who names a `--width` gets that width.
 *
 * `--show` is a per-SIDE flag: `switch a x , switch b y` is two commands,
 * and one flag at the end reaches only the second, which would leave the
 * first printing its lines and the bar with half a result. So each side
 * is asked separately, which is also what makes a multi-side run the
 * several-document case the bar draws a section each for.
 */
export function echoArgv(argv: string[]): string[] {
	const out: string[] = [];
	let side: string[] = [];
	const flush = (): void => {
		if (side.length && !hasShow(side) && !NO_ECHO.has(commandOf(side) ?? '')) {
			side = [...side, SHOW, JSON_SHOW];
		}
		out.push(...side);
		side = [];
	};
	for (const word of argv) {
		if (word === ',') {
			flush();
			out.push(word);
		} else {
			side.push(word);
		}
	}
	flush();
	return out;
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
