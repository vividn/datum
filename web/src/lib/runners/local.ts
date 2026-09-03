// The TypeScript runner — the seam's default implementation.
//
// It covers the log verbs (`occur`, `switch`, `start`, `stop`, `add`,
// `note`) against the OPFS database, which is what the app needs to be
// useful today. Every other datum command returns
// `EXIT_UNKNOWN_COMMAND`: this is a stand-in for the real CLI, not a second
// implementation of it, and da-1vat retires the whole file by compiling
// `datum-cli` to wasm and installing it via `setRunner`.

import { insertEntry, getFieldSpecs, saveFieldSpec } from '../db.js';
import { parseArgv } from '../parse.js';
import { generateId, buildSlug, contentHash } from '../entryId.js';
import { deriveStoredVerb } from '../verb.js';
import { EXIT_DUPLICATE, EXIT_UNKNOWN_COMMAND, type RunResult } from '../run.js';
import { isAnnotated, type DefaultAction, type Entry, type FieldSpec } from '../types.js';

const DEVICE_ID_KEY = 'datum-device-id';

/** This browser's device id, minted on first use and kept in localStorage. */
export function deviceId(): string {
	const existing = localStorage.getItem(DEVICE_ID_KEY);
	if (existing) return existing;
	const minted = 'web-' + Math.random().toString(36).slice(2, 10);
	localStorage.setItem(DEVICE_ID_KEY, minted);
	return minted;
}

export async function localRun(argv: string[]): Promise<RunResult> {
	if (argv[0] === 'spec' && argv[1] === 'set') return specSet(argv);

	const parsed = parseArgv(argv);
	if (!parsed) {
		return {
			argv,
			code: EXIT_UNKNOWN_COMMAND,
			stdout: '',
			stderr: `datum: ${argv[0]}: not available yet in the browser (needs the wasm CLI)`
		};
	}

	const entry = await buildEntry(parsed, argv.join(' '));
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

async function buildEntry(
	parsed: NonNullable<ReturnType<typeof parseArgv>>,
	rawInput: string
): Promise<Entry> {
	const now = new Date();
	let entryTime = now;
	if (parsed.timeOffset !== undefined) {
		entryTime = new Date(now.getTime() + parsed.timeOffset * 60 * 1000);
	} else if (parsed.timeAbsolute) {
		const [hStr, mStr] = parsed.timeAbsolute.match(/^(\d{2})(\d{2})$/)?.slice(1) ?? [];
		if (hStr && mStr) {
			entryTime = new Date(now);
			entryTime.setHours(parseInt(hStr), parseInt(mStr), 0, 0);
		}
	}

	// A unit rides inside the value, `{"v": 75.2, "u": "kg"}` — there is no
	// `unit` column in schema.sql. Same annotation `coerce.rs` builds for a
	// `weight=75.2kg` data token on the Rust side.
	const state =
		parsed.unit !== undefined && typeof parsed.state === 'number'
			? { v: parsed.state, u: parsed.unit }
			: parsed.state;

	const occurTime = {
		utc: entryTime.toISOString(),
		offsetMinutes: -now.getTimezoneOffset(),
		timezone: Intl.DateTimeFormat().resolvedOptions().timeZone
	};

	return {
		hid: generateId(),
		slug: buildSlug(parsed.field, occurTime),
		contentHash: await contentHash(
			parsed.field,
			occurTime,
			state,
			parsed.dur,
			undefined,
			parsed.data
		),
		field: parsed.field,
		state,
		dur: parsed.dur,
		data: parsed.data,
		verb: deriveStoredVerb(state, parsed.dur, undefined),
		occurTime,
		createTime: now.toISOString(),
		modifyTime: now.toISOString(),
		deviceId: deviceId(),
		rawInput
	};
}

/**
 * One line of stdout for a logged entry.
 *
 * A plain-text stand-in for `render_entry_line`
 * (crates/datum-commands/src/render/action.rs) — same shape (time, field,
 * state) without the field badges and colour the real renderer brings.
 */
export function entryLine(entry: Entry): string {
	const time = new Date(entry.occurTime.utc).toLocaleTimeString([], {
		hour: '2-digit',
		minute: '2-digit'
	});
	const parts = [time, entry.field];
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
