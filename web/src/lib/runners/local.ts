// The local runner — the seam's default implementation.
//
// It covers the log verbs against the OPFS database, plus `spec set` for
// the widget bar and bare `tail` so the tail pane has something to render
// — which is what the app needs to be useful today. Every other datum
// command returns `EXIT_UNKNOWN_COMMAND`: this is a stand-in for the real
// CLI, not a second implementation of it, and da-1vat retires the whole
// file by compiling `datum-cli` to wasm and installing it via `setRunner`.
//
// The *parsing* is already not a stand-in. da-7scm moved it into
// `datum-parse` compiled to wasm, so this file no longer decides what an
// argv means or what row it produces: it hands argv to `../parser.js` and
// stores what comes back. What is left here is the database call and the
// two commands the parser does not own.

import { clockLabel, dateLabel } from '../clock.js';
import { fieldNameHint } from '../fieldName.js';
import { insertEntry, getRecentEntries, getFieldSpecs, saveFieldSpec } from '../db.js';
import { parseArgv } from '../parser.js';
import { EXIT_DUPLICATE, EXIT_ERROR, EXIT_UNKNOWN_COMMAND, type RunResult } from '../run.js';
import { isAnnotated, type DefaultAction, type Entry, type FieldSpec } from '../types.js';

const DEVICE_ID_KEY = 'datum-device-id';

/** Loaded on demand: the layout module is 250 KB nobody pays for to log. */
const views = () => import('./views.js');

/** This browser's device id, minted on first use and kept in localStorage. */
export function deviceId(): string {
	const existing = localStorage.getItem(DEVICE_ID_KEY);
	if (existing) return existing;
	const minted = 'web-' + Math.random().toString(36).slice(2, 10);
	localStorage.setItem(DEVICE_ID_KEY, minted);
	return minted;
}

/** `datum tail`'s default row count (`tail_state.rs`: `count.unwrap_or(10)`). */
const TAIL_COUNT = 10;

export async function localRun(argv: string[]): Promise<RunResult> {
	if (argv[0] === 'spec' && argv[1] === 'set') return specSet(argv);
	if (argv.length === 1 && argv[0] === 'tail') return tail();
	// The two visual views are datum's own compute and render, reached
	// through the layout module rather than reimplemented (da-6h6p) — so
	// they live behind the seam like everything else, and their stdout is
	// the same SVG document the CLI writes. The window flags — `--n-days`,
	// `--start-date` and the rest — still belong to the real CLI.
	if (argv[0] === 'dayview') {
		const box = geometry(argv.slice(1));
		if (box) return (await views()).dayview(argv, box);
	}
	if (argv.length === 1 && argv[0] === 'nowview') return (await views()).nowview(argv);

	const parsed = await parseArgv(argv, deviceId());
	if (parsed.unsupported !== undefined) {
		return {
			argv,
			code: EXIT_UNKNOWN_COMMAND,
			stdout: '',
			stderr: `datum: ${argv[0]}: ${unavailable(argv, parsed.unsupported)}`
		};
	}
	if (parsed.output !== undefined) {
		// `help`, `--version` and friends: the parser signals them as error
		// variants, but their contract is "render this and exit 0" and the
		// module has already rendered it. Trailing newline trimmed because
		// stdout here is unterminated (see `tail`) and the REPL's <pre>
		// would otherwise show a blank line under every help page.
		return { argv, code: 0, stdout: parsed.output.replace(/\n+$/, ''), stderr: '' };
	}
	if (parsed.error !== undefined || parsed.entry === undefined) {
		// The parser's own message, verbatim — what `datum` would have said.
		return { argv, code: EXIT_ERROR, stdout: '', stderr: `error: ${parsed.error}` };
	}

	const entry = parsed.entry;
	try {
		await insertEntry(entry);
	} catch (e) {
		const message = e instanceof Error ? e.message : String(e);
		// The slug is UNIQUE (schema.sql `entries_slug`), so re-logging the
		// same field at the same minute collides. The CLI reports that as
		// exit 11 so idempotent loggers can treat it as a benign skip.
		const duplicate = /UNIQUE|constraint/i.test(message);
		return {
			argv,
			code: duplicate ? EXIT_DUPLICATE : 1,
			stdout: '',
			stderr: duplicate ? `datum: already have ${entry.slug}` : `error: ${message}`
		};
	}

	return { argv, code: 0, stdout: entryLine(entry), stderr: '' };
}

/** The commands above that this file does serve — bare, and only bare. */
const BARE_ONLY = new Set(['tail', 'dayview', 'nowview']);

/**
 * `dayview`'s two geometry flags, or `null` when argv carries anything else.
 *
 * The one thing the browser knows that the CLI cannot: how many pixels the
 * band is actually shown in. `datum dayview` authors a 2000px canvas and
 * the pane scaled it to fit, which put the hour axis on screen at 1.5px on
 * a phone and 6.3px on a laptop (da-m0cj) — so the pane says how big its
 * box is, in the CLI's own flags and units, and datum draws a band that
 * size. The values come from `paneArgv`, which floors them.
 *
 * Both spellings the flag splitter accepts, `--width=345` and
 * `--width 345`; a flag this file does not serve, or a value that is not a
 * positive number, falls through to the unknown-command message rather
 * than being silently dropped.
 */
function geometry(rest: string[]): { width?: number; height?: number } | null {
	const box: { width?: number; height?: number } = {};
	for (let i = 0; i < rest.length; i++) {
		const [flag, attached] = splitFlag(rest[i]);
		if (flag !== 'width' && flag !== 'height') return null;
		const value = Number(attached ?? rest[++i]);
		if (!Number.isFinite(value) || value <= 0) return null;
		box[flag] = value;
	}
	return box;
}

/**
 * What is unavailable, said about the right thing.
 *
 * `tail` renders and `tail -n 5` does not, but reporting that as
 * `tail: not available yet in the browser` makes the *command* the subject
 * of the sentence when the command works: a reader takes it as "tail is
 * missing from the web app" and stops trying. For the commands this file
 * serves bare, the thing that is unavailable is the argument, so name that
 * instead — the shape the parser's own flag message already uses (da-3k4y).
 */
function unavailable(argv: string[], reason: string): string {
	if (!BARE_ONLY.has(argv[0]) || argv.length < 2) return reason;
	return `\`${argv[1]}\` is not available yet in the browser`;
}

/**
 * The tail pane's stand-in for `datum tail`.
 *
 * Bare `tail` only. Field filters, `-n`, `--table` and the metric columns
 * are the real CLI's job — answering them here would grow a second datum
 * in TypeScript, which is the thing da-hrz0 says not to do — so
 * `tail mood` falls through to the unknown-command message like every
 * other command the stand-in doesn't cover.
 *
 * Oldest at the top, newest at the bottom, the way `tail` prints.
 */
async function tail(): Promise<RunResult> {
	const entries = await getRecentEntries(TAIL_COUNT);
	entries.reverse();
	return {
		argv: ['tail'],
		code: 0,
		stdout: entries.map(entryLine).join('\n'),
		stderr: ''
	};
}

/**
 * One line of stdout for a logged entry.
 *
 * A plain-text stand-in for `render_entry_line`
 * (crates/datum-commands/src/render/action.rs) — same shape (time, field,
 * state) without the field badges and colour the real renderer brings. The
 * time is datum's one convention, 24-hour ($lib/clock.ts), which is the
 * half of that parity claim the reader can see next to the nowview axis.
 *
 * The stamp is only a clock when the entry has one. `occur coffee -y`
 * stores midnight as an *anchor* and says so in `occurTimeUncertainty`:
 * `P1D`, precise to the day. Printing `00:00` for that answers with a
 * clock time the record disclaims, so a P1D row degrades to its bare date
 * exactly as `format_timestamp_ref` degrades it (da-kb8p). Only `P1D`
 * does — every other value, `undefined` included, is a row with a real
 * time of day.
 */
export function entryLine(entry: Entry): string {
	const stamp =
		entry.occurTimeUncertainty === 'P1D'
			? dateLabel(entry.occurTime)
			: clockLabel(new Date(entry.occurTime.utc));
	const parts = [stamp, entry.field];
	if (entry.state !== undefined) parts.push(`= ${formatValue(entry.state)}`);
	if (entry.data) {
		for (const [k, v] of Object.entries(entry.data as Record<string, unknown>)) {
			parts.push(`${k}=${formatValue(v)}`);
		}
	}
	return parts.join(' ');
}

/** Render a stored value, unwrapping the `{v, u}` unit annotation. */
export function formatValue(value: unknown): string {
	if (isAnnotated(value)) return `${value.v}${value.u}`;
	return typeof value === 'string' ? value : JSON.stringify(value);
}

/** The `--default-verb` values `spec set` accepts, per `DEFAULT_VERB_CHOICES`. */
const DEFAULT_VERB_CHOICES: readonly DefaultAction[] = ['occur', 'start', 'switch', 'add'];

/**
 * `spec set <FIELD> [--default-verb V] [--state S]… [--color C]`.
 *
 * The subset of the real command the widget bar needs: a field's declared
 * shape and its states. Existing keys on the spec are preserved, and
 * repeated `--state` flags *replace* the allowed set rather than appending
 * — both matching `spec_set` in
 * crates/datum-commands/src/commands/spec.rs.
 */
async function specSet(argv: string[]): Promise<RunResult> {
	const fail = (message: string): RunResult => ({
		argv,
		code: 1,
		stdout: '',
		stderr: `error: ${message}`
	});

	let field: string | undefined;
	let defaultVerb: DefaultAction | undefined;
	let color: string | undefined;
	const states: string[] = [];

	const rest = argv.slice(2);
	for (let i = 0; i < rest.length; i++) {
		// Both spellings the CLI's flag splitter accepts: `--state=happy`
		// and `--state happy`.
		const [flag, attached] = splitFlag(rest[i]);
		if (flag === null) {
			if (field !== undefined) return fail(`unexpected argument: '${rest[i]}'`);
			field = rest[i];
			continue;
		}
		const value = attached ?? rest[++i];
		if (value === undefined) return fail(`--${flag} needs a value`);
		switch (flag) {
			case 'default-verb':
				if (!(DEFAULT_VERB_CHOICES as readonly string[]).includes(value)) {
					return fail(`--default-verb must be one of ${DEFAULT_VERB_CHOICES.join(' | ')}`);
				}
				defaultVerb = value as DefaultAction;
				break;
			case 'state':
				states.push(value);
				break;
			case 'color':
				color = value;
				break;
			default:
				return fail(`unknown flag: --${flag} (not available yet in the browser)`);
		}
	}

	if (field === undefined) return fail('missing <FIELD>');

	// Declaring a field is the one place a name is chosen, so it is the
	// place to refuse a name nothing can log to: `my coffee`, `cups=2` and
	// `-weird` were all declarable, and the app then refused to log to the
	// chip it had just made (da-b049). Refused here rather than in the (+)
	// form so that typing the command has the same answer as pressing the
	// button. `datum-parse`'s `spec set` refuses the same names, and
	// `tests/fixtures/field-name.json` is what holds the two together.
	const hint = fieldNameHint(field);
	if (hint !== null) return fail(`cannot declare '${field}': ${hint}`);

	const existing = (await getFieldSpecs()).get(field);
	const spec: FieldSpec = { ...(existing ?? {}), field };
	if (defaultVerb !== undefined) spec.default_verb = defaultVerb;
	else spec.default_verb ??= 'occur';
	if (states.length > 0) spec.allowed_states = states;
	if (color !== undefined) spec.color = color;

	await saveFieldSpec(spec);
	return { argv, code: 0, stdout: `✓ Spec saved for '${field}'`, stderr: '' };
}

/** `--flag=value` → `['flag', 'value']`, `--flag` → `['flag', null]`, else `[null, null]`. */
function splitFlag(token: string): [string | null, string | null] {
	if (!token.startsWith('--')) return [null, null];
	const body = token.slice(2);
	const eq = body.indexOf('=');
	return eq < 0 ? [body, null] : [body.slice(0, eq), body.slice(eq + 1)];
}
