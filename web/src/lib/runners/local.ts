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
// stores what comes back. The *rendering* is not a stand-in either: what a
// logged entry echoes is `datum-commands::render` compiled to wasm, through
// `../renderer.js`. What is left here is the database call and the two
// commands the parser does not own.

import { startOfDay } from '../clock.js';
import { fieldNameHint } from '../fieldName.js';
import {
	insertEntry,
	getEntriesForField,
	getRecentEntries,
	getFieldSpecs,
	saveFieldSpec
} from '../db.js';
import { parseArgv } from '../parser.js';
import { renderLines } from '../renderer.js';
import { EXIT_DUPLICATE, EXIT_ERROR, EXIT_UNKNOWN_COMMAND, type RunResult } from '../run.js';
import { isAnnotated, type DefaultAction, type Entry, type FieldSpec } from '../types.js';
// Type-only, so the 250 KB layout module stays behind `views()` above.
import type { DayviewOptions } from './views.js';

const DEVICE_ID_KEY = 'datum-device-id';

/** How much of a field's history the renderer is handed to compile state from. */
const HISTORY_LIMIT = 500;

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
	// the same SVG document the CLI writes. Which flags each one answers
	// is `viewOptions` below; the multi-day window flags, `--n-days` and
	// `--start-date`, still belong to the real CLI.
	if (argv[0] === 'dayview') {
		const options = viewOptions(argv.slice(1), DAYVIEW_FLAGS);
		if (options) return (await views()).dayview(argv, options);
	}
	if (argv[0] === 'nowview') {
		const options = viewOptions(argv.slice(1), NOWVIEW_FLAGS);
		if (options) return (await views()).nowview(argv, options);
	}

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
	// The field's history, read before the insert: what the CLI compiles the
	// prior state from — the `∅` slot, a transition's colours — it compiles
	// here from these rows. Bounded; a field logged more than this many
	// times still renders, from its most recent stretch.
	const history = await getEntriesForField(entry.field, HISTORY_LIMIT);
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

	return {
		argv,
		code: 0,
		stdout: (await renderLines(entry, 'create', { history })).join('\n'),
		stderr: ''
	};
}

/**
 * The commands above that this file serves in SOME shape but not in every
 * shape.
 *
 * `tail` bare, `dayview` and `nowview` bare or plus their geometry flags,
 * and `spec` as `spec set` alone, which is the command the widget bar's
 * `+` runs. Every other shape of these four reaches the parser and
 * comes back unsupported, and the message for it must name the ARGUMENT:
 * see [`unavailable`].
 */
const PARTLY_SERVED = new Set(['tail', 'dayview', 'nowview', 'spec']);

/**
 * The flags of the drawn views this file serves, in the CLI's own
 * spelling — and `null` when argv carries anything else, so a flag the
 * browser does not answer falls through to the unknown-command message
 * rather than being silently dropped. Bare `dayview` and bare `nowview`
 * reach here too, as an empty options object meaning "the defaults".
 *
 * `--width` / `--height` are the one thing the browser knows that the
 * CLI cannot: how many pixels the band is actually shown in. `datum
 * dayview` authors a 2000px canvas and the pane scaled it to fit, which
 * put the hour axis on screen at 1.5px on a phone and 6.3px on a laptop
 * (da-m0cj) — so the pane says how big its box is, in the CLI's own
 * flags and units, and datum draws a band that size. `nowview` authors
 * 520x276 and was scaled the same way, only in the dimension nobody
 * measured (da-45wt).
 *
 * The other three are the day strip's (da-cgyh). It stacks earlier days
 * above today as one band per day, and a band is one of these calls:
 * `--end-date -3` is the band three days back, `--day-height` is pinned
 * to the same number on every band so their hour gridlines land on the
 * same x, and `--time-axis-height 0` suppresses the axis strip on every
 * band but today's — the only one whose axis the reader needs.
 *
 * Both spellings the flag splitter accepts, `--width=345` and
 * `--width 345`.
 */
const DAYVIEW_FLAGS = ['width', 'height', 'day-height', 'time-axis-height', 'end-date'];
const NOWVIEW_FLAGS = ['width', 'height'];

function viewOptions(rest: string[], served: readonly string[]): DayviewOptions | null {
	const options: DayviewOptions = {};
	for (let i = 0; i < rest.length; i++) {
		const [flag, attached] = splitFlag(rest[i]);
		if (flag === null || !served.includes(flag)) return null;
		const value = attached ?? rest[++i];
		if (value === undefined) return null;
		if (flag === 'end-date') {
			const day = localDayStart(value);
			if (day === null) return null;
			options.dayStart = day;
			continue;
		}
		// Every other flag is a pixel count. Only the axis strip may be
		// zero, and zero is exactly what suppresses it on a stacked band.
		const n = Number(value);
		if (!Number.isFinite(n) || n < 0 || (n === 0 && flag !== 'time-axis-height')) return null;
		if (flag === 'width') options.width = n;
		else if (flag === 'height') options.height = n;
		else if (flag === 'day-height') options.day_height = n;
		else options.time_axis_height = n;
	}
	return options;
}

/**
 * `--end-date`, as the local midnight the band's day starts at.
 *
 * The two spellings `resolve_date_arg` resolves without a calendar
 * vocabulary: an ISO date, and a signed offset in days from today, so
 * `-1` is yesterday and `0` is today. Offsets are what the strip asks
 * for, and they are also what survives a midnight — a band re-renders
 * against whatever "today" has become rather than against the date it
 * was first drawn for. Day words and weekday names are `parse_date`'s
 * larger vocabulary and stay with the real CLI.
 */
function localDayStart(value: string): Date | null {
	const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
	if (iso) return new Date(Number(iso[1]), Number(iso[2]) - 1, Number(iso[3]));
	if (!/^[+-]?\d+$/.test(value)) return null;
	const day = startOfDay(new Date());
	day.setDate(day.getDate() + Number(value));
	return day;
}

/**
 * What is unavailable, said about the right thing.
 *
 * `tail` renders and `tail -n 5` does not, but reporting that as
 * `tail: not available yet in the browser` makes the *command* the subject
 * of the sentence when the command works: a reader takes it as "tail is
 * missing from the web app" and stops trying. For the commands this file
 * serves in some shape, the thing that is unavailable is the argument, so
 * name that instead — the shape the parser's own flag message already uses
 * (da-3k4y).
 *
 * `spec` is the same sentence about a subcommand (da-c081). `spec set`
 * works and the `+` button runs it, so `spec show` answering
 * `spec: not available yet in the browser` calls the whole command missing
 * while the reader is one keystroke from the half that works. Naming the
 * subcommand — `` `show` `` — says which half. That reading matters most
 * on `spec remove`, because there is no other way to remove a field in the
 * browser today: the honest message is all this app can offer, and whether
 * to implement removal is a separate call.
 */
function unavailable(argv: string[], reason: string): string {
	if (!PARTLY_SERVED.has(argv[0]) || argv.length < 2) return reason;
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
	// Each row renders against its field's history — one query per field
	// shown, the renderer excluding the row itself by hid.
	const fields = [...new Set(entries.map((e) => e.field))];
	const histories = new Map(
		await Promise.all(
			fields.map(async (field) => [field, await getEntriesForField(field, HISTORY_LIMIT)] as const)
		)
	);
	const lines = await Promise.all(
		entries.map((e) => renderLines(e, null, { history: histories.get(e.field) ?? [] }))
	);
	return {
		argv: ['tail'],
		code: 0,
		// One info line per row — the CLI's own — until da-1vat brings its table.
		stdout: lines.flat().join('\n'),
		stderr: ''
	};
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
